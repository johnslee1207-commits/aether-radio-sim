# Grafana dashboards (optional)

Import `aether_radio_overview.json` into Grafana.

Metrics are produced by:

```bash
cargo run -p aether-radio-cli -- prom-dump --out data/reports/metrics.prom
# or live scrape:
cargo run -p aether-radio-cli -- prom-serve
# scrape http://127.0.0.1:9898/metrics (see configs/ops/prometheus_scrape.yaml)
```
