from __future__ import annotations

import datetime as dt
import traceback
from pathlib import Path
from typing import Optional

from .. import runtime


def utc_now_iso() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat()


def log_line(message: str, *, log_path: Optional[Path] = None) -> None:
    line = f"[{utc_now_iso()}] {message}\n"
    try:
        path = log_path or runtime.BOT_LOG_PATH
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as f:
            f.write(line)
    except Exception:
        # Keep the bot functional even when file logging fails.
        try:
            import sys

            sys.stderr.write(line)
        except Exception:
            pass


def log_error(msg: str, exc: Optional[BaseException] = None, *, log_path: Optional[Path] = None) -> None:
    tail = ""
    if exc is not None:
        tb = "".join(traceback.format_exception(type(exc), exc, exc.__traceback__))
        tail = f"\n{tb}"
    log_line(f"{msg}{tail}", log_path=log_path)
