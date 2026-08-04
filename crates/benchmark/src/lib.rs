//! Benchmark result aggregation and end-to-end simulation harness.
//! Tunables load from `configs/bench_profile.yaml` and related data files.

mod acceptance;
mod fault_drill;
mod pipeline;
mod soak;

pub use acceptance::{
    AcceptanceError, AcceptanceProfile, AcceptanceReport, AcceptanceRunner, GateResult,
};
pub use fault_drill::{FaultDrillError, FaultDrillProfile, FaultDrillReport, FaultDrillRunner};
pub use pipeline::{BenchProfile, PipelineBench, PipelineBenchError};
pub use soak::{SoakError, SoakGate, SoakProfile, SoakReport, SoakRoundHealth, SoakRunner};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LatencyStats {
    pub min_ns: u64,
    pub p50_ns: u64,
    pub p99_ns: u64,
    pub p999_ns: u64,
    pub max_ns: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ThroughputStats {
    pub gbps: f64,
    pub gbs: f64,
    pub pps: f64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RadioDeadlineStats {
    pub slot_miss: u64,
    pub symbol_miss: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BenchReport {
    pub throughput: ThroughputStats,
    pub latency: LatencyStats,
    pub radio: RadioDeadlineStats,
    pub packets: u64,
    pub bytes: u64,
    pub sequence_gaps: u64,
    pub late_packets: u64,
    pub sim_duration_ns: u64,
    #[serde(default)]
    pub recovery_actions: u64,
    #[serde(default)]
    pub ring_occupancy_peak: u64,
}

impl LatencyStats {
    pub fn from_sorted_ns(sorted: &[u64]) -> Self {
        if sorted.is_empty() {
            return Self::default();
        }
        let pct = |p: f64| -> u64 {
            let idx =
                ((p * (sorted.len().saturating_sub(1) as f64)) as usize).min(sorted.len() - 1);
            sorted[idx]
        };
        Self {
            min_ns: sorted[0],
            p50_ns: pct(0.50),
            p99_ns: pct(0.99),
            p999_ns: pct(0.999),
            max_ns: *sorted.last().unwrap(),
        }
    }
}

impl ThroughputStats {
    pub fn from_bytes_and_duration(bytes: u64, duration_s: f64, packets: u64) -> Self {
        if duration_s <= 0.0 {
            return Self::default();
        }
        let gbs = (bytes as f64) / duration_s / 1e9;
        Self {
            gbps: gbs * 8.0,
            gbs,
            pps: (packets as f64) / duration_s,
        }
    }
}

impl BenchReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_percentiles() {
        let samples: Vec<u64> = (1..=100).collect();
        let stats = LatencyStats::from_sorted_ns(&samples);
        assert_eq!(stats.min_ns, 1);
        assert_eq!(stats.max_ns, 100);
        assert_eq!(stats.p50_ns, 50);
    }

    #[test]
    fn throughput_100g_model() {
        let t = ThroughputStats::from_bytes_and_duration(12_500_000_000, 1.0, 1_000_000);
        assert!((t.gbs - 12.5).abs() < 1e-9);
        assert!((t.gbps - 100.0).abs() < 1e-9);
    }
}
