# gpu-cuda

Optional CUDA implementation of `gpu-runtime::GpuBackend` for local NVIDIA GPUs
(RTX 4050 validated on WSL2).

## Build

Default workspace builds **without** linking CUDA:

```bash
cargo test -p gpu-cuda
```

Enable CUDA (WSL2 / Docker with GPU):

```bash
cargo test -p gpu-cuda --features cuda
cargo run -p aether-radio-cli --features cuda -- gpu-info
cargo run -p aether-radio-cli --features cuda -- smoke-cuda
```

## Config

- `configs/backends/gpu_cuda.yaml` — device ordinal, kernel scale, name check
- `configs/simulation_profile_cuda.yaml` — profile with `backends.gpu: cuda`

## PTX

Kernel source: `kernels/phy_scale.cu`  
Checked-in PTX: `kernels/phy_scale.ptx` (sm_80; Ada/RTX 40 JIT OK)

Regenerate with CUDA 12 Docker:

```bash
./scripts/build_cuda_ptx.sh
```
