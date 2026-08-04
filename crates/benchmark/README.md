# benchmark

Latency percentiles, throughput (Gbps / GB/s / pps), `PipelineBench`, and `AcceptanceRunner` SLA gates.

Configs: `configs/bench_profile*.yaml`, `configs/acceptance_profile*.yaml`.

```bash
cargo test -p benchmark
cargo run -p aether-radio-cli -- bench
cargo run -p aether-radio-cli -- accept
```
