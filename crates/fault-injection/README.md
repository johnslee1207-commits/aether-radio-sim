# fault-injection

Configurable datapath faults (Ops Framework §12).

- Loss / burst loss
- Extra latency / GPU kernel delay
- Reorder (hold buffer)
- Sequence jump / timestamp skew

```bash
cargo test -p fault-injection
# stress profile: configs/fault_injection_stress.yaml
```
