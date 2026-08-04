//! Simulation transport engine with stream manager and checkers.

use crate::check::{SequenceChecker, TimestampChecker, TransportDeadlineConfig};
use crate::{
    LinkManager, LinkState, StreamConfig, StreamManager, StreamState, StreamStatus,
    TransportEngine, TransportError,
};
use aether_types::{Packet, StreamId};
use std::collections::HashMap;

#[derive(Debug)]
struct StreamRuntime {
    cfg: StreamConfig,
    state: StreamState,
}

/// Transport engine used by the simulation backend (Sprint 4).
#[derive(Debug)]
pub struct SimTransportEngine {
    streams: HashMap<StreamId, StreamRuntime>,
    inbox: Vec<Packet>,
    outbox: Vec<Packet>,
    link: LinkState,
    seq: SequenceChecker,
    ts: TimestampChecker,
    /// Simulation receive clock (ns); advanced by callers / inject path.
    pub now_ns: u64,
    pub sequence_gaps: u64,
    pub late_packets: u64,
}

impl SimTransportEngine {
    pub fn new(deadline: TransportDeadlineConfig) -> Self {
        Self {
            streams: HashMap::new(),
            inbox: Vec::new(),
            outbox: Vec::new(),
            link: LinkState::Init,
            seq: SequenceChecker::from_config(&deadline),
            ts: TimestampChecker::from_config(&deadline),
            now_ns: 0,
            sequence_gaps: 0,
            late_packets: 0,
        }
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, TransportError> {
        Ok(Self::new(TransportDeadlineConfig::from_yaml_str(yaml)?))
    }

    /// Inject a received packet through sequence + timestamp checks.
    pub fn ingest(&mut self, packet: Packet) -> Result<(), TransportError> {
        if self.link != LinkState::Running && self.link != LinkState::Ready {
            return Err(TransportError::LinkNotReady);
        }
        let runtime = self
            .streams
            .get(&packet.stream_id)
            .ok_or(TransportError::StreamNotFound(packet.stream_id))?;
        if runtime.state != StreamState::Running {
            return Err(TransportError::StreamNotRunning(packet.stream_id));
        }

        if let Err(e) = self.ts.check(&packet, self.now_ns) {
            self.late_packets += 1;
            return Err(e);
        }
        match self.seq.check(&packet) {
            Ok(()) => {
                self.inbox.push(packet);
                Ok(())
            }
            Err(e @ TransportError::SequenceGap { .. }) => {
                self.sequence_gaps += 1;
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    /// After loss, resync checker so subsequent packets can recover.
    pub fn recover_sequence(&mut self, stream_id: StreamId, next_seq: u64) {
        self.seq.resync(stream_id, next_seq);
        if let Some(s) = self.streams.get_mut(&stream_id) {
            s.state = StreamState::Running;
        }
        self.link = LinkState::Running;
    }

    pub fn outbox(&self) -> &[Packet] {
        &self.outbox
    }

    pub fn advance_time(&mut self, delta_ns: u64) {
        self.now_ns = self.now_ns.saturating_add(delta_ns);
    }
}

impl TransportEngine for SimTransportEngine {
    fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        if self.link != LinkState::Running && self.link != LinkState::Ready {
            return Err(TransportError::LinkNotReady);
        }
        let runtime = self
            .streams
            .get(&packet.stream_id)
            .ok_or(TransportError::StreamNotFound(packet.stream_id))?;
        if runtime.state != StreamState::Running {
            return Err(TransportError::StreamNotRunning(packet.stream_id));
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

impl StreamManager for SimTransportEngine {
    fn create_stream(&mut self, cfg: StreamConfig) -> Result<StreamId, TransportError> {
        if self.streams.contains_key(&cfg.stream_id) {
            return Err(TransportError::StreamExists(cfg.stream_id));
        }
        let id = cfg.stream_id;
        self.seq.register_stream(id);
        self.streams.insert(
            id,
            StreamRuntime {
                cfg,
                state: StreamState::Idle,
            },
        );
        Ok(id)
    }

    fn start_stream(&mut self, stream_id: StreamId) -> Result<(), TransportError> {
        let s = self
            .streams
            .get_mut(&stream_id)
            .ok_or(TransportError::StreamNotFound(stream_id))?;
        s.state = StreamState::Running;
        Ok(())
    }

    fn stop_stream(&mut self, stream_id: StreamId) -> Result<(), TransportError> {
        let s = self
            .streams
            .get_mut(&stream_id)
            .ok_or(TransportError::StreamNotFound(stream_id))?;
        s.state = StreamState::Stopped;
        Ok(())
    }

    fn query_stream(&self, stream_id: StreamId) -> Result<StreamStatus, TransportError> {
        let s = self
            .streams
            .get(&stream_id)
            .ok_or(TransportError::StreamNotFound(stream_id))?;
        Ok(StreamStatus {
            stream_id,
            state: s.state,
            last_sequence: self.seq.last_expected(stream_id),
        })
    }
}

impl SimTransportEngine {
    pub fn destroy_stream(&mut self, stream_id: StreamId) -> Result<(), TransportError> {
        if self.streams.remove(&stream_id).is_none() {
            return Err(TransportError::StreamNotFound(stream_id));
        }
        self.seq.unregister_stream(stream_id);
        Ok(())
    }

    pub fn stream_carrier_antenna(
        &self,
        stream_id: StreamId,
    ) -> Result<(u16, u16), TransportError> {
        let s = self
            .streams
            .get(&stream_id)
            .ok_or(TransportError::StreamNotFound(stream_id))?;
        Ok((s.cfg.carrier, s.cfg.antenna))
    }
}

impl LinkManager for SimTransportEngine {
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
        self.sequence_gaps = 0;
        self.late_packets = 0;
        Ok(())
    }

    fn get_status(&self) -> LinkState {
        self.link
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StreamManager, TransportEngine};
    use aether_types::{Packet, Sequence, StreamId, Timestamp};

    fn engine() -> SimTransportEngine {
        SimTransportEngine::from_yaml(
            r#"
version: "1.0.0"
id: transport-deadline-policy
max_latency_ns: 10000
allow_reorder: false
start_sequence: 0
"#,
        )
        .unwrap()
    }

    #[test]
    fn ingest_in_order_and_receive() {
        let mut eng = engine();
        eng.link_up().unwrap();
        let id = StreamId(1);
        eng.create_stream(StreamConfig {
            stream_id: id,
            carrier: 0,
            antenna: 0,
            qos: 0,
            deadline_ns: 10_000,
        })
        .unwrap();
        eng.start_stream(id).unwrap();
        eng.now_ns = 100;
        eng.ingest(Packet::new(id, Sequence(0), Timestamp(50), vec![1]))
            .unwrap();
        let got = eng.receive().unwrap().unwrap();
        assert_eq!(got.sequence.0, 0);
    }

    #[test]
    fn recover_after_gap() {
        let mut eng = engine();
        eng.link_up().unwrap();
        let id = StreamId(2);
        eng.create_stream(StreamConfig {
            stream_id: id,
            carrier: 1,
            antenna: 1,
            qos: 0,
            deadline_ns: 10_000,
        })
        .unwrap();
        eng.start_stream(id).unwrap();
        eng.now_ns = 1_000;
        eng.ingest(Packet::new(id, Sequence(0), Timestamp(0), vec![]))
            .unwrap();
        let err = eng
            .ingest(Packet::new(id, Sequence(2), Timestamp(10), vec![]))
            .unwrap_err();
        assert!(matches!(err, TransportError::SequenceGap { .. }));
        assert_eq!(eng.sequence_gaps, 1);
        eng.recover_sequence(id, 2);
        eng.ingest(Packet::new(id, Sequence(2), Timestamp(20), vec![9]))
            .unwrap();
        // first inbox item is seq0; recovered packet follows
        let _ = eng.receive().unwrap().unwrap();
        assert_eq!(eng.receive().unwrap().unwrap().payload, vec![9]);
    }
}
