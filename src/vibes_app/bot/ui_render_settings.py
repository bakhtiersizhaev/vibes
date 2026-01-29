from __future__ import annotations

from typing import List, Optional, Tuple

from ..constants import DEFAULT_MODEL, DEFAULT_REASONING_EFFORT, LABEL_BACK
from ..core.codex_cmd import MODEL_PRESETS
from ..core.session_models import SessionRecord
from ..telegram_deps import InlineKeyboardButton, InlineKeyboardMarkup
from ..utils.text import h as _h
from .callbacks import cb as _cb
from .ui_render_session import _render_session_compact_info


def _render_model(rec: SessionRecord, *, notice: Optional[str] = None) -> Tuple[str, InlineKeyboardMarkup]:
    current = rec.model
    reasoning_effort = rec.reasoning_effort
    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    lines = [
        f"{notice_html}<b>Run settings</b>",
        "",
        _render_session_compact_info(rec),
        "",
        f"Model: <code>{_h(current)}</code>",
        f"Reasoning effort: <code>{_h(reasoning_effort)}</code>",
        "",
        "Pick overrides below.",
    ]
    rows: List[List[InlineKeyboardButton]] = []

    def _mark(label: str, selected: bool) -> str:
        return f"✅ {label}" if selected else label

    buttons = [
        InlineKeyboardButton(_mark(m, m == current), callback_data=_cb("model_pick", str(i)))
        for i, m in enumerate(MODEL_PRESETS)
    ]
    for i in range(0, len(buttons), 2):
        rows.append(buttons[i : i + 2])
    rows.append(
        [
            InlineKeyboardButton(
                _mark("📝", current not in MODEL_PRESETS),
                callback_data=_cb("model_custom"),
            )
        ]
    )
    rows.append(
        [
            InlineKeyboardButton(_mark("low", reasoning_effort == "low"), callback_data=_cb("reasoning_pick", "low")),
            InlineKeyboardButton(
                _mark("medium", reasoning_effort == "medium"), callback_data=_cb("reasoning_pick", "medium")
            ),
            InlineKeyboardButton(_mark("high", reasoning_effort == "high"), callback_data=_cb("reasoning_pick", "high")),
            InlineKeyboardButton(
                _mark("xhigh", reasoning_effort == "xhigh"), callback_data=_cb("reasoning_pick", "xhigh")
            ),
        ]
    )
    rows.append([InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back"))])
    return "\n".join(lines), InlineKeyboardMarkup(rows)


def _render_model_custom(rec: SessionRecord, *, notice: Optional[str] = None) -> Tuple[str, InlineKeyboardMarkup]:
    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    example = MODEL_PRESETS[0] if MODEL_PRESETS else "o3"
    text_html = (
        f"{notice_html}"
        "<b>Custom model</b>\n\n"
        f"{_render_session_compact_info(rec)}\n\n"
        f"Send a model id (e.g. <code>{_h(example)}</code>) or tap Back."
    )
    kb = InlineKeyboardMarkup([[InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back"))]])
    return text_html, kb


def _render_await_prompt(
    session_name: str,
    *,
    run_mode: str,
    model: Optional[str] = None,
    reasoning_effort: Optional[str] = None,
    path: Optional[str] = None,
    notice: Optional[str] = None,
) -> Tuple[str, InlineKeyboardMarkup]:
    mode_label = "continue (resume)" if run_mode == "continue" else "new prompt"
    notice_html = f"<i>{_h(notice)}</i>\n\n" if notice else ""
    model_label = model or DEFAULT_MODEL
    reasoning_label = reasoning_effort or DEFAULT_REASONING_EFFORT
    path_label = path or ""
    path_line = f"<code>{_h(path_label)}</code>\n" if path_label else ""
    text_html = (
        f"{notice_html}"
        f"<b>Session:</b> <code>{_h(session_name)}</code>\n"
        f"<code>{_h(model_label)}</code> <code>{_h(reasoning_label)}</code>\n"
        f"{path_line}\n"
        "Напиши промт сообщением.\n\n"
        f"<i>Режим:</i> {_h(mode_label)}"
    )
    kb = InlineKeyboardMarkup(
        [[InlineKeyboardButton("⚙️", callback_data=_cb("model")), InlineKeyboardButton(LABEL_BACK, callback_data=_cb("back"))]]
    )
    return text_html, kb

