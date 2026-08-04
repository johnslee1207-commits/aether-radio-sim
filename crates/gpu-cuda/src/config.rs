//! CUDA backend selection and device policy (configs/backends/gpu_cuda.yaml).

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CudaBackendConfig {
    pub version: String,
    pub id: String,
    /// CUDA device ordinal (0 = first visible GPU, e.g. RTX 4050).
    pub device_id: u32,
    /// Multiplier applied by the phy_scale kernel.
    pub scale: f32,
    /// Threads per block for launches.
    pub threads_per_block: u32,
    /// Optional expected GPU name substring for sanity checks (empty = skip).
    #[serde(default)]
    pub expect_name_contains: String,
}

impl CudaBackendConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, CudaConfigError> {
        serde_yaml::from_str(s).map_err(|e| CudaConfigError::Parse(e.to_string()))
    }

    pub fn default_rtx4050() -> Self {
        Self {
            version: "1.0.0".into(),
            id: "gpu-cuda-rtx4050".into(),
            device_id: 0,
            scale: 1.0,
            threads_per_block: 256,
            expect_name_contains: "4050".into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum CudaConfigError {
    #[error("parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_yaml() {
        let cfg = CudaBackendConfig::from_yaml_str(
            r#"
version: "1.0.0"
id: gpu-cuda-rtx4050
device_id: 0
scale: 1.25
threads_per_block: 256
expect_name_contains: "4050"
"#,
        )
        .unwrap();
        assert_eq!(cfg.device_id, 0);
        assert!((cfg.scale - 1.25).abs() < f32::EPSILON);
    }
}
