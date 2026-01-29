from __future__ import annotations

import dataclasses
import os
from typing import Any, Optional

from ..telegram_deps import TelegramError
from ..utils.logging import log_error, log_line
from .render_sync import _sync_input_prompt
from .ui_render_home import _render_home
from .ui_state import _ui_set


def env_flag(name: str) -> bool:
    raw = os.environ.get(name, "").strip().lower()
    return raw in {"1", "true", "yes", "y", "on"}


async def delete_user_message_best_effort(update: Any, *, authorized: bool) -> None:
    if not authorized:
        return
    msg = getattr(update, "message", None)
    if not msg:
        return
    chat = getattr(update, "effective_chat", None)
    chat_type = getattr(chat, "type", None) if chat is not None else None
    if chat_type == "private":
        pass
    elif chat_type in {"group", "supergroup"}:
        if not env_flag("VIBES_DELETE_MESSAGES_IN_GROUPS"):
            return
    else:
        return
    try:
        await msg.delete()
    except TelegramError:
        pass
    except Exception as e:
        log_error("Failed to delete user message.", e)


async def deny_and_render(update: Any, context: Any) -> None:
    manager = context.application.bot_data["manager"]
    panel = context.application.bot_data["panel"]
    chat = getattr(update, "effective_chat", None)
    chat_id = getattr(chat, "id", None) if chat is not None else None
    if not isinstance(chat_id, int):
        return
    _ui_set(context.chat_data, mode="home", notice="Access denied.")
    await panel.render_panel(chat_id, *_render_home(manager, notice="Access denied."))
    await _sync_input_prompt(panel, chat_id=chat_id, chat_data=context.chat_data)


async def ensure_authorized(update: Any, context: Any) -> bool:
    manager = context.application.bot_data["manager"]
    if await manager.ensure_owner(update):
        return True
    user = getattr(update, "effective_user", None)
    chat = getattr(update, "effective_chat", None)
    log_line(f"access_denied user_id={getattr(user, 'id', None)} chat_id={getattr(chat, 'id', None)}")
    await deny_and_render(update, context)
    return False


@dataclasses.dataclass(frozen=True)
class HandlerEnv:
    manager: Any
    panel: Any
    chat_id: int


async def get_handler_env(
    update: Any,
    context: Any,
    *,
    delete_user_message: bool = True,
) -> Optional[HandlerEnv]:
    manager = context.application.bot_data["manager"]
    panel = context.application.bot_data["panel"]
    if not await ensure_authorized(update, context):
        return None
    if delete_user_message:
        await delete_user_message_best_effort(update, authorized=True)
    chat = getattr(update, "effective_chat", None)
    chat_id = getattr(chat, "id", None) if chat is not None else None
    if not isinstance(chat_id, int):
        return None
    return HandlerEnv(manager=manager, panel=panel, chat_id=chat_id)
