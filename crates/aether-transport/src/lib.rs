//! Transport engine and stream/link management.
//!
//! Data-plane path stays synchronous (poll / lock-free). Control-plane
//! orchestration may use tokio in the CLI / simulation host.
//! Deadline / sequence policy loads from `configs/transport_deadline.yaml`.

mod check;
mod sim;

pub use check::{SequenceChecker, TimestampChecker, TransportDeadlineConfig};
pub use sim::SimTransportEngine;

use aether_types::{Packet, StreamId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Transport runtime contract. Backends must not leak DPDK/CUDA/socket APIs.
pub trait TransportEngine {
    fn send(&mut self, packet: Packet) -> Result<(), TransportError>;
    fn receive(&mut self) -> Result<Option<Packet>, TransportError>;
}

/// Stream manager contract.
pub trait StreamManager {
    fn create_stream(&mut self, cfg: StreamConfig) -> Result<StreamId, TransportError>;
    fn start_stream(&mut self, stream_id: StreamId) -> Result<(), TransportError>;
    fn stop_stream(&mut self, stream_id: StreamId) -> Result<(), TransportError>;
    fn query_stream(&self, stream_id: StreamId) -> Result<StreamStatus, TransportError>;
}

/// Link lifecycle contract.
pub trait LinkManager {
    fn link_up(&mut self) -> Result<(), TransportError>;
    fn link_down(&mut self) -> Result<(), TransportError>;
    fn reset(&mut self) -> Result<(), TransportError>;
    fn get_status(&self) -> LinkState;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamConfig {
    pub stream_id: StreamId,
    pub carrier: u16,
    pub antenna: u16,
    pub qos: u8,
    pub deadline_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamState {
    Idle,
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamStatus {
    pub stream_id: StreamId,
    pub state: StreamState,
    pub last_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LinkState {
    #[default]
    Init,
    Discovery,
    Ready,
    Running,
    Error,
    Recovery,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransportError {
    #[error("stream not found: {0:?}")]
    StreamNotFound(StreamId),
    #[error("stream already exists: {0:?}")]
    StreamExists(StreamId),
    #[error("stream not running: {0:?}")]
    StreamNotRunning(StreamId),
    #[error("link not ready")]
    LinkNotReady,
    #[error("sequence gap: expected {expected}, got {got}")]
    SequenceGap { expected: u64, got: u64 },
    #[error("late packet: latency_ns={latency_ns} max={max_ns}")]
    LatePacket { latency_ns: u64, max_ns: u64 },
    #[error("config error: {0}")]
    Config(String),
}

/// In-memory mock transport without sequence checks (unit/interface tests).
#[derive(Debug, Default)]
pub struct MockTransportEngine {
    streams: Vec<StreamConfig>,
    inbox: Vec<Packet>,
    outbox: Vec<Packet>,
    link: LinkState,
}

impl MockTransportEngine {
    pub fn new() -> Self {
        Self {
            link: LinkState::Init,
            ..Self::default()
        }
    }

    pub fn push_inbox(&mut self, packet: Packet) {
        self.inbox.push(packet);
    }

    pub fn outbox(&self) -> &[Packet] {
        &self.outbox
    }
}

impl TransportEngine for MockTransportEngine {
    fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        if self.link != LinkState::Running && self.link != LinkState::Ready {
            return Err(TransportError::LinkNotReady);
        }
        self.outbox.push(packet);
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<Packet>, TransportError> {
        Ok(if self.inbox.is_empty() {
            None
        } else {
            Some(self.inbox.remove(0))
        })
    }
}

impl StreamManager for MockTransportEngine {
    fn create_stream(&mut self, cfg: StreamConfig) -> Result<StreamId, TransportError> {
        if self.streams.iter().any(|s| s.stream_id == cfg.stream_id) {
            return Err(TransportError::StreamExists(cfg.stream_id));
        }
        let id = cfg.stream_id;
        self.streams.push(cfg);
        Ok(id)
    }

    fn start_stream(&mut self, stream_id: StreamId) -> Result<(), TransportError> {
        if !self.streams.iter().any(|s| s.stream_id == stream_id) {
            return Err(TransportError::StreamNotFound(stream_id));
        }
        Ok(())
    }

    fn stop_stream(&mut self, stream_id: StreamId) -> Result<(), TransportError> {
        if !self.streams.iter().any(|s| s.stream_id == stream_id) {
            return Err(TransportError::StreamNotFound(stream_id));
        }
        Ok(())
    }

    fn query_stream(&self, stream_id: StreamId) -> Result<StreamStatus, TransportError> {
        if !self.streams.iter().any(|s| s.stream_id == stream_id) {
            return Err(TransportError::StreamNotFound(stream_id));
        }
        Ok(StreamStatus {
            stream_id,
            state: StreamState::Running,
            last_sequence: 0,
        })
    }
}

impl MockTransportEngine {
    pub fn destroy_stream(&mut self, stream_id: StreamId) -> Result<(), TransportError> {
        let before = self.streams.len();
        self.streams.retain(|s| s.stream_id != stream_id);
        if self.streams.len() == before {
            return Err(TransportError::StreamNotFound(stream_id));
        }
        Ok(())
    }
}

impl LinkManager for MockTransportEngine {
    fn link_up(&mut self) -> Result<(), TransportError> {
        self.link = LinkState::Running;
        Ok(())
    }

    fn link_down(&mut self) -> Result<(), TransportError> {
        self.link = LinkState::Init;
        Ok(())
    }

    fn reset(&mut self) -> Result<(), TransportError> {
        self.link = LinkState::Init;
        self.inbox.clear();
        self.outbox.clear();
        Ok(())
    }

    fn get_status(&self) -> LinkState {
        self.link
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_types::{Packet, Sequence, StreamId, Timestamp};

    #[test]
    fn mock_stream_lifecycle() {
        let mut eng = MockTransportEngine::new();
        eng.link_up().unwrap();
        let id = StreamId(1);
        StreamManager::create_stream(
            &mut eng,
            StreamConfig {
                stream_id: id,
                carrier: 0,
                antenna: 0,
                qos: 0,
                deadline_ns: 10_000,
            },
        )
        .unwrap();
        let pkt = Packet::new(id, Sequence(0), Timestamp(1), vec![9]);
        eng.send(pkt).unwrap();
        assert_eq!(eng.outbox().len(), 1);
        eng.destroy_stream(id).unwrap();
    }
}
