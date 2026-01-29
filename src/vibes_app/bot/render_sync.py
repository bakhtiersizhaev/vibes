from __future__ import annotations

import time
from typing import Any, Dict

from ..telegram_deps import InlineKeyboardButton, InlineKeyboardMarkup
from ..utils.logging import log_error
from ..utils.text import h as _h
from ..utils.time import format_duration as _format_duration
from .callbacks import cb as _cb
from .ui_render_current import _render_current
from .ui_run import _is_running
from .ui_state import _ui_get


async def _clear_input_prompt(panel: Any, *, chat_id: int, chat_data: Dict[str, Any]) -> None:
    ui = _ui_get(chat_data)
    prompt = ui.pop("input_prompt", None)
    if not isinstance(prompt, dict):
        return
    try:
        msg_id = int(prompt.get("message_id"))
    except Exception:
        return
    if msg_id > 0:
        await panel.delete_message_best_effort(chat_id=chat_id, message_id=msg_id)


async def _sync_input_prompt(panel: Any, *, chat_id: int, chat_data: Dict[str, Any]) -> None:
    await _clear_input_prompt(panel, chat_id=chat_id, chat_data=chat_data)


async def _render_and_sync(
    manager: Any,
    panel: Any,
    *,
    context: Any,
    chat_id: int,
) -> None:
    panel_message_id = manager.get_panel_message_id(chat_id)
    if not panel_message_id:
        panel_message_id = await panel.ensure_panel(chat_id)

    ui = _ui_get(context.chat_data)
    mode = ui.get("mode") if isinstance(ui.get("mode"), str) else "sessions"
    session_name = ui.get("session") if isinstance(ui.get("session"), str) else None

    if mode == "session" and isinstance(session_name, str) and session_name in manager.sessions:
        rec = manager.sessions.get(session_name)
        if rec and _is_running(rec) and rec.run:
            try:
                if rec.run.stream.get_chat_id() == chat_id and rec.run.stream.get_message_id() == panel_message_id:
                    try:
                        await manager.pause_other_attached_runs(
                            chat_id=chat_id,
                            message_id=panel_message_id,
                            except_session=rec.name,
                        )
                    except Exception as e:
                        log_error("pause_other_attached_runs failed (_render_and_sync attach).", e)

                    manager.register_run_message(chat_id=chat_id, message_id=panel_message_id, session_name=rec.name)
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

                    if isinstance(ui.get("notice"), str):
                        ui.pop("notice", None)

                    await rec.run.stream.resume()
                    await _sync_input_prompt(panel, chat_id=chat_id, chat_data=context.chat_data)
                    return
            except Exception:
                pass

    try:
        await manager.pause_other_attached_runs(chat_id=chat_id, message_id=panel_message_id)
    except Exception as e:
        log_error("pause_other_attached_runs failed (_render_and_sync).", e)

    text_html, reply_markup = _render_current(manager, chat_data=context.chat_data)
    await panel.render_to_message(
        chat_id=chat_id,
        message_id=panel_message_id,
        text_html=text_html,
        reply_markup=reply_markup,
        update_state_on_replace=True,
    )
    await _sync_input_prompt(panel, chat_id=chat_id, chat_data=context.chat_data)
