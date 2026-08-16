//! Stub DPDK/DOCA adapter boundary (G7 spike).
//!
//! This crate intentionally does **not** link `libdpdk` or DOCA.
//! Hardware open always returns [`AdapterError::Unavailable`].
//! Datapath code must continue to use `cx5_emulator::PacketIO` mocks.

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdapterError {
    #[error("DPDK/DOCA hardware adapter unavailable: {0}")]
    Unavailable(String),
    #[error("config: {0}")]
    Config(String),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AdapterContract {
    pub schema_version: String,
    pub id: String,
    pub status: String,
    pub default_backend: String,
    pub hardware_enabled: bool,
    pub adapter_crate: String,
}

impl AdapterContract {
    pub fn from_yaml_str(s: &str) -> Result<Self, AdapterError> {
        serde_yaml::from_str(s).map_err(|e| AdapterError::Config(e.to_string()))
    }
}

/// Probe result for a future hardware backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareProbe {
    Unavailable { reason: String },
}

/// Always-unavailable hardware probe for this spike build.
pub fn probe_hardware() -> HardwareProbe {
    HardwareProbe::Unavailable {
        reason: "dpdk-adapter spike build has no libdpdk/DOCA linkage; use backend=mock"
            .into(),
    }
}

/// Attempt to open a hardware datapath — always fails closed in this spike.
pub fn open_hardware(_pci_address: &str) -> Result<(), AdapterError> {
    match probe_hardware() {
        HardwareProbe::Unavailable { reason } => Err(AdapterError::Unavailable(reason)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_probe_is_unavailable() {
        assert!(matches!(
            probe_hardware(),
            HardwareProbe::Unavailable { .. }
        ));
        assert!(open_hardware("0000:00:00.0").is_err());
    }

    #[test]
    fn contract_yaml_loads_and_keeps_hardware_off() {
        let yaml = include_str!("../../../configs/backends/dpdk_adapter_contract.yaml");
        let c = AdapterContract::from_yaml_str(yaml).expect("contract");
        assert_eq!(c.id, "dpdk-adapter-contract");
        assert!(!c.hardware_enabled);
        assert_eq!(c.default_backend, "mock");
    }
}
