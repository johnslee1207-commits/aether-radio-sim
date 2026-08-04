//! L4 soak / stress runner (Ops Framework §11 Level 4).

use crate::{BenchProfile, BenchReport, PipelineBench, PipelineBenchError};
use metrics_engine::{HealthManager, HealthThresholds};
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
}

fn default_health_policy() -> String {
    "configs/ops/health_policy.yaml".into()
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
        let bench_path = self.base_dir.join(&p.bench_profile);
        let mut bench_profile = BenchProfile::load_path(&bench_path)?;
        if let Some(n) = p.symbol_count_override {
            bench_profile.symbol_count = n;
        }
        if let Some(s) = p.streams_override {
            bench_profile.streams = s;
        }
        if let Some(ref events) = p.events_path {
            bench_profile.events_path = events.clone();
        }
        if let Some(ref fault) = p.fault_config {
            bench_profile.fault_config = fault.clone();
        }
        bench_profile.report_path = p.report_path.clone();

        let (bench, metrics) = PipelineBench::new(bench_profile)
            .with_base_dir(&self.base_dir)
            .run()?;

        let thr =
            HealthThresholds::load_path(self.base_dir.join(&p.health_policy)).unwrap_or_default();
        let mut health = HealthManager::new(thr);
        let _ = health.evaluate(&metrics.layered_snapshot(), None);
        let health_state = health.state().as_str().to_string();

        let miss_ratio = if bench.packets == 0 {
            1.0
        } else {
            bench.radio.symbol_miss as f64 / bench.packets as f64
        };

        let gates = vec![
            SoakGate {
                name: "min_packets".into(),
                passed: bench.packets >= p.min_packets,
                detail: format!("{} >= {}", bench.packets, p.min_packets),
            },
            SoakGate {
                name: "max_sequence_gaps".into(),
                passed: bench.sequence_gaps <= p.max_sequence_gaps,
                detail: format!("{} <= {}", bench.sequence_gaps, p.max_sequence_gaps),
            },
            SoakGate {
                name: "max_deadline_miss_ratio".into(),
                passed: miss_ratio <= p.max_deadline_miss_ratio,
                detail: format!("{miss_ratio:.4} <= {}", p.max_deadline_miss_ratio),
            },
        ];

        let passed = gates.iter().all(|g| g.passed);
        let recovery_actions = bench.recovery_actions;

        let report = SoakReport {
            profile_id: p.id.clone(),
            passed,
            gates,
            bench,
            fault_config: p.fault_config.clone(),
            health: Some(health_state),
            recovery_actions,
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
        // Keep CI fast: shrink soak for unit test.
        profile.symbol_count_override = Some(16);
        profile.streams_override = Some(1);
        profile.min_packets = 8;
        profile.max_deadline_miss_ratio = 1.0;
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
        assert!(report.health.is_some());
    }
}
