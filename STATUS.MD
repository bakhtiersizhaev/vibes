# STATUS.MD

Last updated: 2026-04-08 12:36 +05
Branch: main
Repo: /Users/bkh/openclaw-assistant-integration-work/vibes
Main SHA: 0b44eaaaf906bdc0430621c1531dec6bb8d09929

## Mission
Finish the Rust rewrite/refactor of `vibes` for Telegram supergroups, forum topics, DMs, and Codex CLI (`codex exec --json`) with green checks and direct push to `bakhtiersizhaev/vibes` `main`.

## Hard constraints
- Rust-first rewrite
- max ~350 lines per file
- modular/extensible architecture
- compact status pings
- use subagents on complex separable branches
- finish with green checks + direct push to `main`

## Current decisions
- Stack target: tokio, teloxide, sqlite persistence, serde_json, tracing, clap, anyhow/thiserror, uuid
- Current implemented persistence layer: `rusqlite` with a future migration path to `sqlx` if async pool semantics become necessary
- Polling first, webhook later
- Typed domain + store + telegram adapter + codex runner
- Topic-first routing
- `serde_json::Value` -> internal `CodexEvent` normalizer for Codex JSONL

## Done
- `CONTEXT.MD`, `PLAN.MD`, `TDD.MD` written in repo
- workspace + crate skeleton fully wired
- core command/scope/session + resume lookup tests green
- app service for `/new` and `/resume` added (`crates/vibes-app/src/service.rs`)
- Telegram message router added for `/new`, `/resume`, and bound prompt routing (`crates/vibes-app/src/router.rs`)
- app controller + presenter pipeline added for Telegram-facing replies and prompt dispatch (`crates/vibes-app/src/controller.rs`, `crates/vibes-app/src/presenter.rs`)
- persistent SQLite binding store added (`crates/vibes-store/src/sqlite.rs`)
- codex runner upgraded to streaming supervision + cancellation (`crates/vibes-codex/src/runner.rs`)
- codex streaming/cancellation tests added (`crates/vibes-codex/tests/runner_stream.rs`)
- topic-title routing integrated into `/new` flow
- full green gate achieved and pushed to `main`:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace --all-features`

## Current loop
1. Sync any leftover working-tree drift with pushed `main`
2. Flesh out real Telegram adapter from controller primitives to update handling + responses
3. Add end-to-end tests for command -> bind -> run -> render flow
4. Continue direct-to-main delivery with green checks after each increment

## Active risks
- `med-today`/heartbeat operational logic still outside this repo and can drift from bot UX expectations
- full Telegram adapter/runtime wiring is not yet end-to-end complete
- need explicit migration path if store backend later moves from `rusqlite` to `sqlx` pool

## User-facing compact context
User is asleep. Continue autonomously. Send compact updates in format `сделал / проверил / дальше`. Final target is direct green delivery into `main`, not PR flow.
