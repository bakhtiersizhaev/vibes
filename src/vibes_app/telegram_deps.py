from __future__ import annotations

import sys


try:
    from telegram import Update
    from telegram import InlineKeyboardButton, InlineKeyboardMarkup
    from telegram.constants import ParseMode
    from telegram.error import BadRequest, RetryAfter, TelegramError
    from telegram.ext import (
        Application,
        ApplicationBuilder,
        CallbackQueryHandler,
        CommandHandler,
        ContextTypes,
        MessageHandler,
        filters,
    )
except Exception as exc:  # noqa: BLE001 - friendly runtime error
    print(
        "Не удалось импортировать python-telegram-bot.\n"
        "Установи зависимость:\n"
        '  pip install "python-telegram-bot>=20,<23"\n'
        f"Оригинальная ошибка: {exc}",
        file=sys.stderr,
    )
    raise SystemExit(1) from exc


__all__ = [
    "Update",
    "InlineKeyboardButton",
    "InlineKeyboardMarkup",
    "ParseMode",
    "BadRequest",
    "RetryAfter",
    "TelegramError",
    "Application",
    "ApplicationBuilder",
    "CallbackQueryHandler",
    "CommandHandler",
    "ContextTypes",
    "MessageHandler",
    "filters",
]
