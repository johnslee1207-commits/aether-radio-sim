# cx5-emulator

ConnectX-5 queue/DMA emulation: `PacketIO`, `SimPacketIO`, and `Cx5Nic` (RX → DMA delay → completion).

`DpdkPacketIO` loads `configs/backends/dpdk.yaml`:
- `backend: mock` — mbuf pool / burst simulation (no libdpdk)
- `backend: hardware` — `BackendUnavailable` until a dedicated adapter crate exists

Config: `configs/nic_dma.yaml`.

```bash
cargo test -p cx5-emulator
```
