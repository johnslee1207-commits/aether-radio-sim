#!/usr/bin/env python3
"""Cursor project hook for AetherOS policy and evidence capture."""

from __future__ import annotations

import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path.cwd()
EVENT_LOG = ROOT / "data" / "reports" / "cursor_agent_hook_events.jsonl"
SECRET_PATTERNS = [
    re.compile(r"(^|[\\/])\.env($|\.)", re.IGNORECASE),
    re.compile(r"id_rsa|id_ed25519|\.pem$|\.p12$", re.IGNORECASE),
    re.compile(r"credential|secret|token", re.IGNORECASE),
]
DESTRUCTIVE_PATTERNS = [
    re.compile(r"\brm\s+-rf\b", re.IGNORECASE),
    re.compile(r"\bRemove-Item\b.*\b-Recurse\b", re.IGNORECASE),
    re.compile(r"\bgit\s+reset\s+--hard\b", re.IGNORECASE),
    re.compile(r"\bgit\s+checkout\s+--\b", re.IGNORECASE),
]


def read_stdin_json() -> dict:
    raw = sys.stdin.read()
    if not raw.strip():
        return {}
    try:
        return json.loads(raw)
    except Exception:
        return {"raw": raw}


def command_text(payload: dict) -> str:
    for key in ("command", "cmd", "shell_command"):
        value = payload.get(key)
        if isinstance(value, str):
            return value
    return json.dumps(payload, ensure_ascii=False)


def deny_reason(event: str, payload: dict) -> str | None:
    text = command_text(payload)
    if event in {"beforeShellExecution", "beforeTabFileRead"}:
        for pattern in SECRET_PATTERNS:
            if pattern.search(text):
                return "AetherOS policy blocks credential or secret access."
        for pattern in DESTRUCTIVE_PATTERNS:
            if pattern.search(text):
                return "AetherOS policy blocks destructive commands without explicit governance authority."
    return None


def append_event(event: str, payload: dict, ok: bool, reason: str | None) -> None:
    EVENT_LOG.parent.mkdir(parents=True, exist_ok=True)
    record = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "event": event,
        "ok": ok,
        "reason": reason,
        "payload_keys": sorted(payload.keys()),
    }
    with EVENT_LOG.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, ensure_ascii=False) + "\n")


def main() -> int:
    event = sys.argv[1] if len(sys.argv) > 1 else "unknown"
    payload = read_stdin_json()
    reason = deny_reason(event, payload)
    ok = reason is None
    append_event(event, payload, ok, reason)
    print(json.dumps({"ok": ok, "reason": reason}, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
