import asyncio
import unittest
from collections import deque
from pathlib import Path


import telegram_stubs

telegram_stubs.install()

import vibes  # noqa: E402


class _FakeProcess:
    returncode = None


class _FakeStream:
    def __init__(self, *, chat_id: int, message_id: int) -> None:
        self._chat_id = chat_id
        self._message_id = message_id
        self.pause_calls = 0

    def get_chat_id(self) -> int:
        return self._chat_id

    def get_message_id(self) -> int:
        return self._message_id

    async def pause(self) -> None:
        self.pause_calls += 1


class _FakePanelUI:
    def __init__(self, *, fixed_panel_message_id: int) -> None:
        self.fixed_panel_message_id = fixed_panel_message_id
        self.last_text_html: str | None = None
        self.last_reply_markup: object | None = None

    async def ensure_panel(self, chat_id: int) -> int:  # pragma: no cover
        return self.fixed_panel_message_id

    async def render_to_message(
        self,
        *,
        chat_id: int,
        message_id: int,
        text_html: str,
        reply_markup: object,
        update_state_on_replace: bool,
    ) -> int:
        self.last_text_html = text_html
        self.last_reply_markup = reply_markup
        return message_id

    async def delete_message_best_effort(self, *, chat_id: int, message_id: int) -> None:  # pragma: no cover
        return None


class SessionIsolationViewTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self) -> None:
        self._tasks: list[asyncio.Task[None]] = []

    async def asyncTearDown(self) -> None:
        for t in self._tasks:
            t.cancel()
        if self._tasks:
            await asyncio.gather(*self._tasks, return_exceptions=True)

    def _task(self) -> asyncio.Task[None]:
        t = asyncio.create_task(asyncio.sleep(3600))
        self._tasks.append(t)
        return t

    def _mk_running_session(self, *, name: str, chat_id: int, message_id: int, paused: bool) -> vibes.SessionRecord:
        rec = vibes.SessionRecord(name=name, path=".")
        rec.status = "running"
        run = vibes.SessionRun(
            process=_FakeProcess(),
            stdout_task=self._task(),
            stderr_task=self._task(),
            stream=_FakeStream(chat_id=chat_id, message_id=message_id),
            stdout_log=Path("stdout.jsonl"),
            stderr_log=Path("stderr.txt"),
            stderr_tail=deque(),
            paused=paused,
        )
        rec.run = run
        return rec

    def test_status_icons_running_and_success(self) -> None:
        running = vibes.SessionRecord(name="A", path=".")
        running.status = "running"
        running.last_result = "never"
        self.assertEqual(vibes._status_emoji(running), "🟢")

        ok = vibes.SessionRecord(name="B", path=".")
        ok.status = "idle"
        ok.last_result = "success"
        self.assertEqual(vibes._status_emoji(ok), "✅")

    def test_session_view_has_no_disconnect_button(self) -> None:
        manager = vibes.SessionManager(admin_id=None)
        rec = vibes.SessionRecord(name="S", path=".")
        manager.sessions = {"S": rec}

        _text, markup = vibes._render_session_view(manager, session_name="S")
        buttons = getattr(markup, "inline_keyboard", [])
        texts = [getattr(btn, "text", "") for row in buttons for btn in (row or [])]
        self.assertNotIn("🔌 Disconnect", texts)
        self.assertIn("⚙️", texts)
        self.assertIn(vibes.LABEL_BACK, texts)
        self.assertIn("🗑", texts)
        self.assertNotIn(vibes.LABEL_LOG, texts)
        self.assertNotIn(vibes.LABEL_START, texts)

    async def test_session_view_running_shows_watch_logs_and_stop(self) -> None:
        chat_id = 100
        message_id = 200

        manager = vibes.SessionManager(admin_id=None)
        rec = self._mk_running_session(name="S", chat_id=chat_id, message_id=message_id, paused=True)
        manager.sessions = {"S": rec}

        _text, markup = vibes._render_session_view(manager, session_name="S")
        buttons = getattr(markup, "inline_keyboard", [])
        texts = [getattr(btn, "text", "") for row in buttons for btn in (row or [])]
        self.assertIn("⬅️", texts)
        self.assertIn("⛔", texts)

    async def test_resolve_attached_running_session_ignores_stale_mapping(self) -> None:
        chat_id = 100
        message_id = 200

        manager = vibes.SessionManager(admin_id=None)

        a = self._mk_running_session(name="A", chat_id=chat_id, message_id=message_id, paused=False)
        b = self._mk_running_session(name="B", chat_id=chat_id, message_id=message_id, paused=True)
        manager.sessions = {"A": a, "B": b}
        manager.register_run_message(chat_id=chat_id, message_id=message_id, session_name="B")

        resolved = manager.resolve_attached_running_session_for_message(chat_id=chat_id, message_id=message_id)
        self.assertEqual(resolved, "A")

    async def test_pause_other_attached_runs_only_pauses_other_unpaused(self) -> None:
        chat_id = 100
        message_id = 200

        manager = vibes.SessionManager(admin_id=None)
        manager.sessions = {}

        a = self._mk_running_session(name="A", chat_id=chat_id, message_id=message_id, paused=False)
        b = self._mk_running_session(name="B", chat_id=chat_id, message_id=message_id, paused=False)
        manager.sessions = {"A": a, "B": b}

        await manager.pause_other_attached_runs(chat_id=chat_id, message_id=message_id, except_session="A")

        self.assertFalse(a.run.paused)
        self.assertTrue(b.run.paused)
        self.assertEqual(getattr(b.run.stream, "pause_calls", -1), 1)

    async def test_render_and_sync_pauses_attached_run_so_logs_ui_is_not_overwritten(self) -> None:
        chat_id = 100
        message_id = 200

        manager = vibes.SessionManager(admin_id=None)
        manager.sessions = {}
        manager.panel_by_chat = {chat_id: message_id}

        a = self._mk_running_session(name="A", chat_id=chat_id, message_id=message_id, paused=False)
        manager.sessions = {"A": a}

        panel = _FakePanelUI(fixed_panel_message_id=message_id)

        class _Ctx:
            def __init__(self) -> None:
                self.chat_data = {"ui": {"mode": "logs", "session": "A"}}
                self.application = object()

        context = _Ctx()

        await vibes._render_and_sync(manager, panel, context=context, chat_id=chat_id)

        self.assertTrue(a.run.paused)
        self.assertEqual(getattr(a.run.stream, "pause_calls", -1), 1)
        self.assertIsNotNone(panel.last_reply_markup)
