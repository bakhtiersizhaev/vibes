import asyncio
import unittest
from collections import deque
from pathlib import Path
from tempfile import TemporaryDirectory


import telegram_stubs

telegram_stubs.install()

import vibes  # noqa: E402


class SessionRunPromptTests(unittest.IsolatedAsyncioTestCase):
    async def test_run_prompt_renders_finished_session_screen(self) -> None:
        class _CapturingPanelUI:
            last_instance: "_CapturingPanelUI | None" = None

            def __init__(self, application: object, manager: object) -> None:
                self.renders: list[tuple[int, int, str, object]] = []
                _CapturingPanelUI.last_instance = self

            async def render_to_message(
                self,
                *,
                chat_id: int,
                message_id: int,
                text_html: str,
                reply_markup: object,
                update_state_on_replace: bool,
            ) -> int:
                self.renders.append((chat_id, message_id, text_html, reply_markup))
                return message_id

        class _CapturingStream:
            def __init__(
                self,
                application: object,
                chat_id: int,
                message_id: int,
                *,
                header_html: str = "",
                header_plain_len: int = 0,
                auto_clear_header_on_first_log: bool = False,
                footer_provider: object | None = None,
                footer_plain_len: int = 0,
                wrap_log_in_pre: bool = False,
                reply_markup: object | None = None,
                on_panel_replaced: object | None = None,
            ) -> None:
                self._chat_id = chat_id
                self._message_id = message_id

            def get_chat_id(self) -> int:
                return self._chat_id

            def get_message_id(self) -> int:
                return self._message_id

            async def set_footer(  # pragma: no cover
                self,
                *,
                footer_provider: object | None,
                footer_plain_len: int | None = None,
                wrap_log_in_pre: bool | None = None,
            ) -> None:
                return None

            async def set_reply_markup(self, reply_markup: object | None) -> None:  # pragma: no cover
                return None

            async def pause(self) -> None:  # pragma: no cover
                return None

            async def resume(self) -> None:  # pragma: no cover
                return None

            async def add_text(self, text: str) -> None:  # pragma: no cover
                return None

            async def stop(self) -> None:  # pragma: no cover
                return None

        class _FakeProcess:
            def __init__(self, return_code: int) -> None:
                self.returncode: int | None = None
                self._return_code = return_code

            async def wait(self) -> int:
                self.returncode = self._return_code
                return self._return_code

        class _RunPromptManager(vibes.SessionManager):
            async def save_state(self) -> None:  # pragma: no cover
                return None

            async def _spawn_process(self, cmd: list[str]) -> object:
                return _FakeProcess(return_code=0)

            async def _read_stdout(  # pragma: no cover
                self,
                *,
                rec: vibes.SessionRecord,
                process: object,
                stream: object,
                log_path: Path,
            ) -> None:
                return None

            async def _read_stderr(  # pragma: no cover
                self,
                *,
                process: object,
                log_path: Path,
                stderr_tail: deque[str],
            ) -> None:
                return None

        old_stream = vibes.TelegramStream
        old_panel = vibes.PanelUI
        old_state_path = vibes.STATE_PATH
        old_log_dir = vibes.LOG_DIR
        old_bot_log_path = vibes.BOT_LOG_PATH

        try:
            with TemporaryDirectory() as td:
                tmp = Path(td)
                vibes.STATE_PATH = tmp / "state.json"
                vibes.LOG_DIR = tmp / "logs"
                vibes.BOT_LOG_PATH = tmp / "bot.log"
                vibes.TelegramStream = _CapturingStream  # type: ignore[assignment]
                vibes.PanelUI = _CapturingPanelUI  # type: ignore[assignment]

                manager = _RunPromptManager(admin_id=None)
                manager.sessions = {"S": vibes.SessionRecord(name="S", path=".")}

                await manager.run_prompt(
                    chat_id=1,
                    panel_message_id=123,
                    application=object(),  # ignored by stubs
                    session_name="S",
                    prompt="hello",
                    run_mode="new",
                )
        finally:
            vibes.TelegramStream = old_stream
            vibes.PanelUI = old_panel
            vibes.STATE_PATH = old_state_path
            vibes.LOG_DIR = old_log_dir
            vibes.BOT_LOG_PATH = old_bot_log_path

        panel = _CapturingPanelUI.last_instance
        self.assertIsNotNone(panel)
        self.assertTrue(panel.renders)

        _chat_id, _message_id, text_html, reply_markup = panel.renders[-1]
        self.assertIn("Send a prompt to continue.", text_html)

        buttons = getattr(reply_markup, "inline_keyboard", [])
        texts = [getattr(btn, "text", "") for row in buttons for btn in (row or [])]
        self.assertIn("🆕", texts)
        self.assertIn("⚙️", texts)
        self.assertIn(vibes.LABEL_BACK, texts)
        self.assertIn("🗑", texts)

    async def test_run_prompt_sends_completion_notice_with_ack_button(self) -> None:
        class _CapturingPanelUI:
            def __init__(self, application: object, manager: object) -> None:  # pragma: no cover
                return None

            async def render_to_message(
                self,
                *,
                chat_id: int,
                message_id: int,
                text_html: str,
                reply_markup: object,
                update_state_on_replace: bool,
            ) -> int:  # pragma: no cover
                return message_id

        class _CapturingStream:
            def __init__(
                self,
                application: object,
                chat_id: int,
                message_id: int,
                *,
                header_html: str = "",
                header_plain_len: int = 0,
                auto_clear_header_on_first_log: bool = False,
                footer_provider: object | None = None,
                footer_plain_len: int = 0,
                wrap_log_in_pre: bool = False,
                reply_markup: object | None = None,
                on_panel_replaced: object | None = None,
            ) -> None:
                self._chat_id = chat_id
                self._message_id = message_id

            def get_chat_id(self) -> int:  # pragma: no cover
                return self._chat_id

            def get_message_id(self) -> int:  # pragma: no cover
                return self._message_id

            async def stop(self) -> None:  # pragma: no cover
                return None

        class _FakeProcess:
            def __init__(self, return_code: int) -> None:
                self.returncode: int | None = None
                self._return_code = return_code

            async def wait(self) -> int:
                self.returncode = self._return_code
                return self._return_code

        class _RunPromptManager(vibes.SessionManager):
            async def save_state(self) -> None:  # pragma: no cover
                return None

            async def _spawn_process(self, cmd: list[str]) -> object:
                return _FakeProcess(return_code=0)

            async def _read_stdout(  # pragma: no cover
                self,
                *,
                rec: vibes.SessionRecord,
                process: object,
                stream: object,
                log_path: Path,
            ) -> None:
                return None

            async def _read_stderr(  # pragma: no cover
                self,
                *,
                process: object,
                log_path: Path,
                stderr_tail: deque[str],
            ) -> None:
                return None

        class _CapturingBot:
            def __init__(self) -> None:
                self.sent: list[dict] = []

            async def send_message(self, **kwargs: object) -> None:
                self.sent.append(dict(kwargs))

        class _App:
            def __init__(self) -> None:
                self.bot = _CapturingBot()

        old_stream = vibes.TelegramStream
        old_panel = vibes.PanelUI
        old_state_path = vibes.STATE_PATH
        old_log_dir = vibes.LOG_DIR
        old_bot_log_path = vibes.BOT_LOG_PATH

        app = _App()

        try:
            with TemporaryDirectory() as td:
                tmp = Path(td)
                vibes.STATE_PATH = tmp / "state.json"
                vibes.LOG_DIR = tmp / "logs"
                vibes.BOT_LOG_PATH = tmp / "bot.log"
                vibes.TelegramStream = _CapturingStream  # type: ignore[assignment]
                vibes.PanelUI = _CapturingPanelUI  # type: ignore[assignment]

                manager = _RunPromptManager(admin_id=None)
                manager.sessions = {"S": vibes.SessionRecord(name="S", path=".")}

                await manager.run_prompt(
                    chat_id=1,
                    panel_message_id=123,
                    application=app,  # has bot.send_message
                    session_name="S",
                    prompt="hello",
                    run_mode="new",
                )
        finally:
            vibes.TelegramStream = old_stream
            vibes.PanelUI = old_panel
            vibes.STATE_PATH = old_state_path
            vibes.LOG_DIR = old_log_dir
            vibes.BOT_LOG_PATH = old_bot_log_path

        self.assertEqual(len(app.bot.sent), 1)
        sent = app.bot.sent[0]
        self.assertEqual(sent.get("chat_id"), 1)

        text = str(sent.get("text") or "")
        self.assertIn("Run finished", text)
        self.assertIn("Session:", text)
        self.assertIn("Path:", text)
        self.assertIn("hello", text)

        markup = sent.get("reply_markup")
        buttons = getattr(markup, "inline_keyboard", [])
        self.assertEqual(len(buttons), 1)
        self.assertEqual(len(buttons[0]), 1)
        btn = buttons[0][0]
        self.assertEqual(getattr(btn, "text", None), "✅")
        self.assertEqual(getattr(btn, "callback_data", None), vibes._cb("ack"))

    async def test_run_prompt_retries_completion_notice_on_retry_after(self) -> None:
        class _CapturingPanelUI:
            def __init__(self, application: object, manager: object) -> None:  # pragma: no cover
                return None

            async def render_to_message(
                self,
                *,
                chat_id: int,
                message_id: int,
                text_html: str,
                reply_markup: object,
                update_state_on_replace: bool,
            ) -> int:  # pragma: no cover
                return message_id

        class _CapturingStream:
            def __init__(
                self,
                application: object,
                chat_id: int,
                message_id: int,
                *,
                header_html: str = "",
                header_plain_len: int = 0,
                auto_clear_header_on_first_log: bool = False,
                footer_provider: object | None = None,
                footer_plain_len: int = 0,
                wrap_log_in_pre: bool = False,
                reply_markup: object | None = None,
                on_panel_replaced: object | None = None,
            ) -> None:
                self._chat_id = chat_id
                self._message_id = message_id

            def get_chat_id(self) -> int:  # pragma: no cover
                return self._chat_id

            def get_message_id(self) -> int:  # pragma: no cover
                return self._message_id

            async def stop(self) -> None:  # pragma: no cover
                return None

        class _FakeProcess:
            def __init__(self, return_code: int) -> None:
                self.returncode: int | None = None
                self._return_code = return_code

            async def wait(self) -> int:
                self.returncode = self._return_code
                return self._return_code

        class _RunPromptManager(vibes.SessionManager):
            async def save_state(self) -> None:  # pragma: no cover
                return None

            async def _spawn_process(self, cmd: list[str]) -> object:
                return _FakeProcess(return_code=0)

            async def _read_stdout(  # pragma: no cover
                self,
                *,
                rec: vibes.SessionRecord,
                process: object,
                stream: object,
                log_path: Path,
            ) -> None:
                return None

            async def _read_stderr(  # pragma: no cover
                self,
                *,
                process: object,
                log_path: Path,
                stderr_tail: deque[str],
            ) -> None:
                return None

        class _RetryBot:
            def __init__(self) -> None:
                self.calls = 0

            async def send_message(self, **kwargs: object) -> None:
                self.calls += 1
                if self.calls == 1:
                    raise vibes.RetryAfter(0.0)

        class _App:
            def __init__(self) -> None:
                self.bot = _RetryBot()

        old_stream = vibes.TelegramStream
        old_panel = vibes.PanelUI
        old_state_path = vibes.STATE_PATH
        old_log_dir = vibes.LOG_DIR
        old_bot_log_path = vibes.BOT_LOG_PATH

        app = _App()

        try:
            with TemporaryDirectory() as td:
                tmp = Path(td)
                vibes.STATE_PATH = tmp / "state.json"
                vibes.LOG_DIR = tmp / "logs"
                vibes.BOT_LOG_PATH = tmp / "bot.log"
                vibes.TelegramStream = _CapturingStream  # type: ignore[assignment]
                vibes.PanelUI = _CapturingPanelUI  # type: ignore[assignment]

                manager = _RunPromptManager(admin_id=None)
                manager.sessions = {"S": vibes.SessionRecord(name="S", path=".")}

                await manager.run_prompt(
                    chat_id=1,
                    panel_message_id=123,
                    application=app,  # has bot.send_message
                    session_name="S",
                    prompt="hello",
                    run_mode="new",
                )
        finally:
            vibes.TelegramStream = old_stream
            vibes.PanelUI = old_panel
            vibes.STATE_PATH = old_state_path
            vibes.LOG_DIR = old_log_dir
            vibes.BOT_LOG_PATH = old_bot_log_path

        self.assertEqual(app.bot.calls, 2)
