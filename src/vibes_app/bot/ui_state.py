from __future__ import annotations

import copy
from typing import Any, Dict, List, Optional, Tuple


def _ui_get(chat_data: Dict[str, Any]) -> Dict[str, Any]:
    ui = chat_data.get("ui")
    if not isinstance(ui, dict):
        ui = {}
        chat_data["ui"] = ui
    return ui


def _ui_set(chat_data: Dict[str, Any], **fields: Any) -> None:
    ui = _ui_get(chat_data)
    ui.update(fields)


_UI_NAV_KEYS: Tuple[str, ...] = ("mode", "session", "new", "await_prompt", "return_to")


def _ui_nav_stack(chat_data: Dict[str, Any]) -> List[Dict[str, Any]]:
    ui = _ui_get(chat_data)
    nav = ui.get("nav")
    if not isinstance(nav, list):
        nav = []
        ui["nav"] = nav
    return nav  # type: ignore[return-value]


def _ui_nav_snapshot(chat_data: Dict[str, Any]) -> Dict[str, Any]:
    ui = _ui_get(chat_data)
    snap: Dict[str, Any] = {}
    for k in _UI_NAV_KEYS:
        if k in ui:
            snap[k] = copy.deepcopy(ui.get(k))
    if "mode" not in snap:
        snap["mode"] = "sessions"
    return snap


def _ui_nav_push(chat_data: Dict[str, Any]) -> None:
    nav = _ui_nav_stack(chat_data)
    nav.append(_ui_nav_snapshot(chat_data))
    if len(nav) > 32:
        del nav[:16]


def _ui_nav_reset(chat_data: Dict[str, Any], *, to: Optional[Dict[str, Any]] = None) -> None:
    ui = _ui_get(chat_data)
    if to is None:
        ui["nav"] = []
        return
    if not isinstance(to, dict):
        ui["nav"] = []
        return
    ui["nav"] = [to]


def _ui_nav_restore(chat_data: Dict[str, Any], snap: Dict[str, Any]) -> None:
    ui = _ui_get(chat_data)
    for k in _UI_NAV_KEYS:
        ui.pop(k, None)
    for k, v in snap.items():
        if k in _UI_NAV_KEYS:
            ui[k] = v


def _ui_nav_pop(chat_data: Dict[str, Any]) -> bool:
    nav = _ui_nav_stack(chat_data)
    if not nav:
        return False
    current = _ui_nav_snapshot(chat_data)
    while nav:
        snap = nav.pop()
        if not isinstance(snap, dict):
            continue
        if snap == current:
            continue
        _ui_nav_restore(chat_data, snap)
        return True
    return False


def _ui_nav_to(chat_data: Dict[str, Any], *, mode: str, push: bool = True, **fields: Any) -> None:
    if push:
        current = _ui_nav_snapshot(chat_data)
        desired: Dict[str, Any] = copy.deepcopy(current)
        desired["mode"] = mode
        for k, v in fields.items():
            if k in _UI_NAV_KEYS:
                desired[k] = copy.deepcopy(v)
        if desired != current:
            _ui_nav_push(chat_data)
    _ui_set(chat_data, mode=mode, **fields)


def _ui_sanitize(manager: "SessionManager", chat_data: Dict[str, Any]) -> None:
    ui = _ui_get(chat_data)
    mode = ui.get("mode") if isinstance(ui.get("mode"), str) else "sessions"
    session_name = ui.get("session") if isinstance(ui.get("session"), str) else None
    if mode in {"session", "logs", "model", "model_custom", "confirm_delete", "confirm_stop", "await_prompt"}:
        if not session_name or session_name not in manager.sessions:
            _ui_set(chat_data, mode="sessions")

