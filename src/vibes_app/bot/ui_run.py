from __future__ import annotations

import html
from typing import Optional

from ..constants import LABEL_BACK, STOP_CONFIRM_QUESTION
from ..telegram_deps import InlineKeyboardButton, InlineKeyboardMarkup
from .callbacks import cb as _cb
from ..core.session_models import SessionRecord


def _h(text: str) -> str:
    return html.escape(text)


def _build_running_header_plain(rec: SessionRecord, *, note: Optional[str] = None) -> str:
    model = rec.model
    reasoning_effort = rec.reasoning_effort
    lines = [
        f"Session: {rec.name}",
        f"Path: {rec.path}",
        f"Model: {model}",
        f"Reasoning effort: {reasoning_effort}",
        f"Status: {rec.status}",
    ]
    if note:
        lines.append(note)
    return "\n".join(lines)


def _build_running_header_plain_len(rec: SessionRecord, *, note: Optional[str] = None) -> int:
    return len(_build_running_header_plain(rec, note=note))


def _build_running_header_html(rec: SessionRecord, *, note: Optional[str] = None) -> str:
    model = rec.model
    reasoning_effort = rec.reasoning_effort
    note_line = f"\n<i>{_h(note)}</i>" if note else ""
    return (
        f"<b>Session:</b> <code>{_h(rec.name)}</code>\n"
        f"<b>Path:</b> <code>{_h(rec.path)}</code>\n"
        f"<b>Model:</b> <code>{_h(model)}</code>\n"
        f"<b>Reasoning effort:</b> <code>{_h(reasoning_effort)}</code>\n"
        f"<b>Status:</b> {_h(rec.status)}"
        f"{note_line}"
    )


_STOP_CONFIRM_QUESTION = STOP_CONFIRM_QUESTION


def _detach_keyboard() -> InlineKeyboardMarkup:
    return InlineKeyboardMarkup([[InlineKeyboardButton(LABEL_BACK, callback_data=_cb("detach"))]])


def _stop_confirm_keyboard() -> InlineKeyboardMarkup:
    return InlineKeyboardMarkup(
        [
            [
                InlineKeyboardButton("✅ Yes, stop", callback_data=_cb("stop_yes")),
                InlineKeyboardButton("❌ No", callback_data=_cb("stop_no")),
            ]
        ]
    )


def _status_emoji(rec: SessionRecord) -> str:
    if rec.status == "running":
        return "🟢"
    if rec.last_result == "success" and rec.status == "idle":
        return "✅"
    if rec.status == "stopped" or rec.last_result == "stopped":
        return "⏹"
    if rec.status == "error" or rec.last_result == "error":
        return "❌"
    if rec.last_result == "never":
        return "🆕"
    return "⚪️"


def _is_running(rec: SessionRecord) -> bool:
    return bool(rec.run and rec.run.process.returncode is None and rec.status == "running")


async def _show_stop_confirmation_in_stream(rec: SessionRecord) -> None:
    if not rec.run:
        return
    rec.run.confirm_stop = True
    rec.run.header_note = _STOP_CONFIRM_QUESTION
    await rec.run.stream.set_header(
        header_html=_build_running_header_html(rec, note=_STOP_CONFIRM_QUESTION),
        header_plain_len=_build_running_header_plain_len(rec, note=_STOP_CONFIRM_QUESTION),
    )
    await rec.run.stream.set_reply_markup(_stop_confirm_keyboard())


async def _restore_run_stream_ui(rec: SessionRecord) -> None:
    if not rec.run:
        return
    rec.run.confirm_stop = False
    rec.run.header_note = None
    await rec.run.stream.set_header(
        header_html=_build_running_header_html(rec),
        header_plain_len=_build_running_header_plain_len(rec),
    )
    await rec.run.stream.set_reply_markup(_detach_keyboard())
