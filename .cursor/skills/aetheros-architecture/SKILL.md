---
name: aetheros-architecture
description: Build ontology, architecture decisions, and implementation contracts.
---

```yaml
skill:
  id: aetheros-architecture
  triggers: ["architecture", "ADR", "ontology", "design"]
  canonical_nodes: ["P5", "P7", "P8"]
  required_inputs: ["intent_contract", "trusted_context_manifest", "ontology_or_domain_pack"]
  outputs: ["ontology_summary", "architecture_decision_records", "implementation_plan"]
  allowed_tools: ["filesystem", "context_mcp", "governance_mcp"]
  allowed_paths: [".aetheros/**", "docs/**"]
  required_agents: ["ontology-architect", "system-architect", "security-reviewer"]
  entry_conditions: ["provenance_valid"]
  gates: ["ontology_graph_valid", "architecture_valid", "implementation_plan_valid"]
  evidence_required: ["adr", "interface_contracts", "risk_review", "implementation_task_contracts"]
  stop_conditions: ["irreversible_architecture_decision", "security_boundary_unresolved", "core_semantic_conflict"]
  rollback_target: "provenance_valid"
```

Architecture may propose transitions; governance alone approves them.
