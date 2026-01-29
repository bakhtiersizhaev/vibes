from __future__ import annotations

import time
from typing import Any

from ..telegram_deps import InlineKeyboardButton, InlineKeyboardMarkup
from ..utils.logging import log_error
from ..utils.text import h as _h
from ..utils.time import format_duration as _format_duration
from .callbacks import cb as _cb

from .ui_run import _is_running


async def auto_detach_if_running(manager: Any, *, chat_id: int, message_id: int) -> None:
    session_name = manager.resolve_attached_running_session_for_message(chat_id=chat_id, message_id=message_id)
    if not session_name:
        session_name = manager.resolve_session_for_run_message(chat_id=chat_id, message_id=message_id)
    if not session_name:
        return

    rec = manager.sessions.get(session_name)
    if not rec or not _is_running(rec) or not rec.run:
        return
    if rec.run.paused:
        return
    rec.run.paused = True
    await rec.run.stream.pause()


async def attach_running_session(
    manager: Any,
    *,
    chat_id: int,
    message_id: int,
    rec: Any,
    reason: str,
) -> None:
    if not rec or not _is_running(rec) or not rec.run:
        return

    try:
        await manager.pause_other_attached_runs(chat_id=chat_id, message_id=message_id, except_session=rec.name)
    except Exception as e:
        log_error(f"pause_other_attached_runs failed ({reason}).", e)

    manager.register_run_message(chat_id=chat_id, message_id=message_id, session_name=rec.name)
    rec.run.paused = False

    def _working_footer_html() -> str:
        elapsed_s = int(time.monotonic() - rec.run.started_mono)
        return f"<code>---- Working {_h(_format_duration(elapsed_s))} ----</code>"

    await rec.run.stream.set_footer(
        footer_provider=_working_footer_html,
        footer_plain_len=len("---- Working 0m 0s ----"),
        wrap_log_in_pre=True,
    )
    await rec.run.stream.set_reply_markup(
        InlineKeyboardMarkup(
            [
                [
                    InlineKeyboardButton("⬅️", callback_data=_cb("back_sessions")),
                    InlineKeyboardButton("⛔", callback_data=_cb("interrupt")),
                ]
            ]
        )
    )
    await rec.run.stream.resume()
