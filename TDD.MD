# TDD.MD

Date: 2026-04-08
Repo: vibes
Mode: RED -> GREEN -> REFACTOR

## Definition of Green
Minimum required green pipeline:
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Task completion later also requires:
- working session/topic binding flows
- working Codex JSONL parsing and streaming
- PR pushed to GitHub

## Test Pyramid

### 1. Unit Tests
Focus:
- command parsing (`/new`, `/resume`, `/status`, invalid inputs)
- scope resolution (`DM`, `supergroup`, `forum topic`)
- session binding rules
- run lifecycle transitions
- completion classifier (`green-only`)
- mobile rendering policy
- Codex event parsing and reduction

### 2. Component Tests
Focus:
- Telegram update handler -> router -> application service
- session registry + persistence
- Codex stream adapter -> Telegram renderer
- forum topic create/bind behavior
- DM continuity behavior

### 3. Contract Tests
Focus:
- Telegram Bot API payload contract fixtures
- `codex exec --json` event fixtures
- storage schema contracts
- resume compatibility contracts

### 4. End-to-End Acceptance Tests
Focus:
- `/new` in forum-enabled supergroup
- `/resume <session-id>` in topic
- two topics in one group -> isolated sessions
- DM flow
- restart recovery
- duplicate Telegram delivery
- fail mid-stream

### 5. Non-Functional Gates
Focus:
- file length <= 350 lines
- idempotency
- resilience to duplicate updates
- parser tolerance to noisy stderr/stdout
- Rust rewrite parity for key semantics

## Core Domain Model To Test
- `ChatScope`
  - DM(chat_id)
  - Group(chat_id)
  - Topic(chat_id, topic_id)
- `SessionBinding`
  - scope
  - codex_session_id
  - display_name
  - workspace path
  - created_at / updated_at
- `RunState`
  - Idle
  - Creating
  - Bound
  - Streaming
  - Completed
  - Failed
  - Cancelled
- `CodexEvent`
  - thread.started
  - turn.started
  - item.completed
  - turn.completed
  - unknown

## Core Invariants
- one active binding per scope
- `/new` always creates a fresh session
- `/resume` only binds an existing Codex session
- topic isolation is strict
- DM bindings are chat-scoped
- success is reported only on explicit green completion
- stream close alone is never success

## Unit Test Suites
### A. Command Parsing
Cases:
- `/new`
- `/new   `
- `/resume <session-id>`
- `/resume <thread-name>`
- `/resume` missing arg
- malformed input
- `/new@bot` form
- unknown commands

### B. Scope Resolution
Cases:
- DM update
- supergroup without topic
- forum topic with `message_thread_id`
- inconsistent/missing topic metadata
- reply inside topic

### C. New Session Flow
Cases:
- no existing binding
- duplicate update delivery
- Codex create failure before session id
- store failure after session creation
- topic creation failure

### D. Resume Flow
Cases:
- existing session id
- existing thread name
- unknown session
- already bound session
- replacing old binding in same topic

### E. Continuity / Persistence
Cases:
- restart with persisted bindings
- partial write recovery
- stale binding cleanup
- migration compatibility

### F. Codex Event Parsing
Cases:
- valid JSONL events
- mixed stderr noise
- malformed JSON line
- partial chunks
- unknown event types
- success completion
- failed completion

### G. Green-Only Completion
Cases:
- completed + success true
- completed + success false
- failed
- cancelled
- missing final success event
- stream closed after partial output

### H. Mobile Rendering
Cases:
- short ack message
- long output chunking
- code blocks
- copyable session identifiers
- topic-specific notices
- concise failure text

## Component Test Suites
### Telegram Adapter
- update -> command mapping
- topic-aware routing
- send/edit payload generation
- reply target preservation

### Store Adapter
- create binding
- update binding
- query by scope
- query by session id
- idempotent upsert

### Codex Adapter
- spawn `codex exec --json`
- spawn `codex exec resume`
- parse incremental events
- supervise exit code and cancel path

## End-to-End Acceptance Scenarios
### Scenario 1 — `/new` in topic-enabled group
- user sends `/new`
- topic created
- session created
- binding persisted
- follow-up message continues same session

### Scenario 2 — `/resume` in topic
- user sends `/resume <id>`
- binding updated
- next message continues existing session

### Scenario 3 — isolation across topics
- Topic A and Topic B each bind different Codex sessions
- prompts interleave without bleed

### Scenario 4 — DM continuity
- `/new` in DM
- prompt continues same session in DM scope

### Scenario 5 — restart recovery
- restart service after binding
- next message resumes correctly

### Scenario 6 — failure mid-stream
- partial output shown
- failure reported clearly
- no false green completion

## Current Red Baseline
Current Python repo baseline already red:
- plain `pytest -q` fails on collection because test runner cannot import `vibes`

This matters because the Rust rewrite must establish a **cleaner baseline than current main**.

## First Implementation Order
1. workspace skeleton
2. core types + parser tests
3. store traits + in-memory repo
4. Codex adapter parser/supervision
5. Telegram routing layer
6. SQLite layer
7. e2e harness

## File-Length Gate
We add and keep a file-length rule:
- fail if a source file exceeds 350 lines
- split by responsibility before it becomes a problem

## Acceptance Exit
The rewrite loop only exits when:
- docs are present
- Rust workspace is present
- tests exist
- required checks are green
- branch is pushed and PR is created
