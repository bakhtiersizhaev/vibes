from __future__ import annotations

import sys
from pathlib import Path


def _add_src_to_sys_path() -> None:
    repo_root = Path(__file__).resolve().parent
    src = repo_root / "src"
    if not src.exists() or not src.is_dir():
        return

    src_str = str(src)
    if sys.path and sys.path[0] == src_str:
        return
    if src_str in sys.path:
        sys.path.remove(src_str)
    sys.path.insert(0, src_str)


_add_src_to_sys_path()

