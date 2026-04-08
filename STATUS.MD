# STATUS.MD

Last updated: 2026-04-08 06:02 Asia/Yekaterinburg
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
- Stack: tokio, teloxide, sqlx/sqlite, serde_json, tracing, clap, anyhow/thiserror, uuid
- Polling first, webhook later
- Typed domain + store + telegram adapter + codex runner
- Topic-first routing
- `serde_json::Value` -> internal `CodexEvent` normalizer for Codex JSONL

## Done
- `CONTEXT.MD`, `PLAN.MD`, `TDD.MD` written in repo
- workspace + crate skeleton fully wired
- core command/scope/session + resume lookup tests green
- app service for `/new` and `/resume` added (`crates/vibes-app/src/service.rs`)
- persistent SQLite binding store added (`crates/vibes-store/src/sqlite.rs`)
- codex runner integration around `codex exec --json` added (`crates/vibes-codex/src/runner.rs`)
- topic-title routing integrated into `/new` flow
- full `cargo test --workspace` green

## Current loop
1. Wire Telegram handlers to `AppService` runtime/store
2. Add end-to-end tests for command -> bind -> run -> render flow
3. Add process streaming supervision and cancellation for long codex runs
4. Prepare PR patchset and push branch

## Active risks
- `med-today`/heartbeat operational logic still outside this repo and can drift from bot UX expectations
- runner currently uses whole-process output capture; streaming UX still pending
- need explicit migration path if we switch store backend from rusqlite to sqlx pool later

## User-facing compact context
User is asleep. Continue autonomously. Send compact updates in format `сделал / делаю / дальше`. Final target is a fresh PR in GitHub only after green checks.
