# gpu-runtime

`GpuBackend` + Phase 1 `SimGpu`, and `GpuRingBuffer` slot state machine:

`Free → Receiving → Ready → Processing → Done → Free`

Config: `configs/gpu_ring.yaml`.

```bash
cargo test -p gpu-runtime
```
