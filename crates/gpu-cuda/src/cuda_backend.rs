//! Real CUDA GpuBackend using driver API + checked-in PTX (no nvcc at cargo build).

use crate::config::CudaBackendConfig;
use cudarc::driver::{CudaDevice, CudaSlice, DeviceSlice, DriverError, LaunchAsync, LaunchConfig};
use cudarc::nvrtc::Ptx;
use gpu_runtime::{GpuBackend, GpuBufferId, GpuError};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

const PHY_SCALE_PTX: &str = include_str!("../kernels/phy_scale.ptx");

#[derive(Debug, Error)]
pub enum CudaGpuError {
    #[error("cuda driver: {0:?}")]
    Driver(DriverError),
    #[error("config: {0}")]
    Config(String),
    #[error("device name mismatch: got '{got}', expected to contain '{expect}'")]
    NameMismatch { got: String, expect: String },
    #[error("no CUDA devices visible (check WSL2 GPU / nvidia-container-toolkit)")]
    NoDevice,
}

impl From<DriverError> for CudaGpuError {
    fn from(value: DriverError) -> Self {
        Self::Driver(value)
    }
}

pub fn cuda_device_count() -> Result<usize, CudaGpuError> {
    // Avoid cudarc helpers that unwrap on empty device lists in some WSL setups.
    match CudaDevice::count() {
        Ok(c) if c > 0 => Ok(c as usize),
        Ok(_) => Err(CudaGpuError::NoDevice),
        Err(e) => Err(CudaGpuError::Driver(e)),
    }
}

pub struct CudaGpu {
    device: Arc<CudaDevice>,
    cfg: CudaBackendConfig,
    /// Device buffers interpreted as f32 words (IQ samples).
    device_bufs: Vec<CudaSlice<f32>>,
    lengths: Vec<usize>,
    pub last_kernel_ns: Option<u64>,
    pub device_name: String,
}

impl CudaGpu {
    pub fn new(cfg: CudaBackendConfig) -> Result<Self, CudaGpuError> {
        let count = CudaDevice::count()?;
        if count == 0 {
            return Err(CudaGpuError::NoDevice);
        }
        if (cfg.device_id as i32) >= count {
            return Err(CudaGpuError::Config(format!(
                "device_id {} out of range (count={count})",
                cfg.device_id
            )));
        }
        let device = CudaDevice::new(cfg.device_id as usize)?;
        let device_name = device.name()?;
        if !cfg.expect_name_contains.is_empty()
            && !device_name
                .to_ascii_lowercase()
                .contains(&cfg.expect_name_contains.to_ascii_lowercase())
        {
            return Err(CudaGpuError::NameMismatch {
                got: device_name,
                expect: cfg.expect_name_contains.clone(),
            });
        }

        let ptx = Ptx::from_src(PHY_SCALE_PTX);
        device.load_ptx(ptx, "phy", &["phy_scale"])?;

        Ok(Self {
            device,
            cfg,
            device_bufs: Vec::new(),
            lengths: Vec::new(),
            last_kernel_ns: None,
            device_name,
        })
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, CudaGpuError> {
        let cfg = CudaBackendConfig::from_yaml_str(yaml)
            .map_err(|e| CudaGpuError::Config(e.to_string()))?;
        Self::new(cfg)
    }

    pub fn info_line(&self) -> String {
        format!(
            "CUDA device {} '{}' (ordinal {})",
            self.cfg.device_id, self.device_name, self.cfg.device_id
        )
    }

    /// Copy host bytes (as f32 LE) to GPU, run phy_scale, copy back.
    pub fn process_bytes(&mut self, data: &[u8]) -> Result<Vec<u8>, CudaGpuError> {
        let n = data.len() / 4;
        let mut words = vec![0f32; n];
        for (i, chunk) in data.chunks_exact(4).enumerate() {
            words[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        let id = self
            .allocate_buffer(words.len() * 4)
            .map_err(|e| CudaGpuError::Config(e.to_string()))?;
        self.device
            .htod_sync_copy_into(&words, &mut self.device_bufs[id as usize])?;
        self.launch_kernel(id, Duration::ZERO)
            .map_err(|e| CudaGpuError::Config(e.to_string()))?;
        self.sync()
            .map_err(|e| CudaGpuError::Config(e.to_string()))?;
        let out_words = self.device.dtoh_sync_copy(&self.device_bufs[id as usize])?;
        let mut out = Vec::with_capacity(out_words.len() * 4);
        for w in out_words {
            out.extend_from_slice(&w.to_le_bytes());
        }
        if !data.len().is_multiple_of(4) {
            out.extend_from_slice(&data[data.len() - (data.len() % 4)..]);
        }
        Ok(out)
    }
}

impl GpuBackend for CudaGpu {
    fn allocate_buffer(&mut self, size: usize) -> Result<GpuBufferId, GpuError> {
        let n = size.div_ceil(4).max(1);
        let buf = self
            .device
            .alloc_zeros::<f32>(n)
            .map_err(|e| GpuError::Config(format!("cuda alloc: {e:?}")))?;
        let id = self.device_bufs.len() as GpuBufferId;
        self.lengths.push(buf.len());
        self.device_bufs.push(buf);
        Ok(id)
    }

    fn launch_kernel(&mut self, buffer: GpuBufferId, _delay: Duration) -> Result<(), GpuError> {
        let n = *self
            .lengths
            .get(buffer as usize)
            .ok_or(GpuError::NotFound(buffer))?;
        if n == 0 {
            return Ok(());
        }

        let func = self
            .device
            .get_func("phy", "phy_scale")
            .ok_or_else(|| GpuError::Config("phy_scale kernel missing".into()))?;

        let threads = self.cfg.threads_per_block.max(1);
        let blocks = (n as u32).div_ceil(threads);
        let cfg = LaunchConfig {
            grid_dim: (blocks, 1, 1),
            block_dim: (threads, 1, 1),
            shared_mem_bytes: 0,
        };

        let start = std::time::Instant::now();
        let slice = &self.device_bufs[buffer as usize];
        unsafe {
            func.launch(cfg, (slice, n as i32, self.cfg.scale))
                .map_err(|e| GpuError::Config(format!("cuda launch: {e:?}")))?;
        }
        self.device
            .synchronize()
            .map_err(|e| GpuError::Config(format!("cuda sync: {e:?}")))?;
        self.last_kernel_ns = Some(start.elapsed().as_nanos() as u64);
        Ok(())
    }

    fn sync(&mut self) -> Result<(), GpuError> {
        self.device
            .synchronize()
            .map_err(|e| GpuError::Config(format!("cuda sync: {e:?}")))?;
        Ok(())
    }
}

/// Probe helper used by CLI `gpu-info`.
pub fn probe_devices() -> Result<Vec<String>, CudaGpuError> {
    let n = CudaDevice::count()?;
    if n == 0 {
        return Err(CudaGpuError::NoDevice);
    }
    let mut out = Vec::new();
    for i in 0..n as usize {
        let d = CudaDevice::new(i)?;
        let name = d.name()?;
        out.push(format!("[{i}] {name}"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuda_smoke_on_device0() {
        let mut cfg = CudaBackendConfig::default_rtx4050();
        cfg.expect_name_contains.clear();
        let mut gpu = match CudaGpu::new(cfg) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("skip cuda smoke (no usable device): {e}");
                return;
            }
        };
        let id = gpu.allocate_buffer(64).unwrap();
        gpu.launch_kernel(id, Duration::ZERO).unwrap();
        gpu.sync().unwrap();
        assert!(gpu.last_kernel_ns.is_some());
        println!(
            "device={} kernel_ns={:?}",
            gpu.device_name, gpu.last_kernel_ns
        );
    }
}
