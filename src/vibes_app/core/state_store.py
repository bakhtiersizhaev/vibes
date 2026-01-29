from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Dict, Tuple

from .. import runtime


def atomic_write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    tmp_path.write_text(content, encoding="utf-8")
    import os

    os.replace(tmp_path, path)


def rewrite_legacy_log_path(path_str: str, *, log_dir: Path) -> str:
    if not path_str:
        return path_str
    try:
        p = Path(path_str)
    except Exception:
        return path_str

    candidates = [runtime.LEGACY_LOG_DIR, runtime.LEGACY_LOG_DIR.resolve(), (Path.cwd() / runtime.LEGACY_LOG_DIR).resolve()]

    for base in candidates:
        try:
            rel = p.relative_to(base)
        except Exception:
            continue
        return str(log_dir / rel)
    return path_str


def rewrite_state_paths_for_runtime_dir(raw: Dict[str, Any], *, log_dir: Path) -> Tuple[Dict[str, Any], bool]:
    sessions = raw.get("sessions")
    if not isinstance(sessions, dict):
        return raw, False

    changed = False
    for payload in sessions.values():
        if not isinstance(payload, dict):
            continue
        for key in ("last_stdout_log", "last_stderr_log"):
            val = payload.get(key)
            if not isinstance(val, str) or not val:
                continue
            rewritten = rewrite_legacy_log_path(val, log_dir=log_dir)
            if rewritten != val:
                payload[key] = rewritten
                changed = True

    return raw, changed


def maybe_migrate_runtime_files() -> None:
    """
    Best-effort migration to keep all runtime state under `.vibes/`.
    Skips migration if paths were monkeypatched (e.g. in tests).
    """
    if (
        runtime.STATE_PATH != runtime.DEFAULT_STATE_PATH
        or runtime.LOG_DIR != runtime.DEFAULT_LOG_DIR
        or runtime.BOT_LOG_PATH != runtime.DEFAULT_BOT_LOG_PATH
    ):
        return

    try:
        runtime.DEFAULT_RUNTIME_DIR.mkdir(parents=True, exist_ok=True)
    except Exception:
        return

    if runtime.LEGACY_BOT_LOG_PATH.exists() and not runtime.BOT_LOG_PATH.exists():
        try:
            runtime.LEGACY_BOT_LOG_PATH.rename(runtime.BOT_LOG_PATH)
        except Exception:
            pass

    if runtime.LEGACY_LOG_DIR.exists() and not runtime.LOG_DIR.exists():
        try:
            runtime.LEGACY_LOG_DIR.rename(runtime.LOG_DIR)
        except Exception:
            pass

    if runtime.LEGACY_STATE_PATH.exists() and not runtime.STATE_PATH.exists():
        raw: Any
        try:
            raw = json.loads(runtime.LEGACY_STATE_PATH.read_text(encoding="utf-8"))
        except Exception:
            return
        if not isinstance(raw, dict):
            return
        raw2, changed2 = rewrite_state_paths_for_runtime_dir(raw, log_dir=runtime.LOG_DIR)
        if changed2:
            try:
                atomic_write_text(runtime.STATE_PATH, json.dumps(raw2, ensure_ascii=False, indent=2))
            except Exception:
                return
        else:
            try:
                runtime.LEGACY_STATE_PATH.rename(runtime.STATE_PATH)
            except Exception:
                pass

