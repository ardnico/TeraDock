# Tera Term launcher MVP across CLI and GUI

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this file in accordance with `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Deliver a Windows-focused Tera Term launcher that wraps Tera Term command-line invocation behind reusable connection profiles. Both GUI and CLI should let users pick a profile, confirm risky connections, and either run Tera Term or show the command for dry runs. A novice should be able to install the app, pick a profile, and connect with guardrails against accidental production access.

## Progress

- [x] (2024-10-30 00:00Z) Drafted ExecPlan outlining goals, context, and steps.
- [x] (2025-05-08 00:30Z) Added workspace scaffolding, default profiles, installer stub, and crate manifests.
- [x] (2025-05-08 01:10Z) Implemented core models/config/command builder/history with unit tests.
- [x] (2025-05-08 01:40Z) Implemented CLI list/connect/history commands with danger confirmation and history writes.
- [x] (2025-05-08 02:10Z) Implemented GUI launcher with search, detail pane, danger confirmation delay, settings editor, and history tab.
- [x] (2025-05-08 03:30Z) Added pinned profile support, recency-aware sorting, and persisted pin toggles in GUI and CLI listings.
- [ ] (2025-05-08 04:10Z; retried 2025-05-08 05:40Z) Validation (cargo fmt completed; cargo clippy -- -D warnings and cargo test blocked by crates.io 403 index fetches despite multiple attempts).
- [x] (2025-05-08 05:45Z) Outcomes & retrospective updated after implementation.

## Surprises & Discoveries

- Observation: Cargo could not fetch crates.io index due to 403 errors in this environment, blocking clippy/test runs.
  Evidence: `cargo clippy -- -D warnings` and `cargo test` failed while downloading `https://index.crates.io/config.json` with HTTP 403.

## Decision Log

- Decision: Profiles will live in a TOML file referenced from a small config (tera term path, profiles path, history path) to keep both CLI and GUI in sync without a heavier settings store.
  Rationale: Keeps user-editable profiles versionable and avoids divergent defaults between frontends.
  Date/Author: 2024-10-30 / Assistant
- Decision: Default configuration searches for a local `config/` directory before falling back to platform config dirs and writes sample profiles if missing.
  Rationale: Ensures the app works out-of-the-box for developers and packaged installs while keeping profiles editable.
  Date/Author: 2025-05-08 / Assistant

## Outcomes & Retrospective

Implemented the planned MVP components: shared core library, CLI, GUI, default profiles, and installer stub. CLI and GUI share profile loading, command generation, and history logging with confirmation flows for dangerous targets. Validation is still partially blocked by crates.io index 403 errors preventing clippy/test downloads even after repeated attempts; formatting succeeded, and behavior is ready for full verification once the network restriction is lifted.

Validated formatting locally; repeated clippy and test runs remain blocked by crates.io index fetch 403 responses. Manual flows (profile listing, danger gating, history logging) are implemented according to the plan and remain pending full automated verification once dependency downloads are allowed again.

## Context and Orientation

The repository is currently empty aside from PROJECT_PLAN.md. We will build a Rust workspace with three crates:
- `crates/core`: library crate for profile models, configuration loading, command generation for ttermpro.exe, danger assessment, and history persistence.
- `crates/cli`: binary crate exposing `ttlaunch` CLI with list, connect, and history commands reusing core logic.
- `crates/gui`: binary crate using egui/eframe to present a launcher UI with search/filter, profile details, connect actions, danger confirmation, settings form, and history tab.
Supporting files: `Cargo.toml` workspace, `.gitignore`, `config/default_profiles.toml`, and `installer/setup.iss` placeholder plus docs if needed.

## Plan of Work

1. Create workspace scaffolding with Cargo.toml workspace, .gitignore, config and dist/installer directories. Add shared dependency versions in workspace.
2. Implement `crates/core`:
   - Define profile/domain models (Protocol, DangerLevel, Profile, ProfileSet) with serde support and validation helpers.
   - Add configuration struct storing Tera Term path, profile path, and history path with load/save defaults and environment overrides.
   - Implement command builder that produces executable path and arguments for a profile, handling host/port, protocol switches, user, macro, title prefix for critical connections, and extra arguments.
   - Implement danger checks and confirmation message generator.
   - Implement history persistence as JSON Lines (append and load with limit) plus helper to record events.
   - Add unit tests for TOML loading and command generation edge cases using sample fixture.
3. Implement `crates/cli` using clap:
   - Commands: `list [--json]`, `connect <profile-id> [--force] [--dry-run]`, `history [--limit N]`.
   - For dangerous profiles, require `--force` or interactive confirmation; dry-run prints command without executing.
   - Use core config paths, render history in table/JSON, and write history entries including forced flag and success/failure.
4. Implement `crates/gui` with eframe/egui:
   - Load profiles and settings at startup; allow editing Tera Term path with existence validation.
   - Left pane search/filter by name/group/tags; right pane shows details, danger badge, and connect button.
   - Dangerous connections trigger modal confirmation with 3-second delay before enabling "Run"; connect uses core command builder and logs history; show errors inline.
   - Add tabs for Profiles, History, and Settings; history tab reads entries sorted newest-first with color-coded success.
5. Add default profile TOML under config with varied danger levels and macro example; provide placeholder installer script and documentation updates if needed.
6. Run fmt, clippy, and tests; update ExecPlan sections (progress, discoveries, decisions, outcomes) to reflect the work.

## Concrete Steps

- From repo root, create workspace scaffolding: Cargo.toml, crates/core, crates/cli, crates/gui, config/default_profiles.toml, installer/setup.iss, dist/.gitkeep.
- Implement core library files under crates/core/src with modules for config, profile, command, history, error. Add tests in crates/core/tests.
- Implement CLI binary at crates/cli/src/main.rs using clap derive.
- Implement GUI binary at crates/gui/src/main.rs using eframe::run_native.
- Run `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` from repo root.

## Validation and Acceptance

- `cargo fmt` shows no diff.
- `cargo clippy -- -D warnings` passes in this environment.
- `cargo test` passes.
- Manual validation notes: running `cargo run -p cli -- list` prints profiles; `cargo run -p cli -- connect <id> --dry-run` prints ttermpro command; GUI launches and shows profiles with connect button and danger confirmation dialog.

## Idempotence and Recovery

The plan is additive. Workspace creation can be re-run safely; config files are deterministic. History appends are additive; deleting history file resets it. If command execution fails due to missing Tera Term, the CLI/GUI report an error without crashing. Re-running tests is safe.

## Artifacts and Notes

Add short transcripts in this section when capturing key outputs during execution.

Revision note (2025-05-08): Recorded validation attempt status and reiterated pending clippy/test runs due to crates.io 403 responses.
Revision note (2025-05-08 05:45Z): Retried clippy/test; still blocked by crates.io index 403. Updated progress and outcomes to capture the repeated failure and current readiness.
Revision note (2025-05-09): Another cargo check/clippy/test attempt failed immediately on crates.io index 403. No code regressions found locally; awaiting network access to validate fully.

## Interfaces and Dependencies

- Rust workspace with edition 2021.
- Dependencies: serde, toml, thiserror, anyhow, clap (CLI), egui/eframe (GUI), directories for config paths, chrono or time for timestamps, tracing/tracing-subscriber for logging.
- Core interfaces:
  - `core::config::AppPaths { tera_term_path: PathBuf, profiles_path: PathBuf, history_path: PathBuf }`
  - `core::profile::{Profile, DangerLevel, Protocol, ProfileSet}` with serde support.
  - `core::command::build_command(profile: &Profile, paths: &AppPaths) -> CommandSpec` where `CommandSpec { program: PathBuf, args: Vec<OsString>, window_title: Option<String> }`.
  - `core::history::{HistoryEntry, HistoryStore::append(&self, entry: &HistoryEntry) -> Result<()>; load(&self, limit: Option<usize>) -> Result<Vec<HistoryEntry>> }`.
