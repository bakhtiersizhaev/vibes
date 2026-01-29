from __future__ import annotations

import asyncio
import dataclasses
import html
import re
import sys
import time
from typing import Awaitable, Callable, List, Optional, Tuple

from ..constants import EDIT_THROTTLE_SECONDS, MAX_TELEGRAM_CHARS
from ..telegram_deps import BadRequest, InlineKeyboardMarkup, ParseMode, RetryAfter, TelegramError
from ..utils.logging import log_error


@dataclasses.dataclass
class Segment:
    kind: str  # "text" | "code"
    content: str

    def plain_len(self) -> int:
        return len(self.content)

    def render_html(self) -> str:
        if self.kind == "code":
            return f"<pre><code>{html.escape(self.content)}</code></pre>"
        return html.escape(self.content)


class TelegramStream:
    def __init__(
        self,
        application: object,
        chat_id: int,
        message_id: int,
        *,
        header_html: str = "",
        header_plain_len: int = 0,
        auto_clear_header_on_first_log: bool = False,
        footer_provider: Optional[Callable[[], str]] = None,
        footer_plain_len: int = 0,
        wrap_log_in_pre: bool = False,
        reply_markup: Optional[InlineKeyboardMarkup] = None,
        on_panel_replaced: Optional[Callable[[int], Awaitable[None]]] = None,
    ) -> None:
        self._app = application
        self._chat_id = chat_id
        self._message_id = message_id
        self._header_html = header_html
        self._header_plain_len = header_plain_len
        self._auto_clear_header_on_first_log = auto_clear_header_on_first_log
        self._footer_provider = footer_provider
        self._footer_plain_len = footer_plain_len
        self._wrap_log_in_pre = wrap_log_in_pre
        self._reply_markup = reply_markup
        self._on_panel_replaced = on_panel_replaced
        self._log_segments: List[Segment] = []
        self._lock = asyncio.Lock()
        self._dirty = asyncio.Event()
        self._stop = asyncio.Event()
        self._resume_event = asyncio.Event()
        self._resume_event.set()
        self._task: asyncio.Task[None] = asyncio.create_task(self._run())
        self._last_edit_mono = 0.0
        self._last_sent_html: Optional[str] = None
        self._last_sent_markup: Optional[InlineKeyboardMarkup] = None
        self._dirty.set()

    async def set_header(self, *, header_html: str, header_plain_len: Optional[int] = None) -> None:
        async with self._lock:
            self._header_html = header_html
            if header_plain_len is not None:
                self._header_plain_len = header_plain_len
            else:
                # Rough estimate to distribute budget for tail logs.
                self._header_plain_len = len(re.sub(r"<[^>]+>", "", header_html))
        self._dirty.set()

    async def set_reply_markup(self, reply_markup: Optional[InlineKeyboardMarkup]) -> None:
        async with self._lock:
            self._reply_markup = reply_markup
        self._dirty.set()

    async def set_footer(
        self,
        *,
        footer_provider: Optional[Callable[[], str]],
        footer_plain_len: Optional[int] = None,
        wrap_log_in_pre: Optional[bool] = None,
    ) -> None:
        async with self._lock:
            self._footer_provider = footer_provider
            if footer_plain_len is not None:
                self._footer_plain_len = footer_plain_len
            else:
                sample = footer_provider() if footer_provider else ""
                self._footer_plain_len = len(re.sub(r"<[^>]+>", "", sample))
            if wrap_log_in_pre is not None:
                self._wrap_log_in_pre = wrap_log_in_pre
        self._dirty.set()

    def get_message_id(self) -> int:
        return self._message_id

    def get_chat_id(self) -> int:
        return self._chat_id

    async def add_text(self, text: str) -> None:
        if not text:
            return
        async with self._lock:
            if self._auto_clear_header_on_first_log:
                self._auto_clear_header_on_first_log = False
                self._header_html = ""
                self._header_plain_len = 0
            if self._log_segments and self._log_segments[-1].kind == "text":
                self._log_segments[-1].content += text
            else:
                self._log_segments.append(Segment(kind="text", content=text))
        self._dirty.set()

    async def add_code(self, code: str) -> None:
        if not code:
            return
        async with self._lock:
            if self._auto_clear_header_on_first_log:
                self._auto_clear_header_on_first_log = False
                self._header_html = ""
                self._header_plain_len = 0
            if not self._log_segments or not self._log_segments[-1].content.endswith("\n"):
                # Separate visually.
                self._log_segments.append(Segment(kind="text", content="\n"))
            self._log_segments.append(Segment(kind="code", content=code))
            self._log_segments.append(Segment(kind="text", content="\n"))
        self._dirty.set()

    async def stop(self) -> None:
        self._stop.set()
        self._dirty.set()
        try:
            await self._task
        except asyncio.CancelledError:
            pass

    async def pause(self) -> None:
        self._resume_event.clear()

    async def resume(self) -> None:
        self._resume_event.set()
        self._dirty.set()

    async def _snapshot(
        self,
    ) -> Tuple[str, int, Optional[Callable[[], str]], int, bool, Optional[InlineKeyboardMarkup], List[Segment]]:
        async with self._lock:
            return (
                self._header_html,
                self._header_plain_len,
                self._footer_provider,
                self._footer_plain_len,
                self._wrap_log_in_pre,
                self._reply_markup,
                list(self._log_segments),
            )

    def _tail_segments(self, segments: List[Segment], max_plain: int) -> List[Segment]:
        total = 0
        kept_rev: List[Segment] = []
        for seg in reversed(segments):
            seg_len = seg.plain_len()
            if total + seg_len <= max_plain:
                kept_rev.append(seg)
                total += seg_len
                continue
            if not kept_rev:
                # One segment is too big — keep its tail.
                kept_rev.append(Segment(kind=seg.kind, content=seg.content[-max_plain:]))
                total = max_plain
            break

        kept = list(reversed(kept_rev))
        if len(kept) < len(segments):
            prefix = Segment(kind="text", content="…previous output hidden…\n\n")
            kept = [prefix] + kept
        return kept

    async def _render_html(self) -> str:
        header_html, header_plain_len, footer_provider, footer_plain_len, wrap_log_in_pre, _reply_markup, log_segments = (
            await self._snapshot()
        )

        footer_html = ""
        if footer_provider:
            try:
                footer_html = footer_provider() or ""
            except Exception:
                footer_html = ""

        header_html = header_html.strip()
        footer_html = footer_html.strip()

        # Leave some room for HTML wrappers and escaping expansion.
        max_plain_total = MAX_TELEGRAM_CHARS - 250
        if max_plain_total < 500:
            max_plain_total = MAX_TELEGRAM_CHARS

        max_plain_log = max_plain_total - header_plain_len - footer_plain_len - 50
        if max_plain_log < 300:
            max_plain_log = 300

        # If Telegram rejects due to length, progressively shrink the log tail budget.
        for _ in range(8):
            tail_segments = self._tail_segments(log_segments, max_plain=max_plain_log)
            if wrap_log_in_pre:
                plain_log = "".join(seg.content for seg in tail_segments).strip("\n")
                log_html = (
                    f"<pre><code>{html.escape(plain_log)}</code></pre>" if plain_log else "<pre><code></code></pre>"
                )
            else:
                log_html = "".join(seg.render_html() for seg in tail_segments).strip()

            parts = [p for p in (header_html, log_html, footer_html) if p]
            text_html = "\n\n".join(parts)
            if len(text_html) <= MAX_TELEGRAM_CHARS:
                return text_html
            max_plain_log = max(80, int(max_plain_log * 0.75))

        parts = [p for p in (header_html, log_html, footer_html) if p]
        return "\n\n".join(parts)

    async def _edit(self, text_html: str, reply_markup: Optional[InlineKeyboardMarkup]) -> None:
        if text_html == self._last_sent_html and reply_markup == self._last_sent_markup:
            return
        attempts = 0
        delay_s = 0.0
        started_mono = time.monotonic()
        max_total_wait_s = 60.0 if self._stop.is_set() else 15.0
        max_attempts = 12 if self._stop.is_set() else 5

        while True:
            attempts += 1
            try:
                bot = getattr(self._app, "bot", None)
                edit = getattr(bot, "edit_message_text", None) if bot is not None else None
                if not callable(edit):
                    return
                await edit(
                    chat_id=self._chat_id,
                    message_id=self._message_id,
                    text=text_html,
                    parse_mode=ParseMode.HTML,
                    disable_web_page_preview=True,
                    reply_markup=reply_markup,
                )
                self._last_sent_html = text_html
                self._last_sent_markup = reply_markup
                return
            except asyncio.CancelledError:
                raise
            except RetryAfter as e:
                retry_after = float(getattr(e, "retry_after", 2.0))
                if retry_after <= 0:
                    retry_after = 2.0
                delay_s = max(retry_after, delay_s * 2 if delay_s > 0 else retry_after)

                # During normal operation: don't block the whole stream for too long.
                # On shutdown/stop: give Telegram more time so the final render doesn't get lost.
                if attempts >= max_attempts or (time.monotonic() - started_mono) > max_total_wait_s:
                    if not self._stop.is_set():
                        self._dirty.set()
                    return

                await asyncio.sleep(delay_s)
                continue
            except BadRequest as e:
                msg = str(e).lower()
                if "message is not modified" in msg:
                    self._last_sent_html = text_html
                    self._last_sent_markup = reply_markup
                    return

                # If the message can't be edited (or doesn't exist), don't create a new message here.
                if (
                    "message can't be edited" in msg
                    or "message to edit not found" in msg
                    or "message_id_invalid" in msg
                    or "chat not found" in msg
                ):
                    self._last_sent_html = text_html
                    self._last_sent_markup = reply_markup
                    return

                log_error(f"Telegram edit failed (BadRequest): {msg}", e)
                raise

    async def _run(self) -> None:
        while True:
            await self._dirty.wait()
            self._dirty.clear()

            now = asyncio.get_running_loop().time()
            wait = max(0.0, EDIT_THROTTLE_SECONDS - (now - self._last_edit_mono))
            if wait > 0 and not self._stop.is_set():
                try:
                    await asyncio.wait_for(self._stop.wait(), timeout=wait)
                except asyncio.TimeoutError:
                    pass

            if not self._resume_event.is_set():
                resume_task = asyncio.create_task(self._resume_event.wait())
                stop_task = asyncio.create_task(self._stop.wait())
                done, _pending = await asyncio.wait({resume_task, stop_task}, return_when=asyncio.FIRST_COMPLETED)
                for task in (resume_task, stop_task):
                    if task not in done:
                        task.cancel()
                if stop_task in done:
                    return

            text_html = await self._render_html()
            _header_html, _header_plain_len, _footer_provider, _footer_plain_len, _wrap_log_in_pre, reply_markup, _segments = (
                await self._snapshot()
            )
            try:
                await self._edit(text_html, reply_markup)
            except TelegramError:
                # Don't crash the whole run due to Telegram errors.
                print("Ошибка Telegram при редактировании сообщения", file=sys.stderr)
            self._last_edit_mono = asyncio.get_running_loop().time()

            if self._stop.is_set() and not self._dirty.is_set():
                return

