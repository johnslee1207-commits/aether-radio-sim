# Aether Radio Data Plane Simulation Platform

**Version:** v1.1 (crate `0.1.1`) · **Ops plane:** Sprint O O001–O025

Software simulation of Aether Radio Transport, eCPRI-like deterministic streaming, CX5/DPDK/GPUDirect-style data paths, and GPU/CPU memory models — without Xilinx FPGA, Mellanox CX5, or GPU servers.

Observability (metrics / events / trace / health / recovery) is first-class. Policies live under `configs/ops/`. Specs:

- Development: [`docs/CURSOR_DEVELOPMENT_SPEC_v1.1.md`](docs/CURSOR_DEVELOPMENT_SPEC_v1.1.md)
- Ops framework: [`docs/AETHER_RADIO_OBSERVABILITY_OPS_FRAMEWORK_v1.0.md`](docs/AETHER_RADIO_OBSERVABILITY_OPS_FRAMEWORK_v1.0.md)
- Coverage: [`data/architecture/observability_coverage_matrix.json`](data/architecture/observability_coverage_matrix.json)
- Agent rules: [`AGENTS.md`](AGENTS.md)

## Requirements

- Rust stable (1.75+ recommended)
- Cargo
- Optional CUDA path: WSL2 + NVIDIA driver (RTX 4050 validated) and/or Docker with NVIDIA Container Toolkit

## Quick start

```bash
cargo test --workspace
cargo run -p aether-radio-cli -- info
cargo run -p aether-radio-cli -- validate-config --path configs/ethernet_model.yaml
cargo run -p aether-radio-cli -- smoke --profile configs/simulation_profile.yaml
cargo run -p aether-radio-cli -- bench --profile configs/bench_profile.yaml
cargo run -p aether-radio-cli -- accept
```

### Observability & ops CLI

```bash
cargo run -p aether-radio-cli -- ops-status
cargo run -p aether-radio-cli -- prom-dump
cargo run -p aether-radio-cli -- ops-report
cargo run -p aether-radio-cli -- soak --profile configs/soak_profile_ci.yaml
cargo run -p aether-radio-cli -- fault-drill
# Local Prometheus scrape (bind from configs/ops/prometheus_scrape.yaml):
cargo run -p aether-radio-cli -- prom-serve --once
# cargo run -p aether-radio-cli -- prom-serve   # listen until Ctrl-C
# Optional: prometheus --config.file=configs/ops/prometheus.yml
```

| Command | Purpose |
|---------|---------|
| `ops-status` | Print ops config (metrics / log / trace / health / recovery) |
| `prom-dump` | Run short bench → write Prometheus text file |
| `ops-report` | Consolidated JSON (bench + health + layered metrics) |
| `prom-serve` | HTTP `GET /metrics` scrape endpoint (CLI-only sockets) |
| `soak` | L4 soak/stress gates (multi-round health poll) |
| `fault-drill` | Stress faults + recovery policy exercise |
| `accept` | Ethernet + PipelineBench SLA gates |

## CUDA on WSL2 + Docker (RTX 4050)

Default builds stay simulation-only (no CUDA link). Real GPU uses feature `cuda`.

### Native WSL2 (recommended for day-to-day)

Prereqs: Windows NVIDIA driver with WSL GPU, `nvidia-smi` works inside WSL.

**Important:** export the WSL NVIDIA driver path before running CUDA binaries:

```bash
export LD_LIBRARY_PATH=/usr/lib/wsl/lib:$LD_LIBRARY_PATH
```

```bash
# inside WSL
cd /mnt/d/Projects/aether-radio-sim
./scripts/wsl_cuda_smoke.sh

# or manually:
cargo run -p aether-radio-cli --features cuda -- gpu-info
cargo run -p aether-radio-cli --features cuda -- smoke-cuda
cargo test -p gpu-cuda --features cuda
```

Config: `configs/backends/gpu_cuda.yaml`, profile `configs/simulation_profile_cuda.yaml`.

Note: host toolkit may be CUDA 11.5 while Ada is sm_89. Checked-in PTX targets `sm_80` and JITs on RTX 4050. Prefer regenerating PTX with CUDA 12 Docker (`./scripts/build_cuda_ptx.sh`) when available. Evidence pack for PipelineBench CUDA: `.aetheros/evidence/cuda_wsl/bench_cuda_summary.json` (via `./scripts/wsl_bench_cuda.sh`).

### Docker (CUDA 12 devel)

```bash
# WSL or Windows with Docker Desktop + WSL2 backend + GPU support
docker compose -f docker-compose.cuda.yml up --build
```

Image: `Dockerfile.cuda` (`nvidia/cuda:12.4.1-devel-ubuntu22.04`).

## Repository layout

```text
aether-radio-sim/
├── Cargo.toml                 # workspace
├── crates/                    # runtime crates (interfaces first)
├── configs/                   # simulation parameters (YAML data)
│   └── ops/                   # observability / health / recovery / scrape
├── data/
│   ├── architecture/          # data-layer + observability coverage registry
│   ├── ops/dashboards/        # Grafana JSON samples
│   └── reports/               # runtime artifacts (gitignored)
├── docs/                      # specs + ops framework
├── scripts/                   # dual-process / CUDA / Docker helpers
└── .github/workflows/ci.yml
```

## Configuration

Tunable models live under `configs/` (not hardcoded in Rust):

| File | Purpose |
|------|---------|
| `configs/simulation_profile.yaml` | MVP profile, backend selection |
| `configs/ethernet_model.yaml` | 100G bandwidth / latency / jitter / loss |
| `configs/nic_dma.yaml` | CX5 DMA latency and queue depths |
| `configs/fault_injection.yaml` | Loss / delay / GPU slowdown |
| `configs/fault_injection_stress.yaml` | Burst / skew stress for soak & drills |
| `configs/fault_drill.yaml` | Fault-drill harness profile |
| `configs/radio_timing.yaml` | Slot/symbol timing for FPGA scheduler |
| `configs/transport_deadline.yaml` | Sequence / late-packet policy |
| `configs/gpu_ring.yaml` | GPU ring slots / kernel delay |
| `configs/bench_profile.yaml` | E2E bench knobs, `gpu_backend`, events path |
| `configs/bench_profile_multistream.yaml` | Multi-stream bench (4 streams) |
| `configs/bench_profile_cuda.yaml` | PipelineBench with `gpu_backend: cuda` |
| `configs/acceptance_profile.yaml` | SLA gates for `accept` CLI |
| `configs/acceptance_profile_multistream.yaml` | Multi-stream acceptance gates |
| `configs/soak_profile.yaml` / `_ci.yaml` / `_wall.yaml` / `_wall_long.yaml` | L4 soak / CI-fast / ~15s wall / ~2min wall |
| `configs/memory_pool.yaml` | Host/GPU pool + H2D/D2H model |
| `configs/ops/observability.yaml` | Metrics / logging / trace / health refs |
| `configs/ops/health_policy.yaml` | Health thresholds |
| `configs/ops/recovery_policy.yaml` | Fault-class → recovery action |
| `configs/ops/prometheus_scrape.yaml` | `prom-serve` bind / refresh (host-local) |
| `configs/ops/prometheus_scrape_compose.yaml` | `prom-serve` bind for Compose (`0.0.0.0`) |
| `configs/ops/prometheus.yml` | Sample Prometheus scrape against `prom-serve` |
| `configs/ops/prometheus_compose.yml` | Prometheus scrape config for Compose ops stack |
| `data/ops/grafana/datasources/prometheus.yaml` | Grafana datasource provisioning sample (host) |
| `data/ops/grafana/datasources/prometheus_compose.yaml` | Grafana datasource for Compose |

| `configs/backends/gpu_cuda.yaml` | CUDA device / kernel policy (RTX 4050) |
| `configs/backends/shm_link.yaml` | Same-host shared-memory ring geometry |
| `configs/backends/shm_link_docker.yaml` | Docker named-volume shm path (`/shm/...`) |
| `configs/backends/dpdk.yaml` | DPDK mock/hardware contract |
| `configs/simulation_profile_cuda.yaml` | Profile selecting `backends.gpu: cuda` |

Data layer ownership: `data/architecture/data_classification_registry.json`.

## Dual-process / dual-container FPGA ↔ Host

```text
fpga-emit  --UDP or shm-->  host-recv
```

UDP (cross-container functional):

```bash
./scripts/dual_process_smoke.sh
# or:
cargo run -p aether-radio-cli -- host-recv --transport udp --net-config configs/backends/net_link_host_local.yaml --symbols 16
cargo run -p aether-radio-cli -- fpga-emit --transport udp --net-config configs/backends/net_link_fpga_local.yaml --symbols 16
```

Shared memory (same-host µs-oriented):

```bash
./scripts/shm_dual_smoke.sh
# or:
cargo run -p aether-radio-cli -- shm-prepare --shm-config configs/backends/shm_link.yaml
cargo run -p aether-radio-cli -- host-recv --transport shm --symbols 32
cargo run -p aether-radio-cli -- fpga-emit --transport shm --symbols 32 --interval-us 50
```

Docker Compose:

```bash
# Ops scrape stack (prom-serve on host + Prometheus/Grafana in Compose)
cargo run -p aether-radio-cli -- prom-serve --config configs/ops/prometheus_scrape_compose.yaml
docker compose -f docker-compose.ops.yml up -d
# http://127.0.0.1:9898/metrics  http://127.0.0.1:9090  http://127.0.0.1:3000
docker compose -f docker-compose.ops.yml down
# Optional in-compose exporter:
docker compose -f docker-compose.ops.yml -f docker-compose.ops.full.yml --profile full up --build -d
docker compose -f docker-compose.ops.yml -f docker-compose.ops.full.yml --profile full down

# UDP split containers
docker compose -f docker-compose.split.yml up --build
docker compose -f docker-compose.split.yml --profile cuda up --build

# Shared-volume shm (file ring at /shm; see configs/backends/shm_link_docker.yaml)
docker compose -f docker-compose.shm.yml up --build
# or: ./scripts/docker_shm_smoke.sh
```

Wall-clock soak evidence (not default CI):

```bash
cargo run -p aether-radio-cli -- soak --profile configs/soak_profile_wall.yaml
# ~2 minute wall evidence:
cargo run -p aether-radio-cli -- soak --profile configs/soak_profile_wall_long.yaml
```

Cross-container UDP is for **functional** integration. µs / 100G acceptance uses same-process bench or shared-memory.

## Bench + events

```bash
cargo run -p aether-radio-cli -- bench
cargo run -p aether-radio-cli -- bench --profile configs/bench_profile_multistream.yaml
cargo run -p aether-radio-cli -- accept
cargo run -p aether-radio-cli -- accept --profile configs/acceptance_profile_multistream.yaml
# JSONL events: data/reports/bench_events.jsonl (or profile events_path)
# Packet traces (when enabled): data/reports/traces/packet_traces.jsonl

# PipelineBench with real CUDA (WSL2 + NVIDIA; needs --features cuda):
export LD_LIBRARY_PATH=/usr/lib/wsl/lib:$LD_LIBRARY_PATH
./scripts/wsl_bench_cuda.sh
# or: cargo run -p aether-radio-cli --features cuda -- bench --profile configs/bench_profile_cuda.yaml
```

## Observability plane (summary)

| Pillar | Implementation |
|--------|----------------|
| Metrics | 5-layer `MetricsBackend` + Prometheus text / HTTP |
| Events | `EventLogger` taxonomy JSONL |
| Trace | `TraceEngine` stage stamps (FPGA→CUDA), default-on in ops YAML |
| Health | `HealthManager` + `health_policy.yaml` |
| Recovery | `RecoveryExecutor` + `recovery_policy.yaml` |
| Fault | `FaultInjector` (loss/burst/reorder/seq jump/skew) |
| Dashboard | Grafana sample under `data/ops/dashboards/` |

## Backend replaceability

Business code depends on traits only (no DPDK/CUDA/sockets in business crates):

- `PacketIO` → `SimPacketIO` / `NetPacketIO` (UDP) / `ShmPacketIO` / `DpdkPacketIO` (`backend: mock` or unavailable `hardware`)
- `MemoryBackend` → `SimMemory` / `PooledMemory` (capacity + H2D/D2H model)
- `GpuBackend` → `SimGpu` or `gpu-cuda::CudaGpu` (`--features cuda`)

## Development rules

See `AGENTS.md`. Sprint order: workspace → protocol → FPGA → transport → memory/GPU ring → benchmark → **Observability Plane (Sprint O, O001–O025 Done)**.

Program/data decoupling: put bandwidth, latency, loss, DMA delay, fault rates, health thresholds, and scrape binds in `configs/` / `configs/ops/` — not in Rust source.

## CI

GitHub Actions runs:

- `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace`
- CLI: `smoke`, `bench`, `accept` (+ multistream), `prom-dump`, `ops-report`, `soak` (CI multi-round), `fault-drill`, `prom-serve --once`
