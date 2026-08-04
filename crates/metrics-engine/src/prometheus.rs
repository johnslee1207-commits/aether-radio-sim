//! Prometheus text exposition format (mock exporter — no HTTP server).

use crate::layers::LayeredMetricsSnapshot;
use crate::HealthState;

/// Render layered metrics as Prometheus text 0.0.4 exposition.
pub fn render_prometheus_text(
    snap: &LayeredMetricsSnapshot,
    health: Option<HealthState>,
    job: &str,
) -> String {
    let mut out = String::with_capacity(2048);
    let job = sanitize_label(job);

    macro_rules! gauge {
        ($name:expr, $help:expr, $val:expr) => {{
            out.push_str(&format!("# HELP {} {}\n", $name, $help));
            out.push_str(&format!("# TYPE {} gauge\n", $name));
            out.push_str(&format!("{}{{job=\"{}\"}} {}\n", $name, job, $val));
        }};
    }
    macro_rules! counter {
        ($name:expr, $help:expr, $val:expr) => {{
            out.push_str(&format!("# HELP {} {}\n", $name, $help));
            out.push_str(&format!("# TYPE {} counter\n", $name));
            out.push_str(&format!("{}{{job=\"{}\"}} {}\n", $name, job, $val));
        }};
    }

    gauge!(
        "aether_link_up",
        "Physical link up (1) or down (0)",
        if snap.physical.link_up { 1 } else { 0 }
    );
    gauge!(
        "aether_link_speed_gbps",
        "Configured or modelled link speed",
        snap.physical.link_speed_gbps
    );
    counter!(
        "aether_transport_rx_packets",
        "Transport RX packets",
        snap.transport.rx_packets
    );
    counter!(
        "aether_transport_tx_packets",
        "Transport TX packets",
        snap.transport.tx_packets
    );
    counter!(
        "aether_transport_rx_bytes",
        "Transport RX bytes",
        snap.transport.rx_bytes
    );
    counter!(
        "aether_transport_gap_count",
        "Sequence gap count",
        snap.transport.gap_count
    );
    counter!(
        "aether_transport_late_packets",
        "Late packet count",
        snap.transport.late_packet
    );
    counter!(
        "aether_transport_drop",
        "Dropped packets",
        snap.transport.drop
    );
    gauge!(
        "aether_transport_latency_last_ns",
        "Last e2e latency sample",
        snap.transport.latency_last_ns
    );
    gauge!(
        "aether_transport_latency_max_ns",
        "Max e2e latency sample",
        snap.transport.latency_max_ns
    );
    gauge!(
        "aether_transport_jitter_ns",
        "Last inter-sample jitter",
        snap.transport.jitter_ns
    );
    counter!(
        "aether_radio_deadline_miss",
        "Radio deadline misses",
        snap.radio.deadline_miss
    );
    counter!(
        "aether_radio_symbol_received",
        "Symbols received",
        snap.radio.symbol_received
    );
    gauge!(
        "aether_memory_ring_occupancy",
        "GPU/host ring occupancy",
        snap.memory.ring_occupancy
    );
    gauge!(
        "aether_compute_kernel_latency_ns",
        "Last kernel latency",
        snap.compute.kernel_latency_ns
    );
    counter!(
        "aether_compute_kernel_executions",
        "Kernel execution count",
        snap.compute.kernel_executions
    );

    if let Some(h) = health {
        let code = match h {
            HealthState::Normal => 0,
            HealthState::Warning => 1,
            HealthState::Degraded => 2,
            HealthState::Failed => 3,
            HealthState::Recovery => 4,
        };
        gauge!(
            "aether_health_state",
            "Health state code NORMAL=0 WARNING=1 DEGRADED=2 FAILED=3 RECOVERY=4",
            code
        );
    }

    out
}

fn sanitize_label(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::LayeredMetricsSnapshot;

    #[test]
    fn renders_counters() {
        let mut snap = LayeredMetricsSnapshot::default();
        snap.physical.link_up = true;
        snap.transport.rx_packets = 7;
        let text = render_prometheus_text(&snap, Some(HealthState::Normal), "aether-sim");
        assert!(text.contains("aether_transport_rx_packets"));
        assert!(text.contains("} 7"));
        assert!(text.contains("aether_health_state"));
    }
}
