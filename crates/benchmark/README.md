# benchmark

Latency percentiles, throughput (Gbps / GB/s / pps), and `PipelineBench` E2E harness.

Config: `configs/bench_profile.yaml`. Report output: `data/reports/bench_last.json`.

```bash
cargo test -p benchmark
cargo run -p aether-radio-cli -- bench
```
