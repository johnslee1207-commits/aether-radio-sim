# AGENTS.md — Aether Radio Sim

This repository implements **Aether Radio Data Plane Simulation Platform v1.1**.

Canonical spec: `docs/CURSOR_DEVELOPMENT_SPEC_v1.1.md`.  
Observability & Ops (first-class): `docs/AETHER_RADIO_OBSERVABILITY_OPS_FRAMEWORK_v1.0.md`.  
Coverage: `data/architecture/observability_coverage_matrix.json`.

## Stack

- Core runtime: Rust
- Control plane / scheduling: tokio
- Data plane: lock-free / poll / zero-copy oriented (no async on hot path)
- Observability plane: Metrics / Log-Event / Trace / Health (see Ops Framework; Trace = Sprint O)

## Cursor Agent rules

1. **Interface → test → mock → backend.** Never bind DPDK, CUDA, or Linux sockets in business crates.
2. **Small tasks only** (`T001` … / observability `O001` …). Do not generate an entire module tree in one shot.
3. Each crate must keep `README.md`, unit tests, and (when applicable) bench hooks.
4. **Observability is first-class.** Every datapath change includes Metrics hook → Log/Event hook → Trace hook (when TraceEngine exists) before considering the task done. Policies live under `configs/ops/`.

## Sprint map

| Sprint | Focus |
|--------|--------|
| 1 | Workspace, crates, CI — **Done** |
| 2 | Protocol encode/decode/validate — **Done** |
| 3 | FPGA IQ + packetizer + slot scheduler — **Done** |
| 4 | Transport stream/sequence/timestamp — **Done** |
| 5 | Host/GPU memory (+ ring BufferState) — **Done** |
| 6 | Benchmark + metrics harness — **Done** |
| O | Observability & Operations Plane — **O001–O020 Done** |

<!-- program-data-decoupling:start -->
# Program/Data Decoupling

Before architecture design, workflow design, coding, or refactoring this
project, MUST read and apply program/data decoupling:

- Keep executable code focused on behavior (loaders, validators, executors, adapters, algorithms).
- Put bandwidth, latency, loss, DMA delay, fault rates, stream counts, backend selection, health thresholds, and observability export paths in `configs/`, `configs/ops/`, or `data/profiles/`.
- Update `data/architecture/data_classification_registry.json` when adding persistent data paths.
- Do not hardcode local absolute paths or secrets in source.

<!-- program-data-decoupling:end -->

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p aether-radio-cli -- smoke
cargo run -p aether-radio-cli -- bench
cargo run -p aether-radio-cli -- accept
cargo run -p aether-radio-cli -- ops-status
cargo run -p aether-radio-cli -- prom-dump
cargo run -p aether-radio-cli -- ops-report
cargo run -p aether-radio-cli -- soak --profile configs/soak_profile_ci.yaml
cargo run -p aether-radio-cli -- fault-drill
cargo run -p aether-radio-cli -- prom-serve --once
# WSL2 + RTX GPU (optional):
# export LD_LIBRARY_PATH=/usr/lib/wsl/lib:$LD_LIBRARY_PATH
# cargo run -p aether-radio-cli --features cuda -- gpu-info
# cargo run -p aether-radio-cli --features cuda -- smoke-cuda
# Dual-container / dual-process:
# ./scripts/dual_process_smoke.sh
# docker compose -f docker-compose.split.yml up --build
```

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **aether-radio-sim** (970 symbols, 1632 relationships, 11 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/aether-radio-sim/context` | Codebase overview, check index freshness |
| `gitnexus://repo/aether-radio-sim/clusters` | All functional areas |
| `gitnexus://repo/aether-radio-sim/processes` | All execution flows |
| `gitnexus://repo/aether-radio-sim/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
