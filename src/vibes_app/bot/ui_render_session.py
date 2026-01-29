from __future__ import annotations

import time
from typing import Any, Dict, Optional, Tuple

from ..constants import LABEL_BACK, MAX_TELEGRAM_CHARS, RUN_START_WAIT_NOTE
from ..core.session_models import SessionRecord
from ..telegram_deps import InlineKeyboardButton, InlineKeyboardMarkup
from ..utils.log_files import (
    extract_last_agent_message_from_stdout_log,
    preview_from_stderr_log,
    preview_from_stdout_log,
)
from ..utils.text import h as _h
from ..utils.text import tail_text as _tail_text
from ..utils.text import truncate_text as _truncate_text
from ..utils.time import format_duration as _format_duration
from .callbacks import cb as _cb
from .ui_render_home import _render_sessions_list
from .ui_run import _is_running


def _render_session_compact_info(rec: SessionRecord) -> str:
    return f"<code>{_h(rec.model)}</code> <code>{_h(rec.reasoning_effort)}</code>\n<code>{_h(rec.path)}</code>"


def _render_session_view(
    manager: "SessionManager",
    *,
    session_name: str,
    notice: Optional[str] = None,
) -> Tuple[str, InlineKeyboardMarkup]:
    rec = manager.sessions.get(session_name)
    if not rec:
        return _render_sessions_list(manager, chat_data={}, notice=f"Unknown session: {session_name}")

    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    compact_info = _render_session_compact_info(rec)

    if _is_running(rec) and rec.run:
        raw = preview_from_stdout_log(rec.last_stdout_log, max_chars=100000).strip()
        log_tail = _tail_text(raw, 3200) if raw else ""
        start_note_html = f"<i>{_h(RUN_START_WAIT_NOTE)}</i>\n\n" if not log_tail else ""
        elapsed_s = int(time.monotonic() - rec.run.started_mono)
        text_html = (
            f"{notice_html}"
            f"{start_note_html}"
            f"<pre><code>{_h(log_tail)}</code></pre>\n\n"
            f"<code>---- Working {_h(_format_duration(elapsed_s))} ----</code>"
        )
        kb = InlineKeyboardMarkup(
            [
                [
                    InlineKeyboardButton("⬅️", callback_data=_cb("back_sessions")),
                    InlineKeyboardButton("⛔", callback_data=_cb("interrupt")),
                ]
            ]
        )
        return text_html, kb

    never_run = (
        rec.last_result == "never"
        and not rec.thread_id
        and not rec.last_stdout_log
        and not rec.last_stderr_log
        and rec.last_run_duration_s is None
    )

    if never_run:
        text_html = f"{notice_html}{compact_info}\n\n<i>Send a prompt to start.</i>"
        kb = InlineKeyboardMarkup(
            [
                [InlineKeyboardButton("⚙️", callback_data=_cb("model"))],
                [
                    InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back")),
                    InlineKeyboardButton("🗑", callback_data=_cb("delete")),
                ],
            ]
        )
        return text_html, kb

    stdout_plain = preview_from_stdout_log(rec.last_stdout_log, max_chars=100000).strip()
    stderr_plain = preview_from_stderr_log(rec.last_stderr_log, max_chars=100000).strip()
    log_plain = stdout_plain or stderr_plain or "(empty)"

    status_kind = "worked"
    if rec.last_result == "stopped" or rec.status == "stopped":
        status_kind = "stopped"
    elif rec.last_result == "error" or rec.status == "error":
        status_kind = "failed"

    duration_s = rec.last_run_duration_s if isinstance(rec.last_run_duration_s, int) else 0
    duration_label = _format_duration(duration_s)
    status_line = {
        "worked": f"<code>---- Worked for {_h(duration_label)} ----</code>",
        "stopped": f"<code>---- Stopped after {_h(duration_label)} ----</code>",
        "failed": f"<code>---- Failed after {_h(duration_label)} ----</code>",
    }[status_kind]

    result_plain = extract_last_agent_message_from_stdout_log(rec.last_stdout_log, max_chars=100000).strip() or ""

    log_max = 2600
    result_max = 1400
    for _ in range(10):
        log_tail = _tail_text(log_plain, log_max)
        result_view = result_plain
        if result_view and len(result_view) > result_max:
            result_view = _truncate_text(result_view, result_max)

        if "\n" in result_view:
            result_html = f"<pre><code>{_h(result_view)}</code></pre>" if result_view else ""
        else:
            result_html = _h(result_view) if result_view else ""

        parts = [
            notice_html.rstrip(),
            f"<pre><code>{_h(log_tail)}</code></pre>",
            compact_info,
            status_line,
        ]
        if result_html:
            parts.append(result_html)
        parts.append("Send a prompt to continue.")

        text_html = "\n\n".join([p for p in parts if p])
        if len(text_html) <= MAX_TELEGRAM_CHARS:
            break
        if log_max > 900:
            log_max = max(900, int(log_max * 0.8))
            continue
        if result_max > 300:
            result_max = max(300, int(result_max * 0.8))
            continue
        break

    kb = InlineKeyboardMarkup(
        [
            [InlineKeyboardButton("🆕", callback_data=_cb("clear")), InlineKeyboardButton("⚙️", callback_data=_cb("model"))],
            [
                InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back")),
                InlineKeyboardButton("🗑", callback_data=_cb("delete")),
            ],
        ]
    )
    return text_html, kb


def _render_logs_view(
    manager: "SessionManager",
    *,
    session_name: str,
    notice: Optional[str] = None,
) -> Tuple[str, InlineKeyboardMarkup]:
    rec = manager.sessions.get(session_name)
    if not rec:
        return _render_sessions_list(manager, chat_data={}, notice=f"Unknown session: {session_name}")

    last_msg = extract_last_agent_message_from_stdout_log(rec.last_stdout_log, max_chars=3200)
    if not last_msg:
        last_msg = preview_from_stdout_log(rec.last_stdout_log, max_chars=3200)
    if not last_msg:
        last_msg = "(empty)"

    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    text_html = (
        f"{notice_html}"
        f"<b>Log</b> <code>{_h(rec.name)}</code>\n\n"
        f"{_render_session_compact_info(rec)}\n\n"
        f"<pre><code>{_h(last_msg)}</code></pre>"
    )

    kb = InlineKeyboardMarkup([[InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back"))]])
    return text_html, kb

