#!/usr/bin/env bash
# Docker shared-volume shm dual-container smoke.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

docker compose -f docker-compose.shm.yml down --remove-orphans 2>/dev/null || true
docker compose -f docker-compose.shm.yml up --build --abort-on-container-exit --exit-code-from fpga-sim-shm
echo "docker shm smoke finished (inspect host-dataplane-shm logs for accepted count)"
