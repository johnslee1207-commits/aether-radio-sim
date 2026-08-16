# ADR-G7-001: DPDK adapter boundary spike

- Status: accepted-for-spike (no libdpdk link)
- Date: 2026-08-16
- Milestone: G7
- Risk: high (hardware path); spike stays mock-default

## Context

`DpdkPacketIO` already supports `backend: mock` and rejects `hardware` with `BackendUnavailable`. Business crates must not link `libdpdk`. Coverage matrix still lists a real DPDK/DOCA adapter as optional.

## Decision

1. Keep **PacketIO** in `cx5-emulator` as the only datapath-facing trait.
2. Introduce a **config-side adapter contract** (`configs/backends/dpdk_adapter_contract.yaml`) describing future hardware adapter obligations without enabling hardware.
3. Add a **stub workspace crate** `dpdk-adapter` that:
   - never links libdpdk
   - exposes `probe_hardware()` → `Unavailable`
   - documents FFI/feature-gate future work
4. Default remains `backend: mock`. Hardware requires explicit future feature + governance.

## Non-goals (this spike)

- Linking libdpdk / DOCA
- Changing PipelineBench hot path
- Making hardware the default

## Consequences

- Clear ownership: business crates → PacketIO; optional adapter crate behind feature later
- Auditors can see hardware remains closed by design
- Next step after evidence: optional `cx5-emulator` feature `dpdk-hardware` that calls adapter (still out of this spike if not needed)
