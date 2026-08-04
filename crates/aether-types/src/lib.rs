//! Core identity and timing types for the Aether Radio data plane.

use serde::{Deserialize, Serialize};

/// Opaque stream identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamId(pub u32);

/// Monotonic packet sequence within a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Sequence(pub u64);

/// Absolute nanosecond timestamp (simulation or wall-clock domain).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(pub u64);

/// System Frame Number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Sfn(pub u32);

/// Slot index within an SFN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Slot(pub u16);

/// OFDM symbol index within a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Symbol(pub u8);

/// Antenna port / element identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AntennaId(pub u16);

/// Carrier identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CarrierId(pub u16);

/// Radio-domain timestamp combining frame timing and nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadioTimestamp {
    pub sfn: u32,
    pub slot: u16,
    pub symbol: u8,
    pub ns: u64,
}

impl RadioTimestamp {
    pub fn new(sfn: u32, slot: u16, symbol: u8, ns: u64) -> Self {
        Self {
            sfn,
            slot,
            symbol,
            ns,
        }
    }
}

/// Complex IQ sample (I/Q as f32).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IQSample {
    pub i: f32,
    pub q: f32,
}

impl IQSample {
    pub fn new(i: f32, q: f32) -> Self {
        Self { i, q }
    }
}

/// Opaque packet buffer owned by the data plane (zero-copy friendly later).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Packet {
    pub stream_id: StreamId,
    pub sequence: Sequence,
    pub timestamp: Timestamp,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn new(
        stream_id: StreamId,
        sequence: Sequence,
        timestamp: Timestamp,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            stream_id,
            sequence,
            timestamp,
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_timestamp_roundtrip_json() {
        let ts = RadioTimestamp::new(10, 2, 7, 1_000_000);
        let json = serde_json::to_string(&ts).unwrap();
        let decoded: RadioTimestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(ts, decoded);
    }

    #[test]
    fn packet_holds_payload() {
        let pkt = Packet::new(StreamId(1), Sequence(42), Timestamp(100), vec![1, 2, 3]);
        assert_eq!(pkt.payload.len(), 3);
        assert_eq!(pkt.sequence, Sequence(42));
    }
}
