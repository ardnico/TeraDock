# SQLite persistence, app directories, and profile CRUD wiring

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Enable TeraDock to persist data in SQLite at the expected config directory on Windows/Linux, seed the schema described in PROJECT_PLAN.md, and expose the first meaningful user-facing commands: creating, listing, showing, and deleting profiles (without secrets). After this work a user should be able to run `td profile add ...`, see the profile stored in `teradock.db`, and list or remove it later. This validates the storage layer and the CLI wiring that future features will build upon.

## Progress

- [x] (2025-12-28 06:10Z) Draft ExecPlan for persistence scaffolding and profile CRUD.
- [x] (2025-12-28 06:35Z) Establish config/log/database path utilities with platform-specific resolution and directory creation.
- [x] (2025-12-28 06:55Z) Introduce SQLite connection management with migrations that create the initial schema (settings, profiles, secrets, command/config tables, op_logs).
- [x] (2025-12-28 07:20Z) Implement profile domain model, validation (ID normalization, reserved-word rejection), and CRUD use cases storing tags/client overrides as JSON.
- [x] (2025-12-28 07:40Z) Wire CLI subcommands `td profile add/list/show/rm` to the core use cases; ensure help text is discoverable.
- [x] (2025-12-28 07:55Z) Add unit tests for ID enforcement in profile creation and happy-path add/list/show/remove against an in-memory SQLite database.
- [ ] (2025-12-28 08:05Z) Run `cargo test` and document results (blocked by crates.io access: CONNECT tunnel 403).
- [ ] (2025-12-31 09:05Z) Retried `cargo test`; crates.io still unreachable (CONNECT 403), so validation remains pending until registry access is restored.
- [ ] (2026-01-05 16:03Z) Retried `cargo test`; crates.io CONNECT 403 persists, so build/test validation remains blocked.
- [x] (2026-01-08 15:26Z) Added profile edit support (core update API + CLI `td profile edit`) including clear flags for group/note/client overrides and tag replacement.

## Surprises & Discoveries

- Cargo registry access is blocked in this environment (CONNECT tunnel 403 to crates.io), preventing `cargo test` from downloading dependencies as of 2025-12-28.
- 2025-12-31 retry shows the same crates.io 403 behavior, so tests cannot yet be executed in this environment.
- 2026-01-05 retry continues to fail with CONNECT 403 when downloading crates.io index/config, leaving dependencies unfetched.

## Decision Log

- Decision: Use a dedicated `profile` subcommand namespace (`td profile add/list/show/rm`) rather than top-level verbs to keep room for other entities like secrets/configs without argument collisions.
  Rationale: Clap’s parser stays unambiguous and future expansion remains coherent with PROJECT_PLAN.md phases.
  Date/Author: 2025-12-28 / assistant
- Decision: Implement profile edits as full-row updates via `td profile edit` rather than piecemeal setters, with explicit clear flags for optional fields.
  Rationale: Keeps the interface compact while still allowing removal of optional metadata without extra commands.
  Date/Author: 2026-01-08 / assistant

## Outcomes & Retrospective

Profile CRUD and persistence scaffolding are implemented and covered by in-memory tests, but full workspace test runs remain blocked by crates.io access. Next step is to rerun `cargo test` once the registry becomes reachable.

## Context and Orientation

The repository already has a Rust workspace with crates `common`, `core`, `cli`, and `tui`. `common` implements ID normalization/validation/generation per PROJECT_PLAN.md Phase 1. `cli` currently only prints help when no subcommand is supplied. There is no persistence layer, config directory handling, or domain model for profiles yet. PROJECT_PLAN.md expects SQLite stored at `%APPDATA%/TeraDock/teradock.db` on Windows and `~/.config/teradock/teradock.db` on Linux, plus a schema covering settings, profiles, secrets, command sets, config sets, and operation logs.

## Plan of Work

Begin by adding path utilities in `crates/core` to resolve the config directory, logs directory, and database path, creating directories if missing. Then add a SQLite module that opens a connection at the resolved database path, enables foreign keys, and runs migrations. Migrations should set `PRAGMA user_version` and create the schema tables outlined in PROJECT_PLAN.md (settings, profiles with ssh/telnet/serial metadata, ssh_forwards, ssh_jump, secrets, cmdsets/cmdsteps/parsers, configsets/configfiles, op_logs). Next, define a `Profile` domain model with enums for protocol type and danger level, JSON-encoded tags and client overrides, and helper structs for creation input. Implement CRUD functions that normalize and validate `profile_id`, auto-generate an ID with prefix `p_` when omitted, and persist timestamps. Expose these functions from `core` for reuse by the CLI.

Update the CLI to introduce a `profile` subcommand group with `add`, `list`, `show`, and `rm`. The `add` command should accept host/user/port/type/danger/group/tags/note and optional `--profile-id`; when omitted, auto-generate. `list` should present tabular output; `show` should print full JSON for now; `rm` should delete by ID. Initialize tracing/logging early in `main` using the config/logs directory (stdout + file) to match PROJECT_PLAN.md logging direction. Keep UX minimal but helpful.

Finally, add unit tests in `crates/core` that use an in-memory SQLite connection to verify ID validation on insert and round-trip CRUD behavior, plus basic CLI parsing smoke tests if time allows. Run `cargo test` from the workspace root to validate.

## Concrete Steps

- Working directory: `/workspace/TeraDock`.
- Update workspace dependencies to include `anyhow`, `thiserror`, `rusqlite`, `directories`, `serde`, `serde_json`, `time` (or `chrono`), and `tracing`/`tracing-subscriber`/`tracing-appender`.
- Implement `core::paths` with config/log/db path helpers and ensure directories exist.
- Implement `core::db` with `Connection` initialization, foreign key enablement, and migration application to schema version 1.
- Define `core::profile` domain types and CRUD functions, storing tags/client overrides as JSON strings and timestamps as UTC integers.
- Extend `cli` with `profile` subcommands delegating to `core` functions; add logging initialization in `main`.
- Write unit tests for profile CRUD and ID validation using in-memory DB.
- Run `cargo test` at workspace root; note outcomes.

## Validation and Acceptance

Acceptance is met when `cargo test` passes and manual CLI checks show persistence works:

- Run `cargo run -p cli -- profile add --name test --host example.com --user alice --type ssh` and see a generated `profile_id` printed.
- Run `cargo run -p cli -- profile list` and observe the inserted profile.
- Run `cargo run -p cli -- profile show <id>` and see JSON details without secrets.
- Run `cargo run -p cli -- profile rm <id>` and confirm it disappears from `list`.

## Idempotence and Recovery

Path creation is idempotent. Migrations use `PRAGMA user_version` to avoid re-creating tables. CLI commands are safe to re-run; duplicate IDs are rejected by validation before insertion. Edits replace the full record; optional fields can be cleared with dedicated flags. Tests use in-memory DBs to avoid altering user data. If database initialization fails, delete the `teradock.db` file and rerun `td` to reapply migrations.

## Artifacts and Notes

Capture notable command outputs (add/list/show) in future updates if behavior changes or for troubleshooting reference.

## Interfaces and Dependencies

- New modules: `crates/core/src/paths.rs`, `crates/core/src/db.rs`, `crates/core/src/profile.rs`.
- Public API (core): `paths::{config_dir, logs_dir, database_path}`, `db::{init_connection}`, `profile::{ProfileType, DangerLevel, Profile, NewProfile, ProfileStore}` with CRUD methods `insert`, `get`, `list`, `delete`.
- Dependencies: `rusqlite` for SQLite, `directories` for platform paths, `serde/serde_json` for JSON columns, `thiserror` for error enums, `anyhow` for CLI error bubbling, `tracing`/`tracing-subscriber`/`tracing-appender` for logging, `time` or `chrono` for UTC timestamps.

Update 2025-12-31 09:13Z: Logged the repeated crates.io access failure and captured the current pending validation status.
