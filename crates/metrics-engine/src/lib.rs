//! Structured metrics and JSON event logging (spec §17).

mod events;

pub use events::{EventLogger, LogEvent};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LinkMetrics {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub bandwidth_bps: f64,
    pub drop: u64,
    pub error: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TransportMetrics {
    pub sequence_gap: u64,
    pub late_packet: u64,
    pub jitter_ns: u64,
    pub latency_ns: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RadioMetrics {
    pub slot_latency_ns: u64,
    pub symbol_latency_ns: u64,
    pub deadline_miss: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub link: LinkMetrics,
    pub transport: TransportMetrics,
    pub radio: RadioMetrics,
}

#[derive(Debug, Default)]
pub struct MetricsEngine {
    snap: MetricsSnapshot,
}

impl MetricsEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_rx(&mut self) {
        self.snap.link.rx_packets += 1;
    }

    pub fn record_tx(&mut self) {
        self.snap.link.tx_packets += 1;
    }

    pub fn record_drop(&mut self) {
        self.snap.link.drop += 1;
    }

    pub fn record_sequence_gap(&mut self) {
        self.snap.transport.sequence_gap += 1;
    }

    pub fn record_late_packet(&mut self) {
        self.snap.transport.late_packet += 1;
    }

    pub fn record_latency_ns(&mut self, latency_ns: u64) {
        self.snap.transport.latency_ns = latency_ns;
    }

    pub fn record_deadline_miss(&mut self) {
        self.snap.radio.deadline_miss += 1;
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        self.snap.clone()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.snap).unwrap_or_else(|_| "{}".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_and_json() {
        let mut m = MetricsEngine::new();
        m.record_rx();
        m.record_tx();
        m.record_drop();
        let s = m.snapshot();
        assert_eq!(s.link.rx_packets, 1);
        assert!(m.to_json().contains("rx_packets"));
    }
}
