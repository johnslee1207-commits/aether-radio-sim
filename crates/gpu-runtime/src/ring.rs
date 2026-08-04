//! GPU memory ring: Free → Receiving → Ready → Processing → Done → Free.

use crate::{BufferState, GpuError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuRingConfig {
    pub version: String,
    pub id: String,
    pub slot_count: usize,
    pub slot_bytes: usize,
    pub kernel_delay_us: f64,
}

impl GpuRingConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, GpuError> {
        serde_yaml::from_str(s).map_err(|e| GpuError::Config(e.to_string()))
    }

    pub fn kernel_delay_ns(&self) -> u64 {
        (self.kernel_delay_us * 1_000.0) as u64
    }
}

#[derive(Debug, Clone)]
pub struct RingSlot {
    pub state: BufferState,
    pub payload: Vec<u8>,
    pub arrive_ns: u64,
    pub done_ns: u64,
}

impl RingSlot {
    fn empty(capacity: usize) -> Self {
        Self {
            state: BufferState::Free,
            payload: Vec::with_capacity(capacity),
            arrive_ns: 0,
            done_ns: 0,
        }
    }
}

/// Fixed-size GPU ring used by the PHY pipeline emulator.
#[derive(Debug)]
pub struct GpuRingBuffer {
    slots: Vec<RingSlot>,
    slot_bytes: usize,
    kernel_delay_ns: u64,
}

impl GpuRingBuffer {
    pub fn new(cfg: GpuRingConfig) -> Self {
        let slots = (0..cfg.slot_count)
            .map(|_| RingSlot::empty(cfg.slot_bytes))
            .collect();
        Self {
            slots,
            slot_bytes: cfg.slot_bytes,
            kernel_delay_ns: cfg.kernel_delay_ns(),
        }
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, GpuError> {
        Ok(Self::new(GpuRingConfig::from_yaml_str(yaml)?))
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn kernel_delay_ns(&self) -> u64 {
        self.kernel_delay_ns
    }

    fn find_state(&self, state: BufferState) -> Option<usize> {
        self.slots.iter().position(|s| s.state == state)
    }

    fn transition(slot: &mut RingSlot, to: BufferState) -> Result<(), GpuError> {
        let ok = matches!(
            (slot.state, to),
            (BufferState::Free, BufferState::Receiving)
                | (BufferState::Receiving, BufferState::Ready)
                | (BufferState::Ready, BufferState::Processing)
                | (BufferState::Processing, BufferState::Done)
                | (BufferState::Done, BufferState::Free)
        );
        if !ok {
            return Err(GpuError::InvalidTransition {
                from: slot.state,
                to,
            });
        }
        slot.state = to;
        Ok(())
    }

    /// Begin DMA/write into a free slot.
    pub fn begin_receive(&mut self, arrive_ns: u64) -> Result<usize, GpuError> {
        let idx = self
            .find_state(BufferState::Free)
            .ok_or(GpuError::RingFull)?;
        let slot = &mut self.slots[idx];
        Self::transition(slot, BufferState::Receiving)?;
        slot.arrive_ns = arrive_ns;
        slot.done_ns = 0;
        slot.payload.clear();
        Ok(idx)
    }

    /// Finish writing payload; slot becomes Ready.
    pub fn complete_receive(&mut self, idx: usize, data: &[u8]) -> Result<(), GpuError> {
        let slot = self
            .slots
            .get_mut(idx)
            .ok_or(GpuError::NoSlot(BufferState::Receiving))?;
        if slot.state != BufferState::Receiving {
            return Err(GpuError::InvalidTransition {
                from: slot.state,
                to: BufferState::Ready,
            });
        }
        if data.len() > self.slot_bytes {
            return Err(GpuError::PayloadTooLarge);
        }
        slot.payload.clear();
        slot.payload.extend_from_slice(data);
        Self::transition(slot, BufferState::Ready)
    }

    /// Start kernel on the oldest Ready slot; advances simulation clock by kernel delay.
    pub fn begin_process(&mut self, now_ns: u64) -> Result<(usize, u64), GpuError> {
        let idx = self
            .find_state(BufferState::Ready)
            .ok_or(GpuError::NoSlot(BufferState::Ready))?;
        let slot = &mut self.slots[idx];
        Self::transition(slot, BufferState::Processing)?;
        let done_at = now_ns.saturating_add(self.kernel_delay_ns);
        Ok((idx, done_at))
    }

    /// Mark processing complete at `done_ns`.
    pub fn complete_process(&mut self, idx: usize, done_ns: u64) -> Result<u64, GpuError> {
        let slot = self
            .slots
            .get_mut(idx)
            .ok_or(GpuError::NoSlot(BufferState::Processing))?;
        if slot.state != BufferState::Processing {
            return Err(GpuError::InvalidTransition {
                from: slot.state,
                to: BufferState::Done,
            });
        }
        slot.done_ns = done_ns;
        let latency = done_ns.saturating_sub(slot.arrive_ns);
        Self::transition(slot, BufferState::Done)?;
        Ok(latency)
    }

    /// Release a Done slot back to Free.
    pub fn release(&mut self, idx: usize) -> Result<(), GpuError> {
        let slot = self
            .slots
            .get_mut(idx)
            .ok_or(GpuError::NoSlot(BufferState::Done))?;
        if slot.state != BufferState::Done {
            return Err(GpuError::InvalidTransition {
                from: slot.state,
                to: BufferState::Free,
            });
        }
        slot.payload.clear();
        Self::transition(slot, BufferState::Free)
    }

    /// Convenience: receive → ready → process → done → free; returns pipeline latency.
    pub fn process_packet(&mut self, data: &[u8], arrive_ns: u64) -> Result<u64, GpuError> {
        let idx = self.begin_receive(arrive_ns)?;
        self.complete_receive(idx, data)?;
        let (idx, done_at) = self.begin_process(arrive_ns)?;
        let latency = self.complete_process(idx, done_at)?;
        self.release(idx)?;
        Ok(latency)
    }

    pub fn count_state(&self, state: BufferState) -> usize {
        self.slots.iter().filter(|s| s.state == state).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BufferState;

    const YAML: &str = r#"
version: "1.0.0"
id: gpu-ring-default
slot_count: 4
slot_bytes: 256
kernel_delay_us: 2.0
"#;

    #[test]
    fn ring_lifecycle_and_latency() {
        let mut ring = GpuRingBuffer::from_yaml(YAML).unwrap();
        assert_eq!(ring.kernel_delay_ns(), 2_000);
        let latency = ring.process_packet(&[1, 2, 3], 100).unwrap();
        assert_eq!(latency, 2_000);
        assert_eq!(ring.count_state(BufferState::Free), 4);
    }

    #[test]
    fn ring_full_errors() {
        let mut ring = GpuRingBuffer::from_yaml(YAML).unwrap();
        for _ in 0..4 {
            let idx = ring.begin_receive(0).unwrap();
            ring.complete_receive(idx, &[9]).unwrap();
        }
        assert!(matches!(ring.begin_receive(1), Err(GpuError::RingFull)));
    }
}
