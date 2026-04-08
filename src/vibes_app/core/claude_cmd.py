from __future__ import annotations

import os
from typing import List

from ..constants import DEFAULT_CLAUDE_MODEL, DEFAULT_CLAUDE_PERMISSION_MODE
from .session_models import SessionRecord


def claude_permission_mode() -> str:
    raw = os.environ.get("VIBES_CLAUDE_PERMISSION_MODE", "").strip()
    return raw or DEFAULT_CLAUDE_PERMISSION_MODE


def claude_model_default() -> str:
    raw = os.environ.get("VIBES_CLAUDE_MODEL", "").strip()
    return raw or DEFAULT_CLAUDE_MODEL


def build_claude_cmd(rec: SessionRecord, *, prompt: str, run_mode: str) -> List[str]:
    model = rec.model or claude_model_default()
    base = [
        "claude",
        "-p",
        "--verbose",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        "--permission-mode",
        claude_permission_mode(),
        "--model",
        model,
    ]

    prompt_s = prompt or ""
    needs_end_of_opts = bool(prompt_s.lstrip().startswith("-"))
    if run_mode == "continue" and rec.thread_id:
        base += ["-r", rec.thread_id]
    if needs_end_of_opts:
        base.append("--")
    base.append(prompt_s)
    return base
