from __future__ import annotations

import asyncio
import sys
import time
from pathlib import Path
from typing import Any, Optional

from ..constants import MEDIA_GROUP_DEBOUNCE_SECONDS
from ..telegram_deps import ContextTypes, Update
from ..utils.paths import can_create_directory as _can_create_directory
from ..utils.paths import safe_resolve_path as _safe_resolve_path
from ..utils.paths import safe_session_name as _safe_session_name
from .attachments import build_prompt_with_downloaded_files as _build_prompt_with_downloaded_files
from .attachments import download_attachments_to_session_root as _download_attachments_to_session_root
from .handlers_common import delete_user_message_best_effort, ensure_authorized, get_handler_env
from .render_sync import _render_and_sync
from .ui_run import _is_running
from .ui_state import _ui_get, _ui_nav_pop, _ui_nav_reset, _ui_nav_to, _ui_set


async def schedule_prompt_run(
    *,
    manager: Any,
    panel: Any,
    context: ContextTypes.DEFAULT_TYPE,
    chat_id: int,
    session_name: str,
    prompt: str,
    ui_mode: str,
    run_mode: str,
) -> None:
    if not isinstance(prompt, str) or not prompt.strip():
        return

    rec = manager.sessions.get(session_name)
    if not rec:
        _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)
        return

    if rec and _is_running(rec):
        return

    if ui_mode == "session":

        async def _run_in_background() -> None:
            try:
                panel_id = await panel.ensure_panel(chat_id)
                await manager.run_prompt(
                    chat_id=chat_id,
                    panel_message_id=panel_id,
                    application=context.application,
                    session_name=session_name,
                    prompt=prompt,
                    run_mode="continue",
                )
            except Exception as e:
                print(f"run_prompt failed: {e}", file=sys.stderr)

        asyncio.create_task(_run_in_background())
        return

    if ui_mode != "await_prompt":
        return

    if run_mode not in {"continue", "new"}:
        run_mode = "new"

    ui = _ui_get(context.chat_data)
    prior_notice = ui.get("notice") if isinstance(ui.get("notice"), str) else ""
    starting_notice = "Starting… (see output message below)"
    if prior_notice and prior_notice.strip() and prior_notice.strip() != starting_notice:
        starting_notice = f"{prior_notice.strip()}\n\n{starting_notice}"

    _ui_set(context.chat_data, mode="session", session=session_name, notice=starting_notice)
    await _render_and_sync(manager, panel, context=context, chat_id=chat_id)

    async def _run_and_refresh() -> None:
        try:
            panel_id = await panel.ensure_panel(chat_id)
            await manager.run_prompt(
                chat_id=chat_id,
                panel_message_id=panel_id,
                application=context.application,
                session_name=session_name,
                prompt=prompt,
                run_mode=run_mode,
            )
        except Exception as e:
            print(f"run_prompt failed: {e}", file=sys.stderr)
        finally:
            ui2 = _ui_get(context.chat_data)
            mode2 = ui2.get("mode") if isinstance(ui2.get("mode"), str) else "sessions"
            session2 = ui2.get("session") if isinstance(ui2.get("session"), str) else None

            if mode2 == "await_prompt":
                if session_name in manager.sessions:
                    _ui_set(context.chat_data, mode="session", session=session_name, notice="Run finished.")
                else:
                    _ui_set(context.chat_data, mode="sessions", notice="Run finished.")
            elif mode2 == "session" and session2 == session_name:
                _ui_set(context.chat_data, notice="Run finished.")
            else:
                _ui_set(context.chat_data, notice=f"Run finished: {session_name}")

    asyncio.create_task(_run_and_refresh())


async def flush_media_group(
    *,
    manager: Any,
    panel: Any,
    context: ContextTypes.DEFAULT_TYPE,
    chat_id: int,
    media_group_id: str,
) -> None:
    if not isinstance(media_group_id, str) or not media_group_id:
        return

    while True:
        await asyncio.sleep(MEDIA_GROUP_DEBOUNCE_SECONDS)
        groups = context.chat_data.get("_media_groups")
        if not isinstance(groups, dict):
            return
        group = groups.get(media_group_id)
        if not isinstance(group, dict):
            return
        last = group.get("last_update_mono")
        last_mono = float(last) if isinstance(last, (int, float)) else 0.0
        if (time.monotonic() - last_mono) < MEDIA_GROUP_DEBOUNCE_SECONDS:
            continue

        groups.pop(media_group_id, None)

        session_name = group.get("session_name")
        ui_mode = group.get("ui_mode")
        run_mode = group.get("run_mode")
        user_text = group.get("user_text")
        filenames = group.get("filenames")

        if not isinstance(session_name, str) or not session_name:
            return
        ui_mode2 = ui_mode if isinstance(ui_mode, str) else "session"
        run_mode2 = run_mode if isinstance(run_mode, str) else "continue"
        prompt = _build_prompt_with_downloaded_files(
            user_text=(user_text if isinstance(user_text, str) else ""),
            filenames=(filenames if isinstance(filenames, list) else []),
        )
        await schedule_prompt_run(
            manager=manager,
            panel=panel,
            context=context,
            chat_id=chat_id,
            session_name=session_name,
            prompt=prompt,
            ui_mode=ui_mode2,
            run_mode=run_mode2,
        )
        return


async def on_attachment(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    env = await get_handler_env(update, context)
    if not env or not update.message:
        return

    ui = _ui_get(context.chat_data)
    mode = ui.get("mode") if isinstance(ui.get("mode"), str) else "sessions"

    ui_mode = mode
    session_name: Optional[str] = None
    run_mode = "continue"

    if mode == "session":
        session_name = ui.get("session") if isinstance(ui.get("session"), str) else None
        run_mode = "continue"
    elif mode == "await_prompt":
        session_name = ui.get("session") if isinstance(ui.get("session"), str) else None
        await_prompt = ui.get("await_prompt")
        run_mode = await_prompt.get("run_mode") if isinstance(await_prompt, dict) else "new"
        if run_mode not in {"continue", "new"}:
            run_mode = "new"
    else:
        _ui_set(context.chat_data, notice="Select a session first.")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return

    if not session_name or session_name not in env.manager.sessions:
        _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return

    rec = env.manager.sessions.get(session_name)
    if not rec:
        return

    caption = (getattr(update.message, "caption", None) or "").strip()

    try:
        filenames, notice = await _download_attachments_to_session_root(
            message=update.message,
            bot=context.application.bot,
            session_root=Path(rec.path),
        )
    except Exception as e:
        _ui_set(context.chat_data, notice=f"Failed to download attachment: {e}")
        await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        return

    if notice:
        _ui_set(context.chat_data, notice=notice)
        if ui_mode == "session":
            await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
        if not filenames:
            return

    if not filenames:
        return

    media_group_id = getattr(update.message, "media_group_id", None)
    if isinstance(media_group_id, str) and media_group_id:
        groups = context.chat_data.get("_media_groups")
        if not isinstance(groups, dict):
            groups = {}
            context.chat_data["_media_groups"] = groups

        group = groups.get(media_group_id)
        if not isinstance(group, dict):
            group = {
                "session_name": session_name,
                "ui_mode": ui_mode,
                "run_mode": run_mode,
                "filenames": list(filenames),
                "last_update_mono": time.monotonic(),
            }
            if caption:
                group["user_text"] = caption
            groups[media_group_id] = group
            asyncio.create_task(
                flush_media_group(
                    manager=env.manager,
                    panel=env.panel,
                    context=context,
                    chat_id=env.chat_id,
                    media_group_id=media_group_id,
                )
            )
            return

        files_list = group.get("filenames")
        if not isinstance(files_list, list):
            files_list = []
            group["filenames"] = files_list
        files_list.extend(filenames)

        if caption:
            current_text = group.get("user_text")
            if not isinstance(current_text, str) or not current_text.strip():
                group["user_text"] = caption

        group["last_update_mono"] = time.monotonic()
        return

    prompt = _build_prompt_with_downloaded_files(user_text=caption, filenames=filenames)
    await schedule_prompt_run(
        manager=env.manager,
        panel=env.panel,
        context=context,
        chat_id=env.chat_id,
        session_name=session_name,
        prompt=prompt,
        ui_mode=ui_mode,
        run_mode=run_mode,
    )


async def on_text(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    manager = context.application.bot_data["manager"]
    panel = context.application.bot_data["panel"]

    if not update.effective_chat or not update.message:
        return
    chat_id = update.effective_chat.id

    text = (update.message.text or "").strip()

    if not await ensure_authorized(update, context):
        return
    await delete_user_message_best_effort(update, authorized=True)
    if not text:
        return

    ui = _ui_get(context.chat_data)
    mode = ui.get("mode") if isinstance(ui.get("mode"), str) else "sessions"

    async def _rerender() -> None:
        await _render_and_sync(manager, panel, context=context, chat_id=chat_id)

    if mode == "new_name":
        safe = _safe_session_name(text)
        if not safe:
            _ui_set(context.chat_data, notice="Invalid name. Allowed: a-zA-Z0-9._- (<=64).")
            await _rerender()
            return
        if safe in manager.sessions:
            _ui_set(context.chat_data, notice="A session with this name already exists.")
            await _rerender()
            return
        _ui_nav_to(context.chat_data, mode="new_path", new={"name": safe})
        await _rerender()
        return

    if mode == "new_path":
        draft = ui.get("new")
        name = draft.get("name") if isinstance(draft, dict) else None
        if not isinstance(name, str) or not name:
            _ui_set(context.chat_data, mode="new_name", notice="Missing draft name. Start again.")
            await _rerender()
            return
        resolved, err = _safe_resolve_path(text)
        if err:
            _ui_set(context.chat_data, notice=err, notice_code=text)
            await _rerender()
            return
        abs_path = str(resolved)
        ui.pop("mkdir", None)
        p = Path(abs_path)
        if p.exists() and not p.is_dir():
            _ui_set(context.chat_data, notice="Это не папка.", notice_code=abs_path)
            await _rerender()
            return
        if not p.exists():
            if _can_create_directory(p):
                _ui_nav_to(context.chat_data, mode="confirm_mkdir", mkdir={"path": abs_path, "flow": "new_path"})
                await _rerender()
                return
            _ui_set(context.chat_data, notice="Папка не найдена.", notice_code=abs_path)
            await _rerender()
            return
        rec, err = await manager.create_session(name=name, path=abs_path)
        if err:
            _ui_set(context.chat_data, notice=err, new={"name": name})
            await _rerender()
            return
        ui.pop("new", None)
        _ui_nav_reset(context.chat_data, to={"mode": "sessions"})
        _ui_set(context.chat_data, mode="session", session=rec.name)
        await _rerender()
        return

    if mode == "paths_add":
        resolved, err = _safe_resolve_path(text)
        if err:
            _ui_set(context.chat_data, notice=err, notice_code=text)
            await _rerender()
            return
        abs_path = str(resolved)
        ui.pop("mkdir", None)
        p = Path(abs_path)
        if p.exists() and not p.is_dir():
            _ui_set(context.chat_data, notice="Это не папка.", notice_code=abs_path)
            await _rerender()
            return
        if not p.exists():
            if _can_create_directory(p):
                _ui_nav_to(context.chat_data, mode="confirm_mkdir", mkdir={"path": abs_path, "flow": "paths_add"})
                await _rerender()
                return
            _ui_set(context.chat_data, notice="Папка не найдена.", notice_code=abs_path)
            await _rerender()
            return
        await manager.upsert_path_preset(abs_path)
        _ui_set(context.chat_data, mode="paths", notice="Added.")
        await _rerender()
        return

    if mode == "model_custom":
        session_name = ui.get("session")
        rec = manager.sessions.get(session_name) if isinstance(session_name, str) else None
        model = text.strip()
        if not rec:
            _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
            await _rerender()
            return
        if not model:
            _ui_set(context.chat_data, notice="Model id can’t be empty.")
            await _rerender()
            return
        rec.model = model
        await manager.save_state()
        _ui_set(context.chat_data, notice=f"Model: {model}")
        if not _ui_nav_pop(context.chat_data):
            _ui_set(context.chat_data, mode="session", session=rec.name)
        await _rerender()
        return

    if mode == "session":
        session_name = ui.get("session")
        if not isinstance(session_name, str) or session_name not in manager.sessions:
            _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
            await _rerender()
            return

        rec = manager.sessions.get(session_name)
        if rec and _is_running(rec):
            return

        async def _run_in_background() -> None:
            try:
                panel_id = await panel.ensure_panel(chat_id)
                await manager.run_prompt(
                    chat_id=chat_id,
                    panel_message_id=panel_id,
                    application=context.application,
                    session_name=session_name,
                    prompt=text,
                    run_mode="continue",
                )
            except Exception as e:
                print(f"run_prompt failed: {e}", file=sys.stderr)

        asyncio.create_task(_run_in_background())
        return

    if mode == "await_prompt":
        session_name = ui.get("session")
        if not isinstance(session_name, str) or session_name not in manager.sessions:
            _ui_set(context.chat_data, mode="sessions", notice="No session selected.")
            await _rerender()
            return
        rec = manager.sessions.get(session_name)
        if rec and _is_running(rec):
            _ui_set(context.chat_data, mode="session", session=rec.name, notice="This session is already running.")
            await _rerender()
            return

        await_prompt = ui.get("await_prompt")
        run_mode = await_prompt.get("run_mode") if isinstance(await_prompt, dict) else "new"
        if run_mode not in {"continue", "new"}:
            run_mode = "new"

        _ui_set(context.chat_data, mode="session", session=session_name, notice="Starting… (see output message below)")
        await _rerender()

        async def _run_and_refresh() -> None:
            try:
                panel_id = await panel.ensure_panel(chat_id)
                await manager.run_prompt(
                    chat_id=chat_id,
                    panel_message_id=panel_id,
                    application=context.application,
                    session_name=session_name,
                    prompt=text,
                    run_mode=run_mode,
                )
            except Exception as e:
                print(f"run_prompt failed: {e}", file=sys.stderr)
            finally:
                ui2 = _ui_get(context.chat_data)
                mode2 = ui2.get("mode") if isinstance(ui2.get("mode"), str) else "sessions"
                session2 = ui2.get("session") if isinstance(ui2.get("session"), str) else None

                if mode2 == "await_prompt":
                    if session_name in manager.sessions:
                        _ui_set(context.chat_data, mode="session", session=session_name, notice="Run finished.")
                    else:
                        _ui_set(context.chat_data, mode="sessions", notice="Run finished.")
                elif mode2 == "session" and session2 == session_name:
                    _ui_set(context.chat_data, notice="Run finished.")
                else:
                    _ui_set(context.chat_data, notice=f"Run finished: {session_name}")

        asyncio.create_task(_run_and_refresh())
        return

    await _rerender()


async def on_unknown_command(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    env = await get_handler_env(update, context)
    if not env:
        return
    await _render_and_sync(env.manager, env.panel, context=context, chat_id=env.chat_id)
