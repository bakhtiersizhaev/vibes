# PLAN.MD

Date: 2026-04-08
Repo: vibes
Execution Mode: LOOP
Branch: `rust-rewrite-telegram-codex`

## Mission
Rewrite and refactor `vibes` into a **Rust-first Telegram application** for Codex CLI that works well in:
- Telegram supergroups
- Telegram forum topics / subtopics
- Telegram DMs
- mobile-first usage

Core user flows:
- `/new` creates a new Codex session and, in forum-enabled supergroups, creates a dedicated topic
- `/resume <session-id-or-name>` binds an existing Codex session to Telegram scope
- messages inside a topic continue the bound Codex session automatically
- Codex output is streamed from `codex exec --json` into Telegram in a clean readable way

## Rewrite Constraints
- Rust rewrite, not incremental Python polish
- latest maintained libraries with low tech debt
- max file size target: 350 lines
- smart modular structure for scaling
- tests first / contracts first
- task is done only when checks are green and PR is ready

## Strategic Decision
This repo already contains a working Python product shape.
We **reuse product semantics**, but **not old architecture**.

Meaning:
- keep good UX concepts
- discard Python monolith boundaries
- replace stringly / mutable state flow with typed domain services
- replace shell-centric runtime assumptions with explicit adapters and repositories

## What The Old Repo Gives Us
Useful preserved concepts from current `vibes`:
- session registry
- Codex CLI command building with resume support
- stream parsing intent
- Telegram session management UI concepts
- topic creation attempt for new sessions in supergroups
- daemon/status/log management intent

## What Must Change Hard
Current problems we explicitly remove:
- oversized Python files
- mixed UI/domain/runtime logic
- fragile runtime packaging/import story
- loose process/event typing
- hidden coupling between Telegram handlers and session state

## Target Rust Layout
```text
vibes/
  CONTEXT.MD
  PLAN.MD
  TDD.MD
  Cargo.toml
  rust-toolchain.toml
  crates/
    vibes-app/
    vibes-core/
    vibes-telegram/
    vibes-codex/
    vibes-store/
    vibes-testkit/
```

## Crate Responsibilities
### `vibes-app`
- app bootstrap
- config loading
- dependency wiring
- polling/webhook runtime start
- graceful shutdown

### `vibes-core`
- domain model
- command grammar
- scope resolution rules
- session lifecycle
- run orchestration
- rendering policy contracts

### `vibes-telegram`
- Telegram Bot API adapter
- update decoding
- forum topic creation/binding
- send/edit message operations
- mobile-friendly formatting adapter

### `vibes-codex`
- `codex exec --json` process adapter
- `codex exec resume` support
- JSONL event parser
- stdout/stderr supervision
- stream batching / cancellation / completion classification

### `vibes-store`
- SQLite schema + migrations
- repositories
- session bindings
- run metadata
- idempotency / dedupe state

### `vibes-testkit`
- fake Codex process
- Telegram fixtures
- NDJSON fixtures
- e2e harness

## Delivery Loops
### LOOP 1 — Docs + Workspace Skeleton
Task:
- capture repo context
- write `CONTEXT.MD`, `PLAN.MD`, `TDD.MD`
- scaffold Rust workspace
Success criteria:
- docs exist in repo
- Rust workspace exists
Verify:
- file existence
- `cargo metadata`
Max attempts: 3

### LOOP 2 — Core Domain + Contracts
Task:
- define commands, scope keys, session bindings, run state
- define Codex event model and completion classifier
Success criteria:
- core types compile
- parser and domain tests exist
Verify:
- `cargo test -p vibes-core`
- `cargo test -p vibes-codex`
Max attempts: 5

### LOOP 3 — Store Layer
Task:
- implement SQLite persistence for session bindings, run state, dedupe info
Success criteria:
- bindings survive restart
Verify:
- repository integration tests
Max attempts: 5

### LOOP 4 — Telegram Adapter
Task:
- map Telegram updates to core commands
- support forum topic routing and creation
Success criteria:
- topic-aware routing works in tests
Verify:
- Telegram adapter tests
Max attempts: 5

### LOOP 5 — Codex Runtime Adapter
Task:
- spawn and supervise `codex exec --json`
- handle resume and live stream events
Success criteria:
- real/fake process contracts work
Verify:
- parser + process supervision tests
Max attempts: 5

### LOOP 6 — E2E UX
Task:
- `/new`, `/resume`, DM flow, restart recovery, duplicate update handling
Success criteria:
- acceptance flows pass
Verify:
- e2e suite via testkit
Max attempts: 5

### LOOP 7 — Hardening + PR
Task:
- fmt/clippy/test green
- docs updated
- commit and push branch
- fresh PR
Success criteria:
- full green pipeline
Verify:
- local checks + GitHub push
Max attempts: 5

## Current Known Baseline Problems
- Python tests already fail on collection (`import vibes` issue)
- large files exceed new size rule
- no typed persistence contracts
- no Rust workspace yet

## Tech Stack Choice
Planned stack:
- Rust 1.92+
- Tokio
- Teloxide
- SQLx + SQLite
- Serde + serde_json
- Tracing
- Clap
- Axum only if webhook mode is enabled

## Verification Pipeline
Target required green:
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Later gated smoke:
- live Codex CLI parser smoke
- live Telegram topic flow smoke

## Completion Rule
This task is not done when code exists.
It is done only when:
- architecture is in repo
- Rust rewrite skeleton is in repo
- tests are in repo
- checks are green
- branch is pushed
- fresh PR exists
