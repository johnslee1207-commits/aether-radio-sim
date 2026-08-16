# ADR-M1-001: Governed ops scrape stack (planning)

- Status: proposed (pending user confirmation)
- Date: 2026-08-16
- Milestone: `M1_ops_scrape_stack_evidence`
- Deciders: user (intent authority) + system-architect (proposal only)

## Context

Coverage matrix and ops samples already provide `prom-serve`, Prometheus scrape YAML, Grafana datasource, and an overview dashboard, but there is no Compose-governed closed loop or AetherOS-bound evidence pack. M1 must deliver a reproducible ops evidence loop without CUDA/DPDK/FPGA.

## Decision (proposed)

1. Treat M1 as an **integration/evidence** milestone over existing CLI ops surfaces, not a dataplane feature.
2. Preferred topology (draft):
   - **Host or Compose service** runs `aether-radio-cli prom-serve` using `configs/ops/prometheus_scrape.yaml` (`127.0.0.1:9898/metrics`).
   - **Prometheus** container uses `configs/ops/prometheus.yml` (or a Compose-mounted equivalent targeting the exporter).
   - **Grafana** container provisions datasource from `data/ops/grafana/datasources/prometheus.yaml` and loads `data/ops/dashboards/aether_radio_overview.json`.
3. Evidence pack must include at least: successful `/metrics` scrape, Prometheus target UP (or equivalent scrape proof), Grafana datasource reachable, and `ops-report` JSON artifact path.
4. Keep ports/URLs/job names in classified data (`local_machine_specific`); do not hardcode in business crates.
5. Defer CUDA/DPDK/FPGA to later milestones; they are non-goals for M1 gates.

## Consequences

- Positive: Uses existing assets; low hardware risk; aligns with coverage `next_tasks`.
- Negative: May need a new `docker-compose.ops.yml` (and possibly small path/network tweaks) when implementation is later authorized.
- Risk: Host vs container networking for `9898`/`9090` must be decided before coding.
- Non-consequence: This ADR does **not** authorize `crates/**` edits or implementation gate entry.

## Alternatives considered

- Manual-only Prometheus/Grafana on host (weaker reproducibility).
- Scraping prom-serve directly from Grafana without Prometheus (weaker ops realism).
- Hardware-backed exporter path (out of M1 scope).
