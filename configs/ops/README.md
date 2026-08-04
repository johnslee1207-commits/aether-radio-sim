# Ops plane configuration (Observability & Operations Framework v1.0)

Policies for metrics export, logging, tracing, health, recovery, and scrape live here.

| File | Role |
|------|------|
| `observability.yaml` | Master ops knobs + refs |
| `health_policy.yaml` | Health thresholds |
| `recovery_policy.yaml` | Fault-class → recovery action |
| `prometheus_scrape.yaml` | CLI `prom-serve` bind |
| `prometheus.yml` | Sample Prometheus scrape config |

Canonical doc: `docs/AETHER_RADIO_OBSERVABILITY_OPS_FRAMEWORK_v1.0.md`

Do not put secrets or machine-absolute paths in these files.
