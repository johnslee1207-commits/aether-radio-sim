//! Aether Radio transport header: encode, decode, validate.

use aether_types::{Sequence, StreamId, Timestamp};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Wire magic identifying an Aether Radio packet.
pub const AETHER_MAGIC: u32 = 0x4154_4852; // "ATHR"

/// Current protocol version.
pub const AETHER_VERSION: u16 = 1;

/// Aether Radio data-plane header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AetherHeader {
    pub magic: u32,
    pub version: u16,
    pub stream_id: u32,
    pub timestamp: u64,
    pub sequence: u64,
    pub payload_len: u32,
}

impl AetherHeader {
    pub fn new(
        stream_id: StreamId,
        timestamp: Timestamp,
        sequence: Sequence,
        payload_len: u32,
    ) -> Self {
        Self {
            magic: AETHER_MAGIC,
            version: AETHER_VERSION,
            stream_id: stream_id.0,
            timestamp: timestamp.0,
            sequence: sequence.0,
            payload_len,
        }
    }

    /// Encode header to little-endian bytes (32 bytes).
    pub fn encode(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        // bytes 6..8 reserved (alignment / flags)
        buf[8..12].copy_from_slice(&self.stream_id.to_le_bytes());
        buf[12..20].copy_from_slice(&self.timestamp.to_le_bytes());
        buf[20..28].copy_from_slice(&self.sequence.to_le_bytes());
        buf[28..32].copy_from_slice(&self.payload_len.to_le_bytes());
        buf
    }

    /// Decode header from bytes. Requires at least 32 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < 32 {
            return Err(ProtocolError::Truncated {
                got: bytes.len(),
                need: 32,
            });
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        let stream_id = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let timestamp = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
        let sequence = u64::from_le_bytes(bytes[20..28].try_into().unwrap());
        let payload_len = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        let header = Self {
            magic,
            version,
            stream_id,
            timestamp,
            sequence,
            payload_len,
        };
        header.validate()?;
        Ok(header)
    }

    /// Validate magic and version.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.magic != AETHER_MAGIC {
            return Err(ProtocolError::BadMagic(self.magic));
        }
        if self.version != AETHER_VERSION {
            return Err(ProtocolError::UnsupportedVersion(self.version));
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("truncated header: got {got} bytes, need {need}")]
    Truncated { got: usize, need: usize },
    #[error("bad magic: 0x{0:08x}")]
    BadMagic(u32),
    #[error("unsupported version: {0}")]
    UnsupportedVersion(u16),
    #[error("payload length mismatch: header={header}, body={body}")]
    PayloadLenMismatch { header: u32, body: usize },
}

/// Encode `Packet` as AetherHeader (32B) + payload.
pub fn encode_frame(packet: &aether_types::Packet) -> Vec<u8> {
    let header = AetherHeader::new(
        packet.stream_id,
        packet.timestamp,
        packet.sequence,
        packet.payload.len() as u32,
    );
    let mut out = Vec::with_capacity(32 + packet.payload.len());
    out.extend_from_slice(&header.encode());
    out.extend_from_slice(&packet.payload);
    out
}

/// Decode AetherHeader + payload into a `Packet`.
pub fn decode_frame(bytes: &[u8]) -> Result<aether_types::Packet, ProtocolError> {
    let header = AetherHeader::decode(bytes)?;
    let body = &bytes[32..];
    if body.len() != header.payload_len as usize {
        return Err(ProtocolError::PayloadLenMismatch {
            header: header.payload_len,
            body: body.len(),
        });
    }
    Ok(aether_types::Packet::new(
        aether_types::StreamId(header.stream_id),
        aether_types::Sequence(header.sequence),
        aether_types::Timestamp(header.timestamp),
        body.to_vec(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_types::{Packet, Sequence, StreamId, Timestamp};

    #[test]
    fn encode_decode_roundtrip() {
        let header = AetherHeader::new(StreamId(7), Timestamp(123456), Sequence(99), 1500);
        let bytes = header.encode();
        let decoded = AetherHeader::decode(&bytes).unwrap();
        assert_eq!(header, decoded);
    }

    #[test]
    fn reject_bad_magic() {
        let mut bytes = AetherHeader::new(StreamId(1), Timestamp(0), Sequence(0), 0).encode();
        bytes[0] = 0;
        assert!(matches!(
            AetherHeader::decode(&bytes),
            Err(ProtocolError::BadMagic(_))
        ));
    }

    #[test]
    fn reject_truncated() {
        assert!(matches!(
            AetherHeader::decode(&[0u8; 8]),
            Err(ProtocolError::Truncated { .. })
        ));
    }

    #[test]
    fn frame_roundtrip() {
        let pkt = Packet::new(StreamId(3), Sequence(9), Timestamp(42), vec![1, 2, 3, 4]);
        let bytes = encode_frame(&pkt);
        let back = decode_frame(&bytes).unwrap();
        assert_eq!(pkt, back);
    }
}
