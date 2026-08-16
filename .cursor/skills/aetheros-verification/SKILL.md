---
name: aetheros-verification
description: Independently verify implementation, benchmarks, and conformance gates.
---

```yaml
skill:
  id: aetheros-verification
  triggers: ["/aetheros-verify", "verify", "test", "benchmark"]
  canonical_nodes: ["P10"]
  required_inputs: ["acceptance_contract", "implementation_artifacts"]
  outputs: ["verification_report", "benchmark_results", "gate_results"]
  allowed_tools: ["shell", "filesystem", "evidence_mcp"]
  allowed_paths: ["tests/**", "scripts/**", "data/reports/**", "docs/**"]
  required_agents: ["test-engineer", "security-reviewer"]
  entry_conditions: ["build_valid_or_verification_only_request"]
  gates: ["tests_pass"]
  evidence_required: ["raw_test_results", "benchmark_results", "security_review"]
  stop_conditions: ["verification_unavailable", "evidence_conflict", "unauthorized_external_dependency"]
  rollback_target: "build_valid"
```

Verification may fail implementation and must not rewrite acceptance criteria to pass.
