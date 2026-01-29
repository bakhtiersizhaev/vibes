from __future__ import annotations

import asyncio
import time
from collections import deque
from pathlib import Path
from typing import Any, Deque

from ..bot.callbacks import cb as _cb
from ..bot.ui_render_session import _render_session_view
from ..constants import RUN_START_WAIT_NOTE, STDERR_TAIL_LINES
from ..telegram_deps import InlineKeyboardButton, InlineKeyboardMarkup
from ..utils.logging import log_error, utc_now_iso
from ..utils.text import h as _h
from ..utils.time import format_duration as _format_duration
from .completion_notice import send_completion_notice
from .session_models import SessionRecord, SessionRun


async def run_prompt(
    manager: Any,
    *,
    chat_id: int,
    panel_message_id: int,
    application: Any,
    session_name: str,
    prompt: str,
    run_mode: str,
) -> None:
    rec = manager.sessions.get(session_name)
    if not rec:
        return

    if rec.run and rec.run.process.returncode is None:
        return

    if run_mode == "new":
        rec.thread_id = None
        await manager.save_state()

    manager.log_dir.mkdir(parents=True, exist_ok=True)
    ts = manager.now_utc().strftime("%Y%m%d_%H%M%S")
    stdout_log = manager.log_dir / f"{rec.name}_{ts}.jsonl"
    stderr_log = manager.log_dir / f"{rec.name}_{ts}.stderr.txt"

    rec.status = "running"
    rec.last_active = utc_now_iso()
    rec.last_stdout_log = str(stdout_log)
    rec.last_stderr_log = str(stderr_log)
    rec.last_run_duration_s = None
    await manager.save_state()

    started_mono = time.monotonic()

    def _working_footer_html() -> str:
        elapsed_s = int(time.monotonic() - started_mono)
        return f"<code>---- Working {_h(_format_duration(elapsed_s))} ----</code>"

    running_kb = InlineKeyboardMarkup(
        [
            [
                InlineKeyboardButton("⬅️", callback_data=_cb("back_sessions")),
                InlineKeyboardButton("⛔", callback_data=_cb("interrupt")),
            ]
        ]
    )

    try:
        await manager.pause_other_attached_runs(chat_id=chat_id, message_id=panel_message_id, except_session=rec.name)
    except Exception as e:
        log_error("pause_other_attached_runs failed.", e, log_path=manager.bot_log_path)

    manager.register_run_message(chat_id=chat_id, message_id=panel_message_id, session_name=rec.name)

    stream = manager.telegram_stream_cls(
        application,
        chat_id=chat_id,
        message_id=panel_message_id,
        header_html=f"<i>{_h(RUN_START_WAIT_NOTE)}</i>",
        header_plain_len=len(RUN_START_WAIT_NOTE),
        auto_clear_header_on_first_log=True,
        footer_provider=_working_footer_html,
        footer_plain_len=len("---- Working 0m 0s ----"),
        wrap_log_in_pre=True,
        reply_markup=running_kb,
    )

    cmd = manager._build_codex_cmd(rec, prompt=prompt, run_mode=run_mode)

    async def _handle_start_failure(*, stderr_text: str) -> None:
        try:
            stderr_log.parent.mkdir(parents=True, exist_ok=True)
            stderr_log.write_text(stderr_text, encoding="utf-8")
        except Exception as e:
            log_error("Failed to write stderr log for start failure.", e, log_path=manager.bot_log_path)
        rec.status = "error"
        rec.last_result = "error"
        rec.last_active = utc_now_iso()
        rec.last_run_duration_s = int(time.monotonic() - started_mono)
        await manager.save_state()
        await stream.stop()
        manager.unregister_run_message(chat_id=chat_id, message_id=stream.get_message_id())
        try:
            panel = manager.panel_ui_cls(application, manager)
            text_html, reply_markup = _render_session_view(manager, session_name=rec.name, notice="Failed to start.")
            await panel.render_to_message(
                chat_id=chat_id,
                message_id=panel_message_id,
                text_html=text_html,
                reply_markup=reply_markup,
                update_state_on_replace=True,
            )
        except Exception as e:
            log_error("Failed to render start failure panel.", e, log_path=manager.bot_log_path)

    try:
        process = await manager._spawn_process(cmd)
    except FileNotFoundError:
        await _handle_start_failure(stderr_text="`codex` not found in PATH.\n")
        return
    except Exception as e:
        await _handle_start_failure(stderr_text=f"Failed to start Codex: {e}\n")
        return

    stderr_tail: Deque[str] = deque(maxlen=STDERR_TAIL_LINES)
    stdout_task = asyncio.create_task(manager._read_stdout(rec=rec, process=process, stream=stream, log_path=stdout_log))
    stderr_task = asyncio.create_task(manager._read_stderr(process=process, log_path=stderr_log, stderr_tail=stderr_tail))

    rec.run = SessionRun(
        process=process,
        stdout_task=stdout_task,
        stderr_task=stderr_task,
        stream=stream,
        stdout_log=stdout_log,
        stderr_log=stderr_log,
        stderr_tail=stderr_tail,
        started_mono=started_mono,
    )
    await manager.save_state()

    return_code = await process.wait()
    await asyncio.gather(stdout_task, stderr_task, return_exceptions=True)

    paused = bool(rec.run and rec.run.paused)
    rec.last_run_duration_s = int(time.monotonic() - started_mono)
    if rec.run and rec.run.stop_requested:
        rec.status = "stopped"
        rec.last_result = "stopped"
    elif return_code == 0:
        rec.status = "idle"
        rec.last_result = "success"
    else:
        rec.status = "error"
        rec.last_result = "error"

    rec.last_active = utc_now_iso()
    await manager.save_state()
    await stream.stop()
    manager.unregister_run_message(chat_id=chat_id, message_id=stream.get_message_id())

    rec.run = None
    await manager.save_state()

    if not paused:
        try:
            panel = manager.panel_ui_cls(application, manager)
            text_html, reply_markup = _render_session_view(manager, session_name=rec.name)
            await panel.render_to_message(
                chat_id=chat_id,
                message_id=panel_message_id,
                text_html=text_html,
                reply_markup=reply_markup,
                update_state_on_replace=True,
            )
        except Exception:
            pass

    try:
        await send_completion_notice(
            application=application,
            chat_id=chat_id,
            session_name=rec.name,
            path=rec.path,
            prompt=prompt,
        )
    except Exception as e:
        log_error("Failed to send completion notice.", e, log_path=manager.bot_log_path)

    if rec.pending_delete:
        await manager.delete_session(rec.name)

