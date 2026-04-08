#!/usr/bin/env python3
from __future__ import annotations

import re
import subprocess
from datetime import datetime
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
STATUS_PATH = REPO_ROOT / "STATUS.MD"


def _git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(REPO_ROOT), *args], text=True).strip()


def _branch_name() -> str:
    try:
        return _git("symbolic-ref", "--short", "HEAD")
    except subprocess.CalledProcessError:
        return "detached"


def _timestamp() -> str:
    now = datetime.now().astimezone()
    offset = now.strftime("%z")
    if len(offset) == 5:
        offset = f"{offset[:3]}:{offset[3:]}"
    return f"{now.strftime('%Y-%m-%d %H:%M')} {offset}"


def _replace_line(text: str, prefix: str, value: str) -> str:
    pattern = rf"(?m)^{re.escape(prefix)}.*$"
    replacement = f"{prefix}{value}"
    if re.search(pattern, text):
        return re.sub(pattern, replacement, text, count=1)
    return f"{replacement}\n{text}"


def main() -> int:
    text = STATUS_PATH.read_text(encoding="utf-8")
    updated = text
    updated = _replace_line(updated, "Last updated: ", _timestamp())
    updated = _replace_line(updated, "Branch: ", _branch_name())
    updated = _replace_line(updated, "Repo: ", str(REPO_ROOT))
    updated = _replace_line(updated, "Main SHA: ", _git("rev-parse", "HEAD"))

    if updated != text:
        STATUS_PATH.write_text(updated, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
