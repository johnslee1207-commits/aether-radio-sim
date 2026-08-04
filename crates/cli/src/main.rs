//! Control-plane CLI for Aether Radio simulation (tokio).
//! Data-plane crates remain poll-based and free of async on the hot path.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ethernet_model::EthernetModelConfig;
use std::fs;
use std::path::PathBuf;
use tracing::info;

#[derive(Parser, Debug)]
#[command(
    name = "aether-radio-sim",
    version,
    about = "Aether Radio Data Plane Simulation"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Print platform version and MVP profile path
    Info,
    /// Validate a YAML config against ethernet / fault loaders
    ValidateConfig {
        #[arg(long, default_value = "configs/ethernet_model.yaml")]
        path: PathBuf,
    },
    /// Run a single-stream smoke path (FPGA → transport → CX5 → GPU ring)
    Smoke {
        #[arg(long, default_value = "configs/simulation_profile.yaml")]
        profile: PathBuf,
    },
    /// Run end-to-end latency/throughput bench and write JSON report
    Bench {
        #[arg(long, default_value = "configs/bench_profile.yaml")]
        profile: PathBuf,
    },
    /// Run acceptance gates (ethernet model + PipelineBench SLA)
    Accept {
        #[arg(long, default_value = "configs/acceptance_profile.yaml")]
        profile: PathBuf,
    },
    /// Print observability plane status (metrics layers / health / ops config)
    OpsStatus {
        #[arg(long, default_value = "configs/ops/observability.yaml")]
        ops_config: PathBuf,
    },
    /// L4 soak / stress run (Ops Framework)
    Soak {
        #[arg(long, default_value = "configs/soak_profile.yaml")]
        profile: PathBuf,
    },
    /// Dump layered metrics as Prometheus text (mock exporter)
    PromDump {
        #[arg(long, default_value = "data/reports/metrics.prom")]
        out: PathBuf,
        #[arg(long, default_value = "configs/bench_profile.yaml")]
        bench_profile: PathBuf,
        #[arg(long, default_value = "aether-sim")]
        job: String,
    },
    /// Write consolidated ops JSON report (bench + health + layered metrics)
    OpsReport {
        #[arg(long, default_value = "data/reports/ops_report.json")]
        out: PathBuf,
        #[arg(long, default_value = "configs/bench_profile.yaml")]
        bench_profile: PathBuf,
        #[arg(long, default_value = "configs/ops/observability.yaml")]
        ops_config: PathBuf,
    },
    /// Serve Prometheus text over HTTP (ops scrape; binds per configs/ops)
    PromServe {
        #[arg(long, default_value = "configs/ops/prometheus_scrape.yaml")]
        config: PathBuf,
        /// Exit after serving one successful /metrics scrape
        #[arg(long, default_value_t = false)]
        once: bool,
    },
    /// Fault drill: stress faults + recovery policy
    FaultDrill {
        #[arg(long, default_value = "configs/fault_drill.yaml")]
        profile: PathBuf,
    },
    /// List CUDA devices (requires `--features cuda`)
    GpuInfo,
    /// Smoke path using real CUDA GpuBackend (requires `--features cuda`)
    SmokeCuda {
        #[arg(long, default_value = "configs/simulation_profile_cuda.yaml")]
        profile: PathBuf,
        #[arg(long, default_value = "configs/backends/gpu_cuda.yaml")]
        cuda_config: PathBuf,
    },
    /// FPGA container/process: emit symbols as UDP/SHM Aether frames
    FpgaEmit {
        #[arg(long, default_value = "udp")]
        transport: String,
        #[arg(long, default_value = "configs/backends/net_link_fpga_local.yaml")]
        net_config: PathBuf,
        #[arg(long, default_value = "configs/backends/shm_link.yaml")]
        shm_config: PathBuf,
        #[arg(long, default_value = "configs/radio_timing.yaml")]
        timing: PathBuf,
        #[arg(long, default_value_t = 32)]
        symbols: u64,
        #[arg(long, default_value_t = 200)]
        interval_us: u64,
        #[arg(long, default_value_t = 1)]
        stream_id: u32,
    },
    /// Host dataplane: receive UDP/SHM frames → transport → CX5 → GPU
    HostRecv {
        #[arg(long, default_value = "udp")]
        transport: String,
        #[arg(long, default_value = "configs/backends/net_link_host_local.yaml")]
        net_config: PathBuf,
        #[arg(long, default_value = "configs/backends/shm_link.yaml")]
        shm_config: PathBuf,
        #[arg(long, default_value = "configs/transport_deadline_net.yaml")]
        deadline: PathBuf,
        #[arg(long, default_value = "configs/nic_dma.yaml")]
        nic_config: PathBuf,
        #[arg(long, default_value = "configs/gpu_ring.yaml")]
        gpu_ring: PathBuf,
        #[arg(long, default_value_t = 32)]
        symbols: u64,
        #[arg(long, default_value_t = 500)]
        warmup_ms: u64,
        #[arg(long, default_value_t = 1)]
        stream_id: u32,
        #[arg(long, default_value_t = false)]
        use_cuda: bool,
        #[arg(long, default_value = "configs/backends/gpu_cuda.yaml")]
        cuda_config: PathBuf,
    },
    /// Create/truncate shared-memory ring (producer init)
    ShmPrepare {
        #[arg(long, default_value = "configs/backends/shm_link.yaml")]
        shm_config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Info => {
            println!("aether-radio-sim v{}", env!("CARGO_PKG_VERSION"));
            println!("spec: Aether Radio Data Plane Simulation Platform v1.1");
            println!("default profile: configs/simulation_profile.yaml");
        }
        Commands::ValidateConfig { path } => {
            let text =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let cfg = EthernetModelConfig::from_yaml_str(&text)?;
            info!(
                event = "config_validated",
                id = %cfg.id,
                bandwidth_gbps = cfg.bandwidth_gbps,
                "ethernet model ok"
            );
            println!("ok: {} ({} Gbps)", cfg.id, cfg.bandwidth_gbps);
        }
        Commands::Smoke { profile } => {
            run_smoke(&profile)?;
        }
        Commands::Bench { profile } => {
            run_bench(&profile)?;
        }
        Commands::Accept { profile } => {
            run_accept(&profile)?;
        }
        Commands::OpsStatus { ops_config } => {
            run_ops_status(&ops_config)?;
        }
        Commands::Soak { profile } => {
            run_soak(&profile)?;
        }
        Commands::PromDump {
            out,
            bench_profile,
            job,
        } => {
            run_prom_dump(&out, &bench_profile, &job)?;
        }
        Commands::OpsReport {
            out,
            bench_profile,
            ops_config,
        } => {
            run_ops_report(&out, &bench_profile, &ops_config)?;
        }
        Commands::PromServe { config, once } => {
            run_prom_serve(&config, once)?;
        }
        Commands::FaultDrill { profile } => {
            run_fault_drill(&profile)?;
        }
        Commands::GpuInfo => {
            run_gpu_info()?;
        }
        Commands::SmokeCuda {
            profile,
            cuda_config,
        } => {
            run_smoke_cuda(&profile, &cuda_config)?;
        }
        Commands::FpgaEmit {
            transport,
            net_config,
            shm_config,
            timing,
            symbols,
            interval_us,
            stream_id,
        } => {
            run_fpga_emit(
                &transport,
                &net_config,
                &shm_config,
                &timing,
                symbols,
                interval_us,
                stream_id,
            )?;
        }
        Commands::HostRecv {
            transport,
            net_config,
            shm_config,
            deadline,
            nic_config,
            gpu_ring,
            symbols,
            warmup_ms,
            stream_id,
            use_cuda,
            cuda_config,
        } => {
            run_host_recv(
                &transport,
                &net_config,
                &shm_config,
                &deadline,
                &nic_config,
                &gpu_ring,
                symbols,
                warmup_ms,
                stream_id,
                use_cuda,
                &cuda_config,
            )?;
        }
        Commands::ShmPrepare { shm_config } => {
            run_shm_prepare(&shm_config)?;
        }
    }
    Ok(())
}

fn run_smoke(profile: &PathBuf) -> Result<()> {
    use aether_transport::{
        LinkManager, SimTransportEngine, StreamConfig, StreamManager, TransportEngine,
    };
    use aether_types::StreamId;
    use cx5_emulator::{Cx5Nic, PacketIO};
    use fpga_emulator::{FpgaEmulator, RadioTimingConfig};
    use gpu_runtime::GpuRingBuffer;
    use metrics_engine::{
        taxonomy, EventLogger, HealthManager, HealthThresholds, LogEvent, MetricsBackend,
        MetricsEngine, ObservabilityConfig, RecoveryPolicy, TraceEngine, TraceStage,
    };

    let _profile_text = fs::read_to_string(profile)
        .with_context(|| format!("read profile {}", profile.display()))?;

    let ops = ObservabilityConfig::load_path("configs/ops/observability.yaml")
        .context("load configs/ops/observability.yaml")?;
    let mut events = EventLogger::create(&ops.logging.events_path)?;
    let recovery =
        RecoveryPolicy::load_path(&ops.recovery.policy_path).context("load recovery policy")?;
    let _ = events.emit(
        &LogEvent::now(taxonomy::RUNTIME_STARTED)
            .with_component("cli")
            .with_detail(format!("smoke recovery={}", recovery.id)),
    );

    let mut trace = TraceEngine::from_config(
        true, // smoke always traces one packet for observability gate
        ops.trace.ring_capacity.min(64),
        &ops.trace.export_path,
    );

    let timing = RadioTimingConfig::from_yaml_str(
        &fs::read_to_string("configs/radio_timing.yaml")
            .context("read configs/radio_timing.yaml")?,
    )?;
    let deadline_yaml = fs::read_to_string("configs/transport_deadline.yaml")
        .context("read configs/transport_deadline.yaml")?;
    let mut fpga = FpgaEmulator::new(timing, StreamId(1));
    let mut transport = SimTransportEngine::from_yaml(&deadline_yaml)?;
    transport.link_up()?;
    let _ = events.emit(&LogEvent::now(taxonomy::LINK_UP).with_component("transport"));
    transport.create_stream(StreamConfig {
        stream_id: StreamId(1),
        carrier: 0,
        antenna: 0,
        qos: 0,
        deadline_ns: 10_000,
    })?;
    let _ = events.emit(
        &LogEvent::now(taxonomy::STREAM_CREATE)
            .with_component("transport")
            .with_stream(1),
    );
    transport.start_stream(StreamId(1))?;

    let packet = fpga.emit_symbol();
    let tid = trace.start(
        packet.stream_id.0,
        packet.sequence.0,
        TraceStage::FpgaTx,
        packet.timestamp.0,
    );
    let mut now = packet.timestamp.0;
    transport.now_ns = now + 10_000;
    transport.ingest(packet)?;
    let packet = transport
        .receive()?
        .context("expected ingested packet on receive")?;
    trace.stamp(tid, TraceStage::Cx5Rx, now + 10_000);

    let mut nic = Cx5Nic::from_yaml(
        &fs::read_to_string("configs/nic_dma.yaml").context("read configs/nic_dma.yaml")?,
    )?;
    nic.advance_time(now);
    nic.submit_rx(packet)?;
    now += nic.dma_latency_ns();
    nic.advance_time(now);
    trace.stamp(tid, TraceStage::DmaDone, now);

    let mut ring = GpuRingBuffer::from_yaml(
        &fs::read_to_string("configs/gpu_ring.yaml").context("read configs/gpu_ring.yaml")?,
    )?;
    let mut metrics = MetricsEngine::new();
    metrics.set_link_up(true, 100.0);

    for pkt in nic.rx_burst(32) {
        metrics.record_rx_bytes(pkt.payload.len() as u64);
        metrics.record_symbol();
        trace.stamp(tid, TraceStage::GpuEnqueue, now);
        let latency = ring.process_packet(&pkt.payload, now)?;
        now += latency;
        metrics.record_kernel_ns(latency);
        metrics.record_tx_bytes(pkt.payload.len() as u64);
        metrics.record_latency_sample(latency);
        trace.stamp(tid, TraceStage::CudaDone, now);
        let _ = events.emit(
            &LogEvent::now(taxonomy::PACKET_RX)
                .with_component("gpu_ring")
                .with_stream(pkt.stream_id.0)
                .with_sequence(pkt.sequence.0)
                .with_latency_us(latency as f64 / 1_000.0),
        );
    }

    let health_thr = HealthThresholds::load_path(&ops.health.policy_path).unwrap_or_default();
    let mut health = HealthManager::new(health_thr);
    let _ = health.evaluate(&metrics.layered_snapshot(), Some(&mut events));
    let _ = events.flush();
    let exported = trace.export_configured().unwrap_or(0);

    println!(
        "smoke ok: rx={} tx={} now_ns={} seq_gaps={} late={} health={} recovery={} traces_exported={} metrics={}",
        metrics.snapshot().link.rx_packets,
        metrics.snapshot().link.tx_packets,
        now,
        transport.sequence_gaps,
        transport.late_packets,
        health.state().as_str(),
        recovery.id,
        exported,
        metrics.to_json()
    );
    println!("layered: {}", metrics.to_layered_json());
    println!("events: {}", events.path);
    Ok(())
}

fn run_ops_status(ops_config: &PathBuf) -> Result<()> {
    use metrics_engine::{HealthThresholds, ObservabilityConfig};

    let ops = ObservabilityConfig::load_path(ops_config)
        .with_context(|| format!("load {}", ops_config.display()))?;
    let health = HealthThresholds::load_path(&ops.health.policy_path)
        .with_context(|| format!("load {}", ops.health.policy_path))?;
    println!("ops config: {}", ops.id);
    println!(
        "metrics.enabled={} export_json={} prometheus_text={}",
        ops.metrics.enabled, ops.metrics.export_json, ops.metrics.export_prometheus_text
    );
    println!(
        "logging.events_path={} level={}",
        ops.logging.events_path, ops.logging.default_level
    );
    println!(
        "trace.enabled={} ring={} export={}",
        ops.trace.enabled, ops.trace.ring_capacity, ops.trace.export_path
    );
    println!(
        "health.max_latency_p99_ns={} max_seq_gap={}",
        health.max_latency_p99_ns, health.max_seq_gap_per_window
    );
    println!("recovery.policy_path={}", ops.recovery.policy_path);
    println!(
        "prometheus_scrape.config_path={}",
        ops.prometheus_scrape.config_path
    );
    Ok(())
}

fn run_soak(profile_path: &PathBuf) -> Result<()> {
    use benchmark::{SoakProfile, SoakRunner};

    let profile = SoakProfile::load_path(profile_path)
        .with_context(|| format!("load {}", profile_path.display()))?;
    match SoakRunner::new(profile).with_base_dir(".").run() {
        Ok(report) => {
            println!("{}", serde_json::to_string_pretty(&report)?);
            println!(
                "soak ok: profile={} packets={}",
                report.profile_id, report.bench.packets
            );
            Ok(())
        }
        Err(benchmark::SoakError::Failed(report)) => {
            println!("{}", serde_json::to_string_pretty(&*report)?);
            anyhow::bail!("soak failed: profile={}", report.profile_id);
        }
        Err(e) => Err(e.into()),
    }
}

fn run_prom_dump(out: &PathBuf, bench_profile: &PathBuf, job: &str) -> Result<()> {
    use benchmark::{BenchProfile, PipelineBench};
    use metrics_engine::{render_prometheus_text, HealthManager, HealthThresholds};

    let mut profile = BenchProfile::load_path(bench_profile)
        .with_context(|| format!("load {}", bench_profile.display()))?;
    profile.symbol_count = profile.symbol_count.min(32);
    let (report, metrics) = PipelineBench::new(profile)
        .with_base_dir(".")
        .run()
        .context("bench for prom-dump")?;
    let thr = HealthThresholds::load_path("configs/ops/health_policy.yaml").unwrap_or_default();
    let mut health = HealthManager::new(thr);
    let _ = health.evaluate(&metrics.layered_snapshot(), None);
    let text = render_prometheus_text(&metrics.layered_snapshot(), Some(health.state()), job);
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, &text).with_context(|| format!("write {}", out.display()))?;
    println!(
        "prom-dump ok: packets={} health={} out={}",
        report.packets,
        health.state().as_str(),
        out.display()
    );
    Ok(())
}

fn run_ops_report(out: &PathBuf, bench_profile: &PathBuf, ops_config: &PathBuf) -> Result<()> {
    use benchmark::{BenchProfile, PipelineBench};
    use metrics_engine::{
        render_prometheus_text, HealthManager, HealthThresholds, ObservabilityConfig,
        RecoveryPolicy,
    };
    use serde_json::json;

    let ops = ObservabilityConfig::load_path(ops_config)
        .with_context(|| format!("load {}", ops_config.display()))?;
    let recovery = RecoveryPolicy::load_path(&ops.recovery.policy_path)
        .with_context(|| format!("load {}", ops.recovery.policy_path))?;
    let mut profile = BenchProfile::load_path(bench_profile)
        .with_context(|| format!("load {}", bench_profile.display()))?;
    profile.symbol_count = profile.symbol_count.min(32);
    let (report, metrics) = PipelineBench::new(profile)
        .with_base_dir(".")
        .run()
        .context("bench for ops-report")?;
    let thr = HealthThresholds::load_path(&ops.health.policy_path).unwrap_or_default();
    let mut health = HealthManager::new(thr);
    let _ = health.evaluate(&metrics.layered_snapshot(), None);
    let prom = render_prometheus_text(
        &metrics.layered_snapshot(),
        Some(health.state()),
        "aether-sim",
    );
    let body = json!({
        "ops_config_id": ops.id,
        "recovery_policy_id": recovery.id,
        "health": health.state().as_str(),
        "trace_enabled": ops.trace.enabled,
        "bench": report,
        "layered_metrics": metrics.layered_snapshot(),
        "prometheus_text_bytes": prom.len(),
    });
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, serde_json::to_string_pretty(&body)?)
        .with_context(|| format!("write {}", out.display()))?;
    println!(
        "ops-report ok: health={} packets={} out={}",
        health.state().as_str(),
        body["bench"]["packets"],
        out.display()
    );
    Ok(())
}

fn run_prom_serve(config_path: &PathBuf, once: bool) -> Result<()> {
    use benchmark::{BenchProfile, PipelineBench};
    use metrics_engine::{render_prometheus_text, HealthManager, HealthThresholds};
    use serde::Deserialize;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[derive(Debug, Deserialize)]
    struct PromScrapeConfig {
        bind: String,
        #[serde(default = "default_metrics_path")]
        path: String,
        #[serde(default = "default_true")]
        refresh_on_scrape: bool,
        #[serde(default = "default_bench")]
        bench_profile: String,
        #[serde(default = "default_symbols")]
        symbol_count: u64,
        #[serde(default = "default_job")]
        job: String,
    }
    fn default_metrics_path() -> String {
        "/metrics".into()
    }
    fn default_true() -> bool {
        true
    }
    fn default_bench() -> String {
        "configs/bench_profile.yaml".into()
    }
    fn default_symbols() -> u64 {
        8
    }
    fn default_job() -> String {
        "aether-sim".into()
    }

    let text = fs::read_to_string(config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    let cfg: PromScrapeConfig =
        serde_yaml::from_str(&text).context("parse prometheus_scrape.yaml")?;

    let render = || -> Result<String> {
        let mut profile = BenchProfile::load_path(&cfg.bench_profile)
            .with_context(|| format!("load {}", cfg.bench_profile))?;
        profile.symbol_count = cfg.symbol_count;
        let (_report, metrics) = PipelineBench::new(profile)
            .with_base_dir(".")
            .run()
            .context("bench for prom-serve")?;
        let thr = HealthThresholds::load_path("configs/ops/health_policy.yaml").unwrap_or_default();
        let mut health = HealthManager::new(thr);
        let _ = health.evaluate(&metrics.layered_snapshot(), None);
        Ok(render_prometheus_text(
            &metrics.layered_snapshot(),
            Some(health.state()),
            &cfg.job,
        ))
    };

    let mut cached = render()?;
    let listener = TcpListener::bind(&cfg.bind).with_context(|| format!("bind {}", cfg.bind))?;
    println!(
        "prom-serve listening on http://{}{} (once={})",
        cfg.bind, cfg.path, once
    );

    for stream in listener.incoming() {
        let mut stream = stream.context("accept")?;
        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]);
        let is_metrics = req.starts_with("GET ")
            && (req.contains(&format!(" {} ", cfg.path))
                || req.contains(&format!(" {}?", cfg.path))
                || req.contains(&format!(" {} HTTP", cfg.path)));
        if is_metrics {
            if cfg.refresh_on_scrape {
                cached = render()?;
            }
            let body = cached.as_bytes();
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes())?;
            stream.write_all(body)?;
            if once {
                println!("prom-serve once: served /metrics then exit");
                break;
            }
        } else {
            let body = b"use GET /metrics\n";
            let header = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body);
        }
    }
    Ok(())
}

fn run_fault_drill(profile_path: &PathBuf) -> Result<()> {
    use benchmark::{FaultDrillProfile, FaultDrillRunner};

    let profile = FaultDrillProfile::load_path(profile_path)
        .with_context(|| format!("load {}", profile_path.display()))?;
    let report = FaultDrillRunner::new(profile)
        .with_base_dir(".")
        .run()
        .context("fault-drill")?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.passed {
        anyhow::bail!(
            "fault-drill did not observe fault/recovery signals (packets={})",
            report.bench.packets
        );
    }
    println!(
        "fault-drill ok: recovery_actions={} health={}",
        report.recovery_actions, report.health
    );
    Ok(())
}

fn run_bench(profile_path: &PathBuf) -> Result<()> {
    use benchmark::{BenchProfile, PipelineBench};

    let profile = BenchProfile::load_path(profile_path)
        .with_context(|| format!("load {}", profile_path.display()))?;
    let bench = PipelineBench::new(profile).with_base_dir(".");
    let (report, metrics) = bench.run().context("pipeline bench")?;
    let out = bench.write_report(&report).context("write report")?;
    info!(
        event = "bench_complete",
        packets = report.packets,
        p50_ns = report.latency.p50_ns,
        gbps = report.throughput.gbps,
        path = %out.display(),
        "bench ok"
    );
    println!("{}", report.to_json());
    println!("report written: {}", out.display());
    println!("metrics: {}", metrics.to_json());
    Ok(())
}

fn run_accept(profile_path: &PathBuf) -> Result<()> {
    use benchmark::{AcceptanceProfile, AcceptanceRunner};

    let profile = AcceptanceProfile::load_path(profile_path)
        .with_context(|| format!("load {}", profile_path.display()))?;
    match AcceptanceRunner::new(profile).with_base_dir(".").run() {
        Ok(report) => {
            println!("{}", serde_json::to_string_pretty(&report)?);
            println!("accept ok: profile={}", report.profile_id);
            Ok(())
        }
        Err(benchmark::AcceptanceError::Failed(report)) => {
            println!("{}", serde_json::to_string_pretty(&*report)?);
            anyhow::bail!("acceptance failed: profile={}", report.profile_id);
        }
        Err(e) => Err(e.into()),
    }
}

fn run_gpu_info() -> Result<()> {
    #[cfg(feature = "cuda")]
    {
        let devices = gpu_cuda::probe_devices().context("probe CUDA devices")?;
        println!("cuda feature: enabled");
        for d in devices {
            println!("{d}");
        }
        Ok(())
    }
    #[cfg(not(feature = "cuda"))]
    {
        anyhow::bail!(
            "CUDA support not compiled in. Rebuild with: cargo run -p aether-radio-cli --features cuda -- gpu-info"
        )
    }
}

fn run_smoke_cuda(profile: &PathBuf, cuda_config: &PathBuf) -> Result<()> {
    #[cfg(feature = "cuda")]
    {
        use aether_transport::{
            LinkManager, SimTransportEngine, StreamConfig, StreamManager, TransportEngine,
        };
        use aether_types::StreamId;
        use cx5_emulator::{Cx5Nic, PacketIO};
        use fpga_emulator::{FpgaEmulator, RadioTimingConfig};
        use gpu_cuda::CudaGpu;
        use gpu_runtime::{GpuBackend, GpuRingBuffer};
        use metrics_engine::MetricsEngine;
        use std::time::Duration;

        let _profile_text = fs::read_to_string(profile)
            .with_context(|| format!("read profile {}", profile.display()))?;
        let cuda_yaml = fs::read_to_string(cuda_config)
            .with_context(|| format!("read {}", cuda_config.display()))?;

        let mut gpu = CudaGpu::from_yaml(&cuda_yaml).context("init CudaGpu")?;
        println!("{}", gpu.info_line());

        let timing = RadioTimingConfig::from_yaml_str(
            &fs::read_to_string("configs/radio_timing.yaml")
                .context("read configs/radio_timing.yaml")?,
        )?;
        let deadline_yaml = fs::read_to_string("configs/transport_deadline.yaml")
            .context("read configs/transport_deadline.yaml")?;
        let mut fpga = FpgaEmulator::new(timing, StreamId(1));
        let mut transport = SimTransportEngine::from_yaml(&deadline_yaml)?;
        transport.link_up()?;
        transport.create_stream(StreamConfig {
            stream_id: StreamId(1),
            carrier: 0,
            antenna: 0,
            qos: 0,
            deadline_ns: 10_000,
        })?;
        transport.start_stream(StreamId(1))?;

        let packet = fpga.emit_symbol();
        let mut now = packet.timestamp.0;
        transport.now_ns = now + 10_000;
        transport.ingest(packet)?;
        let packet = transport
            .receive()?
            .context("expected ingested packet on receive")?;

        let mut nic = Cx5Nic::from_yaml(
            &fs::read_to_string("configs/nic_dma.yaml").context("read configs/nic_dma.yaml")?,
        )?;
        nic.advance_time(now);
        nic.submit_rx(packet)?;
        now += nic.dma_latency_ns();
        nic.advance_time(now);

        let mut ring = GpuRingBuffer::from_yaml(
            &fs::read_to_string("configs/gpu_ring.yaml").context("read configs/gpu_ring.yaml")?,
        )?;
        let mut metrics = MetricsEngine::new();

        for pkt in nic.rx_burst(32) {
            metrics.record_rx();
            // Host ring state machine + real CUDA kernel on payload.
            let idx = ring.begin_receive(now)?;
            ring.complete_receive(idx, &pkt.payload)?;
            let (idx, _) = ring.begin_process(now)?;
            let processed = gpu
                .process_bytes(&pkt.payload)
                .context("cuda process_bytes")?;
            let kernel_ns = gpu.last_kernel_ns.unwrap_or(0);
            now += kernel_ns;
            let _ = ring.complete_process(idx, now)?;
            ring.release(idx)?;
            // Also exercise GpuBackend trait path.
            let buf = gpu.allocate_buffer(processed.len())?;
            gpu.launch_kernel(buf, Duration::ZERO)?;
            gpu.sync()?;
            metrics.record_tx();
            println!(
                "cuda packet: bytes={} kernel_ns={} device={}",
                processed.len(),
                kernel_ns,
                gpu.device_name
            );
        }

        println!(
            "smoke-cuda ok: rx={} tx={} now_ns={} seq_gaps={} late={} metrics={}",
            metrics.snapshot().link.rx_packets,
            metrics.snapshot().link.tx_packets,
            now,
            transport.sequence_gaps,
            transport.late_packets,
            metrics.to_json()
        );
        Ok(())
    }
    #[cfg(not(feature = "cuda"))]
    {
        let _ = (profile, cuda_config);
        anyhow::bail!(
            "CUDA support not compiled in. Rebuild with: cargo run -p aether-radio-cli --features cuda -- smoke-cuda"
        )
    }
}

fn run_fpga_emit(
    transport: &str,
    net_config: &PathBuf,
    shm_config: &PathBuf,
    timing_path: &PathBuf,
    symbols: u64,
    interval_us: u64,
    stream_id: u32,
) -> Result<()> {
    use aether_types::StreamId;
    use fpga_emulator::{FpgaEmulator, RadioTimingConfig};
    use std::thread;
    use std::time::Duration;

    let timing = RadioTimingConfig::from_yaml_str(
        &fs::read_to_string(timing_path)
            .with_context(|| format!("read {}", timing_path.display()))?,
    )?;
    let mut fpga = FpgaEmulator::new(timing, StreamId(stream_id));

    match transport.to_ascii_lowercase().as_str() {
        "udp" | "net" => {
            use net_io::{NetLinkConfig, NetPacketSink};
            let net_yaml = fs::read_to_string(net_config)
                .with_context(|| format!("read {}", net_config.display()))?;
            let net_cfg = NetLinkConfig::from_yaml_str(&net_yaml)?;
            let mut sink = NetPacketSink::bind(&net_cfg).context("bind FPGA net sink")?;
            for i in 0..symbols {
                let pkt = fpga.emit_symbol();
                sink.send_packet(&pkt)
                    .with_context(|| format!("send symbol {i}"))?;
                if interval_us > 0 {
                    thread::sleep(Duration::from_micros(interval_us));
                }
            }
            println!(
                "fpga-emit ok: transport=udp sent={} peer={}",
                sink.sent, net_cfg.peer_addr
            );
        }
        "shm" => {
            use shm_io::{ShmLinkConfig, ShmPacketSink};
            let yaml = fs::read_to_string(shm_config)
                .with_context(|| format!("read {}", shm_config.display()))?;
            let mut cfg = ShmLinkConfig::from_yaml_str(&yaml)?;
            // Do not truncate an existing ring (host may already be attached).
            cfg.create = !std::path::Path::new(&cfg.path).exists();
            let mut sink = ShmPacketSink::open(&cfg).context("open shm sink")?;
            for i in 0..symbols {
                let pkt = fpga.emit_symbol();
                sink.send_packet(&pkt)
                    .with_context(|| format!("shm send symbol {i}"))?;
                if interval_us > 0 {
                    thread::sleep(Duration::from_micros(interval_us));
                }
            }
            println!(
                "fpga-emit ok: transport=shm sent={} path={}",
                sink.pushed(),
                cfg.path
            );
        }
        other => anyhow::bail!("unknown --transport {other} (use udp or shm)"),
    }
    Ok(())
}

fn run_shm_prepare(shm_config: &PathBuf) -> Result<()> {
    use shm_io::{ShmLinkConfig, ShmPacketSink};
    let yaml =
        fs::read_to_string(shm_config).with_context(|| format!("read {}", shm_config.display()))?;
    let mut cfg = ShmLinkConfig::from_yaml_str(&yaml)?;
    cfg.create = true;
    let _sink = ShmPacketSink::open(&cfg)?;
    println!("shm-prepare ok: path={}", cfg.path);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_host_recv(
    transport: &str,
    net_config: &PathBuf,
    shm_config: &PathBuf,
    deadline_path: &PathBuf,
    nic_path: &PathBuf,
    gpu_ring_path: &PathBuf,
    symbols: u64,
    warmup_ms: u64,
    stream_id: u32,
    use_cuda: bool,
    cuda_config: &PathBuf,
) -> Result<()> {
    use aether_transport::{
        LinkManager, SimTransportEngine, StreamConfig, StreamManager, TransportEngine,
    };
    use aether_types::StreamId;
    use cx5_emulator::{Cx5Nic, PacketIO};
    use gpu_runtime::GpuRingBuffer;
    use metrics_engine::{EventLogger, LogEvent, MetricsEngine};
    use std::thread;
    use std::time::{Duration, Instant};

    let mut transport_eng = SimTransportEngine::from_yaml(
        &fs::read_to_string(deadline_path)
            .with_context(|| format!("read {}", deadline_path.display()))?,
    )?;
    transport_eng.link_up()?;
    let sid = StreamId(stream_id);
    transport_eng.create_stream(StreamConfig {
        stream_id: sid,
        carrier: 0,
        antenna: 0,
        qos: 0,
        deadline_ns: 50_000_000,
    })?;
    transport_eng.start_stream(sid)?;

    let mut cx5 = Cx5Nic::from_yaml(
        &fs::read_to_string(nic_path).with_context(|| format!("read {}", nic_path.display()))?,
    )?;
    let mut ring = GpuRingBuffer::from_yaml(
        &fs::read_to_string(gpu_ring_path)
            .with_context(|| format!("read {}", gpu_ring_path.display()))?,
    )?;
    let mut metrics = MetricsEngine::new();
    let ops = metrics_engine::ObservabilityConfig::load_path("configs/ops/observability.yaml").ok();
    let events_path = ops
        .as_ref()
        .map(|o| o.logging.events_path.clone())
        .unwrap_or_else(|| "data/reports/host_recv_events.jsonl".into());
    let mut events = EventLogger::create(&events_path)?;
    let _ = events.emit(
        &LogEvent::now(metrics_engine::taxonomy::RUNTIME_STARTED)
            .with_component("host-recv")
            .with_detail(transport.to_string()),
    );

    #[cfg(feature = "cuda")]
    let mut cuda_gpu = if use_cuda {
        let yaml = fs::read_to_string(cuda_config)
            .with_context(|| format!("read {}", cuda_config.display()))?;
        Some(gpu_cuda::CudaGpu::from_yaml(&yaml).context("init CudaGpu")?)
    } else {
        None
    };
    #[cfg(not(feature = "cuda"))]
    let cuda_gpu: Option<()> = {
        let _ = cuda_config;
        if use_cuda {
            anyhow::bail!("--use-cuda requires `--features cuda`");
        }
        None
    };

    enum RxBackend {
        Udp(net_io::NetPacketIO),
        Shm(shm_io::ShmPacketIO),
    }

    let mut rx = match transport.to_ascii_lowercase().as_str() {
        "udp" | "net" => {
            let net_yaml = fs::read_to_string(net_config)
                .with_context(|| format!("read {}", net_config.display()))?;
            let net_cfg = net_io::NetLinkConfig::from_yaml_str(&net_yaml)?;
            RxBackend::Udp(net_io::NetPacketIO::bind(&net_cfg)?)
        }
        "shm" => {
            let yaml = fs::read_to_string(shm_config)
                .with_context(|| format!("read {}", shm_config.display()))?;
            let mut cfg = shm_io::ShmLinkConfig::from_yaml_str(&yaml)?;
            cfg.create = false;
            RxBackend::Shm(shm_io::ShmPacketIO::open(&cfg)?)
        }
        other => anyhow::bail!("unknown --transport {other} (use udp or shm)"),
    };

    if warmup_ms > 0 {
        thread::sleep(Duration::from_millis(warmup_ms));
    }

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut accepted = 0u64;
    let mut now = 0u64;
    let mut udp_rx = 0u64;
    let mut decode_err = 0u64;

    while accepted < symbols && Instant::now() < deadline {
        let burst = match &mut rx {
            RxBackend::Udp(n) => {
                let pkts = n.rx_burst(32);
                udp_rx = n.received;
                decode_err = n.decode_errors;
                pkts
            }
            RxBackend::Shm(s) => s.rx_burst(32),
        };
        for pkt in burst {
            transport_eng.now_ns = pkt.timestamp.0.saturating_add(1_000_000);
            match transport_eng.ingest(pkt.clone()) {
                Ok(()) => {}
                Err(aether_transport::TransportError::SequenceGap { got, .. }) => {
                    transport_eng.recover_sequence(sid, got);
                    metrics.record_sequence_gap();
                    if transport_eng.ingest(pkt.clone()).is_err() {
                        continue;
                    }
                }
                Err(aether_transport::TransportError::LatePacket { .. }) => {
                    metrics.record_deadline_miss();
                    metrics.record_late_packet();
                    continue;
                }
                Err(_) => continue,
            }
            if let Some(ingested) = transport_eng.receive()? {
                cx5.advance_time(now);
                cx5.submit_rx(ingested.clone())?;
                now = now.saturating_add(cx5.dma_latency_ns());
                cx5.advance_time(now);
                for delivered in cx5.rx_burst(32) {
                    metrics.record_rx();
                    #[cfg(feature = "cuda")]
                    if let Some(gpu) = cuda_gpu.as_mut() {
                        let _ = gpu.process_bytes(&delivered.payload)?;
                        now = now.saturating_add(gpu.last_kernel_ns.unwrap_or(0));
                    } else {
                        let lat = ring.process_packet(&delivered.payload, now)?;
                        now = now.saturating_add(lat);
                    }
                    #[cfg(not(feature = "cuda"))]
                    {
                        let _ = cuda_gpu;
                        let lat = ring.process_packet(&delivered.payload, now)?;
                        now = now.saturating_add(lat);
                    }
                    let e2e = now.saturating_sub(delivered.timestamp.0);
                    let _ = events.emit(
                        &LogEvent::now(metrics_engine::taxonomy::PACKET_RX)
                            .with_component("host-recv")
                            .with_stream(delivered.stream_id.0)
                            .with_sequence(delivered.sequence.0)
                            .with_latency_us(e2e as f64 / 1_000.0),
                    );
                    metrics.record_tx();
                    accepted += 1;
                    if accepted >= symbols {
                        break;
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    let _ = events.flush();

    println!(
        "host-recv ok: transport={} accepted={}/{} udp_rx={} decode_err={} seq_gaps={} late={} metrics={}",
        transport,
        accepted,
        symbols,
        udp_rx,
        decode_err,
        transport_eng.sequence_gaps,
        transport_eng.late_packets,
        metrics.to_json()
    );
    if accepted < symbols {
        anyhow::bail!("timed out: accepted {accepted}/{symbols}");
    }
    Ok(())
}
