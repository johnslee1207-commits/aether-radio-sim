# memory-manager

Host/GPU memory backends:

- `SimMemory` — unbounded in-process buffers
- `PooledMemory` — capacity limits + modelled H2D/D2H copy latency (`configs/memory_pool.yaml`)

```bash
cargo test -p memory-manager
```
