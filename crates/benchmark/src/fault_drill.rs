//! Fault drill: stress faults + recovery policy exercise (Ops Framework §12).

use crate::{BenchProfile, BenchReport, PipelineBench, PipelineBenchError};
use metrics_engine::{HealthManager, HealthState, HealthThresholds, RecoveryPolicy};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaultDrillProfile {
    pub version: String,
    pub id: String,
    pub bench_profile: String,
    pub fault_config: String,
    pub recovery_policy: String,
    #[serde(default)]
    pub symbol_count: Option<u64>,
    #[serde(default)]
    pub streams: Option<u32>,
    pub report_path: String,
    #[serde(default)]
    pub events_path: Option<String>,
}

impl FaultDrillProfile {
    pub fn from_yaml_str(s: &str) -> Result<Self, FaultDrillError> {
        serde_yaml::from_str(s).map_err(|e| FaultDrillError::Config(e.to_string()))
    }

    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, FaultDrillError> {
        let text = fs::read_to_string(path.as_ref()).map_err(|e| {
            FaultDrillError::Config(format!("read {}: {e}", path.as_ref().display()))
        })?;
        Self::from_yaml_str(&text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultDrillReport {
    pub profile_id: String,
    pub passed: bool,
    pub recovery_policy_id: String,
    pub recovery_actions: u64,
    pub health: String,
    pub bench: BenchReport,
    pub fault_config: String,
}

#[derive(Debug, Error)]
pub enum FaultDrillError {
    #[error("config error: {0}")]
    Config(String),
    #[error("bench error: {0}")]
    Bench(#[from] PipelineBenchError),
    #[error("io error: {0}")]
    Io(String),
}

pub struct FaultDrillRunner {
    profile: FaultDrillProfile,
    base_dir: PathBuf,
}

impl FaultDrillRunner {
    pub fn new(profile: FaultDrillProfile) -> Self {
        Self {
            profile,
            base_dir: PathBuf::from("."),
        }
    }

    pub fn with_base_dir(mut self, base: impl Into<PathBuf>) -> Self {
        self.base_dir = base.into();
        self
    }

    pub fn run(&self) -> Result<FaultDrillReport, FaultDrillError> {
        let p = &self.profile;
        let mut bench_profile = BenchProfile::load_path(self.base_dir.join(&p.bench_profile))?;
        bench_profile.fault_config = p.fault_config.clone();
        if let Some(n) = p.symbol_count {
            bench_profile.symbol_count = n;
        }
        if let Some(s) = p.streams {
            bench_profile.streams = s;
        }
        if let Some(ref events) = p.events_path {
            bench_profile.events_path = events.clone();
        }
        bench_profile.report_path = p.report_path.clone();

        let recovery = RecoveryPolicy::load_path(self.base_dir.join(&p.recovery_policy))
            .map_err(|e| FaultDrillError::Config(e.to_string()))?;

        let (bench, metrics) = PipelineBench::new(bench_profile)
            .with_base_dir(&self.base_dir)
            .run()?;

        let thr = HealthThresholds::load_path(self.base_dir.join("configs/ops/health_policy.yaml"))
            .unwrap_or_default();
        let mut health = HealthManager::new(thr);
        let _ = health.evaluate(&metrics.layered_snapshot(), None);

        // Drill passes if the datapath completed and recovery policy is loadable;
        // stress faults are expected to exercise recovery_actions / gaps.
        let passed = bench.packets > 0
            && (bench.recovery_actions > 0
                || bench.sequence_gaps > 0
                || bench.late_packets > 0
                || health.state() != HealthState::Normal);

        let report = FaultDrillReport {
            profile_id: p.id.clone(),
            passed,
            recovery_policy_id: recovery.id,
            recovery_actions: bench.recovery_actions,
            health: health.state().as_str().to_string(),
            bench,
            fault_config: p.fault_config.clone(),
        };

        if let Some(parent) = Path::new(&p.report_path).parent() {
            let _ = fs::create_dir_all(self.base_dir.join(parent));
        }
        fs::write(
            self.base_dir.join(&p.report_path),
            serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into()),
        )
        .map_err(|e| FaultDrillError::Io(e.to_string()))?;

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fault_drill_runs() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut profile =
            FaultDrillProfile::load_path(root.join("configs/fault_drill.yaml")).unwrap();
        profile.symbol_count = Some(24);
        profile.events_path = Some(format!(
            "data/reports/fault_drill_events_test_{}.jsonl",
            std::process::id()
        ));
        let report = FaultDrillRunner::new(profile)
            .with_base_dir(root)
            .run()
            .expect("drill");
        assert!(report.bench.packets > 0);
        assert!(!report.recovery_policy_id.is_empty());
    }
}
