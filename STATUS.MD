# STATUS.MD

Last updated: 2026-04-09 06:51 +05:00
Branch: main
Repo: /Users/bkh/openclaw-assistant-integration-work/vibes
Main SHA: b42302a9e30d6a19b908d308e1f571c905ce52e9

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
- end-to-end controller flow tests added in `crates/vibes-testkit/src/lib.rs` for `/new` -> topic bind -> prompt, `/resume` -> rebind -> prompt, and unbound-topic help flow
- topic-title routing integrated into `/new` flow
- `vibes-telegram` update adapter added (`crates/vibes-telegram/src/update_adapter.rs`) with real `teloxide` update -> `TelegramEnvelope` extraction and reply target mapping
- adapter-level Telegram fixtures added for forum-topic text, private text, and ignored non-message updates
- `vibes-app` update ingestion added (`crates/vibes-app/src/update_ingest.rs`) to bridge real Telegram updates into controller actions/replies/dispatch
- app-level update ingestion tests added for forum `/new`, bound DM prompt dispatch, and non-message ignore flow
- `vibes-app` telegram runtime/requester loop added (`crates/vibes-app/src/telegram_runtime.rs`) for real reply sending vs prompt dispatch separation
- requester/runtime tests added in `crates/vibes-app/tests/telegram_runtime.rs`
- concrete teloxide polling entrypoint added in `crates/vibes-app/src/main.rs`
- prompt execution flow unified through runtime completion path so polling main and tests use the same send/execute logic
- runtime send-failure coverage added so reply delivery failures are exercised in tests
- direct-chat and topic-caption runtime coverage expanded across success, execution-failure, fallback, and send-failure paths in `crates/vibes-app/tests/telegram_runtime.rs`
- full green gate achieved and pushed to `main`:
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace --all-features`

## Current loop
1. Expand coverage from runtime loop into deeper end-to-end send/reply execution flow
2. Harden the live polling path around delivery errors, execution failures, and topic-thread behavior
3. Continue direct-to-main delivery with green checks after each increment

## Active risks
- `med-today`/heartbeat operational logic still outside this repo and can drift from bot UX expectations
- full Telegram adapter/runtime wiring is not yet end-to-end complete
- need explicit migration path if store backend later moves from `rusqlite` to `sqlx` pool

## User-facing compact context
User is asleep. Continue autonomously. Send compact updates in format `сделал / проверил / дальше`. Final target is direct green delivery into `main`, not PR flow.
