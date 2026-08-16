---
name: aetheros-autonomous-rd
description: Cursor total-control skill for AetherOS autonomous R&D orchestration.
---

```yaml
skill:
  id: aetheros-autonomous-rd
  triggers: ["/aetheros-start", "/aetheros-continue", "/aetheros-resume", "AetherOS autonomous R&D"]
  canonical_nodes: ["P0", "P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "P9", "P10", "P11", "P12", "P13"]
  required_inputs: ["user_intent_or_project_state", "authorized_root"]
  outputs: ["stage_result", "transition_request", "evidence_refs"]
  allowed_tools: ["filesystem", "shell", "context_mcp", "governance_mcp", "evidence_mcp"]
  allowed_paths: [".aetheros/**", ".cursor/**", "docs/**", "src/**", "tests/**", "scripts/**"]
  required_agents: ["intent-analyst", "context-engineer", "system-architect", "implementation-agent", "test-engineer", "evidence-auditor"]
  entry_conditions: ["canonical_core_approved", "project_root_authorized", "budget_available"]
  gates: ["project_intake_valid", "intent_valid", "architecture_valid", "tests_pass", "evidence_complete"]
  evidence_required: ["project_state_snapshot", "tool_results", "verification_results", "evidence_bundle"]
  stop_conditions: ["permission_expansion_required", "credential_required", "major_ambiguity", "verification_unavailable", "evidence_conflict", "hard_budget_limit"]
  rollback_target: "last_verified_gate"
```

Procedure:
1. Load `.aetheros/project-state.json` when present; otherwise run bootstrap only up to the first user confirmation gate.
2. Query governance for allowed transitions and select the smallest eligible stage skill.
3. Dispatch role-specific task contracts; never let implementation approve verification or evidence.
4. Record evidence before requesting any state transition.
5. If a required service, tool, or proof is missing, return `BLOCKED` with a precise recovery target.
