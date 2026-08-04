//! Five-layer metrics model + MetricsBackend trait (Ops Framework §3–4).

use serde::{Deserialize, Serialize};

/// Layer 1 — Physical / Link.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PhysicalMetrics {
    pub link_up: bool,
    pub link_speed_gbps: f64,
    pub mtu: u32,
    pub crc_error: u64,
    pub fec_corrected: u64,
    pub fec_uncorrected: u64,
}

/// Layer 2 — Transport.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TransportLayerMetrics {
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub gap_count: u64,
    pub duplicate_count: u64,
    pub out_of_order_count: u64,
    pub late_packet: u64,
    pub drop: u64,
    pub latency_min_ns: u64,
    pub latency_max_ns: u64,
    pub latency_last_ns: u64,
    pub latency_sum_ns: u64,
    pub latency_samples: u64,
    pub jitter_ns: u64,
}

impl TransportLayerMetrics {
    pub fn latency_avg_ns(&self) -> u64 {
        self.latency_sum_ns
            .checked_div(self.latency_samples)
            .unwrap_or(0)
    }
}

/// Layer 3 — Radio.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RadioLayerMetrics {
    pub slot_received: u64,
    pub slot_processed: u64,
    pub slot_lost: u64,
    pub slot_deadline_miss: u64,
    pub symbol_received: u64,
    pub symbol_gap: u64,
    pub symbol_latency_ns: u64,
    pub iq_loss: u64,
    pub deadline_miss: u64,
}

/// Layer 4 — Memory.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MemoryLayerMetrics {
    pub host_buffer_used: u64,
    pub gpu_buffer_used: u64,
    pub buffer_full: u64,
    pub overflow: u64,
    pub underflow: u64,
    pub ring_occupancy: u64,
    pub ring_stall_ns: u64,
}

/// Layer 5 — Compute.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ComputeLayerMetrics {
    pub kernel_latency_ns: u64,
    pub kernel_executions: u64,
    pub cuda_stream_idle: u64,
    pub gpu_utilization_pct: f64,
}

/// Full five-layer snapshot (Observability Framework).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LayeredMetricsSnapshot {
    pub physical: PhysicalMetrics,
    pub transport: TransportLayerMetrics,
    pub radio: RadioLayerMetrics,
    pub memory: MemoryLayerMetrics,
    pub compute: ComputeLayerMetrics,
}

/// Backend trait — business code depends on this, not exporters.
pub trait MetricsBackend {
    fn record_rx_bytes(&mut self, bytes: u64);
    fn record_tx_bytes(&mut self, bytes: u64);
    fn record_gap(&mut self);
    fn record_late(&mut self);
    fn record_drop(&mut self);
    fn record_latency_sample(&mut self, latency_ns: u64);
    fn record_deadline_miss(&mut self);
    fn record_symbol(&mut self);
    fn record_kernel_ns(&mut self, ns: u64);
    fn set_link_up(&mut self, up: bool, speed_gbps: f64);
    fn set_ring_occupancy(&mut self, occupancy: u64);
    fn layered_snapshot(&self) -> LayeredMetricsSnapshot;
}
