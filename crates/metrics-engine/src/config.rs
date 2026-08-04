//! Observability plane config (`configs/ops/observability.yaml`).

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub version: String,
    pub id: String,
    #[serde(default)]
    pub metrics: MetricsExportConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub trace: TraceConfig,
    #[serde(default)]
    pub health: HealthRefConfig,
    #[serde(default)]
    pub recovery: RecoveryRefConfig,
    #[serde(default)]
    pub prometheus_scrape: PrometheusScrapeRefConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsExportConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub export_json: bool,
    #[serde(default)]
    pub export_prometheus_text: bool,
    #[serde(default = "default_snapshot_ms")]
    pub snapshot_interval_ms: u64,
}

impl Default for MetricsExportConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            export_json: true,
            export_prometheus_text: false,
            snapshot_interval_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_info")]
    pub default_level: String,
    #[serde(default = "default_true")]
    pub structured_jsonl: bool,
    #[serde(default = "default_events_path")]
    pub events_path: String,
    #[serde(default = "default_true")]
    pub elevate_on_degraded: bool,
    #[serde(default = "default_debug")]
    pub elevated_level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            default_level: "INFO".into(),
            structured_jsonl: true,
            events_path: "data/reports/ops_events.jsonl".into(),
            elevate_on_degraded: true,
            elevated_level: "DEBUG".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ring")]
    pub ring_capacity: usize,
    #[serde(default = "default_trace_path")]
    pub export_path: String,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ring_capacity: 4096,
            export_path: "data/reports/traces/packet_traces.jsonl".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthRefConfig {
    #[serde(default = "default_health_path")]
    pub policy_path: String,
}

impl Default for HealthRefConfig {
    fn default() -> Self {
        Self {
            policy_path: "configs/ops/health_policy.yaml".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveryRefConfig {
    #[serde(default = "default_recovery_path")]
    pub policy_path: String,
}

impl Default for RecoveryRefConfig {
    fn default() -> Self {
        Self {
            policy_path: "configs/ops/recovery_policy.yaml".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrometheusScrapeRefConfig {
    #[serde(default = "default_prom_scrape_path")]
    pub config_path: String,
}

impl Default for PrometheusScrapeRefConfig {
    fn default() -> Self {
        Self {
            config_path: "configs/ops/prometheus_scrape.yaml".into(),
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_snapshot_ms() -> u64 {
    1000
}
fn default_info() -> String {
    "INFO".into()
}
fn default_debug() -> String {
    "DEBUG".into()
}
fn default_events_path() -> String {
    "data/reports/ops_events.jsonl".into()
}
fn default_ring() -> usize {
    4096
}
fn default_trace_path() -> String {
    "data/reports/traces/packet_traces.jsonl".into()
}
fn default_health_path() -> String {
    "configs/ops/health_policy.yaml".into()
}
fn default_recovery_path() -> String {
    "configs/ops/recovery_policy.yaml".into()
}
fn default_prom_scrape_path() -> String {
    "configs/ops/prometheus_scrape.yaml".into()
}

#[derive(Debug, Error)]
pub enum OpsConfigError {
    #[error("config error: {0}")]
    Config(String),
}

impl ObservabilityConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, OpsConfigError> {
        serde_yaml::from_str(s).map_err(|e| OpsConfigError::Config(e.to_string()))
    }

    pub fn load_path(path: impl AsRef<std::path::Path>) -> Result<Self, OpsConfigError> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| OpsConfigError::Config(format!("read: {e}")))?;
        Self::from_yaml_str(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_repo_observability_yaml() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let cfg =
            ObservabilityConfig::load_path(root.join("configs/ops/observability.yaml")).unwrap();
        assert!(cfg.metrics.enabled);
        assert!(cfg.trace.enabled);
        assert!(cfg.logging.events_path.contains("ops_events"));
        assert!(cfg.recovery.policy_path.contains("recovery_policy"));
        assert!(cfg
            .prometheus_scrape
            .config_path
            .contains("prometheus_scrape"));
    }
}
