# Rust Cutover

## Goal
Finish the migration of `vibes` from the legacy Python runtime into the Rust workspace.

This file is the concrete cutover inventory so the remaining work is explicit and finite.

## Current state

### Already on Rust
- Main Telegram/runtime core in `crates/`
- App/runtime/startup decomposition under `crates/vibes-app/src/`
- Codex/store/telegram/core crates
- Large Rust test surface for runtime/update/listener/loop behavior

### Still Python-backed
These files are still present and tracked, which means the project is **not yet full Rust**.

## Remaining Python runtime surface

### 1) Top-level executable/runtime entrypoints
- `vibes.py`
- `vibes`

These are the highest-priority cutover targets because they are user-facing/process-facing entrypoints.

### 2) Python daemon / process management layer
- `src/vibes_app/daemon/cli.py`
- `src/vibes_app/daemon/commands.py`
- `src/vibes_app/daemon/envfile.py`
- `src/vibes_app/daemon/process.py`
- `src/vibes_app/daemon/state.py`

This layer appears to own start/stop/status/log/env management around the old runtime.

### 3) Python Telegram bot/runtime layer
- `src/vibes_app/runtime.py`
- `src/vibes_app/telegram_deps.py`
- `src/vibes_app/telegram/panel.py`
- `src/vibes_app/telegram/stream.py`
- `src/vibes_app/bot/**`

This is the largest legacy application surface.

### 4) Python core/session layer still present
- `src/vibes_app/core/**`
- `src/vibes_app/constants.py`
- `src/vibes_app/utils/**`

Need to verify which parts are still actually used by the live entrypoints vs only retained for old tests/compat.

### 5) Python tests still present
- `tests/**/*.py`

These are not the first blocker for runtime cutover, but they are still part of the repo’s non-Rust surface.

### 6) Python-specific setup/docs/scripts
- `requirements.txt`
- `setup.sh`
- `uninstall.sh`
- `scripts/update_status_md.py`
- README sections that still describe `python3`, `vibes.py`, `python-telegram-bot`

## Practical cutover order

### Phase A — replace live entrypoints
1. Replace `vibes.py` runtime entry with Rust equivalent
2. Replace `vibes` daemon/CLI wrapper with Rust equivalent
3. Stop relying on `src/vibes_app/daemon/*` at runtime

### Phase B — remove Python Telegram runtime usage
4. Identify which `src/vibes_app/bot/**` and `src/vibes_app/telegram/**` paths are still actually live
5. Port or delete those paths behind the Rust runtime
6. Remove `telegram_deps.py` / `python-telegram-bot` dependency from live path

### Phase C — remove dead Python support code
7. Audit `src/vibes_app/core/**` and `utils/**`
8. Remove dead compatibility shims
9. Delete Python tests that only validate removed Python behavior, or replace with Rust coverage

### Phase D — final cleanup
10. Remove `requirements.txt` if no longer needed
11. Clean README/setup/uninstall to reflect Rust-only operation
12. Delete remaining obsolete Python files

## Definition of done
Project is “full Rust” only when all of the following are true:
- No live runtime path depends on `python3`
- No top-level user/daemon entrypoint is Python-backed
- No `src/vibes_app/**` code is required for normal operation
- Python-only setup/runtime docs are removed or clearly marked legacy

## Immediate next concrete target
Replace the live Python entrypoint/daemon surface:
- `vibes.py`
- `vibes`
- `src/vibes_app/daemon/*`

That is the real blocker between “mostly Rust” and “full Rust”.
