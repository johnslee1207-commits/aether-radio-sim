//! Health manager state machine (Ops Framework §8).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::layers::LayeredMetricsSnapshot;
use crate::taxonomy;
use crate::{EventLogger, LogEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HealthState {
    Normal,
    Warning,
    Degraded,
    Failed,
    Recovery,
}

impl HealthState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Warning => "WARNING",
            Self::Degraded => "DEGRADED",
            Self::Failed => "FAILED",
            Self::Recovery => "RECOVERY",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthThresholds {
    pub max_loss_rate: f64,
    pub max_seq_gap_per_window: u64,
    pub max_latency_p99_ns: u64,
    pub max_jitter_ns: u64,
    pub max_symbol_deadline_miss_per_window: u64,
    pub max_gpu_stall_ns: u64,
    pub window_ms: u64,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            max_loss_rate: 0.001,
            max_seq_gap_per_window: 0,
            max_latency_p99_ns: 50_000,
            max_jitter_ns: 5_000,
            max_symbol_deadline_miss_per_window: 8,
            max_gpu_stall_ns: 100_000,
            window_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct HealthPolicyFile {
    pub version: String,
    pub id: String,
    pub thresholds: HealthThresholds,
}

#[derive(Debug, Error)]
pub enum HealthError {
    #[error("config error: {0}")]
    Config(String),
}

impl HealthThresholds {
    pub fn from_policy_yaml(s: &str) -> Result<Self, HealthError> {
        let file: HealthPolicyFile =
            serde_yaml::from_str(s).map_err(|e| HealthError::Config(e.to_string()))?;
        Ok(file.thresholds)
    }

    pub fn load_path(path: impl AsRef<std::path::Path>) -> Result<Self, HealthError> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| HealthError::Config(format!("read: {e}")))?;
        Self::from_policy_yaml(&text)
    }
}

pub struct HealthManager {
    thresholds: HealthThresholds,
    state: HealthState,
    pub transitions: u64,
}

impl HealthManager {
    pub fn new(thresholds: HealthThresholds) -> Self {
        Self {
            thresholds,
            state: HealthState::Normal,
            transitions: 0,
        }
    }

    pub fn state(&self) -> HealthState {
        self.state
    }

    pub fn thresholds(&self) -> &HealthThresholds {
        &self.thresholds
    }

    /// Evaluate layered metrics and update state. Returns previous state if changed.
    pub fn evaluate(
        &mut self,
        snap: &LayeredMetricsSnapshot,
        events: Option<&mut EventLogger>,
    ) -> Option<HealthState> {
        let rx = snap.transport.rx_packets.max(1);
        let loss_rate = snap.transport.drop as f64 / rx as f64;
        let latency = snap
            .transport
            .latency_last_ns
            .max(snap.transport.latency_max_ns);

        let next = if snap.transport.gap_count > self.thresholds.max_seq_gap_per_window
            || snap.radio.deadline_miss > self.thresholds.max_symbol_deadline_miss_per_window * 4
            || loss_rate > self.thresholds.max_loss_rate * 10.0
        {
            HealthState::Failed
        } else if snap.transport.gap_count > 0
            || snap.radio.deadline_miss > self.thresholds.max_symbol_deadline_miss_per_window
            || latency > self.thresholds.max_latency_p99_ns
            || snap.memory.ring_stall_ns > self.thresholds.max_gpu_stall_ns
            || loss_rate > self.thresholds.max_loss_rate
        {
            if latency > self.thresholds.max_latency_p99_ns * 2
                || snap.radio.deadline_miss > self.thresholds.max_symbol_deadline_miss_per_window
            {
                HealthState::Degraded
            } else {
                HealthState::Warning
            }
        } else if self.state == HealthState::Failed || self.state == HealthState::Degraded {
            HealthState::Recovery
        } else {
            HealthState::Normal
        };

        if next != self.state {
            let prev = self.state;
            self.state = next;
            self.transitions += 1;
            if let Some(log) = events {
                let _ = log.emit(
                    &LogEvent::now(taxonomy::HEALTH_CHANGED)
                        .with_component("health")
                        .with_detail(format!("{} -> {}", prev.as_str(), next.as_str())),
                );
            }
            return Some(prev);
        }
        // Recovery → Normal when clean
        if self.state == HealthState::Recovery && next == HealthState::Normal {
            let prev = self.state;
            self.state = HealthState::Normal;
            self.transitions += 1;
            return Some(prev);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::LayeredMetricsSnapshot;

    #[test]
    fn healthy_stays_normal() {
        let mut h = HealthManager::new(HealthThresholds::default());
        let snap = LayeredMetricsSnapshot::default();
        assert!(h.evaluate(&snap, None).is_none());
        assert_eq!(h.state(), HealthState::Normal);
    }

    #[test]
    fn gap_triggers_warning_or_failed() {
        let mut h = HealthManager::new(HealthThresholds::default());
        let mut snap = LayeredMetricsSnapshot::default();
        snap.transport.rx_packets = 100;
        snap.transport.gap_count = 1;
        let _ = h.evaluate(&snap, None);
        assert!(matches!(
            h.state(),
            HealthState::Warning | HealthState::Failed | HealthState::Degraded
        ));
    }

    #[test]
    fn load_repo_health_policy() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let t = HealthThresholds::load_path(root.join("configs/ops/health_policy.yaml")).unwrap();
        assert_eq!(t.max_seq_gap_per_window, 0);
    }
}
