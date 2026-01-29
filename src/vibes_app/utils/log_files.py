from __future__ import annotations

import json
import os
from pathlib import Path
from typing import List, Optional

from ..constants import UI_PREVIEW_MAX_CHARS, UI_TAIL_MAX_BYTES
from ..core.codex_events import (
    extract_item,
    extract_item_text,
    extract_item_type,
    extract_text_delta,
    extract_tool_command,
    extract_tool_output,
    get_event_type,
    maybe_extract_diff,
)
from .text import truncate_text


def tail_text_file(path: Path, *, max_bytes: int = UI_TAIL_MAX_BYTES) -> str:
    if not path.exists() or not path.is_file():
        return ""
    try:
        size = path.stat().st_size
        to_read = min(size, max_bytes)
        with path.open("rb") as f:
            if to_read < size:
                f.seek(-to_read, os.SEEK_END)
            data = f.read(to_read)
        return data.decode("utf-8", errors="replace")
    except Exception:
        return ""


def extract_last_agent_message_from_stdout_log(path: Optional[str], *, max_chars: int = UI_PREVIEW_MAX_CHARS) -> str:
    if not path:
        return ""
    p = Path(path)
    raw = tail_text_file(p)
    if not raw.strip():
        return ""

    for line in reversed(raw.splitlines()[-500:]):
        s = line.strip()
        if not s:
            continue
        try:
            obj = json.loads(s)
        except Exception:
            continue
        if not isinstance(obj, dict):
            continue
        event_type = get_event_type(obj)
        if event_type in {"agent_message", "assistant_message"}:
            text = obj.get("text")
            if isinstance(text, str) and text.strip():
                return truncate_text(text.strip(), max_chars)
        if event_type.startswith("item."):
            item = extract_item(obj)
            if isinstance(item, dict):
                item_type = extract_item_type(item)
                if item_type in {"assistant_message", "message"}:
                    item_text = extract_item_text(item)
                    if isinstance(item_text, str) and item_text.strip():
                        return truncate_text(item_text.strip(), max_chars)
    return ""


def preview_from_stdout_log(path: Optional[str], *, max_chars: int = UI_PREVIEW_MAX_CHARS) -> str:
    if not path:
        return ""
    p = Path(path)
    raw = tail_text_file(p)
    if not raw.strip():
        return ""

    pieces: List[str] = []
    last_cmd: Optional[str] = None
    for line in raw.splitlines()[-250:]:
        s = line.strip()
        if not s:
            continue
        try:
            obj = json.loads(s)
        except Exception:
            pieces.append(line)
            continue
        if not isinstance(obj, dict):
            pieces.append(line)
            continue

        event_type = get_event_type(obj)
        if event_type.startswith("item."):
            item = extract_item(obj)
            if isinstance(item, dict):
                item_type = extract_item_type(item)
                if item_type == "reasoning":
                    continue
                if item_type == "command_execution":
                    cmd = item.get("command")
                    out = item.get("aggregated_output")
                    exit_code = item.get("exit_code")
                    status = item.get("status")
                    is_start = event_type.endswith("started") or status == "in_progress"
                    is_done = event_type.endswith("completed") or status in {"completed", "failed"}

                    cmd_s = cmd.strip() if isinstance(cmd, str) else ""
                    if cmd_s and (is_start or is_done) and cmd_s != last_cmd:
                        pieces.append(f"\n$ {cmd_s}\n")
                        last_cmd = cmd_s
                    if is_done:
                        if isinstance(out, str) and out.strip():
                            pieces.append(truncate_text(out, 800) + "\n")
                        if isinstance(exit_code, int):
                            pieces.append(f"(exit_code: {exit_code})\n")
                    continue

                item_text = extract_item_text(item)
                if item_text:
                    pieces.append(item_text)
                    continue

        if event_type == "text":
            delta = extract_text_delta(obj)
            if delta:
                pieces.append(delta)
            continue
        if event_type in {"agent_message", "assistant_message"}:
            msg = obj.get("text")
            if isinstance(msg, str) and msg:
                pieces.append("\n" + msg + "\n")
            continue
        if event_type == "tool_use":
            cmd = extract_tool_command(obj) or ""
            pieces.append(f"\n[tool_use]\n{cmd}\n")
            continue
        if event_type == "tool_result":
            out = extract_tool_output(obj) or ""
            pieces.append(f"\n[tool_result]\n{truncate_text(out, 800)}\n")
            continue

        diff = maybe_extract_diff(obj)
        if diff:
            pieces.append(f"\n[file_change]\n{truncate_text(diff, 800)}\n")
            continue

        delta = extract_text_delta(obj)
        if delta:
            pieces.append(delta)

    text = "".join(pieces).strip()
    return truncate_text(text, max_chars)


def preview_from_stderr_log(path: Optional[str], *, max_chars: int = 1200) -> str:
    if not path:
        return ""
    p = Path(path)
    raw = tail_text_file(p)
    if not raw.strip():
        return ""
    tail = "\n".join(raw.splitlines()[-40:])
    return truncate_text(tail, max_chars)

