# Aether Radio Data Plane Simulation Platform v1.1

# Cursor Development Specification

版本：v1.1

---

# 1. 文档目的

本文定义 **Aether Radio Data Plane Simulation Platform v1.1** 的工程实现规范。

目标：

在没有真实硬件：

* Xilinx FPGA
* Mellanox CX5
* GPU Server

情况下，通过软件仿真验证：

* Aether Radio Transport架构；
* eCPRI-like deterministic streaming；
* CX5 + DPDK + GPUDirect数据路径；
* GPU/CPU memory模型；
* 多100G扩展能力；
* us级实时性能目标。

本规范用于：

* Cursor Agent开发；
* OpenCode开发；
* Codex代码审查。

---

# 2. 总体工程目标

最终系统模拟：

```text
Remote Radio FPGA Emulator
        |
 Aether Radio Transport
        |
 CX5 / DPDK Runtime Emulator
        |
+----------------+
| Host Memory    |
+----------------+
+----------------+
| GPU Memory     |
+----------------+
        |
 CUDA PHY Pipeline Emulator
```

---

# 3. 开发原则

## 3.1 先定义接口，再实现后端

所有硬件相关能力必须抽象：`PacketIO`、`MemoryBackend`、`GpuBackend`、`NicBackend`、`MetricsBackend`。

禁止业务代码直接依赖 DPDK API、CUDA API、Linux socket。

## 3.2 Backend可替换

同一套 Runtime 支持：Simulation → DPDK → DOCA/GPUNetIO → Real Hardware。

---

# 4. 技术栈

* Core Runtime：Rust
* Async（control plane / scheduling / metrics）：tokio
* Data Plane：lock-free ring、poll loop、zero-copy（避免 async await）

---

# 5–6. Repository / Crate 职责

见根目录 `README.md` 与各 `crates/*/README.md`。

---

# 7–19. 子系统设计摘要

* FPGA：IQ Generator、Slot Scheduler、Packetizer、Timestamp/Sequence
* Ethernet：bandwidth/mtu/latency/jitter/loss（`configs/ethernet_model.yaml`）
* CX5：Rx/Tx/CQ + DMA latency（`configs/nic_dma.yaml`）
* DPDK 抽象：`PacketIO` / `SimPacketIO`
* Memory：`MemoryBackend` Host+GPU
* GPU：Phase1 `SimGpu` sleep；Phase2 CUDA `GpuBackend`
* Stream / Link Manager、Metrics、Fault Injection、Benchmark

---

# 20. 测试体系

Unit：`cargo test` per crate。Integration：单 stream、100G 模型、多 stream、故障恢复。

当前已落地：Test1（FPGA→Transport→Memory）、Test2（100G serialize model）。

---

# 21. Cursor Agent 规则

1. interface → test → mock → backend  
2. 禁止大模块一次生成（T001…）  
3. 每模块 README + API docs + unit tests + benchmark  

---

# 22–25. Sprint / MVP / 硬件替换 / 最终目标

Sprint 1–6 见 `AGENTS.md`。MVP：FPGA 出数、transport、memory、GPU 模拟、metrics。硬件替换不改 protocol/transport/metrics/benchmark。

最终形成 Aether Radio Data Plane Runtime，支撑 AI-RAN / 6G distributed PHY / GPU baseband 验证。
