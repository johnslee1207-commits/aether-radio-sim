# shm-io

Same-host shared-memory SPSC `PacketIO` for µs-oriented FPGA↔host links.

- `ShmPacketSink` — producer (FPGA process)
- `ShmPacketIO` — consumer (host process)

Config: `configs/backends/shm_link.yaml`

```bash
cargo test -p shm-io
./scripts/shm_dual_smoke.sh
```
