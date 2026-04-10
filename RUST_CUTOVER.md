# Rust Cutover

## Goal
Finish the migration of `vibes` from the legacy Python runtime into the Rust workspace.

This file is the concrete cutover inventory so the remaining work is explicit and finite.

## Current state

### Already on Rust
- Main Telegram/runtime core in `crates/`
- App/runtime/startup decomposition under `crates/vibes-app/src/`
- Rust binary entrypoint is now exposed via Cargo as `vibes`
- Codex/store/telegram/core crates
- Large Rust test surface for runtime/update/listener/loop behavior

### Still Python-backed
These files are still present and tracked, which means the project is **not yet full Rust**.

## Remaining Python runtime surface

### 1) Top-level executable/runtime entrypoints
- `vibes.py`
- `vibes`

These remain the highest-priority cutover targets because they are still the user-facing/process-facing Python entry layer.

### 2) Python daemon / process management layer
- `src/vibes_app/daemon/cli.py`
- `src/vibes_app/daemon/commands.py`
- `src/vibes_app/daemon/envfile.py`
- `src/vibes_app/daemon/process.py`
- `src/vibes_app/daemon/state.py`

This layer still owns start/stop/status/log/env management around the old runtime.

### 3) Python Telegram bot/runtime layer
- `src/vibes_app/runtime.py`
- `src/vibes_app/telegram_deps.py`
- `src/vibes_app/telegram/panel.py`
- `src/vibes_app/telegram/stream.py`
- `src/vibes_app/bot/**`

This is the biggest remaining live Python application surface.

### 4) Python core/session layer still present
- `src/vibes_app/core/**`
- `src/vibes_app/constants.py`
- `src/vibes_app/utils/**`

Need to verify which parts are still genuinely live vs only compatibility/test leftovers.

### 5) Python tests still present
- `tests/**/*.py`

Not the first runtime blocker, but still part of the non-Rust surface.

### 6) Python-specific setup/docs/scripts
- `requirements.txt`
- `setup.sh`
- `uninstall.sh`
- `scripts/update_status_md.py`
- README sections that still describe `python3`, `vibes.py`, and `python-telegram-bot`

## Practical cutover order

### Phase A — replace live entrypoints
1. Stop treating Python `vibes` / `vibes.py` as the primary runtime entry.
2. Make the Rust `vibes` binary the default operator entrypoint.
3. Remove runtime dependence on `src/vibes_app/daemon/*` for normal start/stop/status flows.

### Phase B — remove Python Telegram runtime usage
4. Identify which `src/vibes_app/bot/**` and `src/vibes_app/telegram/**` paths are still actually live.
5. Port or delete those paths behind the Rust runtime.
6. Remove `telegram_deps.py` / `python-telegram-bot` dependency from the live path.

### Phase C — remove dead Python support code
7. Audit `src/vibes_app/core/**` and `utils/**`.
8. Remove dead compatibility shims.
9. Delete Python tests that only validate removed Python behavior, or replace with Rust coverage.

### Phase D — final cleanup
10. Remove `requirements.txt` if no longer needed.
11. Clean README/setup/uninstall to reflect Rust-only operation.
12. Delete remaining obsolete Python files.

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

This remains the real blocker between “mostly Rust” and “full Rust”.
