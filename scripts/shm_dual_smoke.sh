#!/usr/bin/env bash
# Same-host shared-memory dual-process smoke (µs-oriented link).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p data/reports
rm -f data/reports/shm_link.bin

cargo build -p aether-radio-cli
cargo run -p aether-radio-cli -- shm-prepare --shm-config configs/backends/shm_link.yaml

HOST_LOG=$(mktemp)
cargo run -p aether-radio-cli -- host-recv \
  --transport shm \
  --shm-config configs/backends/shm_link.yaml \
  --deadline configs/transport_deadline_net.yaml \
  --symbols 32 \
  --warmup-ms 200 >"$HOST_LOG" 2>&1 &
HOST_PID=$!
cleanup() { kill "$HOST_PID" 2>/dev/null || true; }
trap cleanup EXIT

sleep 0.3
cargo run -p aether-radio-cli -- fpga-emit \
  --transport shm \
  --shm-config configs/backends/shm_link.yaml \
  --symbols 32 \
  --interval-us 50

wait "$HOST_PID" || true
echo "---- host-recv log ----"
cat "$HOST_LOG"
echo "shm dual-process smoke done"
