from __future__ import annotations

from pathlib import Path


DEFAULT_RUNTIME_DIR = Path("./.vibes")
DEFAULT_STATE_PATH = DEFAULT_RUNTIME_DIR / "vibe_state.json"
DEFAULT_LOG_DIR = DEFAULT_RUNTIME_DIR / "vibe_logs"
DEFAULT_BOT_LOG_PATH = DEFAULT_RUNTIME_DIR / "vibe_bot.log"

# NOTE: tests monkeypatch these module-level paths.
STATE_PATH = DEFAULT_STATE_PATH
LOG_DIR = DEFAULT_LOG_DIR
BOT_LOG_PATH = DEFAULT_BOT_LOG_PATH

# Legacy (pre-.vibes) locations.
LEGACY_STATE_PATH = Path("./vibe_state.json")
LEGACY_LOG_DIR = Path("./vibe_logs")
LEGACY_BOT_LOG_PATH = Path("./vibe_bot.log")

