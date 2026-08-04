//! Sequence and timestamp checkers for the transport data plane.

use crate::TransportError;
use aether_types::{Packet, StreamId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportDeadlineConfig {
    pub version: String,
    pub id: String,
    pub max_latency_ns: u64,
    pub allow_reorder: bool,
    pub start_sequence: u64,
}

impl TransportDeadlineConfig {
    pub fn from_yaml_str(s: &str) -> Result<Self, TransportError> {
        serde_yaml::from_str(s).map_err(|e| TransportError::Config(e.to_string()))
    }
}

/// Tracks expected sequence per stream.
#[derive(Debug, Clone)]
pub struct SequenceChecker {
    next: HashMap<StreamId, u64>,
    start: u64,
    allow_reorder: bool,
}

impl SequenceChecker {
    pub fn new(start_sequence: u64, allow_reorder: bool) -> Self {
        Self {
            next: HashMap::new(),
            start: start_sequence,
            allow_reorder,
        }
    }

    pub fn from_config(cfg: &TransportDeadlineConfig) -> Self {
        Self::new(cfg.start_sequence, cfg.allow_reorder)
    }

    pub fn register_stream(&mut self, stream_id: StreamId) {
        self.next.entry(stream_id).or_insert(self.start);
    }

    pub fn unregister_stream(&mut self, stream_id: StreamId) {
        self.next.remove(&stream_id);
    }

    pub fn check(&mut self, packet: &Packet) -> Result<(), TransportError> {
        let expected = *self.next.get(&packet.stream_id).unwrap_or(&self.start);
        let got = packet.sequence.0;
        if got == expected {
            self.next.insert(packet.stream_id, expected.wrapping_add(1));
            return Ok(());
        }
        if self.allow_reorder && got > expected {
            // Gap: advance to got+1 and report gap for metrics callers.
            self.next.insert(packet.stream_id, got.wrapping_add(1));
            return Err(TransportError::SequenceGap { expected, got });
        }
        Err(TransportError::SequenceGap { expected, got })
    }

    pub fn last_expected(&self, stream_id: StreamId) -> u64 {
        self.next
            .get(&stream_id)
            .copied()
            .unwrap_or(self.start)
            .saturating_sub(1)
    }

    /// After a detected loss, resync expected sequence to `next_seq`.
    pub fn resync(&mut self, stream_id: StreamId, next_seq: u64) {
        self.next.insert(stream_id, next_seq);
    }
}

/// Checks packet age against a receive-time reference and max latency.
#[derive(Debug, Clone)]
pub struct TimestampChecker {
    max_latency_ns: u64,
}

impl TimestampChecker {
    pub fn new(max_latency_ns: u64) -> Self {
        Self { max_latency_ns }
    }

    pub fn from_config(cfg: &TransportDeadlineConfig) -> Self {
        Self::new(cfg.max_latency_ns)
    }

    /// `now_ns` is the simulation clock at receive time.
    pub fn check(&self, packet: &Packet, now_ns: u64) -> Result<(), TransportError> {
        let sent = packet.timestamp.0;
        let latency = now_ns.saturating_sub(sent);
        if latency > self.max_latency_ns {
            return Err(TransportError::LatePacket {
                latency_ns: latency,
                max_ns: self.max_latency_ns,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_types::{Packet, Sequence, StreamId, Timestamp};

    #[test]
    fn sequence_accepts_in_order() {
        let mut chk = SequenceChecker::new(0, false);
        let id = StreamId(1);
        chk.register_stream(id);
        let p0 = Packet::new(id, Sequence(0), Timestamp(0), vec![]);
        let p1 = Packet::new(id, Sequence(1), Timestamp(1), vec![]);
        chk.check(&p0).unwrap();
        chk.check(&p1).unwrap();
    }

    #[test]
    fn sequence_detects_gap() {
        let mut chk = SequenceChecker::new(0, false);
        let id = StreamId(1);
        chk.register_stream(id);
        let p0 = Packet::new(id, Sequence(0), Timestamp(0), vec![]);
        let p2 = Packet::new(id, Sequence(2), Timestamp(2), vec![]);
        chk.check(&p0).unwrap();
        assert!(matches!(
            chk.check(&p2),
            Err(TransportError::SequenceGap {
                expected: 1,
                got: 2
            })
        ));
    }

    #[test]
    fn timestamp_detects_late() {
        let chk = TimestampChecker::new(1_000);
        let pkt = Packet::new(StreamId(1), Sequence(0), Timestamp(0), vec![]);
        assert!(matches!(
            chk.check(&pkt, 5_000),
            Err(TransportError::LatePacket { .. })
        ));
        chk.check(&pkt, 500).unwrap();
    }
}
