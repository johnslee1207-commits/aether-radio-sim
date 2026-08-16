//! L4 soak / stress runner (Ops Framework §11 Level 4).

use crate::{BenchProfile, BenchReport, PipelineBench, PipelineBenchError};
use metrics_engine::{HealthManager, HealthState, HealthThresholds};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoakProfile {
    pub version: String,
    pub id: String,
    pub bench_profile: String,
    #[serde(default)]
    pub fault_config: Option<String>,
    #[serde(default)]
    pub symbol_count_override: Option<u64>,
    #[serde(default)]
    pub streams_override: Option<u32>,
    pub report_path: String,
    #[serde(default)]
    pub events_path: Option<String>,
    pub max_sequence_gaps: u64,
    pub max_deadline_miss_ratio: f64,
    pub min_packets: u64,
    #[serde(default = "default_health_policy")]
    pub health_policy: String,
    /// Number of PipelineBench rounds; health is evaluated after each.
    #[serde(default = "default_rounds")]
    pub rounds: u32,
    /// When true, every round must end in NORMAL health.
    #[serde(default)]
    pub require_health_normal: bool,
    /// Wall-clock sleep between rounds (ms). 0 = no sleep (sim-time only).
    #[serde(default)]
    pub round_interval_ms: u64,
}

fn default_health_policy() -> String {
    "configs/ops/health_policy.yaml".into()
}

fn default_rounds() -> u32 {
    1
}

impl SoakProfile {
    pub fn from_yaml_str(s: &str) -> Result<Self, SoakError> {
        serde_yaml::from_str(s).map_err(|e| SoakError::Config(e.to_string()))
    }

    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, SoakError> {
        let text = fs::read_to_string(path.as_ref())
            .map_err(|e| SoakError::Config(format!("read {}: {e}", path.as_ref().display())))?;
        Self::from_yaml_str(&text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoakGate {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoakRoundHealth {
    pub round: u32,
    pub health: String,
    pub packets: u64,
    pub sequence_gaps: u64,
    pub recovery_actions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoakReport {
    pub profile_id: String,
    pub passed: bool,
    pub gates: Vec<SoakGate>,
    pub bench: BenchReport,
    pub fault_config: Option<String>,
    #[serde(default)]
    pub health: Option<String>,
    #[serde(default)]
    pub recovery_actions: u64,
    #[serde(default)]
    pub rounds: u32,
    #[serde(default)]
    pub round_health: Vec<SoakRoundHealth>,
    #[serde(default)]
    pub elapsed_wall_ms: u64,
    #[serde(default)]
    pub round_interval_ms: u64,
}

#[derive(Debug, Error)]
pub enum SoakError {
    #[error("config error: {0}")]
    Config(String),
    #[error("bench error: {0}")]
    Bench(#[from] PipelineBenchError),
    #[error("io error: {0}")]
    Io(String),
    #[error("soak failed")]
    Failed(Box<SoakReport>),
}

pub struct SoakRunner {
    profile: SoakProfile,
    base_dir: PathBuf,
}

impl SoakRunner {
    pub fn new(profile: SoakProfile) -> Self {
        Self {
            profile,
            base_dir: PathBuf::from("."),
        }
    }

    pub fn with_base_dir(mut self, base: impl Into<PathBuf>) -> Self {
        self.base_dir = base.into();
        self
    }

    pub fn run(&self) -> Result<SoakReport, SoakError> {
        let p = &self.profile;
        let rounds = p.rounds.max(1);
        let thr =
            HealthThresholds::load_path(self.base_dir.join(&p.health_policy)).unwrap_or_default();
        let mut health = HealthManager::new(thr);

        let mut aggregate = BenchReport::default();
        let mut round_health = Vec::with_capacity(rounds as usize);
        let mut last_health = HealthState::Normal;
        let wall_start = std::time::Instant::now();

        for r in 1..=rounds {
            if r > 1 && p.round_interval_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(p.round_interval_ms));
            }
            let bench_path = self.base_dir.join(&p.bench_profile);
            let mut bench_profile = BenchProfile::load_path(&bench_path)?;
            if let Some(n) = p.symbol_count_override {
                bench_profile.symbol_count = n;
            }
            if let Some(s) = p.streams_override {
                bench_profile.streams = s;
            }
            if let Some(ref events) = p.events_path {
                if rounds == 1 {
                    bench_profile.events_path = events.clone();
                } else {
                    let stem = Path::new(events)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("soak_events");
                    let parent = Path::new(events)
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| PathBuf::from("data/reports"));
                    bench_profile.events_path = parent
                        .join(format!("{stem}_r{r}.jsonl"))
                        .to_string_lossy()
                        .into();
                }
            }
            if let Some(ref fault) = p.fault_config {
                bench_profile.fault_config = fault.clone();
            }
            bench_profile.report_path = p.report_path.clone();

            let (bench, metrics) = PipelineBench::new(bench_profile)
                .with_base_dir(&self.base_dir)
                .run()?;

            let _ = health.evaluate(&metrics.layered_snapshot(), None);
            last_health = health.state();
            round_health.push(SoakRoundHealth {
                round: r,
                health: last_health.as_str().to_string(),
                packets: bench.packets,
                sequence_gaps: bench.sequence_gaps,
                recovery_actions: bench.recovery_actions,
            });

            aggregate.packets = aggregate.packets.saturating_add(bench.packets);
            aggregate.bytes = aggregate.bytes.saturating_add(bench.bytes);
            aggregate.sequence_gaps = aggregate.sequence_gaps.saturating_add(bench.sequence_gaps);
            aggregate.late_packets = aggregate.late_packets.saturating_add(bench.late_packets);
            aggregate.recovery_actions = aggregate
                .recovery_actions
                .saturating_add(bench.recovery_actions);
            aggregate.radio.symbol_miss = aggregate
                .radio
                .symbol_miss
                .saturating_add(bench.radio.symbol_miss);
            aggregate.radio.slot_miss = aggregate
                .radio
                .slot_miss
                .saturating_add(bench.radio.slot_miss);
            aggregate.sim_duration_ns = aggregate
                .sim_duration_ns
                .saturating_add(bench.sim_duration_ns);
            aggregate.ring_occupancy_peak =
                aggregate.ring_occupancy_peak.max(bench.ring_occupancy_peak);
            // Keep latest latency / throughput sample for report readability.
            aggregate.latency = bench.latency;
            aggregate.throughput = bench.throughput;
        }

        let miss_ratio = if aggregate.packets == 0 {
            1.0
        } else {
            aggregate.radio.symbol_miss as f64 / aggregate.packets as f64
        };

        let mut gates = vec![
            SoakGate {
                name: "min_packets".into(),
                passed: aggregate.packets >= p.min_packets,
                detail: format!("{} >= {}", aggregate.packets, p.min_packets),
            },
            SoakGate {
                name: "max_sequence_gaps".into(),
                passed: aggregate.sequence_gaps <= p.max_sequence_gaps,
                detail: format!("{} <= {}", aggregate.sequence_gaps, p.max_sequence_gaps),
            },
            SoakGate {
                name: "max_deadline_miss_ratio".into(),
                passed: miss_ratio <= p.max_deadline_miss_ratio,
                detail: format!("{miss_ratio:.4} <= {}", p.max_deadline_miss_ratio),
            },
            SoakGate {
                name: "rounds_completed".into(),
                passed: round_health.len() as u32 == rounds,
                detail: format!("{} == {}", round_health.len(), rounds),
            },
        ];

        if p.require_health_normal {
            let all_normal = round_health
                .iter()
                .all(|h| h.health == HealthState::Normal.as_str());
            gates.push(SoakGate {
                name: "health_normal_each_round".into(),
                passed: all_normal,
                detail: format!(
                    "{:?}",
                    round_health.iter().map(|h| &h.health).collect::<Vec<_>>()
                ),
            });
        }

        let passed = gates.iter().all(|g| g.passed);
        let recovery_actions = aggregate.recovery_actions;
        let elapsed_wall_ms = wall_start.elapsed().as_millis() as u64;

        let report = SoakReport {
            profile_id: p.id.clone(),
            passed,
            gates,
            bench: aggregate,
            fault_config: p.fault_config.clone(),
            health: Some(last_health.as_str().to_string()),
            recovery_actions,
            rounds,
            round_health,
            elapsed_wall_ms,
            round_interval_ms: p.round_interval_ms,
        };

        if let Some(parent) = Path::new(&p.report_path).parent() {
            let _ = fs::create_dir_all(self.base_dir.join(parent));
        }
        let out = self.base_dir.join(&p.report_path);
        fs::write(
            &out,
            serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into()),
        )
        .map_err(|e| SoakError::Io(e.to_string()))?;

        if !passed {
            return Err(SoakError::Failed(Box::new(report)));
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soak_mvp_runs() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut profile = SoakProfile::load_path(root.join("configs/soak_profile.yaml")).unwrap();
        profile.symbol_count_override = Some(16);
        profile.streams_override = Some(1);
        profile.min_packets = 8;
        profile.max_deadline_miss_ratio = 1.0;
        profile.rounds = 2;
        profile.require_health_normal = false;
        profile.events_path = Some(format!(
            "data/reports/soak_events_test_{}.jsonl",
            std::process::id()
        ));
        let report = SoakRunner::new(profile)
            .with_base_dir(root)
            .run()
            .expect("soak");
        assert!(report.passed);
        assert!(report.bench.packets >= 8);
        assert_eq!(report.rounds, 2);
        assert_eq!(report.round_health.len(), 2);
        assert!(report.health.is_some());
        assert!(report.elapsed_wall_ms > 0 || report.round_interval_ms == 0);
    }
}
