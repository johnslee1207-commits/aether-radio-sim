//! Fault injection policy loaded from YAML configs.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaultInjectionConfig {
    pub version: String,
    pub id: String,
    pub enabled: bool,
    pub loss_rate: f64,
    pub extra_latency_us: f64,
    pub burst_length: u32,
    pub kernel_delay_us: f64,
}

impl FaultInjectionConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, FaultError> {
        serde_yaml::from_str(s).map_err(|e| FaultError::Config(e.to_string()))
    }

    /// Deterministic drop decision for tests: drop when (seq % period) == 0 and enabled.
    pub fn should_drop_deterministic(&self, sequence: u64) -> bool {
        if !self.enabled || self.loss_rate <= 0.0 {
            return false;
        }
        let period = (1.0 / self.loss_rate).round().max(1.0) as u64;
        sequence.is_multiple_of(period)
    }

    /// Burst loss: drop `burst_length` packets starting at `burst_start` inclusive.
    pub fn should_drop_burst(&self, sequence: u64, burst_start: u64) -> bool {
        if !self.enabled || self.burst_length == 0 {
            return false;
        }
        sequence >= burst_start && sequence < burst_start + u64::from(self.burst_length)
    }

    pub fn extra_latency_ns(&self) -> u64 {
        if !self.enabled {
            return 0;
        }
        (self.extra_latency_us * 1_000.0) as u64
    }

    pub fn kernel_delay_ns(&self) -> u64 {
        if !self.enabled {
            return 0;
        }
        (self.kernel_delay_us * 1_000.0) as u64
    }
}

#[derive(Debug, Error)]
pub enum FaultError {
    #[error("config error: {0}")]
    Config(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
version: "1.0.0"
id: fault-injection-defaults
classification: project_specific
enabled: true
loss_rate: 0.001
extra_latency_us: 5.0
burst_length: 10
kernel_delay_us: 100.0
"#;

    #[test]
    fn load_and_extra_latency() {
        let cfg = FaultInjectionConfig::from_yaml_str(SAMPLE).unwrap();
        assert_eq!(cfg.extra_latency_ns(), 5_000);
        assert!(cfg.should_drop_deterministic(0));
        assert!(!cfg.should_drop_deterministic(1));
        assert!(cfg.should_drop_burst(5, 5));
        assert!(cfg.should_drop_burst(14, 5));
        assert!(!cfg.should_drop_burst(15, 5));
    }
}
