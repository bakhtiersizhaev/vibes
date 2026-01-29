from __future__ import annotations

import asyncio
import datetime as dt
import json
import os
import signal
import subprocess
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

from ..constants import DEFAULT_MODEL, DEFAULT_REASONING_EFFORT, STATE_VERSION
from ..telegram.panel import PanelUI
from ..telegram.stream import TelegramStream
from ..utils.logging import utc_now_iso
from ..utils.paths import safe_resolve_path, safe_session_name
from . import codex_cmd, state_store
from .process_io import handle_json_event as _handle_json_event_impl
from .process_io import read_stderr as _read_stderr_impl
from .process_io import read_stdout as _read_stdout_impl
from .session_models import SessionRecord
from .session_runner import run_prompt as _run_prompt_impl


class SessionManager:
    def __init__(
        self,
        *,
        admin_id: Optional[int],
        state_path: Optional[Path] = None,
        log_dir: Optional[Path] = None,
        bot_log_path: Optional[Path] = None,
        telegram_stream_cls: Optional[type] = None,
        panel_ui_cls: Optional[type] = None,
    ) -> None:
        from .. import runtime

        self._admin_id = admin_id
        self._state_lock = asyncio.Lock()

        self.state_path = Path(state_path) if state_path is not None else runtime.STATE_PATH
        self.log_dir = Path(log_dir) if log_dir is not None else runtime.LOG_DIR
        self.bot_log_path = Path(bot_log_path) if bot_log_path is not None else runtime.BOT_LOG_PATH

        self.telegram_stream_cls = telegram_stream_cls or TelegramStream
        self.panel_ui_cls = panel_ui_cls or PanelUI

        self.sessions: Dict[str, SessionRecord] = {}
        self.panel_by_chat: Dict[int, int] = {}
        self._run_message_to_session: Dict[Tuple[int, int], str] = {}
        self.path_presets: List[str] = []
        self.owner_id: Optional[int] = None

        if state_path is None and log_dir is None and bot_log_path is None:
            state_store.maybe_migrate_runtime_files()

        self._load_state()

    def now_utc(self) -> dt.datetime:
        return dt.datetime.now(dt.timezone.utc)

    def register_run_message(self, *, chat_id: int, message_id: int, session_name: str) -> None:
        if chat_id and message_id and session_name:
            self._run_message_to_session[(chat_id, message_id)] = session_name

    def unregister_run_message(self, *, chat_id: int, message_id: int) -> None:
        self._run_message_to_session.pop((chat_id, message_id), None)

    def resolve_session_for_run_message(self, *, chat_id: int, message_id: int) -> Optional[str]:
        return self._run_message_to_session.get((chat_id, message_id))

    def resolve_attached_running_session_for_message(self, *, chat_id: int, message_id: int) -> Optional[str]:
        for name, rec in self.sessions.items():
            if not rec.run or rec.status != "running":
                continue
            try:
                if rec.run.stream.get_chat_id() != chat_id:
                    continue
                if rec.run.stream.get_message_id() != message_id:
                    continue
            except Exception:
                continue
            if rec.run.paused:
                continue
            return name
        return None

    async def pause_other_attached_runs(
        self,
        *,
        chat_id: int,
        message_id: int,
        except_session: Optional[str] = None,
    ) -> None:
        for name, rec in self.sessions.items():
            if except_session and name == except_session:
                continue
            if not rec.run or rec.status != "running":
                continue
            try:
                if rec.run.stream.get_chat_id() != chat_id:
                    continue
                if rec.run.stream.get_message_id() != message_id:
                    continue
            except Exception:
                continue
            if rec.run.paused:
                continue
            rec.run.paused = True
            await rec.run.stream.pause()

    async def ensure_owner(self, update: Any) -> bool:
        user = getattr(update, "effective_user", None)
        user_id = getattr(user, "id", None) if user is not None else None
        if not isinstance(user_id, int):
            return False
        if self._admin_id is not None:
            return user_id == self._admin_id
        if self.owner_id is None:
            self.owner_id = user_id
            await self.save_state()
            return True
        return user_id == self.owner_id

    def _load_state(self) -> None:
        if not self.state_path.exists():
            return
        try:
            raw = json.loads(self.state_path.read_text(encoding="utf-8"))
        except Exception:
            return
        if not isinstance(raw, dict):
            return

        sessions = raw.get("sessions", {})
        if isinstance(sessions, dict):
            for name, payload in sessions.items():
                if not isinstance(payload, dict):
                    continue
                safe_name = safe_session_name(str(name))
                if not safe_name:
                    continue
                path = payload.get("path")
                if not isinstance(path, str) or not path:
                    continue

                rec = SessionRecord(
                    name=safe_name,
                    path=path,
                    thread_id=payload.get("thread_id")
                    if isinstance(payload.get("thread_id"), str)
                    else payload.get("session_id")
                    if isinstance(payload.get("session_id"), str)
                    else None,
                    model=payload.get("model") if isinstance(payload.get("model"), str) and payload.get("model") else DEFAULT_MODEL,
                    reasoning_effort=payload.get("reasoning_effort")
                    if isinstance(payload.get("reasoning_effort"), str) and payload.get("reasoning_effort")
                    else payload.get("model_reasoning_effort")
                    if isinstance(payload.get("model_reasoning_effort"), str) and payload.get("model_reasoning_effort")
                    else DEFAULT_REASONING_EFFORT,
                    status=payload.get("status") if isinstance(payload.get("status"), str) else "idle",
                    last_result=payload.get("last_result")
                    if isinstance(payload.get("last_result"), str)
                    and payload.get("last_result") in {"never", "success", "error", "stopped"}
                    else "never",
                    created_at=payload.get("created_at") if isinstance(payload.get("created_at"), str) else utc_now_iso(),
                    last_active=payload.get("last_active") if isinstance(payload.get("last_active"), str) else None,
                    last_stdout_log=payload.get("last_stdout_log") if isinstance(payload.get("last_stdout_log"), str) else None,
                    last_stderr_log=payload.get("last_stderr_log") if isinstance(payload.get("last_stderr_log"), str) else None,
                    last_run_duration_s=payload.get("last_run_duration_s")
                    if isinstance(payload.get("last_run_duration_s"), int)
                    else None,
                    pending_delete=payload.get("pending_delete") if isinstance(payload.get("pending_delete"), bool) else False,
                )

                if rec.last_stdout_log:
                    rec.last_stdout_log = state_store.rewrite_legacy_log_path(rec.last_stdout_log, log_dir=self.log_dir)
                if rec.last_stderr_log:
                    rec.last_stderr_log = state_store.rewrite_legacy_log_path(rec.last_stderr_log, log_dir=self.log_dir)

                if rec.status == "running":
                    rec.status = "idle"
                self.sessions[safe_name] = rec

        panel = raw.get("panel_by_chat", {})
        if isinstance(panel, dict):
            for chat_id_str, msg_id in panel.items():
                try:
                    chat_id = int(chat_id_str)
                    message_id = int(msg_id)
                except Exception:
                    continue
                if chat_id and message_id:
                    self.panel_by_chat[chat_id] = message_id

        presets = raw.get("path_presets", [])
        if isinstance(presets, list):
            seen: set[str] = set()
            for p in presets:
                if not isinstance(p, str):
                    continue
                p2 = p.strip()
                if not p2 or p2 in seen:
                    continue
                seen.add(p2)
                self.path_presets.append(p2)

        owner_id = raw.get("owner_id")
        if isinstance(owner_id, int):
            self.owner_id = owner_id

    async def save_state(self) -> None:
        async with self._state_lock:
            payload: Dict[str, Any] = {
                "version": STATE_VERSION,
                "owner_id": self.owner_id,
                "sessions": {},
                "panel_by_chat": {str(k): v for k, v in self.panel_by_chat.items()},
                "path_presets": list(self.path_presets),
            }
            sessions: Dict[str, Any] = payload["sessions"]
            for name, rec in self.sessions.items():
                sessions[name] = {
                    "path": rec.path,
                    "thread_id": rec.thread_id,
                    "model": rec.model,
                    "reasoning_effort": rec.reasoning_effort,
                    "status": rec.status if rec.status != "running" else "idle",
                    "last_result": rec.last_result,
                    "created_at": rec.created_at,
                    "last_active": rec.last_active,
                    "last_stdout_log": rec.last_stdout_log,
                    "last_stderr_log": rec.last_stderr_log,
                    "last_run_duration_s": rec.last_run_duration_s,
                    "pending_delete": rec.pending_delete,
                }
            text = json.dumps(payload, ensure_ascii=False, indent=2)
            await asyncio.to_thread(state_store.atomic_write_text, self.state_path, text)

    def get_panel_message_id(self, chat_id: int) -> Optional[int]:
        return self.panel_by_chat.get(chat_id)

    async def set_panel_message_id(self, chat_id: int, message_id: int) -> None:
        self.panel_by_chat[chat_id] = message_id
        await self.save_state()

    async def upsert_path_preset(self, path: str) -> None:
        path = path.strip()
        if not path:
            return
        if path in self.path_presets:
            return
        self.path_presets.append(path)
        await self.save_state()

    async def delete_path_preset(self, index: int) -> bool:
        if index < 0 or index >= len(self.path_presets):
            return False
        self.path_presets.pop(index)
        await self.save_state()
        return True

    def next_auto_session_name(self) -> str:
        n = 1
        while True:
            cand = f"session-{n}"
            if cand not in self.sessions:
                return cand
            n += 1

    async def shutdown(self) -> None:
        stop_tasks = []
        for name in list(self.sessions.keys()):
            rec = self.sessions.get(name)
            if rec and rec.run and rec.run.process.returncode is None:
                stop_tasks.append(self.stop(name, reason="shutdown"))
        if stop_tasks:
            await asyncio.gather(*stop_tasks, return_exceptions=True)
        await self.save_state()

    async def create_session(self, *, name: str, path: str) -> Tuple[Optional[SessionRecord], str]:
        safe_name = safe_session_name(name)
        if not safe_name:
            return None, "Invalid name. Allowed: a-zA-Z0-9._- (<=64)."

        resolved, err = safe_resolve_path(path)
        if err:
            return None, err
        abs_path = str(resolved)
        p = Path(abs_path)
        if not p.exists() or not p.is_dir():
            return None, f"Directory not found: {abs_path}"

        if safe_name in self.sessions:
            return None, f"Session '{safe_name}' already exists."

        rec = SessionRecord(name=safe_name, path=abs_path, status="idle", last_result="never")
        self.sessions[safe_name] = rec
        await self.save_state()
        return rec, ""

    async def delete_session(self, name: str) -> Tuple[bool, str]:
        rec = self.sessions.get(name)
        if not rec:
            return False, f"Unknown session: {name}"

        if rec.run and rec.run.process.returncode is None:
            rec.pending_delete = True
            await self.save_state()
            await self.stop(name)
            return True, "Stop requested. Session will be deleted after it finishes."

        self._delete_session_artifacts(rec)
        del self.sessions[name]

        await self.save_state()
        return True, "Deleted."

    async def clear_session_state(self, name: str) -> Tuple[bool, str]:
        rec = self.sessions.get(name)
        if not rec:
            return False, f"Unknown session: {name}"
        if rec.run and rec.run.process.returncode is None:
            return False, "This session is running."

        self._delete_session_artifacts(rec)
        rec.thread_id = None
        rec.status = "idle"
        rec.last_result = "never"
        rec.last_active = None
        rec.last_stdout_log = None
        rec.last_stderr_log = None
        rec.last_run_duration_s = None
        rec.pending_delete = False
        rec.run = None

        await self.save_state()
        return True, "Cleared."

    def _delete_session_artifacts(self, rec: SessionRecord) -> None:
        seen: set[Path] = set()

        def add_file(p: Optional[str]) -> None:
            if not p:
                return
            try:
                seen.add(Path(p))
            except Exception:
                return

        add_file(rec.last_stdout_log)
        add_file(rec.last_stderr_log)

        if self.log_dir.exists() and self.log_dir.is_dir():
            for p in self.log_dir.glob(f"{rec.name}_*.jsonl"):
                seen.add(p)
            for p in self.log_dir.glob(f"{rec.name}_*.stderr.txt"):
                seen.add(p)

        for p in seen:
            try:
                if p.exists() and p.is_file():
                    p.unlink()
            except Exception:
                pass

    async def run_prompt(
        self,
        *,
        chat_id: int,
        panel_message_id: int,
        application: Any,
        session_name: str,
        prompt: str,
        run_mode: str,
    ) -> None:
        await _run_prompt_impl(
            self,
            chat_id=chat_id,
            panel_message_id=panel_message_id,
            application=application,
            session_name=session_name,
            prompt=prompt,
            run_mode=run_mode,
        )

    async def stop(self, name: str, *, reason: str = "user") -> bool:
        rec = self.sessions.get(name)
        if not rec or not rec.run:
            return False
        run = rec.run
        run.stop_requested = True

        proc = run.process
        if getattr(proc, "returncode", None) is not None:
            return True

        try:
            if os.name == "posix":
                os.killpg(proc.pid, signal.SIGTERM)
            else:
                proc.terminate()
        except ProcessLookupError:
            return True
        except Exception:
            try:
                proc.terminate()
            except Exception:
                pass

        try:
            await asyncio.wait_for(proc.wait(), timeout=5.0)
        except asyncio.TimeoutError:
            try:
                if os.name == "posix":
                    os.killpg(proc.pid, signal.SIGKILL)
                else:
                    proc.kill()
            except Exception:
                pass
        return True

    def _build_codex_cmd(self, rec: SessionRecord, *, prompt: str, run_mode: str) -> List[str]:
        return codex_cmd.build_codex_cmd(rec, prompt=prompt, run_mode=run_mode)

    async def _spawn_process(self, cmd: List[str]) -> asyncio.subprocess.Process:
        kwargs: Dict[str, Any] = {
            "stdout": asyncio.subprocess.PIPE,
            "stderr": asyncio.subprocess.PIPE,
        }
        if os.name == "posix":
            kwargs["start_new_session"] = True
        else:
            kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP  # type: ignore[attr-defined]
        return await asyncio.create_subprocess_exec(*cmd, **kwargs)

    async def _read_stdout(self, *, rec: SessionRecord, process: Any, stream: Any, log_path: Path) -> None:
        await _read_stdout_impl(self, rec=rec, process=process, stream=stream, log_path=log_path)

    async def _read_stderr(self, *, process: Any, log_path: Path, stderr_tail: Any) -> None:
        await _read_stderr_impl(self, process=process, log_path=log_path, stderr_tail=stderr_tail)

    async def _handle_json_event(self, *, rec: SessionRecord, obj: Dict[str, Any], stream: Any) -> None:
        await _handle_json_event_impl(self, rec=rec, obj=obj, stream=stream)
