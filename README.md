# vibes

Telegram bot that manages Codex CLI sessions from your phone. Create sessions, resume them, and interact with Codex — all through Telegram supergroups, forum topics, and DMs.

Built in Rust with teloxide, tokio, and SQLite persistence.

## Quick Start

### 1. Prerequisites

- Rust 1.92+ (install via [rustup](https://rustup.rs))
- `codex` CLI in `PATH` with a configured API key
- A Telegram bot token from [@BotFather](https://t.me/BotFather)

### 2. Build

```bash
git clone https://github.com/bakhtiersizhaev/vibes.git
cd vibes
cargo build --release
```

The binary is at `target/release/vibes`.

### 3. Configure

```bash
./target/release/vibes init
```

This creates a `.env` file. Edit it and set your bot token:

```bash
# Required
VIBES_TOKEN=your-telegram-bot-token

# Optional
# VIBES_ADMIN_ID=your-telegram-user-id
# VIBES_CODEX_SANDBOX=networking
# VIBES_CODEX_APPROVAL_POLICY=never
# VIBES_CLAUDE_MODEL=sonnet
# VIBES_CLAUDE_PERMISSION_MODE=bypassPermissions
# VIBES_DEFAULT_PROJECTS_DIR=~/projects
```

### 4. Run

Start the bot directly:

```bash
VIBES_TOKEN=your-token ./target/release/vibes
```

Or as a background daemon:

```bash
./target/release/vibes start
```

### 5. Use

Open Telegram and message your bot:

- `/new` — Create a new Codex session (creates a forum topic in supergroups)
- `/resume <session-id>` — Resume an existing session
- Any text message — Sends a prompt to the bound Codex session

## CLI Commands

```bash
vibes              # Start bot in foreground (polling mode)
vibes init         # Create .env template
vibes start        # Start bot as background daemon
vibes start --restart  # Restart running daemon
vibes status       # Check if daemon is running
vibes stop         # Stop the daemon
vibes logs         # Show recent log output
vibes logs --follow    # Tail logs in real-time
vibes setup        # Interactive setup (creates .env, optionally starts bot)
```

## Architecture

The project is a Rust workspace with modular crates:

```
crates/
  vibes-app/       # App bootstrap, CLI, polling runtime, Telegram routing
  vibes-core/      # Domain model: commands, scopes, sessions, bindings
  vibes-telegram/  # Telegram Bot API adapter (teloxide), forum topic handling
  vibes-codex/     # Codex CLI process adapter, JSONL event parser, streaming
  vibes-store/     # SQLite persistence for session bindings
  vibes-testkit/   # Test fixtures and e2e harness
```

## Features

- **Supergroup forum topics** — Each `/new` session gets its own topic thread
- **DM sessions** — Works in private messages without topics
- **Session persistence** — Bindings survive restarts (SQLite)
- **Codex streaming** — Parses `codex exec --json` output in real-time
- **Session resume** — `/resume <id>` rebinds to an existing Codex session
- **Daemon management** — start/stop/status/logs like a proper service
- **Mobile-first UX** — Compact, readable messages for phone screens

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `VIBES_TOKEN` | Yes | Telegram Bot API token |
| `VIBES_ADMIN_ID` | No | Telegram user ID for admin commands |
| `VIBES_DB_PATH` | No | SQLite database path (default: `.vibes/db.sqlite`) |
| `VIBES_WORKSPACE_ROOT` | No | Root directory for Codex sessions |
| `VIBES_CODEX_SANDBOX` | No | Codex sandbox mode |
| `VIBES_CODEX_APPROVAL_POLICY` | No | Codex approval policy |
| `VIBES_CLAUDE_MODEL` | No | Claude model override |
| `VIBES_CLAUDE_PERMISSION_MODE` | No | Claude permission mode |
| `VIBES_DEFAULT_PROJECTS_DIR` | No | Default directory for new sessions |

## File Locations

| File | Path |
|------|------|
| Config | `.env` |
| Daemon state | `.vibes/daemon.json` |
| Daemon log | `.vibes/daemon.log` |
| Session database | `.vibes/db.sqlite` |

## Development

```bash
# Run tests
cargo test --workspace --all-features

# Check formatting + lints
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Build debug
cargo build --workspace
```

## Troubleshooting

- **Bot doesn't respond**: Check `vibes status` and `vibes logs -f`
- **No codex in PATH**: Install Codex CLI and configure your API key
- **Permission denied**: Make sure the binary is executable (`chmod +x`)
- **Token not found**: Set `VIBES_TOKEN` in `.env` or pass `--token` flag

## License

Apache-2.0
