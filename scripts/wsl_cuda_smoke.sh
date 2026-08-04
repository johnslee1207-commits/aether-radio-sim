#!/usr/bin/env bash
# Native WSL2 smoke: requires NVIDIA driver in Windows + CUDA libs in WSL.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# WSL stores the NVIDIA user-mode driver here; required for libcuda.
export LD_LIBRARY_PATH="/usr/lib/wsl/lib:${LD_LIBRARY_PATH:-}"

echo "== GPU =="
nvidia-smi -L

echo "== Build with CUDA feature =="
cargo build -p aether-radio-cli --features cuda

echo "== gpu-info =="
cargo run -p aether-radio-cli --features cuda -- gpu-info

echo "== smoke-cuda =="
cargo run -p aether-radio-cli --features cuda -- smoke-cuda

echo "== gpu-cuda tests =="
cargo test -p gpu-cuda --features cuda -- --nocapture

echo "OK"
