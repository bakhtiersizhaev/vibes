from __future__ import annotations

from typing import Any, Dict, Tuple

from ..telegram_deps import InlineKeyboardMarkup
from .ui_render_home import _render_home, _render_new_name, _render_new_path, _render_sessions_list
from .ui_render_paths import _render_confirm_delete, _render_confirm_mkdir, _render_confirm_stop, _render_paths, _render_paths_add
from .ui_render_session import _render_logs_view, _render_session_view
from .ui_render_settings import _render_await_prompt, _render_model, _render_model_custom
from .ui_state import _ui_get


def _render_current(manager: "SessionManager", *, chat_data: Dict[str, Any]) -> Tuple[str, InlineKeyboardMarkup]:
    ui = _ui_get(chat_data)
    mode = ui.get("mode") if isinstance(ui.get("mode"), str) else "sessions"
    notice = ui.pop("notice", None) if isinstance(ui.get("notice"), str) else None
    notice_code = ui.pop("notice_code", None) if isinstance(ui.get("notice_code"), str) else None

    if mode == "home":
        return _render_home(manager, notice=notice)
    if mode == "sessions":
        return _render_sessions_list(manager, chat_data=chat_data, notice=notice)
    if mode == "new_name":
        return _render_new_name(manager, chat_data=chat_data, notice=notice)
    if mode == "new_path":
        return _render_new_path(manager, chat_data=chat_data, notice=notice, notice_code=notice_code)
    if mode == "paths":
        return _render_paths(manager, chat_data=chat_data, notice=notice)
    if mode == "paths_add":
        return _render_paths_add(notice=notice, notice_code=notice_code)
    if mode == "await_prompt":
        session_name = ui.get("session")
        await_prompt = ui.get("await_prompt")
        run_mode = await_prompt.get("run_mode") if isinstance(await_prompt, dict) else "new"
        if isinstance(session_name, str) and session_name:
            rec = manager.sessions.get(session_name)
            return _render_await_prompt(
                session_name,
                run_mode=run_mode,
                model=(rec.model if rec else None),
                reasoning_effort=(rec.reasoning_effort if rec else None),
                path=(rec.path if rec else None),
                notice=notice,
            )
        return _render_sessions_list(manager, chat_data=chat_data, notice="No session selected.")
    if mode == "confirm_delete":
        session_name2 = ui.get("session")
        rec = manager.sessions.get(session_name2) if isinstance(session_name2, str) else None
        if rec:
            return _render_confirm_delete(rec, notice=notice)
        return _render_sessions_list(manager, chat_data=chat_data, notice="Unknown session.")
    if mode == "confirm_mkdir":
        return _render_confirm_mkdir(chat_data=chat_data, notice=notice)
    if mode == "confirm_stop":
        session_name_stop = ui.get("session")
        if isinstance(session_name_stop, str) and session_name_stop in manager.sessions:
            return _render_confirm_stop(session_name_stop, notice=notice)
        return _render_sessions_list(manager, chat_data=chat_data, notice="No session selected.")
    if mode == "model":
        session_name3 = ui.get("session")
        rec2 = manager.sessions.get(session_name3) if isinstance(session_name3, str) else None
        if rec2:
            return _render_model(rec2, notice=notice)
        return _render_sessions_list(manager, chat_data=chat_data, notice="Unknown session.")
    if mode == "model_custom":
        session_name_custom = ui.get("session")
        rec_custom = manager.sessions.get(session_name_custom) if isinstance(session_name_custom, str) else None
        if rec_custom:
            return _render_model_custom(rec_custom, notice=notice)
        return _render_sessions_list(manager, chat_data=chat_data, notice="No session selected.")
    if mode == "logs":
        session_name4 = ui.get("session")
        if isinstance(session_name4, str) and session_name4:
            return _render_logs_view(manager, session_name=session_name4, notice=notice)
        return _render_sessions_list(manager, chat_data=chat_data, notice="No session selected.")
    if mode == "session":
        session_name5 = ui.get("session")
        if isinstance(session_name5, str) and session_name5:
            return _render_session_view(manager, session_name=session_name5, notice=notice)
        return _render_sessions_list(manager, chat_data=chat_data, notice=notice)

    return _render_sessions_list(manager, chat_data=chat_data, notice=notice)

