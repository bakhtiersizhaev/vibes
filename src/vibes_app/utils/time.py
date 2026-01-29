from __future__ import annotations


def format_duration(seconds: int) -> str:
    if seconds < 0:
        seconds = 0
    minutes, secs = divmod(int(seconds), 60)
    return f"{minutes}m {secs}s"

