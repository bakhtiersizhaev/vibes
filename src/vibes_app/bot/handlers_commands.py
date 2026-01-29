from __future__ import annotations

from typing import Any

from ..telegram_deps import ContextTypes, Update
from ..utils.logging import log_error
from ..utils.text import parse_tokens as _parse_tokens
from .handlers_common import delete_user_message_best_effort, get_handler_env
from .render_sync import _render_and_sync
from .ui_run import _is_running
from .ui_state import _ui_get, _ui_nav_reset, _ui_set


async def cmd_start(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    env = await get_handler_env(update, context, delete_user_message=False)
    if not env:
        return
    _ui_nav_reset(context.chat_data)
    _ui_set(context.chat_data, mode="sessions")

    old_panel_id = env.manager.get_panel_message_id(env.chat_id)
    has_running_in_chat = False
    for rec in env.manager.sessions.values():
        if not rec.run or rec.status != "running":
            continue
        try:
            if rec.run.stream.get_chat_id() == env.chat_id:
                has_running_in_chat = True
                break
        except Exception:
            continue

    if not has_running_in_chat:
        env.manager.panel_by_chat.pop(env.chat_id, None)
    try:
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
    except Exception as e:
        if (
            not has_running_in_chat
            and old_panel_id is not None
            and env.manager.get_panel_message_id(env.chat_id) is None
        ):
            env.manager.panel_by_chat[env.chat_id] = old_panel_id
        log_error("cmd_start failed.", e)
        return

    if not has_running_in_chat and old_panel_id:
        new_panel_id = env.manager.get_panel_message_id(env.chat_id)
        if new_panel_id and new_panel_id != old_panel_id:
            await env.panel.delete_message_best_effort(chat_id=env.chat_id, message_id=old_panel_id)

    await delete_user_message_best_effort(update, authorized=True)


async def cmd_menu(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    env = await get_handler_env(update, context, delete_user_message=False)
    if not env:
        return
    _ui_nav_reset(context.chat_data)
    _ui_set(context.chat_data, mode="sessions")

    old_panel_id = env.manager.get_panel_message_id(env.chat_id)
    has_running_in_chat = False
    for rec in env.manager.sessions.values():
        if not rec.run or rec.status != "running":
            continue
        try:
            if rec.run.stream.get_chat_id() == env.chat_id:
                has_running_in_chat = True
                break
        except Exception:
            continue

    if not has_running_in_chat:
        env.manager.panel_by_chat.pop(env.chat_id, None)
    try:
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
    except Exception as e:
        if (
            not has_running_in_chat
            and old_panel_id is not None
            and env.manager.get_panel_message_id(env.chat_id) is None
        ):
            env.manager.panel_by_chat[env.chat_id] = old_panel_id
        log_error("cmd_menu failed.", e)
        return

    if not has_running_in_chat and old_panel_id:
        new_panel_id = env.manager.get_panel_message_id(env.chat_id)
        if new_panel_id and new_panel_id != old_panel_id:
            await env.panel.delete_message_best_effort(chat_id=env.chat_id, message_id=old_panel_id)

    await delete_user_message_best_effort(update, authorized=True)


async def cmd_list(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    env = await get_handler_env(update, context)
    if not env:
        return
    _ui_set(context.chat_data, mode="sessions")
    await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)


async def cmd_use(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    msg_text = update.message.text if update.message else ""
    env = await get_handler_env(update, context)
    if not env:
        return

    tokens = _parse_tokens(msg_text or "")
    if len(tokens) != 2:
        _ui_set(context.chat_data, mode="sessions", notice="Usage: /use <name>")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return
    name = tokens[1]
    if name not in env.manager.sessions:
        _ui_set(context.chat_data, mode="sessions", notice=f"Unknown session: {name}")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return

    _ui_set(context.chat_data, mode="session", session=name)
    await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)


async def cmd_new(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    msg_text = update.message.text if update.message else ""
    env = await get_handler_env(update, context)
    if not env:
        return

    tokens = _parse_tokens(msg_text or "")
    if len(tokens) >= 3:
        name = tokens[1]
        path = tokens[2]
        rec, err = await env.manager.create_session(name=name, path=path)
        if err:
            _ui_set(context.chat_data, mode="new_name", notice=err)
            await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
            return
        _ui_set(context.chat_data, mode="session", session=rec.name)
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return

    _ui_set(context.chat_data, mode="new_name", new={})
    await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)


async def cmd_stop(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    msg_text = update.message.text if update.message else ""
    env = await get_handler_env(update, context)
    if not env:
        return

    tokens = _parse_tokens(msg_text or "")
    ui = _ui_get(context.chat_data)
    fallback = ui.get("session") if isinstance(ui.get("session"), str) else None
    target = tokens[1] if len(tokens) >= 2 else fallback
    if not isinstance(target, str) or not target:
        _ui_set(context.chat_data, mode="sessions", notice="No session selected to stop.")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return

    rec = env.manager.sessions.get(target)
    if not rec:
        _ui_set(context.chat_data, mode="sessions", notice=f"Unknown session: {target}")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return

    if not _is_running(rec):
        _ui_set(context.chat_data, mode="session", session=rec.name, notice="This session is not running.")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return

    await env.manager.stop(rec.name)
    if rec.run and rec.run.paused:
        _ui_set(context.chat_data, mode="session", session=rec.name, notice="Stop requested…")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)


async def cmd_logs(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    msg_text = update.message.text if update.message else ""
    env = await get_handler_env(update, context)
    if not env:
        return

    tokens = _parse_tokens(msg_text or "")
    ui = _ui_get(context.chat_data)
    fallback = ui.get("session") if isinstance(ui.get("session"), str) else None
    target = tokens[1] if len(tokens) >= 2 else fallback
    if not isinstance(target, str) or not target:
        _ui_set(context.chat_data, mode="sessions", notice="No session selected. Use /logs <name>.")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return

    if target not in env.manager.sessions:
        _ui_set(context.chat_data, mode="sessions", notice=f"Unknown session: {target}")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return

    _ui_set(context.chat_data, mode="logs", session=target)
    await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)


def resolve_session_for_callback_message(
    manager: Any,
    *,
    chat_id: int,
    message_id: int | None,
    fallback: str | None,
) -> str | None:
    if message_id is None:
        return fallback
    return (
        manager.resolve_attached_running_session_for_message(chat_id=chat_id, message_id=message_id)
        or manager.resolve_session_for_run_message(chat_id=chat_id, message_id=message_id)
        or fallback
    )
