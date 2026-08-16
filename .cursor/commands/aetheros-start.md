# /aetheros-start

Purpose: initialize or enter a guided AetherOS project from a user intent.

Inputs: project intent, target users, scope, non-goals, constraints, initial autonomy level.

Behavior:
1. Detect existing `.aetheros/project-state.json`.
2. If absent, instantiate the Cursor project template idempotently.
3. Create an intent draft and morphology report.
4. Stop before the first user-confirmation gate unless governance already records approval.
5. Return current stage, blockers, and next confirmation target.
