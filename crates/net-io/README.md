# net-io

UDP adapter implementing cross-container FPGA → host packet delivery.

- `NetPacketSink` — FPGA container TX
- `NetPacketIO` — host container RX (`PacketIO`)

Wire format: `AetherHeader` (32B) + payload.

Config: `configs/backends/net_link_*.yaml`

```bash
cargo test -p net-io
```
