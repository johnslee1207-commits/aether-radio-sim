# Aether Radio Data Plane Simulation Platform

**Version:** v1.1 (crate `0.1.1`)

Software simulation of Aether Radio Transport, eCPRI-like deterministic streaming, CX5/DPDK/GPUDirect-style data paths, and GPU/CPU memory models — without Xilinx FPGA, Mellanox CX5, or GPU servers.

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
```

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

Note: host toolkit may be CUDA 11.5 while Ada is sm_89. Checked-in PTX targets `sm_80` and JITs on RTX 4050. Prefer regenerating PTX with CUDA 12 Docker (`./scripts/build_cuda_ptx.sh`) when available.

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
├── data/                      # profiles + data-layer registry
├── tests/                     # integration tests
├── docs/                      # development specification
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
| `configs/radio_timing.yaml` | Slot/symbol timing for FPGA scheduler |
| `configs/transport_deadline.yaml` | Sequence / late-packet policy |
| `configs/gpu_ring.yaml` | GPU ring slots / kernel delay |
| `configs/bench_profile.yaml` | E2E bench knobs, `gpu_backend`, events path |
| `configs/bench_profile_multistream.yaml` | Multi-stream bench (4 streams) |
| `configs/bench_profile_cuda.yaml` | PipelineBench with `gpu_backend: cuda` |
| `configs/backends/gpu_cuda.yaml` | CUDA device / kernel policy (RTX 4050) |
| `configs/backends/shm_link.yaml` | Same-host shared-memory ring geometry |
| `configs/backends/shm_link_docker.yaml` | Docker named-volume shm path (`/shm/...`) |
| `configs/backends/dpdk.yaml` | DPDK stub contract (unavailable until adapter crate) |
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
# UDP split containers
docker compose -f docker-compose.split.yml up --build
docker compose -f docker-compose.split.yml --profile cuda up --build

# Shared-volume shm (file ring at /shm; see configs/backends/shm_link_docker.yaml)
docker compose -f docker-compose.shm.yml up --build
# or: ./scripts/docker_shm_smoke.sh
```

Cross-container UDP is for **functional** integration. µs / 100G acceptance uses same-process bench or shared-memory.

## Bench + events

```bash
cargo run -p aether-radio-cli -- bench
cargo run -p aether-radio-cli -- bench --profile configs/bench_profile_multistream.yaml
# JSONL events: data/reports/bench_events.jsonl (or profile events_path)

# PipelineBench with real CUDA (WSL2 + NVIDIA; needs --features cuda):
export LD_LIBRARY_PATH=/usr/lib/wsl/lib:$LD_LIBRARY_PATH
./scripts/wsl_bench_cuda.sh
# or: cargo run -p aether-radio-cli --features cuda -- bench --profile configs/bench_profile_cuda.yaml
```

## Backend replaceability

Business code depends on traits only:

- `PacketIO` → `SimPacketIO` / `NetPacketIO` (UDP) / `ShmPacketIO` / `DpdkPacketIO` (stub, unavailable)
- `MemoryBackend` → `SimMemory`
- `GpuBackend` → `SimGpu` or `gpu-cuda::CudaGpu` (`--features cuda`)

## Development rules

See `AGENTS.md` and `docs/CURSOR_DEVELOPMENT_SPEC_v1.1.md`.

Sprint order: workspace → protocol → FPGA → transport → memory/GPU ring → benchmark.

## CI

GitHub Actions runs `fmt`, `clippy -D warnings`, `cargo test --workspace`, CLI smoke, and bench.
