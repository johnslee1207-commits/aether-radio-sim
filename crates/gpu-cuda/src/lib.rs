//! CUDA backend crate. Business code uses `gpu-runtime::GpuBackend` only.
//! Build with `--features cuda` on WSL2/Docker hosts that expose an NVIDIA GPU.

mod config;
#[cfg(feature = "cuda")]
mod cuda_backend;

pub use config::{CudaBackendConfig, CudaConfigError};

#[cfg(feature = "cuda")]
pub use cuda_backend::{cuda_device_count, probe_devices, CudaGpu, CudaGpuError};

/// Backend kind selected from YAML / CLI (data-driven).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackendKind {
    Simulation,
    Cuda,
}

impl GpuBackendKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "simulation" | "sim" => Some(Self::Simulation),
            "cuda" | "gpu" | "realtime" => Some(Self::Cuda),
            _ => None,
        }
    }
}
