from __future__ import annotations

import asyncio
import dataclasses
import time
from pathlib import Path
from typing import Deque, Optional

from ..constants import DEFAULT_MODEL, DEFAULT_REASONING_EFFORT
from ..utils.logging import utc_now_iso
from ..telegram.stream import TelegramStream


@dataclasses.dataclass
class SessionRun:
    process: asyncio.subprocess.Process
    stdout_task: asyncio.Task[None]
    stderr_task: asyncio.Task[None]
    stream: TelegramStream
    stdout_log: Path
    stderr_log: Path
    stderr_tail: Deque[str]
    started_mono: float = dataclasses.field(default_factory=time.monotonic)
    last_cmd: Optional[str] = None
    stop_requested: bool = False
    confirm_stop: bool = False
    header_note: Optional[str] = None
    paused: bool = False


@dataclasses.dataclass
class SessionRecord:
    name: str
    path: str
    thread_id: Optional[str] = None
    model: str = DEFAULT_MODEL
    reasoning_effort: str = DEFAULT_REASONING_EFFORT
    status: str = "idle"  # idle | running | error | stopped
    last_result: str = "never"  # never | success | error | stopped
    created_at: str = dataclasses.field(default_factory=utc_now_iso)
    last_active: Optional[str] = None
    last_stdout_log: Optional[str] = None
    last_stderr_log: Optional[str] = None
    last_run_duration_s: Optional[int] = None
    pending_delete: bool = False
    run: Optional[SessionRun] = None

