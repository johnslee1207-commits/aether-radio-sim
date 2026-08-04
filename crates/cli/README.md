# aether-radio-cli

Control-plane CLI (`aether-radio-sim`). Uses tokio for control; data path stays sync.

```bash
cargo run -p aether-radio-cli -- info
cargo run -p aether-radio-cli -- validate-config
cargo run -p aether-radio-cli -- smoke
cargo run -p aether-radio-cli -- bench
cargo run -p aether-radio-cli -- accept
cargo run -p aether-radio-cli -- bench --profile configs/bench_profile_multistream.yaml

# Dual-process UDP / shared-memory
cargo run -p aether-radio-cli -- shm-prepare
cargo run -p aether-radio-cli -- host-recv --transport udp|shm --symbols 16
cargo run -p aether-radio-cli -- fpga-emit --transport udp|shm --symbols 16
./scripts/dual_process_smoke.sh
./scripts/shm_dual_smoke.sh
docker compose -f docker-compose.split.yml up --build
docker compose -f docker-compose.shm.yml up --build

# WSL2 + RTX 4050
export LD_LIBRARY_PATH=/usr/lib/wsl/lib:$LD_LIBRARY_PATH
cargo run -p aether-radio-cli --features cuda -- gpu-info
cargo run -p aether-radio-cli --features cuda -- smoke-cuda
./scripts/wsl_bench_cuda.sh
```
