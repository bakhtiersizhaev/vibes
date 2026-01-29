from __future__ import annotations

from typing import Any, Dict, List, Optional, Tuple

from ..constants import LABEL_BACK
from ..telegram_deps import InlineKeyboardButton, InlineKeyboardMarkup
from ..utils.paths import shorten_path
from ..utils.text import h as _h
from .callbacks import cb as _cb
from .ui_state import _ui_get, _ui_set
from .ui_run import _status_emoji


def _home_keyboard() -> InlineKeyboardMarkup:
    return InlineKeyboardMarkup(
        [
            [
                InlineKeyboardButton("📂", callback_data=_cb("sessions")),
                InlineKeyboardButton("➕", callback_data=_cb("new")),
            ],
        ]
    )


def _render_home(manager: "SessionManager", *, notice: Optional[str] = None) -> Tuple[str, InlineKeyboardMarkup]:
    admin_note = ""
    if getattr(manager, "_admin_id", None) is None:
        admin_note = "\n\n<i>Warning:</i> this bot is running without <code>--admin</code> — anyone who finds it can control it."

    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    text_html = (
        f"{notice_html}"
        "<b>Vibes</b> is a lightweight session manager for Codex CLI.\n\n"
        "It keeps this chat clean by editing a single panel message and deleting your messages.\n\n"
        "Use the buttons below to manage sessions, pick working directories, and run prompts."
        f"{admin_note}"
    )
    return text_html, _home_keyboard()


def _render_sessions_list(
    manager: "SessionManager",
    *,
    chat_data: Dict[str, Any],
    notice: Optional[str] = None,
) -> Tuple[str, InlineKeyboardMarkup]:
    names = sorted(manager.sessions.keys())
    _ui_set(chat_data, sess_list=names)

    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    if not names:
        text_html = (
            f"{notice_html}"
            "<b>Vibes</b> is a lightweight session manager for Codex CLI.\n\n"
            "Choose or create session:"
        )

        kb = InlineKeyboardMarkup(
            [
                [InlineKeyboardButton("➕", callback_data=_cb("new"))],
                [InlineKeyboardButton("🔄", callback_data=_cb("restart"))],
            ]
        )
        return text_html, kb

    rows: List[List[InlineKeyboardButton]] = []
    for i, name in enumerate(names):
        rec = manager.sessions[name]
        label = f"{_status_emoji(rec)} {name}"
        rows.append([InlineKeyboardButton(label, callback_data=_cb("sess", str(i)))])

    rows.append([InlineKeyboardButton("➕", callback_data=_cb("new"))])
    rows.append([InlineKeyboardButton("🔄", callback_data=_cb("restart"))])
    text_html = (
        f"{notice_html}"
        "<b>Vibes</b> is a lightweight session manager for Codex CLI.\n\n"
        "Choose or create session:"
    )
    return text_html, InlineKeyboardMarkup(rows)


def _render_new_name(
    manager: "SessionManager",
    *,
    chat_data: Dict[str, Any],
    notice: Optional[str] = None,
) -> Tuple[str, InlineKeyboardMarkup]:
    auto_name = manager.next_auto_session_name()
    _ui_set(chat_data, auto_name=auto_name)
    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    text_html = (
        f"{notice_html}"
        "<b>Step 1/2 — Name</b>\n\n"
        "Send a session name: <code>a-zA-Z0-9._-</code>.\n"
        "Or tap the suggested name below."
    )
    kb = InlineKeyboardMarkup(
        [
            [InlineKeyboardButton(f"{auto_name}", callback_data=_cb("new_auto"))],
            [InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back"))],
        ]
    )
    return text_html, kb


def _render_new_path(
    manager: "SessionManager",
    *,
    chat_data: Dict[str, Any],
    notice: Optional[str] = None,
    notice_code: Optional[str] = None,
) -> Tuple[str, InlineKeyboardMarkup]:
    ui = _ui_get(chat_data)
    draft = ui.get("new")

    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    notice_code_html = f"<b>Путь:</b> <code>{_h(notice_code)}</code>\n\n" if notice_code else ""
    text_html = (
        f"{notice_html}"
        "<b>Step 2/2 — Path</b>\n\n"
        f"{notice_code_html}"
        "Send a directory path, or choose a preset below.\n\n"
        "<i>Tip: you can use <code>~/</code> as your home directory.</i>\n"
        "<i>For example: <code>~/projects/my-app</code></i>\n\n"
        "<b>Click on path to copy!</b>"
    )

    rows: List[List[InlineKeyboardButton]] = []
    for i, p in enumerate(manager.path_presets):
        rows.append([InlineKeyboardButton(f"📁 {shorten_path(p)}", callback_data=_cb("path_pick", str(i)))])

    rows.append([InlineKeyboardButton("⚙️", callback_data=_cb("paths"))])
    rows.append([InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back"))])
    return text_html, InlineKeyboardMarkup(rows)
