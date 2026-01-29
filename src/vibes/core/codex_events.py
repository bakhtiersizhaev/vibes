from __future__ import annotations

from typing import Any, Dict, List, Optional

from ..utils.uuid import looks_like_uuid


def extract_session_id_explicit(obj: Dict[str, Any]) -> Optional[str]:
    candidates: List[Any] = []
    candidates.extend([obj.get("session_id"), obj.get("thread_id")])

    thread = obj.get("thread")
    if isinstance(thread, dict):
        candidates.append(thread.get("id"))
    session = obj.get("session")
    if isinstance(session, dict):
        candidates.append(session.get("id"))

    data = obj.get("data")
    if isinstance(data, dict):
        candidates.extend([data.get("session_id"), data.get("thread_id")])
        thread2 = data.get("thread")
        if isinstance(thread2, dict):
            candidates.append(thread2.get("id"))
        session2 = data.get("session")
        if isinstance(session2, dict):
            candidates.append(session2.get("id"))

    for cand in candidates:
        uuid_val = looks_like_uuid(cand)
        if uuid_val:
            return uuid_val
    return None


def get_event_type(obj: Dict[str, Any]) -> str:
    for key in ("type", "event", "kind", "name"):
        val = obj.get(key)
        if isinstance(val, str) and val.strip():
            return val.strip()
    return ""


def extract_text_delta(obj: Dict[str, Any]) -> Optional[str]:
    # На практике у разных версий/провайдеров поля могут отличаться.
    for key in ("delta", "text", "content"):
        val = obj.get(key)
        if isinstance(val, str) and val:
            return val
    # Вариант вида: {"data": {"text": "..."}}
    data = obj.get("data")
    if isinstance(data, dict):
        for key in ("delta", "text", "content"):
            val = data.get(key)
            if isinstance(val, str) and val:
                return val
    return None


def extract_item(obj: Dict[str, Any]) -> Optional[Dict[str, Any]]:
    item = obj.get("item")
    if isinstance(item, dict):
        return item
    data = obj.get("data")
    if isinstance(data, dict):
        item2 = data.get("item")
        if isinstance(item2, dict):
            return item2
    return None


def extract_item_type(item: Dict[str, Any]) -> str:
    val = item.get("type")
    return val.strip() if isinstance(val, str) else ""


def extract_item_text(item: Dict[str, Any]) -> Optional[str]:
    for key in ("delta", "text", "content"):
        val = item.get(key)
        if isinstance(val, str) and val:
            return val
    return None


def extract_tool_command(obj: Dict[str, Any]) -> Optional[str]:
    # Ожидаем что-то вроде:
    # {"type":"tool_use","name":"shell_command","input":{"command":"ls"}} или варианты.
    for key in ("command", "cmd"):
        val = obj.get(key)
        if isinstance(val, str) and val.strip():
            return val.strip()

    data = obj.get("data")
    if isinstance(data, dict):
        for key in ("command", "cmd"):
            val = data.get(key)
            if isinstance(val, str) and val.strip():
                return val.strip()

    tool_input = obj.get("input")
    if isinstance(tool_input, dict):
        cmd = tool_input.get("command")
        if isinstance(cmd, str) and cmd.strip():
            return cmd.strip()
    if isinstance(data, dict):
        tool_input2 = data.get("input")
        if isinstance(tool_input2, dict):
            cmd = tool_input2.get("command")
            if isinstance(cmd, str) and cmd.strip():
                return cmd.strip()

    return None


def extract_tool_output(obj: Dict[str, Any]) -> Optional[str]:
    for key in ("output", "stdout", "result", "text"):
        val = obj.get(key)
        if isinstance(val, str) and val:
            return val
    data = obj.get("data")
    if isinstance(data, dict):
        for key in ("output", "stdout", "result", "text"):
            val = data.get(key)
            if isinstance(val, str) and val:
                return val
    return None


def maybe_extract_diff(obj: Dict[str, Any]) -> Optional[str]:
    for key in ("diff", "patch", "unified_diff"):
        val = obj.get(key)
        if isinstance(val, str) and val.strip():
            return val
    data = obj.get("data")
    if isinstance(data, dict):
        for key in ("diff", "patch", "unified_diff"):
            val = data.get(key)
            if isinstance(val, str) and val.strip():
                return val
    return None

