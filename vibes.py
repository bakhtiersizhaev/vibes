#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""
vibes.py — Telegram-бот “session manager” для Codex CLI.

This file is intentionally small: it is a compatibility shim used by tests (`import vibes`)
and a thin runtime entrypoint (`python vibes.py`).
All implementation lives under `src/vibes_app/*`.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Optional


_REPO_ROOT = Path(__file__).resolve().parent
_SRC_DIR = _REPO_ROOT / "src"
if _SRC_DIR.exists() and str(_SRC_DIR) not in sys.path:
    sys.path.insert(0, str(_SRC_DIR))


from vibes_app import runtime as _runtime  # noqa: E402
from vibes_app.constants import (  # noqa: E402
    CB_PREFIX,
    LABEL_BACK,
    LABEL_LOG,
    LABEL_START,
    MAX_DOWNLOADED_FILENAME_LEN,
)
from vibes_app.core.codex_cmd import MODEL_PRESETS  # noqa: E402
from vibes_app.core.codex_events import extract_session_id_explicit as _extract_session_id_explicit  # noqa: E402
from vibes_app.core.codex_events import extract_text_delta as _extract_text_delta  # noqa: E402
from vibes_app.core.codex_events import extract_tool_command as _extract_tool_command  # noqa: E402
from vibes_app.core.codex_events import extract_tool_output as _extract_tool_output  # noqa: E402
from vibes_app.core.codex_events import get_event_type as _get_event_type  # noqa: E402
from vibes_app.core.codex_events import maybe_extract_diff as _maybe_extract_diff  # noqa: E402
from vibes_app.core.session_manager import SessionManager as _CoreSessionManager  # noqa: E402
from vibes_app.core.session_models import SessionRecord, SessionRun  # noqa: E402
from vibes_app.telegram.panel import PanelUI  # noqa: E402
from vibes_app.telegram.stream import Segment, TelegramStream  # noqa: E402
from vibes_app.telegram_deps import RetryAfter  # noqa: E402
from vibes_app.utils.log_files import (  # noqa: E402
    extract_last_agent_message_from_stdout_log as _extract_last_agent_message_from_stdout_log,
)
from vibes_app.utils.log_files import preview_from_stderr_log as _preview_from_stderr_log  # noqa: E402
from vibes_app.utils.log_files import preview_from_stdout_log as _preview_from_stdout_log  # noqa: E402
from vibes_app.utils.paths import safe_resolve_path as _safe_resolve_path  # noqa: E402
from vibes_app.utils.paths import safe_session_name as _safe_session_name  # noqa: E402
from vibes_app.utils.text import parse_tokens as _parse_tokens  # noqa: E402
from vibes_app.utils.text import truncate_text as _truncate_text  # noqa: E402
from vibes_app.utils.uuid import find_first_uuid as _find_first_uuid  # noqa: E402
from vibes_app.utils.uuid import looks_like_uuid as _looks_like_uuid  # noqa: E402

from vibes_app.bot.attachments import pick_unique_dest_path as _pick_unique_dest_path  # noqa: E402
from vibes_app.bot.attachments import sanitize_attachment_basename as _sanitize_attachment_basename  # noqa: E402
from vibes_app.bot.callbacks import cb as _cb  # noqa: E402
from vibes_app.bot.handlers_callback import on_callback  # noqa: E402
from vibes_app.bot.handlers_messages import on_text  # noqa: E402
from vibes_app.bot.render_sync import _render_and_sync  # noqa: E402
from vibes_app.bot.ui_render_session import _render_session_view  # noqa: E402
from vibes_app.bot.ui_run import _STOP_CONFIRM_QUESTION  # noqa: E402
from vibes_app.bot.ui_run import _restore_run_stream_ui  # noqa: E402
from vibes_app.bot.ui_run import _show_stop_confirmation_in_stream  # noqa: E402
from vibes_app.bot.ui_run import _status_emoji  # noqa: E402


DEFAULT_RUNTIME_DIR = _runtime.DEFAULT_RUNTIME_DIR
DEFAULT_STATE_PATH = _runtime.DEFAULT_STATE_PATH
DEFAULT_LOG_DIR = _runtime.DEFAULT_LOG_DIR
DEFAULT_BOT_LOG_PATH = _runtime.DEFAULT_BOT_LOG_PATH

# NOTE: tests monkeypatch these module-level paths.
STATE_PATH = DEFAULT_STATE_PATH
LOG_DIR = DEFAULT_LOG_DIR
BOT_LOG_PATH = DEFAULT_BOT_LOG_PATH


class SessionManager(_CoreSessionManager):
    def __init__(self, *, admin_id: Optional[int]) -> None:
        super().__init__(
            admin_id=admin_id,
            state_path=STATE_PATH,
            log_dir=LOG_DIR,
            bot_log_path=BOT_LOG_PATH,
            telegram_stream_cls=TelegramStream,
            panel_ui_cls=PanelUI,
        )


def main() -> None:
    from vibes_app.bot.app import main as _main  # noqa: E402

    _main()


if __name__ == "__main__":
    main()
