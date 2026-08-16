# dpdk-adapter (spike)

Stub boundary for a future DPDK/DOCA hardware adapter (goal **G7**).

- **Does not** link `libdpdk` / DOCA
- `probe_hardware()` / `open_hardware()` always return unavailable
- Contract data: `configs/backends/dpdk_adapter_contract.yaml`
- Datapath remains on `cx5_emulator::PacketIO` + `backend: mock`

```bash
cargo test -p dpdk-adapter
```

See `.aetheros/plans/g7-dpdk-adapter/adr_g7_001_dpdk_adapter_boundary.md`.
