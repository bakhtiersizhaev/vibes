from __future__ import annotations

import os
import re
from pathlib import Path
from typing import Optional, Tuple


def safe_session_name(name: str) -> Optional[str]:
    name = name.strip()
    if not name:
        return None
    if len(name) > 64:
        return None
    if not re.fullmatch(r"[a-zA-Z0-9._-]+", name):
        return None
    return name


def can_create_directory(path: Path) -> bool:
    """
    Best-effort check whether `path` (which does not exist yet) can likely be created.
    """
    try:
        if path.exists():
            return False
    except Exception:
        return False

    parent = path.parent
    while True:
        try:
            if parent.exists():
                if not parent.is_dir():
                    return False
                return bool(os.access(parent, os.W_OK | os.X_OK))
        except Exception:
            return False

        if parent.parent == parent:
            return False
        parent = parent.parent


def safe_resolve_path(raw: str) -> Tuple[Optional[Path], str]:
    raw_s = (raw or "").strip()
    if not raw_s:
        return None, "Empty path."
    if "\x00" in raw_s:
        return None, "Invalid path: contains NUL byte."
    try:
        p = Path(raw_s).expanduser()
    except Exception as e:
        return None, f"Invalid path: {raw_s!r} ({e})"
    try:
        return p.resolve(), ""
    except Exception as e:
        return None, f"Failed to resolve path: {raw_s!r} ({e})"


def shorten_path(path: str, *, max_len: int = 34) -> str:
    p = path.strip()
    if len(p) <= max_len:
        return p
    parts = p.replace("\\", "/").split("/")
    tail = "/".join(parts[-2:]) if len(parts) >= 2 else parts[-1]
    if len(tail) + 2 >= max_len:
        return "…" + tail[-(max_len - 1) :]
    return f"…/{tail}"

