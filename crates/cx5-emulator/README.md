# cx5-emulator

ConnectX-5 queue/DMA emulation: `PacketIO`, `SimPacketIO`, and `Cx5Nic` (RX → DMA delay → completion).

`DpdkPacketIO` is an unavailable stub (no libdpdk link); see `configs/backends/dpdk.yaml`.

Config: `configs/nic_dma.yaml`.

```bash
cargo test -p cx5-emulator
```
