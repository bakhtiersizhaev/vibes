from __future__ import annotations

import datetime as dt
from dataclasses import dataclass
from pathlib import Path
from typing import Any, List, Optional, Tuple

from ..constants import MAX_DOWNLOADED_FILENAME_LEN


def max_attachment_bytes() -> Optional[int]:
    import os

    raw = os.environ.get("VIBES_MAX_ATTACHMENT_MB", "").strip()
    if not raw:
        return None
    try:
        mb = int(raw)
    except Exception:
        return None
    if mb <= 0:
        return None
    return mb * 1024 * 1024


def sanitize_attachment_basename(name: str) -> str:
    # Avoid path traversal and platform-specific path separators.
    base = (name or "").strip().replace("\x00", "")
    base = base.replace("/", "_").replace("\\", "_")
    base = "".join(ch if (ch >= " " and ch != "\x7f") else "_" for ch in base).strip()
    if not base or base in {".", ".."}:
        return "file"

    if len(base) > MAX_DOWNLOADED_FILENAME_LEN:
        p = Path(base)
        suffix = p.suffix
        if suffix and len(suffix) < MAX_DOWNLOADED_FILENAME_LEN:
            keep = MAX_DOWNLOADED_FILENAME_LEN - len(suffix)
            base = p.stem[:keep] + suffix
        else:
            base = base[:MAX_DOWNLOADED_FILENAME_LEN]
    return base


def pick_unique_dest_path(dest_dir: Path, basename: str) -> Path:
    safe = sanitize_attachment_basename(basename)
    cand = dest_dir / safe
    if not cand.exists():
        return cand

    p = Path(safe)
    stem = p.stem or "file"
    suffix = p.suffix
    for i in range(2, 10_000):
        cand2 = dest_dir / f"{stem}_{i}{suffix}"
        if not cand2.exists():
            return cand2

    ts = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d_%H%M%S")
    return dest_dir / f"{stem}_{ts}{suffix}"


@dataclass(frozen=True)
class AttachmentRef:
    file_id: str
    file_unique_id: Optional[str]
    preferred_name: Optional[str]
    default_stem: str
    file_size: Optional[int]


def extract_message_attachments(message: Any) -> List[AttachmentRef]:
    """
    Best-effort extraction of file-like Telegram attachments from a message.
    Returns a list to support media groups (each message usually has one attachment).
    """
    att = getattr(message, "effective_attachment", None)
    if not att:
        return []

    # Photos come as a list of sizes; pick the biggest.
    if isinstance(att, list):
        if not att:
            return []
        best = att[-1]
        file_id = getattr(best, "file_id", None)
        if not isinstance(file_id, str) or not file_id:
            return []
        unique = getattr(best, "file_unique_id", None)
        uniq = unique if isinstance(unique, str) and unique else None
        size = getattr(best, "file_size", None)
        file_size = int(size) if isinstance(size, int) and size > 0 else None
        stem = f"photo_{uniq or file_id}"
        return [
            AttachmentRef(
                file_id=file_id,
                file_unique_id=uniq,
                preferred_name=None,
                default_stem=stem,
                file_size=file_size,
            )
        ]

    file_id = getattr(att, "file_id", None)
    if not isinstance(file_id, str) or not file_id:
        return []
    unique = getattr(att, "file_unique_id", None)
    uniq = unique if isinstance(unique, str) and unique else None

    preferred = getattr(att, "file_name", None)
    preferred_name = preferred if isinstance(preferred, str) and preferred.strip() else None
    size = getattr(att, "file_size", None)
    file_size = int(size) if isinstance(size, int) and size > 0 else None

    # Derive a stable-ish stem from attachment "type".
    type_hint = "file"
    for attr, hint in (
        ("document", "document"),
        ("audio", "audio"),
        ("video", "video"),
        ("voice", "voice"),
        ("video_note", "video_note"),
        ("animation", "animation"),
        ("sticker", "sticker"),
    ):
        if getattr(message, attr, None) is att:
            type_hint = hint
            break

    stem = f"{type_hint}_{uniq or file_id}"
    return [
        AttachmentRef(
            file_id=file_id,
            file_unique_id=uniq,
            preferred_name=preferred_name,
            default_stem=stem,
            file_size=file_size,
        )
    ]


async def download_attachments_to_session_root(
    *,
    message: Any,
    bot: Any,
    session_root: Path,
) -> Tuple[List[str], Optional[str]]:
    if not session_root.exists() or not session_root.is_dir():
        raise FileNotFoundError(f"Session directory not found: {session_root}")

    refs = extract_message_attachments(message)
    if not refs:
        return [], None

    saved: List[str] = []
    skipped: List[str] = []
    max_bytes = max_attachment_bytes()
    for ref in refs:
        if max_bytes is not None and isinstance(ref.file_size, int) and ref.file_size > max_bytes:
            label = ref.preferred_name or f"{ref.default_stem} (id:{ref.file_id})"
            skipped.append(label)
            continue

        tg_file = await bot.get_file(ref.file_id)
        file_path = getattr(tg_file, "file_path", None)
        suffix = ""
        if isinstance(file_path, str) and file_path:
            suffix = Path(file_path).suffix
        if not suffix:
            suffix = ""

        preferred = ref.preferred_name
        if preferred is None:
            preferred = f"{ref.default_stem}{suffix}"

        dest_path = pick_unique_dest_path(session_root, preferred)
        await tg_file.download_to_drive(custom_path=str(dest_path))
        saved.append(dest_path.name)

    notice = None
    if skipped and max_bytes is not None:
        lim_mb = max_bytes / (1024 * 1024)
        skipped_view = ", ".join(skipped[:6])
        more = f" (+{len(skipped) - 6} more)" if len(skipped) > 6 else ""
        notice = f"Attachment too large (limit: {lim_mb:.0f} MB). Skipped: {skipped_view}{more}"

    return saved, notice


def build_prompt_with_downloaded_files(*, user_text: str, filenames: List[str]) -> str:
    names = [n for n in (filenames or []) if isinstance(n, str) and n.strip()]
    names = sorted(set(names))
    file_list = "\n".join(f"- {n}" for n in names) if names else "- (нет)"
    user_text = (user_text or "").strip()

    if user_text:
        return (
            "В корне рабочей директории этой сессии сохранены файлы (скачаны из Telegram).\n"
            "Обрати на них внимание и в ответе перечисли их имена списком:\n"
            f"{file_list}\n\n"
            "Сообщение пользователя:\n"
            f"{user_text}"
        ).strip()

    return (
        "В корне рабочей директории этой сессии сохранены файлы (скачаны из Telegram).\n"
        "Обрати на них внимание и в ответе перечисли их имена списком:\n"
        f"{file_list}\n\n"
        "Текущего текста от пользователя нет.\n"
        "Если задача/промпт находится в этих файлах (текст, PDF, изображения и т.п.) — извлеки его и выполни."
    ).strip()

