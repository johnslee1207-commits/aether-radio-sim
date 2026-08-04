# aether-transport

Core transport runtime: `TransportEngine`, `StreamManager`, `LinkManager`.

Sprint 4:

- `SequenceChecker` / `TimestampChecker`
- `SimTransportEngine` (ingest + recover_sequence)
- Policy: `configs/transport_deadline.yaml`

```bash
cargo test -p aether-transport
```
