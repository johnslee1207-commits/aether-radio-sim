# /aetheros-resume

Purpose: recover from interruption using authoritative checkpoints.

Behavior:
1. Load project state and latest evidence bundle.
2. Reconcile repository identity and incomplete tasks.
3. Resume from the last verified gate, not from the last chat message.
4. Return `BLOCKED` if reconciliation cannot be proven.
