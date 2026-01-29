from __future__ import annotations

import re
from typing import List


STATE_VERSION = 4

MAX_TELEGRAM_CHARS = 4096
EDIT_THROTTLE_SECONDS = 2.0
STDERR_TAIL_LINES = 80
UI_PREVIEW_MAX_CHARS = 2400
UI_TAIL_MAX_BYTES = 64 * 1024

MEDIA_GROUP_DEBOUNCE_SECONDS = 0.8
MAX_DOWNLOADED_FILENAME_LEN = 180

UUID_RE = re.compile(
    r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b"
)

CB_PREFIX = "v3"

DEFAULT_MODEL_PRESETS: List[str] = [
    # Keep this list short; prefer reading the user's Codex config below.
    "gpt-5.2-codex",
    "gpt-5.1-codex-max",
    "gpt-5.1-codex-mini",
    "gpt-5.2",
]

DEFAULT_MODEL = "gpt-5.2"
DEFAULT_REASONING_EFFORT = "high"

CODEX_SANDBOX_MODES = {"read-only", "workspace-write", "danger-full-access"}
CODEX_APPROVAL_POLICIES = {"untrusted", "on-failure", "on-request", "never"}

LABEL_BACK = "⬅️"
LABEL_LOG = "📜"
LABEL_START = "🚀"

RUN_START_WAIT_NOTE = (
    "The request has been sent. During startup (especially for larger models), the first logs may appear after about one minute — please wait…"
)

STOP_CONFIRM_QUESTION = "Are you sure you want to stop this run?"

