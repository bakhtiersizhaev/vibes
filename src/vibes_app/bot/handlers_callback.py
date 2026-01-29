from __future__ import annotations

import asyncio
from pathlib import Path

from ..constants import CB_PREFIX
from ..core.codex_cmd import MODEL_PRESETS
from ..telegram_deps import ContextTypes, TelegramError, Update
from ..utils.logging import log_error, log_line
from ..utils.paths import safe_resolve_path as _safe_resolve_path
from .callbacks import cb as _cb
from .handlers_callback_utils import attach_running_session, auto_detach_if_running
from .handlers_commands import resolve_session_for_callback_message
from .handlers_common import ensure_authorized
from .render_sync import _render_and_sync
from .ui_run import _is_running
from .ui_state import _ui_get, _ui_nav_pop, _ui_nav_reset, _ui_nav_to, _ui_sanitize, _ui_set


async def on_callback(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    manager = context.application.bot_data["manager"]
    panel = context.application.bot_data["panel"]

    query = update.callback_query
    chat_id = update.effective_chat.id if update.effective_chat else None
    if not query or chat_id is None:
        return
    data = query.data or ""
    msg_id = query.message.message_id if query.message else None
    log_line(f"callback chat_id={chat_id} message_id={msg_id} data={data!r}")

    cb_action = ""
    if data.startswith(CB_PREFIX + ":"):
        parts_preview = data.split(":")
        cb_action = parts_preview[1] if len(parts_preview) >= 2 else ""
    if query.message and manager.get_panel_message_id(chat_id) is None and cb_action != "ack":
        try:
            await manager.set_panel_message_id(chat_id, query.message.message_id)
        except Exception:
            pass

    try:
        await query.answer()
    except TelegramError:
        pass
    except Exception as e:
        log_error("Failed to answer callback query.", e)

    if not await ensure_authorized(update, context):
        return

    if not data.startswith(CB_PREFIX + ":"):
        return

    parts = data.split(":")
    action = parts[1] if len(parts) >= 2 else ""
    arg = parts[2] if len(parts) >= 3 else None

    ui = _ui_get(context.chat_data)
    ui_session = ui.get("session") if isinstance(ui.get("session"), str) else None

    if action not in {"stop", "stop_yes", "stop_no", "interrupt", "detach"}:
        if query.message:
            try:
                await auto_detach_if_running(manager, chat_id=chat_id, message_id=query.message.message_id)
            except Exception as e:
                log_error("auto_detach_if_running failed.", e)

    if action in {"session_back", "new_back", "new_cancel", "paths_back", "await_cancel"}:
        action = "back"

    if action == "ack":
        if query.message:
            await panel.delete_message_best_effort(chat_id=chat_id, message_id=query.message.message_id)
        return

    if action == "home":
        _ui_nav_reset(context.chat_data)
        _ui_set(context.chat_data, mode="sessions")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "back":
        if not _ui_nav_pop(context.chat_data):
            _ui_set(context.chat_data, mode="sessions")
        _ui_sanitize(manager, context.chat_data)
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "back_sessions":
        session_name_detach = resolve_session_for_callback_message(
            manager,
            chat_id=chat_id,
            message_id=(query.message.message_id if query.message else None),
            fallback=ui_session,
        )
        rec_detach = manager.sessions.get(session_name_detach) if isinstance(session_name_detach, str) else None
        if rec_detach and _is_running(rec_detach) and rec_detach.run:
            rec_detach.run.paused = True
            await rec_detach.run.stream.pause()
        _ui_nav_reset(context.chat_data)
        _ui_set(context.chat_data, mode="sessions")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "sessions":
        _ui_nav_to(context.chat_data, mode="sessions")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "restart":
        running = [
            name
            for name, rec in manager.sessions.items()
            if rec.run and getattr(rec.run.process, "returncode", None) is None
        ]
        if running:
            _ui_set(context.chat_data, notice="Stop all running sessions before restarting the bot.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
            return

        restart_event = context.application.bot_data.get("restart_event")
        if not isinstance(restart_event, asyncio.Event):
            _ui_set(context.chat_data, notice="Restart is not available in this environment.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
            return

        _ui_set(context.chat_data, mode="sessions", notice="Restarting…")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)

        async def _schedule_restart() -> None:
            await asyncio.sleep(0.25)
            restart_event.set()

        asyncio.create_task(_schedule_restart())
        return
    elif action in {"session", "session_back"}:
        session_name_open = arg if isinstance(arg, str) and arg else ui_session
        if isinstance(session_name_open, str) and session_name_open in manager.sessions:
            _ui_nav_to(context.chat_data, mode="session", session=session_name_open)
        else:
            _ui_nav_to(context.chat_data, mode="sessions", notice="No session selected.")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "sess":
        try:
            idx = int(arg or "-1")
        except Exception:
            idx = -1
        names = ui.get("sess_list")
        if not isinstance(names, list):
            names = sorted(manager.sessions.keys())
        if idx < 0 or idx >= len(names):
            _ui_set(context.chat_data, mode="sessions", notice="Stale session list. Refreshing…")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
        else:
            name = str(names[idx])
            if name not in manager.sessions:
                _ui_set(context.chat_data, mode="sessions", notice="Session not found. Refreshing…")
                await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
            else:
                _ui_nav_to(context.chat_data, mode="session", session=name)
                await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "new":
        _ui_nav_to(context.chat_data, mode="new_name", new={})
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "new_auto":
        auto_name = ui.get("auto_name") if isinstance(ui.get("auto_name"), str) else manager.next_auto_session_name()
        if auto_name in manager.sessions:
            _ui_set(context.chat_data, mode="new_name", notice="Auto-name is taken. Pick another.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
        else:
            _ui_nav_to(context.chat_data, mode="new_path", new={"name": auto_name})
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "path_pick":
        draft = ui.get("new")
        name = draft.get("name") if isinstance(draft, dict) else None
        if not isinstance(name, str) or not name:
            _ui_set(context.chat_data, mode="new_name", notice="Missing draft name. Start again.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
        else:
            try:
                idx = int(arg or "-1")
            except Exception:
                idx = -1
            if idx < 0 or idx >= len(manager.path_presets):
                _ui_set(context.chat_data, mode="new_path", notice="Invalid preset index.")
                await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
            else:
                preset = manager.path_presets[idx]
                resolved_p, err = _safe_resolve_path(preset)
                if err:
                    _ui_set(context.chat_data, mode="new_path", notice=err, notice_code=preset)
                    await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
                elif not resolved_p.exists() or not resolved_p.is_dir():
                    _ui_set(context.chat_data, mode="new_path", notice="Папка не найдена.", notice_code=str(resolved_p))
                    await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
                else:
                    rec, err = await manager.create_session(name=name, path=str(resolved_p))
                    if err:
                        _ui_set(context.chat_data, mode="new_path", notice=err, new={"name": name})
                        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
                    else:
                        _ui_nav_reset(context.chat_data, to={"mode": "sessions"})
                        _ui_set(context.chat_data, mode="session", session=rec.name)
                        ui.pop("new", None)
                        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "paths":
        _ui_nav_to(context.chat_data, mode="paths")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "paths_add":
        _ui_nav_to(context.chat_data, mode="paths_add")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "path_del":
        try:
            idx = int(arg or "-1")
        except Exception:
            idx = -1
        ok = await manager.delete_path_preset(idx)
        _ui_set(context.chat_data, mode="paths", notice="Deleted." if ok else "Invalid preset index.")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "logs":
        session_name = ui.get("session")
        if not isinstance(session_name, str) or session_name not in manager.sessions:
            _ui_nav_to(context.chat_data, mode="sessions", notice="No session selected.")
        else:
            _ui_nav_to(context.chat_data, mode="logs", session=session_name)
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "log":
        session_name = ui.get("session")
        rec = manager.sessions.get(session_name) if isinstance(session_name, str) else None
        if not rec:
            _ui_nav_to(context.chat_data, mode="sessions", notice="No session selected.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
            return

        if _is_running(rec) and rec.run:
            if query.message:
                await attach_running_session(
                    manager, chat_id=chat_id, message_id=query.message.message_id, rec=rec, reason="log->attach"
                )
            else:
                rec.run.paused = False
                await rec.run.stream.resume()
            return

        _ui_nav_to(context.chat_data, mode="logs", session=rec.name)
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "disconnect":
        _ui_nav_reset(context.chat_data)
        _ui_set(context.chat_data, mode="sessions")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action in {"start", "run", "continue", "newprompt"}:
        session_name = ui.get("session")
        if not isinstance(session_name, str) or session_name not in manager.sessions:
            _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
        else:
            _ui_set(context.chat_data, mode="session", session=session_name)
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "model":
        session_name3 = ui.get("session")
        if not isinstance(session_name3, str) or session_name3 not in manager.sessions:
            _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
        else:
            _ui_nav_to(context.chat_data, mode="model", session=session_name3)
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "model_default":
        _ui_set(context.chat_data, notice="Default model selection is disabled.")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action in {"reasoning_default", "verbosity_default"}:
        _ui_set(context.chat_data, notice="Default reasoning option is disabled.")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "model_pick":
        session_name5 = ui.get("session")
        rec2 = manager.sessions.get(session_name5) if isinstance(session_name5, str) else None
        if not rec2:
            _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
        else:
            try:
                idx = int(arg or "-1")
            except Exception:
                idx = -1
            if idx < 0 or idx >= len(MODEL_PRESETS):
                _ui_set(context.chat_data, mode="model", notice="Invalid model.")
            else:
                rec2.model = MODEL_PRESETS[idx]
                await manager.save_state()
                _ui_set(context.chat_data, mode="model", session=rec2.name, notice=f"Model: {rec2.model}")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action in {"reasoning_pick", "verbosity_pick"}:
        session_name5b = ui.get("session")
        rec_v2 = manager.sessions.get(session_name5b) if isinstance(session_name5b, str) else None
        if not rec_v2:
            _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
        else:
            level = (arg or "").strip()
            if level not in {"low", "medium", "high", "xhigh"}:
                _ui_set(context.chat_data, mode="model", notice="Invalid reasoning effort.")
            else:
                rec_v2.reasoning_effort = level
                await manager.save_state()
                _ui_set(context.chat_data, mode="model", session=rec_v2.name, notice=f"Reasoning effort: {level}")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "model_custom":
        _ui_nav_to(context.chat_data, mode="model_custom")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "delete":
        session_name7 = ui.get("session")
        if not isinstance(session_name7, str) or session_name7 not in manager.sessions:
            _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
        else:
            _ui_set(context.chat_data, mode="confirm_delete", session=session_name7)
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "delete_no":
        session_name8 = ui.get("session")
        if isinstance(session_name8, str) and session_name8 in manager.sessions:
            _ui_set(context.chat_data, mode="session", session=session_name8)
        else:
            _ui_set(context.chat_data, mode="sessions")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "delete_yes":
        session_name9 = ui.get("session")
        if not isinstance(session_name9, str) or session_name9 not in manager.sessions:
            _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
        else:
            ok, msg = await manager.delete_session(session_name9)
            if session_name9 in manager.sessions:
                _ui_set(context.chat_data, mode="session", session=session_name9, notice=msg)
            else:
                _ui_set(context.chat_data, mode="sessions", notice=msg)
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "mkdir_no":
        ui.pop("mkdir", None)
        if not _ui_nav_pop(context.chat_data):
            _ui_set(context.chat_data, mode="sessions")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "mkdir_yes":
        mkdir = ui.get("mkdir")
        path = mkdir.get("path") if isinstance(mkdir, dict) else None
        flow = mkdir.get("flow") if isinstance(mkdir, dict) else None

        if not isinstance(path, str) or not path:
            _ui_set(context.chat_data, mode="sessions", notice="No pending directory to create.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
            return

        try:
            Path(path).mkdir(parents=True, exist_ok=True)
            p = Path(path)
            if not p.exists() or not p.is_dir():
                raise OSError("not a directory after mkdir")
        except Exception as e:
            _ui_set(context.chat_data, mode="confirm_mkdir", notice=f"Failed to create directory: {e}")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
            return

        if flow == "new_path":
            draft = ui.get("new")
            name = draft.get("name") if isinstance(draft, dict) else None
            if not isinstance(name, str) or not name:
                ui.pop("mkdir", None)
                _ui_set(context.chat_data, mode="new_name", notice="Missing draft name. Start again.")
                await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
                return
            rec, err = await manager.create_session(name=name, path=path)
            if err:
                _ui_set(context.chat_data, mode="new_path", notice=err, new={"name": name})
                ui.pop("mkdir", None)
                await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
                return
            ui.pop("mkdir", None)
            ui.pop("new", None)
            _ui_nav_reset(context.chat_data, to={"mode": "sessions"})
            _ui_set(context.chat_data, mode="session", session=rec.name)
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
            return

        if flow == "paths_add":
            await manager.upsert_path_preset(path)
            ui.pop("mkdir", None)
            _ui_set(context.chat_data, mode="paths", notice="Added.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
            return

        _ui_set(context.chat_data, mode="sessions", notice="Unknown mkdir flow.")
        ui.pop("mkdir", None)
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "clear":
        session_name_clear = ui.get("session")
        if not isinstance(session_name_clear, str) or session_name_clear not in manager.sessions:
            _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
        else:
            ok, msg = await manager.clear_session_state(session_name_clear)
            if ok:
                _ui_set(context.chat_data, mode="session", session=session_name_clear, notice=msg)
            else:
                _ui_set(context.chat_data, notice=msg)
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action in {"stop", "interrupt", "stop_yes"}:
        session_name10 = resolve_session_for_callback_message(
            manager,
            chat_id=chat_id,
            message_id=(query.message.message_id if query.message else None),
            fallback=ui_session,
        )
        rec3 = manager.sessions.get(session_name10) if isinstance(session_name10, str) else None
        if not rec3 or not _is_running(rec3) or not rec3.run:
            _ui_set(context.chat_data, notice="Not running.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
        else:
            await manager.stop(rec3.name)
            if rec3.run.paused:
                _ui_set(context.chat_data, mode="session", session=rec3.name, notice="Stop requested…")
                await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
            else:
                return
    elif action == "stop_no":
        session_name11 = resolve_session_for_callback_message(
            manager,
            chat_id=chat_id,
            message_id=(query.message.message_id if query.message else None),
            fallback=ui_session,
        )
        rec4 = manager.sessions.get(session_name11) if isinstance(session_name11, str) else None
        if rec4 and _is_running(rec4) and rec4.run:
            rec4.run.paused = False
            await rec4.run.stream.resume()
            return
        _ui_set(context.chat_data, notice="Not running.")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "detach":
        session_name13 = resolve_session_for_callback_message(
            manager,
            chat_id=chat_id,
            message_id=(query.message.message_id if query.message else None),
            fallback=ui_session,
        )
        rec6 = manager.sessions.get(session_name13) if isinstance(session_name13, str) else None
        if rec6 and _is_running(rec6) and rec6.run:
            rec6.run.paused = True
            await rec6.run.stream.pause()
        _ui_nav_reset(context.chat_data)
        _ui_set(context.chat_data, mode="sessions")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    elif action == "attach":
        session_name14 = ui.get("session")
        rec7 = manager.sessions.get(session_name14) if isinstance(session_name14, str) else None
        if rec7 and _is_running(rec7) and rec7.run:
            if query.message:
                await attach_running_session(
                    manager, chat_id=chat_id, message_id=query.message.message_id, rec=rec7, reason="attach"
                )
            else:
                rec7.run.paused = False
                await rec7.run.stream.resume()
        else:
            _ui_set(context.chat_data, mode="sessions", notice="Run is not active.")
            await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
    else:
        _ui_set(context.chat_data, mode="sessions", notice="Unknown action.")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)

    current_panel_id = manager.get_panel_message_id(chat_id)
    if query.message and current_panel_id and query.message.message_id != current_panel_id:
        if manager.resolve_session_for_run_message(chat_id=chat_id, message_id=query.message.message_id):
            return
        try:
            await panel.delete_message_best_effort(chat_id=chat_id, message_id=query.message.message_id)
        except Exception as e:
            log_error("Failed to delete stale panel message.", e)
