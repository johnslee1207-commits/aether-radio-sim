# metrics-engine

Observability plane (first-class): metrics, events, trace, health, recovery, Prometheus text.

Framework: `docs/AETHER_RADIO_OBSERVABILITY_OPS_FRAMEWORK_v1.0.md`

| Module | Role |
|--------|------|
| `MetricsBackend` / `MetricsEngine` | 5-layer counters + latency / memory |
| `EventLogger` / `taxonomy` | Structured JSONL (what happened) |
| `TraceEngine` | Packet stage stamps (FPGA→CUDA) |
| `HealthManager` | NORMAL…RECOVERY from policy YAML |
| `RecoveryExecutor` | Fault-class → recovery action from policy |
| `render_prometheus_text` | Prometheus exposition format |
| `ObservabilityConfig` | `configs/ops/observability.yaml` |

```bash
cargo test -p metrics-engine
cargo run -p aether-radio-cli -- ops-status
cargo run -p aether-radio-cli -- prom-dump
cargo run -p aether-radio-cli -- prom-serve --once
cargo run -p aether-radio-cli -- fault-drill
```
