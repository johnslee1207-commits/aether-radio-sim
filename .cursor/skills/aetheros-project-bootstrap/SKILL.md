---
name: aetheros-project-bootstrap
description: Initialize or reconcile an AetherOS project template in Cursor.
---

```yaml
skill:
  id: aetheros-project-bootstrap
  triggers: ["/aetheros-start", "new AetherOS project", "bootstrap AetherOS"]
  canonical_nodes: ["P0", "P2"]
  required_inputs: ["raw_intent", "repository_state", "authorized_root"]
  outputs: ["project_skeleton", "morphology_report", "intent_draft"]
  allowed_tools: ["filesystem", "governance_mcp"]
  allowed_paths: ["AGENTS.md", ".aetheros/**", ".cursor/**", "docs/**", "scripts/**", "src/**", "tests/**"]
  required_agents: ["intent-analyst"]
  entry_conditions: ["authorized_root", "no_conflicting_project_state"]
  gates: ["project_intake_valid", "morphology_valid"]
  evidence_required: ["created_paths", "project_state_snapshot", "bootstrap_idempotency_result"]
  stop_conditions: ["existing_project_conflict", "permission_expansion_required", "major_ambiguity"]
  rollback_target: null
```

Initialize idempotently from `materializations/cursor/templates/project`. `.aetheros/` stores facts; `.cursor/` stores Cursor execution configuration only.
