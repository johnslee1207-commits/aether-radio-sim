//! Observability plane: metrics (5-layer), events, trace, health.
//! Framework: `docs/AETHER_RADIO_OBSERVABILITY_OPS_FRAMEWORK_v1.0.md`

mod config;
mod events;
mod health;
mod layers;
mod prometheus;
mod recovery;
mod trace;

pub use config::{ObservabilityConfig, OpsConfigError};
pub use events::{taxonomy, EventLogger, LogEvent};
pub use health::{HealthError, HealthManager, HealthState, HealthThresholds};
pub use layers::{
    ComputeLayerMetrics, LayeredMetricsSnapshot, MemoryLayerMetrics, MetricsBackend,
    PhysicalMetrics, RadioLayerMetrics, TransportLayerMetrics,
};
pub use prometheus::render_prometheus_text;
pub use recovery::{RecoveryAction, RecoveryError, RecoveryExecutor, RecoveryPolicy};
pub use trace::{PacketTrace, StageStamp, TraceEngine, TraceStage};

use serde::{Deserialize, Serialize};

/// Legacy flat snapshot (kept for CLI/smoke JSON compatibility).
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

/// In-process metrics engine implementing [`MetricsBackend`].
#[derive(Debug, Default)]
pub struct MetricsEngine {
    layered: LayeredMetricsSnapshot,
    last_latency_ns: u64,
}

impl MetricsEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_rx(&mut self) {
        self.layered.transport.rx_packets += 1;
    }

    pub fn record_tx(&mut self) {
        self.layered.transport.tx_packets += 1;
    }

    pub fn record_drop(&mut self) {
        self.layered.transport.drop += 1;
    }

    pub fn record_sequence_gap(&mut self) {
        self.layered.transport.gap_count += 1;
    }

    pub fn record_late_packet(&mut self) {
        self.layered.transport.late_packet += 1;
    }

    pub fn record_latency_ns(&mut self, latency_ns: u64) {
        self.record_latency_sample(latency_ns);
    }

    pub fn record_deadline_miss(&mut self) {
        self.layered.radio.deadline_miss += 1;
        self.layered.radio.slot_deadline_miss += 1;
    }

    pub fn record_error(&mut self) {
        self.layered.physical.crc_error += 1;
    }

    pub fn set_ring_occupancy(&mut self, occupancy: u64) {
        MetricsBackend::set_ring_occupancy(self, occupancy);
    }

    pub fn record_buffer_full(&mut self) {
        self.layered.memory.buffer_full = self.layered.memory.buffer_full.saturating_add(1);
        self.layered.memory.overflow = self.layered.memory.overflow.saturating_add(1);
    }

    pub fn set_memory_buffers(&mut self, host_used: u64, gpu_used: u64) {
        self.layered.memory.host_buffer_used = host_used;
        self.layered.memory.gpu_buffer_used = gpu_used;
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            link: LinkMetrics {
                rx_packets: self.layered.transport.rx_packets,
                tx_packets: self.layered.transport.tx_packets,
                bandwidth_bps: 0.0,
                drop: self.layered.transport.drop,
                error: self.layered.physical.crc_error,
            },
            transport: TransportMetrics {
                sequence_gap: self.layered.transport.gap_count,
                late_packet: self.layered.transport.late_packet,
                jitter_ns: self.layered.transport.jitter_ns,
                latency_ns: self.last_latency_ns,
            },
            radio: RadioMetrics {
                slot_latency_ns: 0,
                symbol_latency_ns: self.layered.radio.symbol_latency_ns,
                deadline_miss: self.layered.radio.deadline_miss,
            },
        }
    }

    pub fn layered_snapshot(&self) -> LayeredMetricsSnapshot {
        self.layered.clone()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.snapshot()).unwrap_or_else(|_| "{}".into())
    }

    pub fn to_layered_json(&self) -> String {
        serde_json::to_string(&self.layered).unwrap_or_else(|_| "{}".into())
    }
}

impl MetricsBackend for MetricsEngine {
    fn record_rx_bytes(&mut self, bytes: u64) {
        self.layered.transport.rx_packets += 1;
        self.layered.transport.rx_bytes = self.layered.transport.rx_bytes.saturating_add(bytes);
    }

    fn record_tx_bytes(&mut self, bytes: u64) {
        self.layered.transport.tx_packets += 1;
        self.layered.transport.tx_bytes = self.layered.transport.tx_bytes.saturating_add(bytes);
    }

    fn record_gap(&mut self) {
        self.record_sequence_gap();
    }

    fn record_late(&mut self) {
        self.record_late_packet();
    }

    fn record_drop(&mut self) {
        self.layered.transport.drop += 1;
    }

    fn record_latency_sample(&mut self, latency_ns: u64) {
        let t = &mut self.layered.transport;
        if t.latency_samples == 0 {
            t.latency_min_ns = latency_ns;
            t.latency_max_ns = latency_ns;
        } else {
            t.latency_min_ns = t.latency_min_ns.min(latency_ns);
            t.latency_max_ns = t.latency_max_ns.max(latency_ns);
            if t.latency_last_ns > 0 {
                let delta = latency_ns.abs_diff(t.latency_last_ns);
                t.jitter_ns = delta;
            }
        }
        t.latency_last_ns = latency_ns;
        t.latency_sum_ns = t.latency_sum_ns.saturating_add(latency_ns);
        t.latency_samples += 1;
        self.last_latency_ns = latency_ns;
    }

    fn record_deadline_miss(&mut self) {
        self.layered.radio.deadline_miss += 1;
        self.layered.radio.slot_deadline_miss += 1;
    }

    fn record_symbol(&mut self) {
        self.layered.radio.symbol_received += 1;
        self.layered.radio.slot_received += 1;
        self.layered.radio.slot_processed += 1;
    }

    fn record_kernel_ns(&mut self, ns: u64) {
        self.layered.compute.kernel_latency_ns = ns;
        self.layered.compute.kernel_executions += 1;
    }

    fn set_link_up(&mut self, up: bool, speed_gbps: f64) {
        self.layered.physical.link_up = up;
        self.layered.physical.link_speed_gbps = speed_gbps;
    }

    fn set_ring_occupancy(&mut self, occupancy: u64) {
        self.layered.memory.ring_occupancy = occupancy;
    }

    fn layered_snapshot(&self) -> LayeredMetricsSnapshot {
        self.layered.clone()
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

    #[test]
    fn layered_latency_samples() {
        let mut m = MetricsEngine::new();
        m.set_link_up(true, 100.0);
        m.record_latency_sample(1000);
        m.record_latency_sample(3000);
        let layered = m.layered_snapshot();
        assert!(layered.physical.link_up);
        assert_eq!(layered.transport.latency_min_ns, 1000);
        assert_eq!(layered.transport.latency_max_ns, 3000);
        assert_eq!(layered.transport.latency_samples, 2);
        assert!(m.to_layered_json().contains("physical"));
    }
}
