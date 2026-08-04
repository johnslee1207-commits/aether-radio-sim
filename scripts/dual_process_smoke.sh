#!/usr/bin/env bash
# Dual-process localhost smoke (no Docker required).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo build -p aether-radio-cli

HOST_LOG=$(mktemp)
cargo run -p aether-radio-cli -- host-recv \
  --net-config configs/backends/net_link_host_local.yaml \
  --symbols 16 \
  --warmup-ms 300 >"$HOST_LOG" 2>&1 &
HOST_PID=$!

cleanup() { kill "$HOST_PID" 2>/dev/null || true; }
trap cleanup EXIT

sleep 0.5
cargo run -p aether-radio-cli -- fpga-emit \
  --net-config configs/backends/net_link_fpga_local.yaml \
  --symbols 16 \
  --interval-us 200

wait "$HOST_PID" || true
echo "---- host-recv log ----"
cat "$HOST_LOG"
echo "dual-process smoke done"
