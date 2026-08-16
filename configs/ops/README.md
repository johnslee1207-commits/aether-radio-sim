# Ops plane configuration (Observability & Operations Framework v1.0)

Policies for metrics export, logging, tracing, health, recovery, and scrape live here.

| File | Role |
|------|------|
| `observability.yaml` | Master ops knobs + refs |
| `health_policy.yaml` | Health thresholds |
| `recovery_policy.yaml` | Fault-class → recovery action |
| `prometheus_scrape.yaml` | CLI `prom-serve` bind (host-local `127.0.0.1`) |
| `prometheus_scrape_compose.yaml` | Compose `prom-serve` bind (`0.0.0.0`) |
| `prometheus.yml` | Sample Prometheus scrape config (host-local) |
| `prometheus_compose.yml` | Prometheus scrape for Compose (host.docker.internal:9898) |
| `prometheus_compose_full.yml` | Prometheus scrape for `--profile full` (prom-serve:9898) |

Canonical doc: `docs/AETHER_RADIO_OBSERVABILITY_OPS_FRAMEWORK_v1.0.md`

Do not put secrets or machine-absolute paths in these files.
