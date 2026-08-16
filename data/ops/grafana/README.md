# Grafana provisioning samples for Aether Radio Sim ops.

| Path | Role |
|------|------|
| `datasources/prometheus.yaml` | Host-local Prometheus datasource (`127.0.0.1:9090`) |
| `datasources/prometheus_compose.yaml` | Compose datasource (`http://prometheus:9090`) |
| `dashboards_provider.yaml` | Compose dashboard provider → `/var/lib/grafana/dashboards` |
| `../dashboards/aether_radio_overview.json` | Overview dashboard JSON |

## Docker Compose ops stack (recommended)

Default (fast): host `prom-serve` + Compose Prometheus/Grafana:

```bash
# Terminal A
cargo run -p aether-radio-cli -- prom-serve --config configs/ops/prometheus_scrape_compose.yaml
# Terminal B
docker compose -f docker-compose.ops.yml up -d
# metrics:    http://127.0.0.1:9898/metrics
# prometheus: http://127.0.0.1:9090
# grafana:    http://127.0.0.1:3000  (admin/admin)
docker compose -f docker-compose.ops.yml down
```

Optional full profile (in-compose prom-serve; slower first build):

```bash
docker compose -f docker-compose.ops.yml -f docker-compose.ops.full.yml --profile full up --build -d
# Prometheus scrapes prom-serve:9898 via configs/ops/prometheus_compose_full.yml
docker compose -f docker-compose.ops.yml -f docker-compose.ops.full.yml --profile full down
```

## Host-local stack

```bash
cargo run -p aether-radio-cli -- prom-serve
# optional: prometheus --config.file=configs/ops/prometheus.yml
# point Grafana datasource at Prometheus, or scrape prom-serve directly if using a compatible agent
```

Import the dashboard from `data/ops/dashboards/aether_radio_overview.json`.
Datasource provisioning sample: `data/ops/grafana/datasources/prometheus.yaml`.
