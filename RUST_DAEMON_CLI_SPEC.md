# Rust Daemon CLI Spec

## Purpose
Port the live Python daemon/launcher surface to Rust without losing current operator behavior.

Source of truth for this spec:
- `vibes`
- `src/vibes_app/daemon/cli.py`
- `src/vibes_app/daemon/commands.py`

## Required commands

### `vibes start`
Start the bot in background mode.

Inputs:
- `--token <token>` optional CLI override
- `--admin <user_id>` optional CLI override
- `--python <path>` optional in Python world; Rust cutover should make this obsolete
- `--env <path>` optional env file path
- `--restart` restart existing running bot if it is confirmed to be a vibes process

Current behavior to preserve:
- Creates runtime dir/state/log locations
- Checks existing pid from state file
- Refuses to restart/stop unrelated pid unless explicitly safe
- Resolves token from CLI or env aliases
- Resolves admin id from env aliases
- Starts background process and writes state with:
  - pid
  - started_at
  - cmd
  - cwd
  - env_path
  - daemon_log
- If child exits immediately, returns failure and points to log path

Rust cutover note:
- `--python` should become deprecated/ignored or removed once Rust daemon path is primary.
- Rust daemon should preserve the same operator-facing safety checks around stale pidfiles and wrong processes.

### `vibes status`
Show whether daemon is running.

Current behavior to preserve:
- If no state file => “stopped”
- If stale pid => “not running”
- If running => print pid, uptime, cpu, rss when available
- Print command line when available
- Print daemon log path when known
- Prefer psutil when available in Python, otherwise fallback to `ps`

Rust cutover note:
- Rust implementation can use native process inspection instead of Python/psutil, but user-facing meaning should stay equivalent.

### `vibes stop`
Stop the daemon.

Inputs:
- `--force`
- `--timeout <seconds>`

Current behavior to preserve:
- If state missing/stale => already stopped
- Validates pid belongs to vibes unless `--force`
- Sends SIGTERM, waits up to timeout, then SIGKILL if needed
- Removes stale state file
- Prints “Остановлено.” on success

### `vibes init`
Create `.env` template.

Inputs:
- `--force`
- `--env <path>`

Current behavior to preserve:
- Refuses to overwrite existing `.env` unless `--force`
- Writes template with keys/comments for:
  - `VIBES_TOKEN`
  - `VIBES_ADMIN_ID`
  - `VIBES_CODEX_SANDBOX`
  - `VIBES_CODEX_APPROVAL_POLICY`
  - `VIBES_CLAUDE_MODEL`
  - `VIBES_CLAUDE_PERMISSION_MODE`
  - `VIBES_DEFAULT_PROJECTS_DIR`
  - `VIBES_PYTHON` (legacy)

Rust cutover note:
- Keep compatible env names initially.
- `VIBES_PYTHON` should be marked legacy once Rust runtime replaces Python launcher.

### `vibes setup`
Interactive env setup.

Inputs:
- `--start`
- `--restart`
- `--python <path>` (legacy)
- `--env <path>`

Current behavior to preserve:
- Prompt for token if missing
- Optionally prompt for admin id
- Update `.env`
- Optionally chain into `start`

### `vibes logs`
Print daemon log path or follow logs.

Inputs:
- `--follow`

Current behavior to preserve:
- Print current daemon log path
- On `--follow`, print tail and continue following
- Return useful errors when file missing/unreadable

### `vibes help`
Help for whole CLI or specific topic.

Current behavior to preserve:
- `vibes help`
- `vibes help start|status|stop|setup|logs|init`

## Environment resolution rules

### Token lookup order
CLI token override first, otherwise first non-empty of:
- `VIBES_TOKEN`
- `VIBES_TELEGRAM_TOKEN`
- `TELEGRAM_BOT_TOKEN`
- `BOT_TOKEN`
- `TALKING_TOKEN`
- `TALKING`
- `Talking`

### Admin id lookup order
- `VIBES_ADMIN_ID`
- `VIBES_TELEGRAM_ADMIN_ID`
- `TELEGRAM_ADMIN_ID`
- `ADMIN_ID`

### Python path lookup order (legacy)
- `VIBES_PYTHON`
- `VIBES_PYTHON_BIN`

## State/log contract to preserve initially
State file currently includes at least:
- `pid`
- `started_at`
- `cmd`
- `cwd`
- `env_path`
- `daemon_log`

Log path should remain discoverable through `status` and `logs`.

## Immediate Rust cutover milestone
Implement a Rust-native `vibes` CLI that covers:
1. `start`
2. `status`
3. `stop`
4. `logs`

Those are the operational commands that make the Python daemon layer unnecessary.

`init` and `setup` can follow, but replacing the live start/stop/status/log flow is the critical cutover.
