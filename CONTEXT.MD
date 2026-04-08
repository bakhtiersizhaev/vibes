# CONTEXT.MD

Date: 2026-04-08
Repo: vibes
Mode: LOOP

## What This Repository Is Today
`vibes` is currently a Python Telegram bot that acts as a session manager for Codex CLI.

Observed current shape:
- Python runtime
- `python-telegram-bot` + `psutil`
- app code under `src/vibes_app`
- tests under `tests/`
- shell-based setup/start/daemon workflow

Existing major areas:
- Telegram bot handlers and UI rendering
- Codex command building and event parsing
- session manager/state store
- daemon/start-stop/status CLI wrapper
- streaming output back into Telegram

## What We Are Building Now
A **full Rust rewrite + refactor** of this product for:
- Telegram supergroups
- Telegram forum topics / subtopics
- Telegram DMs
- Codex CLI integration through `codex exec --json`
- comfortable mobile-first UX
- session creation and resume from Telegram

Main target UX:
- `/new` from a group or control surface creates a new Codex session and topic
- `/resume <session-id-or-name>` binds an existing Codex session to Telegram scope
- topic messages continue the correct session automatically
- DMs work without topic semantics
- output feels like Codex in terminal, but readable on mobile Telegram

## Hard Constraints
- Full rewrite/refactor in Rust
- Prefer current, maintained libraries with low tech debt
- Max file size target: 350 lines
- Scalable architecture
- Modular folders and crates
- Green tests are required before completion
- Final state must be PR-ready in GitHub

## Facts Collected From Existing Repo
Current Python hotspots by size:
- `src/vibes_app/bot/handlers_messages.py` — 484 lines
- `src/vibes_app/bot/handlers_callback.py` — 465 lines
- `src/vibes_app/core/session_manager.py` — 458 lines
- `src/vibes_app/daemon/commands.py` — 432 lines

Current test baseline is red on collection when running plain `pytest -q`:
- `ModuleNotFoundError: No module named 'vibes'`
Meaning current project ergonomics/import path are already imperfect even before rewrite.

## What Must Be Preserved Conceptually
From the old app we preserve these concepts, not the old code shape:
- session registry
- Codex command builder / resume support
- stream parsing from Codex output
- session/topic binding
- Telegram mobile operator UX
- completion/failure reporting
- daemon/runtime management intent

## What We Are Intentionally Changing
We are not doing a blind 1:1 line-by-line port.
We are doing a domain-led rewrite.

Key changes:
- Python monolith -> Rust workspace
- mutable ad-hoc state -> typed domain + repositories
- handler-heavy flow -> command router + service layer
- stringly parsing -> typed Codex event model
- implicit runtime assumptions -> explicit config + storage + adapters

## Live Codex CLI Facts Already Verified
Local Codex CLI supports:
- `codex exec --json`
- `codex exec resume`
- session resume by id / thread name

Observed JSONL shape includes:
- `thread.started`
- `turn.started`
- `item.completed`
- `turn.completed`

The new parser must tolerate non-JSON noise mixed into stdout/stderr.

## Telegram Product Direction
Bot-first architecture is the right target.
For the new app:
- official Telegram Bot API path is primary
- forum topics are first-class
- message routing uses topic/thread identifiers
- mobile readability is a hard product requirement, not an afterthought

## Rewrite Structure (Target)
We will build inside this repo a Rust workspace with dedicated crates for:
- app/bootstrap
- core domain
- Telegram adapter
- Codex runtime adapter
- persistence/store
- testkit

## Success Criteria For This Rewrite
This task is done only when:
1. architecture docs exist
2. Rust workspace exists inside the repo
3. tests exist for the new architecture
4. critical flows compile and pass
5. green checks run
6. repo is pushed and a fresh PR is ready

## Immediate Loop Objective
Current loop scope:
1. inspect current repo deeply
2. write `CONTEXT.MD`, `PLAN.MD`, `TDD.MD`
3. scaffold Rust workspace in this repo
4. define first parser/domain/store tests
5. start implementation toward green
