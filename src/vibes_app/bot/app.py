from __future__ import annotations

import argparse
import asyncio
import os
import signal
import sys
from typing import Any, List, Optional

from ..core.session_manager import SessionManager
from ..core.state_store import maybe_migrate_runtime_files
from ..telegram.panel import PanelUI
from ..telegram_deps import (
    ApplicationBuilder,
    CallbackQueryHandler,
    CommandHandler,
    ContextTypes,
    MessageHandler,
    Update,
    filters,
)
from ..utils.logging import log_error, log_line
from .handlers_callback import on_callback
from .handlers_commands import cmd_list, cmd_logs, cmd_menu, cmd_new, cmd_start, cmd_stop, cmd_use
from .handlers_messages import on_attachment, on_text, on_unknown_command


async def run_bot(*, token: str, admin_id: Optional[int]) -> None:
    maybe_migrate_runtime_files()
    manager = SessionManager(admin_id=admin_id)

    app = ApplicationBuilder().token(token).build()
    app.bot_data["manager"] = manager
    app.bot_data["panel"] = PanelUI(app, manager)
    app.bot_data["restart_event"] = asyncio.Event()

    async def _error_handler(update: object, context: ContextTypes.DEFAULT_TYPE) -> None:
        err = getattr(context, "error", None)
        if err:
            log_error("Unhandled exception in handler.", err)
        else:
            log_error("Unhandled exception in handler (no error object).")

    app.add_handler(CommandHandler("start", cmd_start))
    app.add_handler(CommandHandler("menu", cmd_menu))
    app.add_handler(CommandHandler("new", cmd_new))
    app.add_handler(CommandHandler("use", cmd_use))
    app.add_handler(CommandHandler("list", cmd_list))
    app.add_handler(CommandHandler("logs", cmd_logs))
    app.add_handler(CommandHandler("stop", cmd_stop))

    app.add_handler(CallbackQueryHandler(on_callback))
    app.add_handler(MessageHandler(filters.ATTACHMENT & ~filters.COMMAND, on_attachment))
    app.add_handler(MessageHandler(filters.TEXT & ~filters.COMMAND, on_text))
    app.add_handler(MessageHandler(filters.COMMAND, on_unknown_command))
    app.add_error_handler(_error_handler)

    stop_event = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        try:
            loop.add_signal_handler(sig, stop_event.set)
        except NotImplementedError:
            pass

    await app.initialize()
    await app.start()
    await app.updater.start_polling(allowed_updates=Update.ALL_TYPES)

    restart_requested = False
    restart_event: asyncio.Event = app.bot_data["restart_event"]
    stop_task = asyncio.create_task(stop_event.wait())
    restart_task = asyncio.create_task(restart_event.wait())
    pending: set[asyncio.Task[Any]] = set()

    try:
        done, pending = await asyncio.wait({stop_task, restart_task}, return_when=asyncio.FIRST_COMPLETED)
        restart_requested = (restart_task in done) and restart_event.is_set() and not stop_event.is_set()
    finally:
        for task in pending:
            task.cancel()
        await asyncio.gather(*pending, return_exceptions=True)
        await manager.shutdown()
        await app.updater.stop()
        await app.stop()
        await app.shutdown()

    if restart_requested:
        log_line("Restart requested; restarting process (execv).")
        os.execv(sys.executable, [sys.executable] + sys.argv)


def parse_args(argv: List[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Telegram bot: local session manager for Codex CLI")
    p.add_argument("--token", default=None, help="Telegram bot token (or env VIBES_TOKEN/TELEGRAM_BOT_TOKEN)")
    p.add_argument("--admin", type=int, default=None, help="Allowed Telegram user_id (or env VIBES_ADMIN_ID)")
    return p.parse_args(argv)


def main(argv: Optional[List[str]] = None) -> None:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    token = args.token or os.environ.get("VIBES_TOKEN") or os.environ.get("VIBES_TELEGRAM_TOKEN") or os.environ.get(
        "TELEGRAM_BOT_TOKEN"
    )
    if not token:
        print(
            "Не задан токен Telegram-бота.\n"
            "Передай `--token ...` или задай env `VIBES_TOKEN=...`.",
            file=sys.stderr,
        )
        raise SystemExit(2)

    admin_id = args.admin
    if admin_id is None:
        raw = os.environ.get("VIBES_ADMIN_ID") or os.environ.get("VIBES_TELEGRAM_ADMIN_ID") or os.environ.get(
            "TELEGRAM_ADMIN_ID"
        )
        if raw:
            try:
                admin_id = int(raw)
            except ValueError:
                admin_id = None
    try:
        asyncio.run(run_bot(token=token, admin_id=admin_id))
    except KeyboardInterrupt:
        pass
