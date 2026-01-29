from __future__ import annotations

from typing import Any, Optional

from ..constants import UUID_RE


def looks_like_uuid(value: Any) -> Optional[str]:
    if isinstance(value, str):
        m = UUID_RE.search(value)
        if m:
            return m.group(0)
    return None


def find_first_uuid(obj: Any, max_depth: int = 6) -> Optional[str]:
    seen: set[int] = set()

    def walk(node: Any, depth: int) -> Optional[str]:
        if depth > max_depth:
            return None
        node_id = id(node)
        if node_id in seen:
            return None
        seen.add(node_id)

        uuid_val = looks_like_uuid(node)
        if uuid_val:
            return uuid_val

        if isinstance(node, dict):
            for key in ("session_id", "thread_id", "id"):
                if key in node:
                    uuid_val2 = looks_like_uuid(node.get(key))
                    if uuid_val2:
                        return uuid_val2
            for val in node.values():
                found = walk(val, depth + 1)
                if found:
                    return found
            return None

        if isinstance(node, list):
            for val in node:
                found = walk(val, depth + 1)
                if found:
                    return found
            return None

        return None

    return walk(obj, 0)

