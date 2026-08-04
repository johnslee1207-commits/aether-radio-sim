//! End-to-end FPGA → ethernet → transport → CX5 DMA → GPU ring bench.

use crate::{BenchReport, LatencyStats, RadioDeadlineStats, ThroughputStats};
use aether_transport::{
    LinkManager, SimTransportEngine, StreamConfig, StreamManager, TransportEngine,
};
use aether_types::StreamId;
use cx5_emulator::{Cx5Nic, PacketIO};
use ethernet_model::EthernetModelConfig;
use fault_injection::FaultInjectionConfig;
use fpga_emulator::{FpgaEmulator, RadioTimingConfig};
use gpu_runtime::GpuRingBuffer;
use metrics_engine::{EventLogger, LogEvent, MetricsEngine};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchProfile {
    pub version: String,
    pub id: String,
    pub symbol_count: u64,
    pub streams: u32,
    pub latency_budget_ns: u64,
    pub include_ethernet_delay: bool,
    pub include_nic_dma: bool,
    pub include_gpu_kernel: bool,
    /// `simulation` (default) or `cuda` (requires `--features cuda` on CLI/bench).
    #[serde(default = "default_gpu_backend")]
    pub gpu_backend: String,
    pub report_path: String,
    #[serde(default = "default_events_path")]
    pub events_path: String,
    pub radio_timing_config: String,
    pub transport_deadline_config: String,
    pub ethernet_config: String,
    pub nic_config: String,
    pub gpu_ring_config: String,
    pub fault_config: String,
    #[serde(default)]
    pub gpu_cuda_config: Option<String>,
}

fn default_gpu_backend() -> String {
    "simulation".into()
}

fn default_events_path() -> String {
    "data/reports/bench_events.jsonl".into()
}

impl BenchProfile {
    pub fn from_yaml_str(s: &str) -> Result<Self, PipelineBenchError> {
        serde_yaml::from_str(s).map_err(|e| PipelineBenchError::Config(e.to_string()))
    }

    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, PipelineBenchError> {
        let text = fs::read_to_string(path.as_ref()).map_err(|e| {
            PipelineBenchError::Config(format!("read {}: {e}", path.as_ref().display()))
        })?;
        Self::from_yaml_str(&text)
    }
}

#[derive(Debug, Error)]
pub enum PipelineBenchError {
    #[error("config error: {0}")]
    Config(String),
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("io error: {0}")]
    Io(String),
}

/// End-to-end simulation bench (multi-stream capable).
pub struct PipelineBench {
    profile: BenchProfile,
    base_dir: PathBuf,
}

impl PipelineBench {
    pub fn new(profile: BenchProfile) -> Self {
        Self {
            profile,
            base_dir: PathBuf::from("."),
        }
    }

    pub fn with_base_dir(mut self, base: impl Into<PathBuf>) -> Self {
        self.base_dir = base.into();
        self
    }

    fn resolve(&self, rel: &str) -> PathBuf {
        self.base_dir.join(rel)
    }

    fn read_rel(&self, rel: &str) -> Result<String, PipelineBenchError> {
        let path = self.resolve(rel);
        fs::read_to_string(&path)
            .map_err(|e| PipelineBenchError::Config(format!("read {}: {e}", path.display())))
    }

    pub fn run(&self) -> Result<(BenchReport, MetricsEngine), PipelineBenchError> {
        let p = &self.profile;
        let stream_count = p.streams.max(1);
        let timing = RadioTimingConfig::from_yaml_str(&self.read_rel(&p.radio_timing_config)?)
            .map_err(|e| PipelineBenchError::Config(e.to_string()))?;
        let eth = EthernetModelConfig::from_yaml_str(&self.read_rel(&p.ethernet_config)?)
            .map_err(|e| PipelineBenchError::Config(e.to_string()))?;
        let mut transport =
            SimTransportEngine::from_yaml(&self.read_rel(&p.transport_deadline_config)?)
                .map_err(|e| PipelineBenchError::Config(e.to_string()))?;
        let mut nic = Cx5Nic::from_yaml(&self.read_rel(&p.nic_config)?)
            .map_err(|e| PipelineBenchError::Config(e.to_string()))?;
        let mut ring = GpuRingBuffer::from_yaml(&self.read_rel(&p.gpu_ring_config)?)
            .map_err(|e| PipelineBenchError::Config(e.to_string()))?;
        let fault = FaultInjectionConfig::from_yaml_str(&self.read_rel(&p.fault_config)?)
            .map_err(|e| PipelineBenchError::Config(e.to_string()))?;

        let use_cuda = p.gpu_backend.eq_ignore_ascii_case("cuda");
        #[cfg(feature = "cuda")]
        let mut cuda_gpu = if use_cuda {
            let path = p
                .gpu_cuda_config
                .clone()
                .unwrap_or_else(|| "configs/backends/gpu_cuda.yaml".into());
            let yaml = self.read_rel(&path)?;
            Some(
                gpu_cuda::CudaGpu::from_yaml(&yaml)
                    .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?,
            )
        } else {
            None
        };
        #[cfg(not(feature = "cuda"))]
        if use_cuda {
            return Err(PipelineBenchError::Config(
                "gpu_backend=cuda requires benchmark/cli `--features cuda`".into(),
            ));
        }

        let mut fpgas = Vec::new();
        transport
            .link_up()
            .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?;
        for s in 1..=stream_count {
            let id = StreamId(s);
            transport
                .create_stream(StreamConfig {
                    stream_id: id,
                    carrier: ((s - 1) % 4) as u16,
                    antenna: ((s - 1) / 4) as u16,
                    qos: 0,
                    deadline_ns: p.latency_budget_ns,
                })
                .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?;
            transport
                .start_stream(id)
                .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?;
            fpgas.push(FpgaEmulator::new(timing.clone(), id));
        }

        let mut metrics = MetricsEngine::new();
        let mut events = EventLogger::create(self.resolve(&p.events_path))
            .map_err(|e| PipelineBenchError::Io(e.to_string()))?;
        let mut latencies = Vec::with_capacity((p.symbol_count as usize) * stream_count as usize);
        let mut bytes: u64 = 0;
        let mut packets: u64 = 0;
        let mut symbol_miss = 0u64;
        let mut now_ns = 0u64;

        for _ in 0..p.symbol_count {
            for fpga in &mut fpgas {
                let packet = fpga.emit_symbol();
                let stream = packet.stream_id.0;
                let seq = packet.sequence.0;
                let wire = if p.include_ethernet_delay {
                    eth.serialize_delay_ns(packet.payload.len())
                        .saturating_add(eth.wire_delay_ns())
                } else {
                    0
                };
                now_ns = now_ns
                    .max(packet.timestamp.0)
                    .saturating_add(wire)
                    .saturating_add(fault.extra_latency_ns());

                if fault.should_drop_deterministic(packet.sequence.0) {
                    metrics.record_drop();
                    continue;
                }

                transport.now_ns = now_ns;
                match transport.ingest(packet.clone()) {
                    Ok(()) => {}
                    Err(aether_transport::TransportError::SequenceGap { got, .. }) => {
                        transport.recover_sequence(packet.stream_id, got);
                        metrics.record_sequence_gap();
                        transport
                            .ingest(packet.clone())
                            .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?;
                    }
                    Err(aether_transport::TransportError::LatePacket { .. }) => {
                        metrics.record_deadline_miss();
                        metrics.record_late_packet();
                        continue;
                    }
                    Err(e) => return Err(PipelineBenchError::Runtime(e.to_string())),
                }

                let Some(pkt) = transport
                    .receive()
                    .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?
                else {
                    continue;
                };

                nic.advance_time(now_ns);
                if p.include_nic_dma {
                    nic.submit_rx(pkt.clone())
                        .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?;
                    now_ns = now_ns.saturating_add(nic.dma_latency_ns());
                    nic.advance_time(now_ns);
                } else {
                    nic.submit_rx_ready_at(pkt.clone(), now_ns)
                        .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?;
                }

                for delivered in nic.rx_burst(32) {
                    metrics.record_rx();
                    let arrive = now_ns;
                    let gpu_latency = if p.include_gpu_kernel {
                        #[cfg(feature = "cuda")]
                        if let Some(gpu) = cuda_gpu.as_mut() {
                            let _ = gpu
                                .process_bytes(&delivered.payload)
                                .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?;
                            gpu.last_kernel_ns.unwrap_or(0)
                        } else {
                            ring.process_packet(&delivered.payload, arrive)
                                .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?
                        }
                        #[cfg(not(feature = "cuda"))]
                        {
                            ring.process_packet(&delivered.payload, arrive)
                                .map_err(|e| PipelineBenchError::Runtime(e.to_string()))?
                        }
                    } else {
                        0
                    };
                    now_ns = now_ns.saturating_add(gpu_latency);
                    let e2e = now_ns.saturating_sub(delivered.timestamp.0);
                    latencies.push(e2e);
                    metrics.record_latency_ns(e2e);
                    let _ = events.emit(
                        &LogEvent::now("packet_rx")
                            .with_stream(stream)
                            .with_sequence(seq)
                            .with_latency_us(e2e as f64 / 1_000.0),
                    );
                    if e2e > p.latency_budget_ns {
                        symbol_miss += 1;
                        metrics.record_deadline_miss();
                    }
                    bytes += delivered.payload.len() as u64;
                    packets += 1;
                    metrics.record_tx();
                }
            }
        }
        let _ = events.flush();

        latencies.sort_unstable();
        let duration_s = (now_ns as f64) / 1e9;
        let report = BenchReport {
            throughput: ThroughputStats::from_bytes_and_duration(
                bytes,
                duration_s.max(1e-12),
                packets,
            ),
            latency: LatencyStats::from_sorted_ns(&latencies),
            radio: RadioDeadlineStats {
                slot_miss: 0,
                symbol_miss,
            },
            packets,
            bytes,
            sequence_gaps: transport.sequence_gaps,
            late_packets: transport.late_packets,
            sim_duration_ns: now_ns,
        };
        Ok((report, metrics))
    }

    pub fn write_report(&self, report: &BenchReport) -> Result<PathBuf, PipelineBenchError> {
        let path = self.resolve(&self.profile.report_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| PipelineBenchError::Io(format!("mkdir {}: {e}", parent.display())))?;
        }
        fs::write(&path, report.to_json())
            .map_err(|e| PipelineBenchError::Io(format!("write {}: {e}", path.display())))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn pipeline_bench_runs_from_repo_configs() {
        let root = workspace_root();
        let mut profile = BenchProfile::load_path(root.join("configs/bench_profile.yaml")).unwrap();
        profile.symbol_count = 8;
        profile.streams = 2;
        profile.events_path = format!(
            "data/reports/bench_events_test_{}.jsonl",
            std::process::id()
        );
        let bench = PipelineBench::new(profile).with_base_dir(root);
        let (report, _) = bench.run().unwrap();
        assert_eq!(report.packets, 16); // 8 symbols * 2 streams
        assert!(report.latency.min_ns > 0);
        assert!(report.latency.p50_ns >= 2_000);
    }
}
