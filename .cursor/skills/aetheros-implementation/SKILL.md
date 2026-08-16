---
name: aetheros-implementation
description: Execute authorized implementation tasks with local evidence.
---

```yaml
skill:
  id: aetheros-implementation
  triggers: ["implement", "code", "fix", "build"]
  canonical_nodes: ["P9"]
  required_inputs: ["implementation_task_contract", "architecture_contract", "allowed_paths"]
  outputs: ["code_changes", "local_test_results", "patch_evidence"]
  allowed_tools: ["filesystem", "shell", "evidence_mcp"]
  allowed_paths: ["src/**", "tests/**", "docs/**", "scripts/**"]
  required_agents: ["implementation-agent"]
  entry_conditions: ["implementation_plan_valid", "task_authorized"]
  gates: ["build_valid"]
  evidence_required: ["diff_summary", "command_results", "local_tests"]
  stop_conditions: ["permission_expansion_required", "credential_required", "protected_path_change", "test_environment_missing"]
  rollback_target: "implementation_plan_valid"
```

Implementation must return raw command outcomes and cannot mark its own work complete.
