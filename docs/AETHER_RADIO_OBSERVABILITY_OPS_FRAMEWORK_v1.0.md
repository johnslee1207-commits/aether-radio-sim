# Aether Radio Data Plane Observability & Operations Framework v1.0

版本：v1.0  
状态：Canonical Cursor development module  
关联规范：`docs/CURSOR_DEVELOPMENT_SPEC_v1.1.md`  
覆盖矩阵：`data/architecture/observability_coverage_matrix.json`

---

## 1. 文档目的

本框架将 **Metrics / Logging / Trace / Health / Config / Test / Fault / Bench / Dashboard**
定义为 Aether Radio Data Plane 的**一等公民（First-Class Citizen）**，而不是业务功能完成后的附加项。

本系统是跨硬件边界、跨时钟域、跨软件栈的实时计算路径：

```text
FPGA Radio Endpoint
        |
100G Transport
        |
CX5 / DPDK
        |
Host / GPU Memory
        |
GPU PHY
```

没有完整可观测性时，100G 掉包、GPU 延迟上升、FPGA buffer overflow、slot miss、PCIe 异常将难以定位。

本规范用于：

* Cursor / OpenCode / Codex 开发与审查；
* Sprint 规划（Observability Plane 增量任务 `O001`…）；
* 与 AetherTwin / Cognisphere「系统必须能解释自身运行状态」理念对齐。

---

## 2. 总体架构（三平面）

```text
                 Aether Operations Plane
+------------------------------------------------+
|              Control Plane                     |
|  Link Manager | Stream Manager                 |
|  Config Manager | Health Manager               |
+------------------------------------------------+
|              Observability Plane               |
|  Metrics Engine | Logging Engine               |
|  Trace Engine | Event Engine                   |
+------------------------------------------------+
|              Data Plane                        |
|  FPGA → 100G Transport → CX5/DPDK → GPU        |
+------------------------------------------------+
```

### 2.1 约束（与主规范一致）

1. **Data Plane 热路径禁止 async**；观测采集用 poll / ring / 无锁计数器。
2. **禁止业务 crate 直接依赖** Prometheus、Grafana、DPDK、CUDA、Linux socket。
3. Observability 通过 **trait + mock + exporter adapter** 接入（Interface → test → mock → backend）。
4. 阈值、阈值、采样率、导出路径属于 **数据层**（`configs/ops/`），不得硬编码。

---

## 3. 三类数据必须分离

| 类型 | 回答的问题 | 特征 | 当前落点 |
|------|------------|------|----------|
| **Metrics** | 系统现在是否健康？ | 高频、数值、聚合 | `metrics-engine::MetricsEngine`（部分字段） |
| **Log / Event** | 发生了什么？ | 结构化事件、可检索 | `EventLogger` JSONL + `tracing`（CLI） |
| **Trace** | 一个 packet/slot 经历了什么？ | 端到端时间戳链 | **未实现**（O-sprint 优先） |

禁止把三类数据混进同一无 schema 的 dump。

---

## 4. Metrics 五层模型

### Layer 1 — Physical / Link

`link_up`, `link_speed`, `fec_*`, `mtu`, `crc_error`, optics（温度/光功率）等。  
仿真阶段：由 `ethernet-model` / net-io / shm-io / mock-DPDK 导出**模型量**；硬件阶段由 NIC exporter 填充。

### Layer 2 — Transport

`tx/rx_packet_count`, `tx/rx_bytes`, `pps`, `Gbps`, `gap_count`, `duplicate_count`, `out_of_order_count`, latency percentiles, jitter。

### Layer 3 — Radio

`slot_*`, `symbol_*`, `deadline_miss`, antenna / IQ loss。  
这是区别于普通网络栈的关键层。

### Layer 4 — Memory

FPGA/host/GPU buffer occupancy、overflow/underflow、ring producer/consumer、stall_time。

### Layer 5 — Compute

`kernel_latency`, stream idle, GPU utilization（CUDA exporter；SimGpu 用模型延迟）。

每个 layer 对应独立 metric family；导出时带 `component` / `stream_id` / `antenna` label。

---

## 5. Metrics 采集架构

推荐 Prometheus 模型（adapter 可选）：

```text
Component → MetricsExporter trait → scrape / push → Prometheus → Grafana
```

计划 exporters（不得进入业务热路径逻辑）：

| Exporter | 来源 crate / 后端 |
|----------|-------------------|
| FPGA | `fpga-emulator` |
| Transport | `aether-transport` |
| CX5 / DPDK | `cx5-emulator`, `net-io`, `shm-io`, mock/real DPDK |
| Memory / Ring | `memory-manager`, `gpu-runtime` |
| GPU PHY | `gpu-cuda` / `SimGpu` |
| Runtime aggregate | `metrics-engine` |

仿真默认：JSON snapshot + JSONL；可选 `ops-exporter` crate（后续）对接 Prometheus text format。

---

## 6. Logging 体系

### 6.1 技术选型

* Control / ops：`tracing` + `tracing-subscriber`（已在 CLI）
* Data-plane 事件：`EventLogger` / 未来 `LoggingEngine`（结构化 JSONL，无 printf）

### 6.2 分类

| 类别 | 示例事件 |
|------|----------|
| System | runtime_started, gpu_detected, cx5_initialized |
| Link | LINK_UP, LINK_DOWN, FEC_ERROR, RECOVERY |
| Transport | STREAM_CREATE, PACKET_LOSS, SEQ_ERROR |
| Radio | SLOT_MISS, SYMBOL_TIMEOUT, IQ_DROP |
| Memory | BUFFER_FULL, RING_STALL |
| Compute | KERNEL_SLOW, CUDA_ERROR |

### 6.3 等级

`TRACE < DEBUG < INFO < WARN < ERROR < FATAL`  
生产默认 `INFO`；Health 进入 `DEGRADED`/`FAILED` 时可策略性提升 DEBUG dump（配置驱动）。

---

## 7. Trace 体系（最高优先级缺口）

目标：回答「一个 slot 为什么晚了？」

每个 packet / symbol 携带或关联：

```text
trace_id | stream_id | sequence | stage timestamps
```

建议阶段戳（ns）：

```text
FPGA_TX → WIRE_DEPART → CX5_RX → DMA_DONE → HOST_READY → GPU_ENQUEUE → CUDA_START → CUDA_DONE
```

实现约束：

* 热路径只写固定大小 ring（`TraceSpan` / `StageStamp`）；
* 导出异步/离线到 `data/reports/traces/*.jsonl`；
* 与 Metrics/Log **分离 schema**。

任务编号起点：`O010` TraceEngine MVP。

---

## 8. Health Monitoring

`HealthManager` 状态机：

```text
NORMAL → WARNING → DEGRADED → FAILED → RECOVERY → NORMAL
```

检查维度（阈值在 `configs/ops/health_policy.yaml`）：

* Link：loss / error rate  
* Transport：jitter / latency_p99 / seq gap  
* GPU：buffer stall / kernel delay  
* Radio：slot/symbol deadline miss  

Health 变更必须发 **Log event**，并更新 **Health metric gauge**。

---

## 9. 自动恢复（策略数据化）

| 异常 | 默认动作（可配置） |
|------|-------------------|
| Packet 异常 | mark invalid, continue |
| Buffer overflow | drop oldest, resync slot |
| Sequence gap | recover_sequence（已有） |
| Link down | restart stream / re-init link |
| GPU stall | skip / degrade QoS / alert |

恢复策略放在 `configs/ops/recovery_policy.yaml`，代码只执行。

---

## 10. Configuration Management

已有：`configs/*.yaml`, `configs/backends/*`, `configs/ops/*`（本框架新增）。  
原则：改行为不改代码；`validate-config` / `accept` 作为配置门禁。

---

## 11. Test Framework（四层）

| Level | 含义 | 现状 |
|-------|------|------|
| L1 Unit | header/parser/sequence | 各 crate `cargo test` |
| L2 Component | FPGA→Transport→Memory | `integration_mvp` Test1–6 |
| L3 System | 双进程/双容器/CUDA | smoke, shm/udp scripts, smoke-cuda |
| L4 Stress | 100G/200G/400G 模型与长跑 | **部分**（ethernet model + accept）；缺长稳压测 |

每模块必须自带：metrics hook、log/event hook、unit test。

---

## 12. Fault Injection

现状：`fault-injection` crate + YAML（loss/delay/GPU slowdown）。  
扩展目标：reorder、burst loss、timestamp error、sequence jump、buffer overflow、memory pressure（配置驱动）。

---

## 13. Performance Benchmark

现状：`PipelineBench`、`AcceptanceRunner`、JSON 报告。  
扩展：histogram 导出、jitter 字段填充、多 profile 矩阵（sim / cuda / shm / mock-dpdk）。

---

## 14. Dashboard

Grafana 为**可选后端**。仿真阶段先交付：

1. JSONL + JSON reports（已有）  
2. `ops` CLI 汇总（planned）  
3. Grafana dashboard JSON under `data/ops/dashboards/`（后续，不阻塞核心）

页面：Overview / Radio / Performance / Debug（含 packet trace）。

---

## 15. Cursor 开发顺序（强制）

每个数据面模块按：

```text
Transport/Feature Core
        ↓
Metrics Hook
        ↓
Log / Event Hook
        ↓
Trace Hook
        ↓
Benchmark / Accept Gate
        ↓
Next Feature
```

禁止：「功能完成后再补 metrics」。

Agent 规则补充：

1. 新增 datapath 符号时，同步更新覆盖矩阵。  
2. 观测策略进 `configs/ops/`，更新 `data_classification_registry.json`。  
3. 小任务 `O001`…，禁止一次生成整个 ops 模块树。

---

## 16. Sprint O — Observability Plane（建议）

| ID | 任务 | 目标 |
|----|------|------|
| O001 | Metrics 五层 schema 与 `MetricsBackend` trait | 分层计数/百分位 API |
| O002 | Event taxonomy + 配置化 events_path | gap/late/drop/slot_miss 事件 |
| O003 | TraceEngine MVP（ring + JSONL） | packet stage stamps |
| O004 | HealthManager + health_policy.yaml | NORMAL…RECOVERY |
| O005 | smoke/smoke-cuda 接入 EventLogger | 全 CLI 路径可观测 |
| O006 | Fault injection 扩展（reorder/burst） | **Done** — `FaultInjector` |
| O007 | Stress / soak harness | **Done** — `soak` CLI |
| O008 | Prometheus text exporter mock | **Done** — `prom-dump` |
| O009 | Grafana dashboard 样例 | **Done** — `data/ops/dashboards/` |
| O010 | 文档与 maturity 复评 | **Done** — matrix v1.2 |
| O011 | RecoveryPolicy executor | **Done** — `RecoveryExecutor` |
| O012 | TraceEngine 接入 PipelineBench / soak | **Done** — default-on |
| O013 | Prometheus HTTP scrape | **Done** — `prom-serve` |
| O014 | Memory metrics + fault-drill | **Done** — `fault-drill` |
| O015 | Matrix/maturity/CI closeout | **Done** — matrix v1.3 |

---

## 17. 与现状的差距摘要

**已有（可用）：** Metrics 五层 + Prometheus text/HTTP、EventLogger JSONL、TraceEngine（bench 默认开启）、HealthManager、RecoveryExecutor、FaultInjector、PipelineBench、accept/soak/fault-drill、Grafana 样例。

**剩余缺口（可选）：** 真实 DPDK/DOCA、长时间 wall-clock soak 的持续 Health 轮询、Grafana 对接 live `prom-serve`。

详见：`data/architecture/observability_coverage_matrix.json`。

---

## 18. 最终能力目标

```text
Aether Radio Data Plane
  + Observability Framework
  + Test Framework
  + Fault Injection
  + Benchmark / Acceptance
  + Health / Recovery
```

形成可开发、可验证、可压测、可定位、可运维的工业级实时数据平台。

---

## 19. 修订

| 版本 | 日期 | 说明 |
|------|------|------|
| v1.0 | 2026-08-04 | 首版：三平面 + 三类数据分离 + 五层 Metrics + Sprint O |
