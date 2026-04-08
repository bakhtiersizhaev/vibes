# STATUS.MD

Last updated: 2026-04-08 06:00 +05
Branch: rust-rewrite-telegram-codex
Repo: /Users/bkh/openclaw-assistant-integration-work/vibes

## Mission
Full Rust rewrite/refactor of `vibes` for Telegram supergroups, forum topics, DMs, and Codex CLI (`codex exec --json`) with green checks as the only definition of done.

## Hard constraints
- Rust-first rewrite
- max ~350 lines per file
- modular/extensible architecture
- compact status pings
- use subagents on complex separable branches
- finish with green checks + fresh PR

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
- codex runner integration around `codex exec --json` added (`crates/vibes-codex/src/runner.rs`)
- topic-title routing integrated into `/new` flow
- full green local gate:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace`

## Current loop
1. Flesh out real Telegram adapter from controller primitives to update handling + responses
2. Add end-to-end tests for command -> bind -> run -> render flow
3. Upgrade codex runner from whole-process capture to streaming supervision + cancellation
4. Prepare PR patchset and push branch

## Active risks
- `med-today`/heartbeat operational logic still outside this repo and can drift from bot UX expectations
- runner currently uses whole-process output capture; streaming UX still pending
- need explicit migration path if we switch store backend from `rusqlite` to `sqlx` pool later

## User-facing compact context
User is asleep. Continue autonomously. Send compact updates in format `сделал / делаю / дальше`. Final target is a fresh PR in GitHub only after green checks.
