//! Fault injection policy loaded from YAML configs (Ops Framework §12).

use aether_types::{Packet, Sequence, Timestamp};
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
    /// Inclusive sequence where burst loss begins (when enabled).
    #[serde(default)]
    pub burst_start: u64,
    /// If > 0, delay one packet by this many subsequent packets (reorder).
    #[serde(default)]
    pub reorder_distance: u32,
    /// Added to sequence number to simulate jumps.
    #[serde(default)]
    pub sequence_jump: u64,
    /// Added to timestamp (can be negative via i64 then cast carefully).
    #[serde(default)]
    pub timestamp_skew_ns: i64,
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
        sequence % period == 0
    }

    /// Burst loss: drop `burst_length` packets starting at `burst_start` inclusive.
    pub fn should_drop_burst(&self, sequence: u64, burst_start: u64) -> bool {
        if !self.enabled || self.burst_length == 0 {
            return false;
        }
        sequence >= burst_start && sequence < burst_start + u64::from(self.burst_length)
    }

    pub fn should_drop(&self, sequence: u64) -> bool {
        self.should_drop_deterministic(sequence)
            || self.should_drop_burst(sequence, self.burst_start)
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

    /// Mutate sequence / timestamp faults onto a packet clone.
    pub fn mutate_packet(&self, mut packet: Packet) -> Packet {
        if !self.enabled {
            return packet;
        }
        if self.sequence_jump > 0 {
            packet.sequence = Sequence(packet.sequence.0.saturating_add(self.sequence_jump));
        }
        if self.timestamp_skew_ns != 0 {
            let t = packet.timestamp.0 as i128 + i128::from(self.timestamp_skew_ns);
            packet.timestamp = Timestamp(t.max(0) as u64);
        }
        packet
    }
}

/// Stateful injector: loss + optional single-packet reorder buffer.
#[derive(Debug)]
pub struct FaultInjector {
    cfg: FaultInjectionConfig,
    held: Option<Packet>,
    held_countdown: u32,
    pub dropped: u64,
    pub reordered: u64,
}

impl FaultInjector {
    pub fn new(cfg: FaultInjectionConfig) -> Self {
        Self {
            cfg,
            held: None,
            held_countdown: 0,
            dropped: 0,
            reordered: 0,
        }
    }

    pub fn config(&self) -> &FaultInjectionConfig {
        &self.cfg
    }

    /// Process one packet; returns 0..=2 packets (reorder may flush held).
    pub fn push(&mut self, packet: Packet) -> Vec<Packet> {
        let packet = self.cfg.mutate_packet(packet);
        if self.cfg.should_drop(packet.sequence.0) {
            self.dropped += 1;
            let mut out = Vec::new();
            if let Some(p) = self.tick_hold() {
                out.push(p);
            }
            return out;
        }

        let mut out = Vec::new();
        let did_release = self.held.is_some() && self.held_countdown <= 1;
        if let Some(p) = self.tick_hold() {
            out.push(p);
        }

        // Start a hold only when nothing was released this call (avoid re-hold).
        if self.cfg.enabled && self.cfg.reorder_distance > 0 && self.held.is_none() && !did_release
        {
            self.held = Some(packet);
            self.held_countdown = self.cfg.reorder_distance;
            self.reordered += 1;
            return out;
        }

        out.push(packet);
        out
    }

    fn tick_hold(&mut self) -> Option<Packet> {
        self.held.as_ref()?;
        if self.held_countdown == 0 {
            return self.held.take();
        }
        self.held_countdown = self.held_countdown.saturating_sub(1);
        if self.held_countdown == 0 {
            return self.held.take();
        }
        None
    }

    /// Flush any held packet at end of stream.
    pub fn flush(&mut self) -> Option<Packet> {
        self.held_countdown = 0;
        self.held.take()
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
    use aether_types::{Sequence, StreamId, Timestamp};

    const SAMPLE: &str = r#"
version: "1.0.0"
id: fault-injection-defaults
classification: project_specific
enabled: true
loss_rate: 0.001
extra_latency_us: 5.0
burst_length: 10
kernel_delay_us: 100.0
burst_start: 5
reorder_distance: 0
sequence_jump: 0
timestamp_skew_ns: 0
"#;

    fn pkt(seq: u64) -> Packet {
        Packet::new(StreamId(1), Sequence(seq), Timestamp(seq * 1000), vec![1])
    }

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

    #[test]
    fn sequence_jump_and_skew() {
        let mut cfg = FaultInjectionConfig::from_yaml_str(SAMPLE).unwrap();
        cfg.sequence_jump = 10;
        cfg.timestamp_skew_ns = -100;
        let p = cfg.mutate_packet(pkt(1));
        assert_eq!(p.sequence.0, 11);
        assert_eq!(p.timestamp.0, 900);
    }

    #[test]
    fn reorder_delays_one_packet() {
        let mut cfg = FaultInjectionConfig::from_yaml_str(SAMPLE).unwrap();
        cfg.loss_rate = 0.0;
        cfg.burst_length = 0;
        cfg.reorder_distance = 1;
        let mut inj = FaultInjector::new(cfg);
        assert!(inj.push(pkt(0)).is_empty());
        let out = inj.push(pkt(1));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].sequence.0, 0);
        assert_eq!(out[1].sequence.0, 1);
        assert_eq!(inj.reordered, 1);
    }
}
