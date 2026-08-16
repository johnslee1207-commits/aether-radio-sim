---
name: aetheros-intent-engineering
description: Convert raw intent into an AetherOS intent and KPI contract.
---

```yaml
skill:
  id: aetheros-intent-engineering
  triggers: ["intent contract", "requirements", "KPI", "scope"]
  canonical_nodes: ["P1", "P6"]
  required_inputs: ["raw_intent", "constraints", "non_goals"]
  outputs: ["intent_contract", "success_metrics", "open_decisions"]
  allowed_tools: ["filesystem", "context_mcp", "governance_mcp"]
  allowed_paths: [".aetheros/intent-contract.yaml", ".aetheros/decision-lineage.jsonl", "docs/**"]
  required_agents: ["intent-analyst"]
  entry_conditions: ["project_intake_valid_or_draft"]
  gates: ["intent_valid", "requirement_kpi_valid"]
  evidence_required: ["user_confirmed_intent", "decision_lineage", "contract_validation"]
  stop_conditions: ["major_ambiguity", "conflicting_requirements", "unconfirmed_irreversible_scope"]
  rollback_target: "project_intake_valid"
```

Keep assumptions explicit and request confirmation before promoting requirements into implementation authority.
