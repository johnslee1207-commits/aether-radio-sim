//! Software Ethernet model parameters and delay/loss helpers.
//! Parameters load from YAML (configs/ethernet_model.yaml) — not hardcoded.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EthernetModelConfig {
    pub version: String,
    pub id: String,
    pub bandwidth_gbps: f64,
    pub mtu: u32,
    pub latency_ns: u64,
    pub jitter_ns: u64,
    pub loss_rate: f64,
}

impl EthernetModelConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, EthernetError> {
        serde_yaml::from_str(s).map_err(|e| EthernetError::Config(e.to_string()))
    }

    /// Serialization delay for `bytes` at configured bandwidth (ns).
    pub fn serialize_delay_ns(&self, bytes: usize) -> u64 {
        if self.bandwidth_gbps <= 0.0 {
            return u64::MAX;
        }
        let bits = (bytes as f64) * 8.0;
        let seconds = bits / (self.bandwidth_gbps * 1e9);
        (seconds * 1e9) as u64
    }

    /// Base one-way wire delay (latency only; jitter applied by caller/RNG).
    pub fn wire_delay_ns(&self) -> u64 {
        self.latency_ns
    }
}

#[derive(Debug, Error)]
pub enum EthernetError {
    #[error("config error: {0}")]
    Config(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
version: "1.0.0"
id: ethernet-model-100g
classification: project_specific
bandwidth_gbps: 100
mtu: 9000
latency_ns: 1000
jitter_ns: 500
loss_rate: 0.0
"#;

    #[test]
    fn load_yaml_and_serialize_delay() {
        let cfg = EthernetModelConfig::from_yaml_str(SAMPLE).unwrap();
        assert_eq!(cfg.mtu, 9000);
        // 1250 bytes @ 100Gbps = 100 ns
        assert_eq!(cfg.serialize_delay_ns(1250), 100);
    }
}
