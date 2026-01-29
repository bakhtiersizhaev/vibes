from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any, Dict, Optional


def runtime_dir(root: Path) -> Path:
    return root / ".vibes"


def state_path(runtime_dir_path: Path) -> Path:
    return runtime_dir_path / "daemon.json"


def daemon_log_path(runtime_dir_path: Path) -> Path:
    return runtime_dir_path / "daemon.log"


def load_state(path: Path) -> Optional[Dict[str, Any]]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return None
    except Exception:
        return None
    return data if isinstance(data, dict) else None


def write_state(path: Path, state: Dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(state, ensure_ascii=False, indent=2), encoding="utf-8")
    os.replace(tmp, path)

