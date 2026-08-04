//! FPGA emulator: IQ generation, slot scheduling, packetization.
//! Timing parameters load from `configs/radio_timing.yaml` (data layer).

use aether_types::{IQSample, Packet, RadioTimestamp, Sequence, StreamId, Timestamp};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// IQ sample source for a radio timestamp.
pub trait IQSource {
    fn generate(&mut self, timestamp: RadioTimestamp) -> Vec<IQSample>;
}

/// Packetizer turns IQ batches into transport packets.
pub trait Packetizer {
    fn packetize(
        &mut self,
        stream_id: StreamId,
        timestamp: RadioTimestamp,
        samples: &[IQSample],
    ) -> Packet;
}

/// Advances radio frame/slot/symbol time.
pub trait SlotScheduler {
    fn next_symbol(&mut self) -> RadioTimestamp;
    fn current(&self) -> RadioTimestamp;
    fn reset(&mut self);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadioTimingConfig {
    pub version: String,
    pub id: String,
    pub slots_per_sfn: u16,
    pub symbols_per_slot: u8,
    pub symbol_duration_ns: u64,
    pub samples_per_symbol: usize,
    pub max_antennas: u16,
    pub max_carriers: u16,
}

impl RadioTimingConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, FpgaError> {
        serde_yaml::from_str(s).map_err(|e| FpgaError::Config(e.to_string()))
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FpgaError {
    #[error("config error: {0}")]
    Config(String),
}

/// Monotonic radio timestamp from timing config.
#[derive(Debug, Clone)]
pub struct TimestampGenerator {
    timing: RadioTimingConfig,
    sfn: u32,
    slot: u16,
    symbol: u8,
    ns: u64,
}

impl TimestampGenerator {
    pub fn new(timing: RadioTimingConfig) -> Self {
        Self {
            timing,
            sfn: 0,
            slot: 0,
            symbol: 0,
            ns: 0,
        }
    }

    pub fn current(&self) -> RadioTimestamp {
        RadioTimestamp::new(self.sfn, self.slot, self.symbol, self.ns)
    }

    pub fn tick_symbol(&mut self) -> RadioTimestamp {
        let ts = self.current();
        self.ns = self.ns.saturating_add(self.timing.symbol_duration_ns);
        self.symbol = self.symbol.saturating_add(1);
        if self.symbol >= self.timing.symbols_per_slot {
            self.symbol = 0;
            self.slot = self.slot.saturating_add(1);
            if self.slot >= self.timing.slots_per_sfn {
                self.slot = 0;
                self.sfn = self.sfn.wrapping_add(1);
            }
        }
        ts
    }

    pub fn reset(&mut self) {
        self.sfn = 0;
        self.slot = 0;
        self.symbol = 0;
        self.ns = 0;
    }
}

/// Per-stream sequence counter.
#[derive(Debug, Default, Clone)]
pub struct SequenceGenerator {
    next: u64,
}

impl SequenceGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_sequence(&mut self) -> Sequence {
        let seq = Sequence(self.next);
        self.next = self.next.wrapping_add(1);
        seq
    }

    pub fn peek(&self) -> u64 {
        self.next
    }
}

/// Slot/symbol scheduler backed by [`TimestampGenerator`].
#[derive(Debug, Clone)]
pub struct SimpleSlotScheduler {
    timestamps: TimestampGenerator,
}

impl SimpleSlotScheduler {
    pub fn new(timing: RadioTimingConfig) -> Self {
        Self {
            timestamps: TimestampGenerator::new(timing),
        }
    }

    pub fn timing(&self) -> &RadioTimingConfig {
        &self.timestamps.timing
    }
}

impl SlotScheduler for SimpleSlotScheduler {
    fn next_symbol(&mut self) -> RadioTimestamp {
        self.timestamps.tick_symbol()
    }

    fn current(&self) -> RadioTimestamp {
        self.timestamps.current()
    }

    fn reset(&mut self) {
        self.timestamps.reset();
    }
}

/// Deterministic sine IQ mock for unit tests.
#[derive(Debug, Default)]
pub struct MockIQSource {
    pub samples_per_symbol: usize,
}

impl MockIQSource {
    pub fn new(samples_per_symbol: usize) -> Self {
        Self { samples_per_symbol }
    }
}

impl IQSource for MockIQSource {
    fn generate(&mut self, timestamp: RadioTimestamp) -> Vec<IQSample> {
        (0..self.samples_per_symbol)
            .map(|i| {
                let phase = (timestamp.ns as f32) * 1e-9 + i as f32 * 0.01;
                IQSample::new(phase.cos(), phase.sin())
            })
            .collect()
    }
}

/// Packs IQ as interleaved f32 LE bytes; uses an external sequence when provided.
#[derive(Debug, Default)]
pub struct SimplePacketizer {
    sequence: SequenceGenerator,
}

impl SimplePacketizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_sequence(&self) -> u64 {
        self.sequence.peek()
    }
}

impl Packetizer for SimplePacketizer {
    fn packetize(
        &mut self,
        stream_id: StreamId,
        timestamp: RadioTimestamp,
        samples: &[IQSample],
    ) -> Packet {
        let mut payload = Vec::with_capacity(samples.len() * 8);
        for s in samples {
            payload.extend_from_slice(&s.i.to_le_bytes());
            payload.extend_from_slice(&s.q.to_le_bytes());
        }
        let seq = self.sequence.next_sequence();
        Packet::new(stream_id, seq, Timestamp(timestamp.ns), payload)
    }
}

/// FPGA emulator facade: schedule → IQ → packetize for one stream.
#[derive(Debug)]
pub struct FpgaEmulator {
    pub scheduler: SimpleSlotScheduler,
    pub iq: MockIQSource,
    pub packetizer: SimplePacketizer,
    pub stream_id: StreamId,
}

impl FpgaEmulator {
    pub fn new(timing: RadioTimingConfig, stream_id: StreamId) -> Self {
        let samples = timing.samples_per_symbol;
        Self {
            scheduler: SimpleSlotScheduler::new(timing),
            iq: MockIQSource::new(samples),
            packetizer: SimplePacketizer::new(),
            stream_id,
        }
    }

    /// Emit one symbol worth of packets (single packet for MVP).
    pub fn emit_symbol(&mut self) -> Packet {
        let ts = self.scheduler.next_symbol();
        let samples = self.iq.generate(ts);
        self.packetizer.packetize(self.stream_id, ts, &samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMING_YAML: &str = r#"
version: "1.0.0"
id: radio-timing-nr-like
slots_per_sfn: 20
symbols_per_slot: 14
symbol_duration_ns: 714286
samples_per_symbol: 64
max_antennas: 8
max_carriers: 4
"#;

    #[test]
    fn mock_iq_and_packetize() {
        let mut iq = MockIQSource::new(16);
        let mut pktz = SimplePacketizer::default();
        let ts = RadioTimestamp::new(0, 0, 0, 1_000);
        let samples = iq.generate(ts);
        assert_eq!(samples.len(), 16);
        let pkt = pktz.packetize(StreamId(1), ts, &samples);
        assert_eq!(pkt.payload.len(), 16 * 8);
        assert_eq!(pkt.sequence.0, 0);
    }

    #[test]
    fn slot_scheduler_rolls_symbols_and_slots() {
        let timing = RadioTimingConfig::from_yaml_str(TIMING_YAML).unwrap();
        let mut sched = SimpleSlotScheduler::new(timing);
        let first = sched.next_symbol();
        assert_eq!(first.symbol, 0);
        assert_eq!(first.slot, 0);
        for _ in 0..13 {
            sched.next_symbol();
        }
        let after_slot = sched.next_symbol();
        assert_eq!(after_slot.slot, 1);
        assert_eq!(after_slot.symbol, 0);
        assert_eq!(after_slot.ns, 14 * 714_286);
    }

    #[test]
    fn fpga_emit_symbol_advances_sequence() {
        let timing = RadioTimingConfig::from_yaml_str(TIMING_YAML).unwrap();
        let mut fpga = FpgaEmulator::new(timing, StreamId(7));
        let a = fpga.emit_symbol();
        let b = fpga.emit_symbol();
        assert_eq!(a.stream_id, StreamId(7));
        assert_eq!(a.sequence.0, 0);
        assert_eq!(b.sequence.0, 1);
        assert!(b.timestamp.0 > a.timestamp.0);
    }
}
