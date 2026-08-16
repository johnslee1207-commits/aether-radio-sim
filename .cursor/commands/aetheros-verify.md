# /aetheros-verify

Purpose: run verification and evidence collection only.

Behavior:
1. Do not edit implementation unless the user separately authorizes a fix.
2. Run available `scripts/verify`, tests, lint, benchmarks, and conformance checks.
3. Mark unavailable required checks as `NOT_IMPLEMENTED`.
4. Bind raw results to evidence.
