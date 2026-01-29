from __future__ import annotations

import os
from pathlib import Path
from typing import Dict, Optional, Sequence


def parse_env_file(path: Path) -> Dict[str, str]:
    try:
        content = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return {}

    out: Dict[str, str] = {}
    for raw_line in content.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[len("export ") :].lstrip()
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if not key:
            continue

        if value and value[0] not in ("'", '"'):
            idx = value.find(" #")
            if idx != -1:
                value = value[:idx].rstrip()

        if len(value) >= 2 and value[0] == value[-1] and value[0] in ("'", '"'):
            value = value[1:-1]

        out[key] = value
    return out


def pick_str(cli_value: Optional[str], file_env: Dict[str, str], keys: Sequence[str]) -> Optional[str]:
    if isinstance(cli_value, str) and cli_value.strip():
        return cli_value.strip()
    for key in keys:
        v = os.environ.get(key)
        if isinstance(v, str) and v.strip():
            return v.strip()
    for key in keys:
        v = file_env.get(key)
        if isinstance(v, str) and v.strip():
            return v.strip()
    return None


def pick_int(cli_value: Optional[int], file_env: Dict[str, str], keys: Sequence[str]) -> Optional[int]:
    if isinstance(cli_value, int):
        return cli_value
    raw = pick_str(None, file_env, keys)
    if raw is None:
        return None
    try:
        return int(raw)
    except ValueError:
        return None


def update_env_file(path: Path, updates: Dict[str, Optional[str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        existing = path.read_text(encoding="utf-8").splitlines()
    except FileNotFoundError:
        existing = []

    keep: list[str] = []
    for line in existing:
        stripped = line.strip()
        if stripped.startswith("export "):
            stripped = stripped[len("export ") :].lstrip()
        if "=" in stripped:
            key = stripped.split("=", 1)[0].strip()
            if key in updates:
                continue
        keep.append(line)

    for key, value in updates.items():
        if value is None:
            continue
        keep.append(f"{key}={value}")

    content = "\n".join(keep).rstrip() + "\n"
    path.write_text(content, encoding="utf-8")
    try:
        path.chmod(0o600)
    except Exception:
        pass

