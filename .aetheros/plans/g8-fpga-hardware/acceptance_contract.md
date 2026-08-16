# ADR-G8-001 / Acceptance contract — real FPGA bitstream integration

- Status: **draft — blocked on hardware governance**
- Date: 2026-08-17
- Milestone: G8
- Risk: critical

## Intent

Move from software `fpga-emulator` toward half-physical radio datapath using a real FPGA bitstream / board.

## Non-negotiables (must be confirmed before any lab work)

1. Explicit hardware authority (owner, site, board inventory).
2. No secrets / bitstream IP committed to this repo without license clearance.
3. Business crates still must not bind proprietary vendor SDKs directly — adapter crate + configs.
4. Fail-closed when hardware unavailable (mock/sim remains default).

## Proposed boundaries

| Layer | Owner |
|-------|--------|
| Bitstream / board bring-up | Hardware lab (out of sim default CI) |
| Frame/IQ contract | `aether-protocol` + `configs/radio_timing.yaml` |
| Host link | existing `net-io` / `shm-io` / future FPGA adapter |
| Evidence | `.aetheros/evidence/g8_*` only after authorized run |

## Acceptance criteria (when authorized)

- AC-G8-01: Documented board + bitstream revision recorded in evidence
- AC-G8-02: At least N symbols received with seq_gaps within policy
- AC-G8-03: Sim path still PASSes without hardware present
- AC-G8-04: No libxilinx / vendor SDK linked into business crates by default

## Stop condition

Do **not** implement or connect real FPGA until pending decision `authorize_g8_hardware_lab` is confirmed.
