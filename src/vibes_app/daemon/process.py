from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path
from typing import Optional


def detect_local_venv_python(root: Path) -> Optional[Path]:
    candidates = [
        root / ".venv" / "bin" / "python",
        root / ".venv" / "Scripts" / "python.exe",
    ]
    for c in candidates:
        if c.exists():
            return c
    return None


def maybe_reexec_into_venv(root: Path) -> None:
    venv_python = detect_local_venv_python(root)
    if venv_python is None:
        return
    try:
        if Path(sys.executable).resolve() == venv_python.resolve():
            return
    except Exception:
        return
    try:
        os.execv(
            str(venv_python),
            [str(venv_python), str(root / "vibes"), *sys.argv[1:]],
        )
    except Exception:
        return


def pid_is_running(pid: int) -> bool:
    if pid <= 0:
        return False
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def try_get_cmdline(pid: int) -> Optional[str]:
    try:
        import psutil  # type: ignore

        proc = psutil.Process(pid)
        return " ".join(proc.cmdline())
    except Exception:
        pass

    try:
        out = subprocess.check_output(
            ["ps", "-p", str(pid), "-o", "command="],
            text=True,
            stderr=subprocess.DEVNULL,
        )
        s = out.strip()
        return s if s else None
    except Exception:
        return None


def looks_like_vibes_process(cmdline: str, root: Path) -> bool:
    bot_path = str((root / "vibes.py").resolve())
    if bot_path in cmdline:
        return True
    if "vibes.py" in cmdline and str(root.resolve()) in cmdline:
        return True
    if " -m vibes" in cmdline or cmdline.endswith(" -m vibes") or cmdline.endswith(" -m vibes.py"):
        return True
    return False


def format_timedelta(seconds: float) -> str:
    seconds = max(0, int(seconds))
    h, rem = divmod(seconds, 3600)
    m, s = divmod(rem, 60)
    if h:
        return f"{h:02d}:{m:02d}:{s:02d}"
    return f"{m:02d}:{s:02d}"


def parse_ps_etime(etime: str) -> Optional[int]:
    """
    ps etime formats: [[dd-]hh:]mm:ss
    """
    etime = etime.strip()
    if not etime:
        return None

    days = 0
    if "-" in etime:
        d, rest = etime.split("-", 1)
        try:
            days = int(d)
        except ValueError:
            return None
        etime = rest

    parts = etime.split(":")
    if len(parts) == 2:
        hh = "0"
        mm, ss = parts
    elif len(parts) == 3:
        hh, mm, ss = parts
    else:
        return None

    try:
        h = int(hh)
        m = int(mm)
        s = int(ss)
    except ValueError:
        return None

    return (((days * 24) + h) * 60 + m) * 60 + s

