from __future__ import annotations

from pathlib import Path
from typing import Any, Dict, List, Optional

from ..constants import CODEX_APPROVAL_POLICIES, CODEX_SANDBOX_MODES, DEFAULT_MODEL_PRESETS
from ..utils.git import detect_git_dir
from .session_models import SessionRecord


def read_toml(path: Path) -> Optional[Dict[str, Any]]:
    try:
        import tomllib  # py3.11+
    except Exception:
        return None
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return None


def discover_model_presets() -> List[str]:
    presets: List[str] = []
    seen: set[str] = set()

    allowed = set(DEFAULT_MODEL_PRESETS)

    def add(val: Optional[str]) -> None:
        if not isinstance(val, str):
            return
        s = val.strip()
        if not s or s in seen:
            return
        if allowed and s not in allowed:
            return
        seen.add(s)
        presets.append(s)

    import os

    codex_home = Path(os.environ.get("CODEX_HOME") or (Path.home() / ".codex")).expanduser()
    cfg = codex_home / "config.toml"
    data = read_toml(cfg)
    if isinstance(data, dict):
        model = data.get("model")
        if isinstance(model, str):
            add(model)

        notice = data.get("notice")
        if isinstance(notice, dict):
            migrations = notice.get("model_migrations")
            if isinstance(migrations, dict):
                if isinstance(model, str):
                    migrated = migrations.get(model)
                    add(migrated if isinstance(migrated, str) else None)

    for m in DEFAULT_MODEL_PRESETS:
        add(m)

    return presets


MODEL_PRESETS: List[str] = discover_model_presets()


def codex_sandbox_mode() -> str:
    import os

    raw = os.environ.get("VIBES_CODEX_SANDBOX", "").strip()
    if raw in CODEX_SANDBOX_MODES:
        return raw
    return "workspace-write"


def codex_approval_policy() -> str:
    import os

    raw = os.environ.get("VIBES_CODEX_APPROVAL_POLICY", "").strip()
    if raw in CODEX_APPROVAL_POLICIES:
        return raw
    return "never"


def build_codex_cmd(rec: SessionRecord, *, prompt: str, run_mode: str) -> List[str]:
    sandbox_mode = codex_sandbox_mode()
    approval_policy = codex_approval_policy()
    base = ["codex", "exec", "--json", "--sandbox", sandbox_mode, "-c", f"approval_policy={approval_policy}"]

    # If this is a git repo (or a nested path within a repo) — add gitdir as writable dir.
    # Otherwise include the flag so Codex doesn't fail outside Git.
    git_dir = detect_git_dir(Path(rec.path))
    if git_dir is None:
        base.append("--skip-git-repo-check")
    else:
        base += ["--add-dir", str(git_dir)]

    base += ["-C", rec.path]

    base += ["--model", rec.model]
    base += ["-c", f"model_reasoning_effort={rec.reasoning_effort}"]

    prompt_s = prompt or ""
    needs_end_of_opts = bool(prompt_s.lstrip().startswith("-"))
    if run_mode == "continue" and rec.thread_id:
        base += ["resume", rec.thread_id]
        if needs_end_of_opts:
            base.append("--")
        base.append(prompt_s)
    else:
        if needs_end_of_opts:
            base.append("--")
        base.append(prompt_s)
    return base

