# Refactoring Plan: vibes (KISS, modular, ≤500 LOC per file)

**Goal:** Refactor the current monolithic Python scripts (`vibes.py`, `vibes`) into a small, modular, testable codebase without changing user-facing behavior.

**Constraints**
- No single source file > **500** lines.
- KISS: prefer simple functions/modules over frameworks/clever abstractions.
- After each move: run tests, then commit + push (per assignment).

---

## 0) Current Codebase (Discovery Summary)

### What this project is
- **`vibes.py`**: Telegram bot that acts as a **“session manager” for Codex CLI**.
  - Maintains a list of “sessions” (name → local folder path + Codex thread id + model settings).
  - Runs `codex exec --json ...` inside a chosen session directory.
  - Streams JSONL events/logs into a Telegram message (panel) and provides inline keyboard controls.
  - Supports uploading Telegram attachments into session root and referencing them in the prompt.
  - Stores state + logs under `.vibes/`.
- **`vibes`**: CLI/daemon manager to **start/stop/status/logs** for the bot in background.
  - Reads `.env` token/admin id, spawns the bot process, stores pid/state in `.vibes/daemon.json`, logs to `.vibes/daemon.log`.

### External dependencies
- Python **3.10+**
- `python-telegram-bot>=20,<23` (runtime)
- `psutil` (optional status details in daemon script)
- `codex` CLI (runtime; bot can start without it but can’t run sessions)

### Current pain points
- `vibes.py` ~4.7k LOC and `vibes` ~795 LOC.
- Mixed concerns: Telegram UI, Codex execution, state, parsing, file IO all in one place.
- Tests exist, but daemon-manager script has no direct unit coverage.

---

## 1) Target Architecture (Simple & Modular)

### Proposed folder structure
```
docs/
  refactoring_plan.md
src/
  vibes_app/
    __init__.py
    __main__.py
    bot/
      __init__.py
      attachments.py
      callbacks.py
      app.py
      handlers_common.py
      handlers_commands.py
      handlers_callback.py
      handlers_callback_utils.py
      handlers_messages.py
      render_sync.py
      ui_render_current.py
      ui_render_home.py
      ui_render_paths.py
      ui_render_session.py
      ui_render_settings.py
      ui_run.py
      ui_state.py
    core/
      __init__.py
      codex_cmd.py
      codex_events.py
      completion_notice.py
      process_io.py
      session_models.py
      session_manager.py
      session_runner.py
      state_store.py
    daemon/
      __init__.py
      cli.py
      envfile.py
      process.py
      state.py
    telegram/
      __init__.py
      stream.py
      panel.py
    utils/
      __init__.py
      text.py
      time.py
      paths.py
      git.py
      log_files.py
      logging.py
sitecustomize.py
tests/
```

### Entry points (kept tiny)
- `python vibes.py` (compat) → Telegram bot entry (delegates to `src/vibes_app/bot/app.py`)
- `python -m vibes_app` → Telegram bot entry (`src/vibes_app/__main__.py`)
- Keep the existing `vibes` executable file as a thin wrapper (≤200 LOC), delegating to `src/vibes_app/daemon/cli.py`.

### State & runtime data (unchanged paths)
- `.vibes/vibe_state.json` (bot state)
- `.vibes/vibe_logs/` (bot logs)
- `.vibes/vibe_bot.log` (bot internal log)
- `.vibes/daemon.json`, `.vibes/daemon.log` (daemon manager state/log)

---

## 2) Legacy → New Modules Mapping

### `vibes.py`

**Config / presets**
- `_env_flag`, `_read_toml`, `_discover_model_presets`, `MODEL_PRESETS`
  - → `src/vibes_app/core/state_store.py` (toml read) + `src/vibes_app/core/codex_cmd.py` (model preset discovery)
- `_codex_sandbox_mode`, `_codex_approval_policy`
  - → `src/vibes_app/core/codex_cmd.py`

**Git + paths + misc utils**
- `_detect_git_dir` → `src/vibes_app/utils/git.py`
- `_safe_session_name`, `_safe_resolve_path`, `_can_create_directory` → `src/vibes_app/utils/paths.py`
- `_utc_now_iso`, `_log_line`, `_log_error` → `src/vibes_app/utils/logging.py`

**Text / formatting**
- `_truncate_text`, `_strip_html_tags`, `_telegram_safe_html_code_block`, `_tail_text`
  - → `src/vibes_app/utils/text.py`
- `_format_duration` → `src/vibes/utils/time.py`
- `_format_duration` → `src/vibes_app/utils/time.py`
- UUID helpers (`_looks_like_uuid`, `_find_first_uuid`) → `src/vibes_app/utils/uuid.py`

**Logs**
- `_tail_text_file`, `_extract_last_agent_message_from_stdout_log`, `_preview_from_stdout_log`, `_preview_from_stderr_log`
  - → `src/vibes_app/utils/log_files.py`

**Telegram attachments**
- `_max_attachment_bytes`, `_sanitize_attachment_basename`, `_pick_unique_dest_path`,
  `_extract_message_attachments`, `_download_attachments_to_session_root`,
  `_build_prompt_with_downloaded_files`
  - → `src/vibes_app/bot/attachments.py`

**State migration**
- `_atomic_write_text`, `_rewrite_legacy_log_path`, `_rewrite_state_paths_for_runtime_dir`, `_maybe_migrate_runtime_files`
  - → `src/vibes_app/core/state_store.py`

**Codex JSON event parsing**
- `_get_event_type`, `_extract_text_delta`, `_extract_item*`, `_extract_tool_*`, `_maybe_extract_diff`,
  `_extract_session_id_explicit`
  - → `src/vibes_app/core/codex_events.py`

**Telegram streaming UI**
- `Segment` → `src/vibes_app/telegram/stream.py`
- `TelegramStream` → `src/vibes_app/telegram/stream.py`
- `PanelUI` → `src/vibes_app/telegram/panel.py`

**Session core**
- `SessionRun`, `SessionRecord` → `src/vibes_app/core/session_models.py`
- `SessionManager` → `src/vibes_app/core/session_manager.py`
  - `_build_codex_cmd`, `_spawn_process`, `_read_stdout`, `_read_stderr`, `_handle_json_event` split into:
    - `src/vibes_app/core/codex_cmd.py`
    - `src/vibes_app/core/codex_events.py`
    - `src/vibes_app/core/session_manager.py`

**Bot handlers & rendering**
- `_ui_*` helpers → `src/vibes_app/bot/ui_state.py`
- `_render_*` functions → `src/vibes_app/bot/ui_render.py`
- `cmd_*`, `on_callback`, `_schedule_prompt_run`, `on_text`, `on_attachment`, `on_unknown_command`
  - → `src/vibes_app/bot/handlers.py`
- `run_bot`, `_parse_args`, `main`
  - → `src/vibes_app/bot/app.py` + `src/vibes_app/__main__.py`

### `vibes` (daemon manager)
- Env parsing (`_parse_env_file`, `_pick_str`, `_pick_int`) → `src/vibes/daemon/envfile.py`
- Venv python detection (`_detect_local_venv_python`, `_maybe_reexec_into_venv`) → `src/vibes/daemon/process.py`
- PID/process helpers (`_pid_is_running`, `_try_get_cmdline`, `_looks_like_vibes_process`) → `src/vibes/daemon/process.py`
- State file IO (`_load_state`, `_write_state`) → `src/vibes/daemon/state.py`
- CLI commands (`cmd_init`, `cmd_start`, `cmd_status`, `cmd_stop`, `cmd_setup`, `cmd_logs`) → `src/vibes/daemon/cli.py`
- Keep root-level `vibes` executable as a tiny wrapper that imports and calls `vibes.daemon.cli.main()`.

*(Note: the actual package is `vibes_app` due to name collisions with existing root scripts; all paths above will live under `src/vibes_app/daemon/`.)*

---

## 3) Phase 0 TODO (Plan + Commit)

- [x] Create `docs/refactoring_plan.md` (this file).
- [x] `git add docs/refactoring_plan.md && git commit -m "docs: initialization of refactoring plan" && git push`

---

## 4) Phase 1 TODO (Safety Net / Tests)

### Existing tests
- [x] Run existing suite on current (pre-refactor) code: `python -m unittest -v`
- [x] Fix/adjust tests only if they’re flaky/broken (do **not** refactor code yet).

### Add missing coverage (daemon manager)
- [x] Add unit tests for daemon-manager logic currently living in `vibes`:
  - env parsing precedence (cli/env/.env)
  - state read/write roundtrip
  - “pid looks like vibes bot” check
  - start/stop/status behavior with fake PIDs (no real processes; monkeypatch `subprocess`/`os.kill`)
- [x] Confirm tests pass on old code: `python -m unittest -v`
- [x] `git add tests/ && git commit -m "test: infrastructure and initial coverage" && git push`

### Quality gates (reduce technical debt)
- [x] Re-check core business logic flows end-to-end (new session → run prompt → logs → stop/delete).
- [x] Dependency freshness check:
  - [x] Check latest `python-telegram-bot` and `psutil` releases and confirm our version ranges are sane.
  - [x] Bump `python-telegram-bot` upper bound when safe (`>=20,<23`); keep API compatible (no Context7 needed).
- [x] Keep “tech debt” low:
  - [x] Avoid duplicate logic between wrappers (`vibes.py`, `vibes`) and `src/vibes_app/*` modules.
  - [x] Prefer pure helpers + small modules over shared globals.
- [x] Add `.env.example` and keep `.env` gitignored (no token commits).

---

## 5) Phase 2 TODO (Incremental Refactor, KISS Cycle)

### Bootstrap new structure (no behavior change)
- [x] Add `src/vibes_app/...` package skeleton + `sitecustomize.py` to add `src` to `sys.path`.
- [x] Add minimal `src/vibes_app/__main__.py` that calls `vibes_app.bot.app.main()`.
- [x] Keep daemon manager spawning `vibes.py` (now a thin compat shim); keep root `vibes` wrapper thin.
- [x] Run tests; commit + push.

### Modular moves (one module at a time)
Repeat for each extracted module:
- [x] Move a cohesive group of functions/classes (≤500 LOC/file).
- [x] Update imports + re-exports to keep test surface stable.
- [x] Run tests immediately.
- [x] Commit + push with `refactor: moved <module> and optimized according to KISS`.

**Suggested extraction order (lowest risk → highest risk):**
1. [x] `utils` (text, time, paths, git, logging, log_files)
2. [x] `core/state_store` (atomic write + migration)
3. [x] `core/codex_events` (pure parsing helpers)
4. [x] `telegram/stream` + `telegram/panel` (UI primitives)
5. [x] `core/session_models`
6. [x] `core/codex_cmd` (command builder)
7. [x] `core/session_manager` + `core/session_runner` + `core/process_io`
8. [x] `bot/ui_state` + `bot/ui_render_*`
9. [x] `bot/attachments`
10. [x] `bot/handlers_*` + `bot/app`
11. [x] `daemon/*` split (envfile/state/process/cli), shrink root `vibes` wrapper

---

## 6) Phase 3 TODO (Final Integration & Cleanup)

- [x] Ensure entry points are “thin” (orchestration only): `src/vibes_app/__main__.py` + root `vibes` wrapper + `vibes.py` compat shim.
- [x] Final full test run: `python -m unittest discover -s tests -v`
- [x] Update this plan: mark all tasks completed.
- [x] `git add . && git commit -m "feat: refactoring complete, all tests passed" && git push`
