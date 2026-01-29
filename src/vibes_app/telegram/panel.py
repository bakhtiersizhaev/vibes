from __future__ import annotations

import asyncio
from typing import Any, Dict, Optional

from ..constants import MAX_TELEGRAM_CHARS
from ..telegram_deps import BadRequest, InlineKeyboardMarkup, ParseMode, RetryAfter, TelegramError
from ..utils.logging import log_error
from ..utils.text import strip_html_tags, telegram_safe_html_code_block, truncate_text


class PanelUI:
    def __init__(self, application: Any, manager: Any) -> None:
        self.application = application
        self.manager = manager

    async def ensure_panel(self, chat_id: int) -> int:
        existing = self.manager.get_panel_message_id(chat_id)
        if existing:
            return existing

        msg = await self.application.bot.send_message(
            chat_id=chat_id,
            text="<b>Vibes</b>\n\nLoading…",
            parse_mode=ParseMode.HTML,
            disable_web_page_preview=True,
        )
        await self.manager.set_panel_message_id(chat_id, msg.message_id)
        return msg.message_id

    async def render_panel(
        self,
        chat_id: int,
        text_html: str,
        reply_markup: Optional[InlineKeyboardMarkup] = None,
    ) -> int:
        message_id = await self.ensure_panel(chat_id)
        return await self.render_to_message(
            chat_id=chat_id,
            message_id=message_id,
            text_html=text_html,
            reply_markup=reply_markup,
            update_state_on_replace=True,
        )

    async def render_to_message(
        self,
        *,
        chat_id: int,
        message_id: int,
        text_html: str,
        reply_markup: Optional[InlineKeyboardMarkup],
        update_state_on_replace: bool,
    ) -> int:
        async def _send_new_panel(*, text: str, parse_mode: Optional[str]) -> int:
            kwargs: Dict[str, Any] = {
                "chat_id": chat_id,
                "text": text,
                "disable_web_page_preview": True,
                "reply_markup": reply_markup,
            }
            if parse_mode:
                kwargs["parse_mode"] = parse_mode
            msg = await self.application.bot.send_message(**kwargs)
            if update_state_on_replace:
                await self.manager.set_panel_message_id(chat_id, msg.message_id)
            return msg.message_id

        async def _edit_message(*, text: str, parse_mode: Optional[str]) -> None:
            kwargs: Dict[str, Any] = {
                "chat_id": chat_id,
                "message_id": message_id,
                "text": text,
                "disable_web_page_preview": True,
                "reply_markup": reply_markup,
            }
            if parse_mode:
                kwargs["parse_mode"] = parse_mode
            await self.application.bot.edit_message_text(**kwargs)

        try:
            await _edit_message(text=text_html, parse_mode=ParseMode.HTML)
            return message_id
        except RetryAfter as e:
            try:
                await asyncio.sleep(float(getattr(e, "retry_after", 2.0)))
                await _edit_message(text=text_html, parse_mode=ParseMode.HTML)
                return message_id
            except TelegramError:
                log_error(f"Panel edit retry failed; sending new panel for chat_id={chat_id}, message_id={message_id}")
                return await _send_new_panel(text=text_html, parse_mode=ParseMode.HTML)
        except BadRequest as e:
            msg = str(e).lower()
            if "message is not modified" in msg:
                return message_id
            log_error(f"Panel edit failed (BadRequest): {msg}", e)

            if "message is too long" in msg:
                trimmed_html = telegram_safe_html_code_block(strip_html_tags(text_html))
                try:
                    await _edit_message(text=trimmed_html, parse_mode=ParseMode.HTML)
                    return message_id
                except TelegramError as e2:
                    log_error("Panel edit failed after trimming; falling back.", e2)

            if "can't parse entities" in msg or "can’t parse entities" in msg:
                plain = truncate_text(strip_html_tags(text_html), MAX_TELEGRAM_CHARS)
                try:
                    await _edit_message(text=plain, parse_mode=None)
                    return message_id
                except TelegramError as e2:
                    log_error("Panel edit failed with plain-text fallback; falling back.", e2)

            # If we can no longer edit this message, send a replacement panel.
            if (
                "message can't be edited" in msg
                or "message to edit not found" in msg
                or "message_id_invalid" in msg
                or "chat not found" in msg
            ):
                return await _send_new_panel(text=text_html, parse_mode=ParseMode.HTML)

            # Last resort: try plain-text edit (no HTML). If that also fails, replace the panel.
            plain2 = truncate_text(strip_html_tags(text_html), MAX_TELEGRAM_CHARS)
            try:
                await _edit_message(text=plain2, parse_mode=None)
                return message_id
            except TelegramError as e2:
                log_error("Panel edit failed (plain fallback); sending new panel.", e2)
                return await _send_new_panel(text=plain2, parse_mode=None)
        except TelegramError:
            log_error(f"Panel edit failed (TelegramError); sending new panel for chat_id={chat_id}, message_id={message_id}")
            return await _send_new_panel(text=text_html, parse_mode=ParseMode.HTML)

    async def delete_message_best_effort(self, *, chat_id: int, message_id: int) -> None:
        try:
            await self.application.bot.delete_message(chat_id=chat_id, message_id=message_id)
        except TelegramError:
            pass

