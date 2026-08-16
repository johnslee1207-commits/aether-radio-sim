//! Acceptance gates over PipelineBench reports and ethernet model facts.

use crate::{BenchProfile, BenchReport, PipelineBench, PipelineBenchError};
use ethernet_model::EthernetModelConfig;
use metrics_engine::{HealthManager, HealthState, HealthThresholds};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcceptanceProfile {
    pub version: String,
    pub id: String,
    pub bench_profile: String,
    pub ethernet_config: String,
    pub min_model_bandwidth_gbps: f64,
    pub max_p99_latency_ns: u64,
    pub max_sequence_gaps: u64,
    pub max_late_packets: u64,
    pub max_symbol_miss: u64,
    pub min_packets: u64,
    pub report_path: String,
    #[serde(default = "default_health_policy")]
    pub health_policy: String,
    /// When true, HealthManager must evaluate to NORMAL after the bench.
    #[serde(default)]
    pub require_health_normal: bool,
}

fn default_health_policy() -> String {
    "configs/ops/health_policy.yaml".into()
}

impl AcceptanceProfile {
    pub fn from_yaml_str(s: &str) -> Result<Self, AcceptanceError> {
        serde_yaml::from_str(s).map_err(|e| AcceptanceError::Config(e.to_string()))
    }

    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, AcceptanceError> {
        let text = fs::read_to_string(path.as_ref()).map_err(|e| {
            AcceptanceError::Config(format!("read {}: {e}", path.as_ref().display()))
        })?;
        Self::from_yaml_str(&text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceReport {
    pub profile_id: String,
    pub passed: bool,
    pub gates: Vec<GateResult>,
    pub bench: BenchReport,
    #[serde(default)]
    pub health: Option<String>,
}

#[derive(Debug, Error)]
pub enum AcceptanceError {
    #[error("config error: {0}")]
    Config(String),
    #[error("bench error: {0}")]
    Bench(#[from] PipelineBenchError),
    #[error("io error: {0}")]
    Io(String),
    #[error("acceptance failed")]
    Failed(Box<AcceptanceReport>),
}

pub struct AcceptanceRunner {
    profile: AcceptanceProfile,
    base_dir: PathBuf,
}

impl AcceptanceRunner {
    pub fn new(profile: AcceptanceProfile) -> Self {
        Self {
            profile,
            base_dir: PathBuf::from("."),
        }
    }

    pub fn with_base_dir(mut self, base: impl Into<PathBuf>) -> Self {
        self.base_dir = base.into();
        self
    }

    pub fn run(&self) -> Result<AcceptanceReport, AcceptanceError> {
        let p = &self.profile;
        let eth_path = self.base_dir.join(&p.ethernet_config);
        let eth_yaml = fs::read_to_string(&eth_path)
            .map_err(|e| AcceptanceError::Config(format!("read {}: {e}", eth_path.display())))?;
        let eth = EthernetModelConfig::from_yaml_str(&eth_yaml)
            .map_err(|e| AcceptanceError::Config(e.to_string()))?;

        let mut gates = Vec::new();
        let bw_ok = eth.bandwidth_gbps + f64::EPSILON >= p.min_model_bandwidth_gbps;
        gates.push(GateResult {
            name: "ethernet_model_bandwidth".into(),
            passed: bw_ok,
            detail: format!(
                "model {} Gbps vs min {}",
                eth.bandwidth_gbps, p.min_model_bandwidth_gbps
            ),
        });

        let bench_path = self.base_dir.join(&p.bench_profile);
        let bench_yaml = fs::read_to_string(&bench_path)
            .map_err(|e| AcceptanceError::Config(format!("read {}: {e}", bench_path.display())))?;
        let bench_profile = BenchProfile::from_yaml_str(&bench_yaml)?;
        let (bench, metrics) = PipelineBench::new(bench_profile)
            .with_base_dir(&self.base_dir)
            .run()?;

        gates.push(GateResult {
            name: "min_packets".into(),
            passed: bench.packets >= p.min_packets,
            detail: format!("{} >= {}", bench.packets, p.min_packets),
        });
        gates.push(GateResult {
            name: "max_p99_latency_ns".into(),
            passed: bench.latency.p99_ns <= p.max_p99_latency_ns,
            detail: format!("{} <= {}", bench.latency.p99_ns, p.max_p99_latency_ns),
        });
        gates.push(GateResult {
            name: "max_sequence_gaps".into(),
            passed: bench.sequence_gaps <= p.max_sequence_gaps,
            detail: format!("{} <= {}", bench.sequence_gaps, p.max_sequence_gaps),
        });
        gates.push(GateResult {
            name: "max_late_packets".into(),
            passed: bench.late_packets <= p.max_late_packets,
            detail: format!("{} <= {}", bench.late_packets, p.max_late_packets),
        });
        gates.push(GateResult {
            name: "max_symbol_miss".into(),
            passed: bench.radio.symbol_miss <= p.max_symbol_miss,
            detail: format!("{} <= {}", bench.radio.symbol_miss, p.max_symbol_miss),
        });

        let thr =
            HealthThresholds::load_path(self.base_dir.join(&p.health_policy)).unwrap_or_default();
        let mut health = HealthManager::new(thr);
        let _ = health.evaluate(&metrics.layered_snapshot(), None);
        let health_state = health.state();
        if p.require_health_normal {
            gates.push(GateResult {
                name: "health_normal".into(),
                passed: health_state == HealthState::Normal,
                detail: health_state.as_str().to_string(),
            });
        }

        let passed = gates.iter().all(|g| g.passed);
        let report = AcceptanceReport {
            profile_id: p.id.clone(),
            passed,
            gates,
            bench,
            health: Some(health_state.as_str().to_string()),
        };

        if let Some(parent) = Path::new(&p.report_path).parent() {
            let _ = fs::create_dir_all(self.base_dir.join(parent));
        }
        let out = self.base_dir.join(&p.report_path);
        fs::write(
            &out,
            serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into()),
        )
        .map_err(|e| AcceptanceError::Io(e.to_string()))?;

        if !passed {
            return Err(AcceptanceError::Failed(Box::new(report)));
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    #[test]
    fn acceptance_mvp_passes_from_repo_configs() {
        let root = workspace_root();
        let profile =
            AcceptanceProfile::load_path(root.join("configs/acceptance_profile.yaml")).unwrap();
        let report = AcceptanceRunner::new(profile)
            .with_base_dir(root)
            .run()
            .expect("acceptance should pass");
        assert!(report.passed);
        assert!(report.gates.iter().all(|g| g.passed));
        assert_eq!(report.health.as_deref(), Some("NORMAL"));
    }
}
