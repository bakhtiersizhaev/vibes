from __future__ import annotations

import asyncio
import time
from typing import Any, Dict, List

from ..constants import MAX_TELEGRAM_CHARS
from ..telegram_deps import BadRequest, InlineKeyboardButton, InlineKeyboardMarkup, ParseMode, RetryAfter, TelegramError
from ..utils.logging import log_error
from ..utils.text import h as _h
from ..utils.text import truncate_text as _truncate_text
from ..bot.callbacks import cb as _cb


async def send_completion_notice(
    *,
    application: Any,
    chat_id: int,
    session_name: str,
    path: str,
    prompt: str,
) -> None:
    bot = getattr(application, "bot", None)
    send_message = getattr(bot, "send_message", None) if bot is not None else None
    if not callable(send_message):
        return

    prompt_clean = (prompt or "").strip() or "(empty)"
    prompt_max = 2400
    text_html = ""
    for _ in range(10):
        prompt_view = prompt_clean
        if len(prompt_view) > prompt_max:
            prompt_view = _truncate_text(prompt_view, prompt_max)

        parts = [
            "<b>Run finished</b>",
            f"Session: <code>{_h(session_name)}</code>",
            f"Path: <code>{_h(path)}</code>",
            "",
            "<b>Prompt:</b>",
            f"<pre><code>{_h(prompt_view)}</code></pre>",
        ]
        text_html = "\n".join([p for p in parts if p])
        if len(text_html) <= MAX_TELEGRAM_CHARS:
            break
        prompt_max = max(200, int(prompt_max * 0.7))

    kb = InlineKeyboardMarkup([[InlineKeyboardButton("✅", callback_data=_cb("ack"))]])
    prompt_plain = _truncate_text(prompt_clean, 2000)
    text_plain = "\n".join(
        [
            "Run finished",
            f"Session: {session_name}",
            f"Path: {path}",
            "",
            "Prompt:",
            prompt_plain,
        ]
    ).strip()

    payloads: List[Dict[str, Any]] = [
        {
            "text": text_html,
            "parse_mode": ParseMode.HTML,
            "disable_web_page_preview": True,
            "reply_markup": kb,
        },
        {
            "text": _truncate_text(text_plain, 3500),
            "disable_web_page_preview": True,
            "reply_markup": kb,
        },
    ]

    for payload in payloads:
        delay_s = 1.0
        started_mono = time.monotonic()
        max_total_wait_s = 60.0 * 60.0
        remaining_attempts = 10
        while remaining_attempts > 0:
            try:
                await send_message(chat_id=chat_id, **payload)
                return
            except asyncio.CancelledError:
                raise
            except RetryAfter as e:
                retry_after = float(getattr(e, "retry_after", 2.0))
                await asyncio.sleep(max(0.0, retry_after))
                if (time.monotonic() - started_mono) > max_total_wait_s:
                    log_error("Failed to send completion notice (RetryAfter timeout).")
                    break
                continue
            except BadRequest as e:
                log_error("Failed to send completion notice (BadRequest).", e)
                break
            except TelegramError as e:
                remaining_attempts -= 1
                if remaining_attempts <= 0 or (time.monotonic() - started_mono) > max_total_wait_s:
                    log_error("Failed to send completion notice (TelegramError).", e)
                    break
                await asyncio.sleep(delay_s)
                delay_s = min(30.0, delay_s * 2)
                continue
            except Exception as e:
                log_error("Failed to send completion notice (unexpected exception).", e)
                break

