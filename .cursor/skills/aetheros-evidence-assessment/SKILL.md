---
name: aetheros-evidence-assessment
description: Build evidence bundle, assess maturity, and plan the next loop.
---

```yaml
skill:
  id: aetheros-evidence-assessment
  triggers: ["/aetheros-assess", "evidence", "maturity", "delivery"]
  canonical_nodes: ["P11", "P12", "P13"]
  required_inputs: ["intent_contract", "verification_report", "decision_lineage", "artifact_manifest"]
  outputs: ["evidence_bundle", "claim_assessment", "maturity_report", "next_loop_plan"]
  allowed_tools: ["filesystem", "evidence_mcp", "governance_mcp"]
  allowed_paths: [".aetheros/evidence/**", "data/reports/**", "docs/**"]
  required_agents: ["evidence-auditor"]
  entry_conditions: ["tests_pass_or_assessment_only_request"]
  gates: ["evidence_complete", "maturity_valid", "next_loop_valid"]
  evidence_required: ["claim_to_evidence_map", "maturity_assessment", "unresolved_risks"]
  stop_conditions: ["false_completion_detected", "missing_required_evidence", "open_blocker"]
  rollback_target: "tests_pass"
```

Use `VERIFIED` only when evidence is deterministic and complete; otherwise mark claims `UNVERIFIED`.
