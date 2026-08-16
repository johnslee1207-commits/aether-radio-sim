# /aetheros-continue

Purpose: resume work from authoritative AetherOS state.

Behavior:
1. Load `.aetheros/project-state.json` and current repository identity.
2. Query governance for allowed transitions.
3. Select the next eligible stage skill.
4. Continue only within current autonomy, budget, path, and tool policy.
5. Record evidence and request transition; do not self-approve.
