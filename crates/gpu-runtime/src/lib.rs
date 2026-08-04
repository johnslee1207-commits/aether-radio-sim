//! GPU backend abstraction and ring-buffer slot states.
//! Phase 1 simulates kernel latency without binding CUDA.

mod ring;

pub use ring::{GpuRingBuffer, GpuRingConfig, RingSlot};

use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

pub type GpuBufferId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BufferState {
    Free,
    Receiving,
    Ready,
    Processing,
    Done,
}

pub trait GpuBackend {
    fn allocate_buffer(&mut self, size: usize) -> Result<GpuBufferId, GpuError>;
    fn launch_kernel(&mut self, buffer: GpuBufferId, delay: Duration) -> Result<(), GpuError>;
    fn sync(&mut self) -> Result<(), GpuError>;
}

/// CPU-side GPU emulator: records kernel launches and applies configured delay.
#[derive(Debug, Default)]
pub struct SimGpu {
    next_id: GpuBufferId,
    buffers: Vec<usize>,
    pub last_delay: Option<Duration>,
    /// When true, actually sleep; default false for unit tests.
    pub real_sleep: bool,
}

impl SimGpu {
    pub fn new() -> Self {
        Self::default()
    }
}

impl GpuBackend for SimGpu {
    fn allocate_buffer(&mut self, size: usize) -> Result<GpuBufferId, GpuError> {
        let id = self.next_id;
        self.next_id += 1;
        self.buffers.push(size);
        Ok(id)
    }

    fn launch_kernel(&mut self, buffer: GpuBufferId, delay: Duration) -> Result<(), GpuError> {
        if self.buffers.get(buffer as usize).is_none() {
            return Err(GpuError::NotFound(buffer));
        }
        self.last_delay = Some(delay);
        if self.real_sleep {
            std::thread::sleep(delay);
        }
        Ok(())
    }

    fn sync(&mut self) -> Result<(), GpuError> {
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GpuError {
    #[error("gpu buffer not found: {0}")]
    NotFound(GpuBufferId),
    #[error("ring full")]
    RingFull,
    #[error("invalid buffer state transition: {from:?} -> {to:?}")]
    InvalidTransition { from: BufferState, to: BufferState },
    #[error("no slot in state {0:?}")]
    NoSlot(BufferState),
    #[error("config error: {0}")]
    Config(String),
    #[error("payload exceeds slot capacity")]
    PayloadTooLarge,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_kernel_records_delay() {
        let mut gpu = SimGpu::new();
        let id = gpu.allocate_buffer(1024).unwrap();
        gpu.launch_kernel(id, Duration::from_micros(50)).unwrap();
        assert_eq!(gpu.last_delay, Some(Duration::from_micros(50)));
        gpu.sync().unwrap();
    }
}
