# shm-io

Same-host shared-memory SPSC `PacketIO` for µs-oriented FPGA↔host links.

- `ShmPacketSink` — producer (FPGA process)
- `ShmPacketIO` — consumer (host process)

Config: `configs/backends/shm_link.yaml`

Always run `shm-prepare` (or ensure the ring file exists) before attaching producer/consumer.
`ShmPacketSink` respects `create` — it must not truncate a ring already mapped by `host-recv`.

```bash
cargo test -p shm-io
./scripts/shm_dual_smoke.sh
```
