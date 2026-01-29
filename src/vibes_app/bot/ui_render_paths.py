from __future__ import annotations

from typing import Any, Dict, List, Optional, Tuple

from ..constants import LABEL_BACK
from ..core.session_models import SessionRecord
from ..telegram_deps import InlineKeyboardButton, InlineKeyboardMarkup
from ..utils.text import h as _h
from .callbacks import cb as _cb
from .ui_state import _ui_get


def _render_paths(
    manager: "SessionManager",
    *,
    chat_data: Dict[str, Any],
    notice: Optional[str] = None,
) -> Tuple[str, InlineKeyboardMarkup]:
    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    lines = ["<b>Paths presets</b>", "", "These appear as quick buttons in the New session wizard.", ""]
    if manager.path_presets:
        for i, p in enumerate(manager.path_presets, start=1):
            lines.append(f"{i}. <code>{_h(p)}</code>")
    else:
        lines.append("<i>No presets yet.</i>")

    rows: List[List[InlineKeyboardButton]] = []
    rows.append([InlineKeyboardButton("➕", callback_data=_cb("paths_add"))])
    del_buttons: List[InlineKeyboardButton] = []
    for i, _p in enumerate(manager.path_presets):
        label = f"🗑 #{i+1}"
        del_buttons.append(InlineKeyboardButton(label, callback_data=_cb("path_del", str(i))))
    for i in range(0, len(del_buttons), 3):
        rows.append(del_buttons[i : i + 3])
    rows.append([InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back"))])
    text_html = notice_html + "\n".join(lines)
    return text_html, InlineKeyboardMarkup(rows)


def _render_paths_add(*, notice: Optional[str] = None, notice_code: Optional[str] = None) -> Tuple[str, InlineKeyboardMarkup]:
    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    notice_code_html = f"<b>Путь:</b> <code>{_h(notice_code)}</code>\n\n" if notice_code else ""
    text_html = (
        f"{notice_html}"
        "<b>Add path preset</b>\n\n"
        f"{notice_code_html}"
        "Send a directory path. I will validate it and add it to presets.\n\n"
        "<i>Tip: you can use <code>~/</code> as your home directory.</i>\n"
        "<i>For example: <code>~/projects/my-app</code></i>\n\n"
        "<b>Click on path to copy!</b>"
    )
    kb = InlineKeyboardMarkup([[InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back"))]])
    return text_html, kb


def _render_confirm_delete(rec: SessionRecord, *, notice: Optional[str] = None) -> Tuple[str, InlineKeyboardMarkup]:
    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    text_html = (
        f"{notice_html}"
        "<b>Delete session?</b>\n\n"
        f"Session: <code>{_h(rec.name)}</code>\n"
        f"Path: <code>{_h(rec.path)}</code>\n\n"
        "<b>This will delete only bot artifacts</b> (state + logs).\n"
        "<b>Your project directory will NOT be deleted.</b>"
    )
    kb = InlineKeyboardMarkup(
        [
            [
                InlineKeyboardButton("✅", callback_data=_cb("delete_yes")),
                InlineKeyboardButton("❌", callback_data=_cb("delete_no")),
            ]
        ]
    )
    return text_html, kb


def _render_confirm_mkdir(*, chat_data: Dict[str, Any], notice: Optional[str] = None) -> Tuple[str, InlineKeyboardMarkup]:
    ui = _ui_get(chat_data)
    mkdir = ui.get("mkdir")
    path = mkdir.get("path") if isinstance(mkdir, dict) else None

    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    if not isinstance(path, str) or not path:
        text_html = f"{notice_html}<b>Create directory?</b>\n\n<i>No pending directory.</i>"
        kb = InlineKeyboardMarkup([[InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back"))]])
        return text_html, kb

    text_html = (
        f"{notice_html}"
        "<b>Create directory?</b>\n\n"
        f"<code>{_h(path)}</code>\n\n"
        "This folder doesn’t exist. Create it (including parents)?"
    )
    kb = InlineKeyboardMarkup(
        [
            [
                InlineKeyboardButton("✅", callback_data=_cb("mkdir_yes")),
                InlineKeyboardButton("❌", callback_data=_cb("mkdir_no")),
            ]
        ]
    )
    return text_html, kb


def _render_confirm_stop(session_name: str, *, notice: Optional[str] = None) -> Tuple[str, InlineKeyboardMarkup]:
    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    text_html = (
        f"{notice_html}"
        "<b>Stop run?</b>\n\n"
        f"Session: <code>{_h(session_name)}</code>\n\n"
        "This will interrupt the current run."
    )
    kb = InlineKeyboardMarkup(
        [
            [
                InlineKeyboardButton("✅", callback_data=_cb("stop_yes")),
                InlineKeyboardButton("❌", callback_data=_cb("stop_no")),
            ]
        ]
    )
    return text_html, kb

