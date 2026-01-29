from __future__ import annotations

import subprocess
from pathlib import Path
from typing import Optional


def detect_git_dir(path: Path) -> Optional[Path]:
    """
    Best-effort: return absolute path to the git directory for `path` (usually `.git`).
    Works for:
      - repo root with `.git/`
      - worktrees/submodules with `.git` file pointing to `gitdir: ...`
      - nested paths inside a repo (via `git rev-parse --git-dir`)
    """
    try:
        candidate = path / ".git"
    except Exception:
        candidate = None

    if candidate is not None and candidate.is_dir():
        return candidate.resolve()

    if candidate is not None and candidate.is_file():
        try:
            raw = candidate.read_text(encoding="utf-8", errors="replace").strip()
        except Exception:
            raw = ""
        if raw.lower().startswith("gitdir:"):
            gitdir_str = raw.split(":", 1)[1].strip()
            if gitdir_str:
                gitdir_path = Path(gitdir_str).expanduser()
                if not gitdir_path.is_absolute():
                    gitdir_path = (path / gitdir_path).resolve()
                else:
                    gitdir_path = gitdir_path.resolve()
                if gitdir_path.exists():
                    return gitdir_path

    try:
        out = subprocess.check_output(
            ["git", "-C", str(path), "rev-parse", "--git-dir"],
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
    except Exception:
        return None
    if not out:
        return None
    gitdir_path = Path(out).expanduser()
    if not gitdir_path.is_absolute():
        gitdir_path = (path / gitdir_path).resolve()
    else:
        gitdir_path = gitdir_path.resolve()
    return gitdir_path

