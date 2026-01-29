from __future__ import annotations

from ..constants import CB_PREFIX


def cb(*parts: str) -> str:
    safe_parts = [CB_PREFIX]
    for p in parts:
        safe_parts.append(p.replace(":", "_"))
    return ":".join(safe_parts)

