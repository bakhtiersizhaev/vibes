from __future__ import annotations

import asyncio
import json
import time
from pathlib import Path
from typing import Any, Deque, Dict, Optional

from ..utils.logging import log_error, utc_now_iso
from ..utils.text import truncate_text
from ..utils.uuid import find_first_uuid
from . import codex_events
from .session_models import SessionRecord


def _log_error_for(manager: Any, msg: str, exc: Optional[BaseException] = None) -> None:
    log_path = getattr(manager, "bot_log_path", None)
    try:
        log_error(msg, exc, log_path=log_path)
    except TypeError:
        log_error(msg, exc)


async def read_stdout(
    manager: Any,
    *,
    rec: SessionRecord,
    process: Any,
    stream: Any,
    log_path: Path,
) -> None:
    assert getattr(process, "stdout", None) is not None
    log_f: Optional[Any] = None
    last_open_attempt_mono = 0.0

    def _try_open_log() -> Optional[Any]:
        nonlocal log_f, last_open_attempt_mono
        if log_f is not None:
            return log_f
        now_mono = time.monotonic()
        if (now_mono - last_open_attempt_mono) < 5.0:
            return None
        last_open_attempt_mono = now_mono
        try:
            log_path.parent.mkdir(parents=True, exist_ok=True)
            log_f = log_path.open("a", encoding="utf-8")
            return log_f
        except Exception as e:
            _log_error_for(manager, f"Failed to open stdout log file: {log_path}", e)
            log_f = None
            return None

    try:
        while True:
            try:
                line = await process.stdout.readline()
            except asyncio.CancelledError:
                raise
            except Exception as e:
                _log_error_for(manager, "stdout.readline() failed.", e)
                await asyncio.sleep(0.1)
                continue

            if not line:
                return

            decoded = line.decode("utf-8", errors="replace")

            f = _try_open_log()
            if f is not None:
                try:
                    f.write(decoded)
                    f.flush()
                except Exception as e:
                    _log_error_for(manager, f"Failed to write stdout log file: {log_path}", e)
                    try:
                        f.close()
                    except Exception:
                        pass
                    log_f = None

            decoded_stripped = decoded.strip()
            if not decoded_stripped:
                continue

            try:
                obj: Optional[Dict[str, Any]] = None
                try:
                    maybe = json.loads(decoded_stripped)
                    if isinstance(maybe, dict):
                        obj = maybe
                except Exception:
                    obj = None

                if not obj:
                    await stream.add_text(decoded)
                    continue

                await manager._handle_json_event(rec=rec, obj=obj, stream=stream)
            except asyncio.CancelledError:
                raise
            except Exception as e:
                _log_error_for(manager, "stdout processing failed; continuing to read.", e)
                continue
    finally:
        if log_f is not None:
            try:
                log_f.close()
            except Exception:
                pass


async def read_stderr(
    manager: Any,
    *,
    process: Any,
    log_path: Path,
    stderr_tail: Deque[str],
) -> None:
    assert getattr(process, "stderr", None) is not None
    log_f: Optional[Any] = None
    last_open_attempt_mono = 0.0

    def _try_open_log() -> Optional[Any]:
        nonlocal log_f, last_open_attempt_mono
        if log_f is not None:
            return log_f
        now_mono = time.monotonic()
        if (now_mono - last_open_attempt_mono) < 5.0:
            return None
        last_open_attempt_mono = now_mono
        try:
            log_path.parent.mkdir(parents=True, exist_ok=True)
            log_f = log_path.open("a", encoding="utf-8")
            return log_f
        except Exception as e:
            _log_error_for(manager, f"Failed to open stderr log file: {log_path}", e)
            log_f = None
            return None

    try:
        while True:
            try:
                line = await process.stderr.readline()
            except asyncio.CancelledError:
                raise
            except Exception as e:
                _log_error_for(manager, "stderr.readline() failed.", e)
                await asyncio.sleep(0.1)
                continue

            if not line:
                return

            decoded = line.decode("utf-8", errors="replace")

            f = _try_open_log()
            if f is not None:
                try:
                    f.write(decoded)
                    f.flush()
                except Exception as e:
                    _log_error_for(manager, f"Failed to write stderr log file: {log_path}", e)
                    try:
                        f.close()
                    except Exception:
                        pass
                    log_f = None

            stderr_tail.append(decoded)
    finally:
        if log_f is not None:
            try:
                log_f.close()
            except Exception:
                pass


async def handle_json_event(manager: Any, *, rec: SessionRecord, obj: Dict[str, Any], stream: Any) -> None:
    event_type = codex_events.get_event_type(obj)

    if rec.thread_id is None:
        explicit_id = codex_events.extract_session_id_explicit(obj)
        if explicit_id:
            rec.thread_id = explicit_id
            rec.last_active = utc_now_iso()
            await manager.save_state()

    if event_type in ("thread.started", "thread_started", "thread.start"):
        session_id = codex_events.extract_session_id_explicit(obj) or find_first_uuid(obj)
        if session_id and session_id != rec.thread_id:
            rec.thread_id = session_id
            rec.last_active = utc_now_iso()
            await manager.save_state()
        return

    if event_type.startswith("item."):
        item = codex_events.extract_item(obj)
        if isinstance(item, dict):
            item_type = codex_events.extract_item_type(item)

            if item_type == "reasoning":
                return

            if item_type == "command_execution":
                cmd = item.get("command")
                out = item.get("aggregated_output")
                exit_code = item.get("exit_code")
                status = item.get("status")

                is_start = event_type.endswith("started") or status == "in_progress"
                is_done = event_type.endswith("completed") or status in {"completed", "failed"}

                cmd_s = cmd.strip() if isinstance(cmd, str) else ""
                if cmd_s and (is_start or is_done):
                    last_cmd = rec.run.last_cmd if rec.run else None
                    if cmd_s != last_cmd:
                        await stream.add_text(f"\n$ {cmd_s}\n")
                        if rec.run:
                            rec.run.last_cmd = cmd_s

                if is_done:
                    if isinstance(out, str) and out.strip():
                        out_s = out.rstrip("\n")
                        await stream.add_text(truncate_text(out_s, 2000) + "\n")
                    if isinstance(exit_code, int):
                        await stream.add_text(f"(exit_code: {exit_code})\n")
                return

            item_text = codex_events.extract_item_text(item)
            if item_text:
                await stream.add_text(item_text)
                return

    if event_type == "text":
        delta = codex_events.extract_text_delta(obj)
        if delta:
            await stream.add_text(delta)
        return

    if event_type == "tool_use":
        cmd = codex_events.extract_tool_command(obj)
        if cmd:
            await stream.add_text(f"\n[tool_use]\n{cmd}\n")
        else:
            await stream.add_text("\n[tool_use]\n" + truncate_text(json.dumps(obj, ensure_ascii=False, indent=2), 2000) + "\n")
        return

    if event_type == "tool_result":
        out = codex_events.extract_tool_output(obj)
        if out:
            await stream.add_text("\n[tool_result]\n" + truncate_text(out, 2000) + "\n")
        else:
            await stream.add_text(
                "\n[tool_result]\n" + truncate_text(json.dumps(obj, ensure_ascii=False, indent=2), 2000) + "\n"
            )
        return

    diff = codex_events.maybe_extract_diff(obj)
    if diff:
        await stream.add_text("\n[file_change]\n" + truncate_text(diff, 2500) + "\n")
        return

    delta = codex_events.extract_text_delta(obj)
    if delta:
        await stream.add_text(delta)
