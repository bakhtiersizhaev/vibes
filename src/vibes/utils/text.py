from __future__ import annotations

import html
import re
import shlex
from typing import List

from ..constants import MAX_TELEGRAM_CHARS


def h(text: str) -> str:
    return html.escape(text)


def truncate_text(text: str, limit: int) -> str:
    if len(text) <= limit:
        return text
    head = max(0, (limit // 2) - 10)
    tail = max(0, limit - head - 20)
    return f"{text[:head]}\n…(обрезано)…\n{text[-tail:]}"


def strip_html_tags(text_html: str) -> str:
    raw = text_html or ""
    try:
        raw = re.sub(r"<[^>]+>", "", raw)
    except Exception:
        pass
    try:
        return html.unescape(raw)
    except Exception:
        return raw


def telegram_safe_html_code_block(text: str, *, max_chars: int = MAX_TELEGRAM_CHARS) -> str:
    plain_budget = max(200, max_chars - 50)
    for _ in range(12):
        plain_view = (text or "").strip()
        if len(plain_view) > plain_budget:
            plain_view = truncate_text(plain_view, plain_budget)
        candidate = f"<pre><code>{html.escape(plain_view)}</code></pre>"
        if len(candidate) <= max_chars:
            return candidate
        plain_budget = max(200, int(plain_budget * 0.7))
    plain_view = truncate_text((text or "").strip(), max(200, max_chars // 2))
    return f"<pre><code>{html.escape(plain_view)}</code></pre>"


def tail_text(text: str, limit: int, *, prefix: str = "…") -> str:
    if len(text) <= limit:
        return text
    keep = max(0, limit - len(prefix))
    if keep <= 0:
        return text[-limit:]
    return prefix + text[-keep:]


def parse_tokens(message_text: str) -> List[str]:
    try:
        tokens = shlex.split(message_text, posix=True)
    except ValueError:
        tokens = message_text.split()
    if tokens:
        # /cmd@botname -> /cmd
        tokens[0] = tokens[0].split("@", 1)[0]
    return tokens

