#!/usr/bin/env bash
# Compile phy_scale.cu to PTX for RTX 40-series (sm_89) using a CUDA 12 devel image.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/crates/gpu-cuda/kernels/phy_scale.ptx"
SRC="$ROOT/crates/gpu-cuda/kernels/phy_scale.cu"
IMAGE="${CUDA_DOCKER_IMAGE:-nvidia/cuda:12.4.1-devel-ubuntu22.04}"

echo "Building PTX with $IMAGE ..."
docker run --rm \
  -v "$ROOT:/src:ro" \
  -v "$(dirname "$OUT"):/out" \
  "$IMAGE" \
  nvcc -ptx -O3 -arch=sm_89 -o /out/phy_scale.ptx /src/crates/gpu-cuda/kernels/phy_scale.cu

echo "Wrote $OUT"
ls -la "$OUT"
