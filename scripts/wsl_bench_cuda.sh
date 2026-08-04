#!/usr/bin/env bash
# PipelineBench with real CUDA GpuBackend (WSL2 / Linux + NVIDIA).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export LD_LIBRARY_PATH="/usr/lib/wsl/lib:${LD_LIBRARY_PATH:-}"
mkdir -p data/reports

PROFILE="${1:-configs/bench_profile_cuda.yaml}"
cargo run -p aether-radio-cli --features cuda --release -- bench --profile "$PROFILE"
echo "bench-cuda done: see report path in profile ($PROFILE)"
