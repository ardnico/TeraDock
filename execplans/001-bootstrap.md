# Bootstrap workspace, ID rules, and minimal CLI

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Establish a compilable Rust workspace with the core crates (`core`, `cli`, `tui`, `common`) so `td --version` works cross-platform, and implement the baseline ID normalization/validation/generation rules defined in PROJECT_PLAN.md. This lays the foundation for subsequent features (profiles, secrets, doctor) by locking down naming rules and build scaffolding.

## Progress

- [x] (2025-01-05 00:00Z) Draft ExecPlan for workspace bootstrap and ID rules.
- [x] (2025-01-05 00:20Z) Workspace created (workspace Cargo.toml, crates/core, crates/cli, crates/tui, crates/common).
- [x] (2025-01-05 00:25Z) Minimal CLI wired (`td --version` path via clap help/version).
- [x] (2025-01-05 00:30Z) ID normalization/validation/generation in `crates/common` with unit tests.
- [ ] (2025-12-31 09:05Z) Attempted `cargo test`; blocked by crates.io 403 (CONNECT tunnel failure), so validation remains pending until registry access is available.
- [ ] (2026-01-05 16:03Z) Retried `cargo test`; crates.io access still blocked with CONNECT tunnel 403, validation deferred until registry is reachable.

## Surprises & Discoveries

- Network access to crates.io failed during `cargo test` (CONNECT tunnel 403), so dependencies could not be fetched. Need network allowance or vendored crates to proceed with build/test in this environment.
- Reattempt on 2025-12-31 confirms crates.io access is still blocked (CONNECT 403), leaving workspace validation pending.
- Reattempt on 2026-01-05 shows the same crates.io CONNECT 403 behavior; no artifacts downloaded yet.

## Decision Log

- Decision: Start with workspace scaffolding plus ID rules before deeper features (profiles, secrets).
  Rationale: ID rules are prerequisite for most persisted entities and safer to stabilize early; workspace is required to run anything.
  Date/Author: 2025-01-05 / assistant

## Outcomes & Retrospective

Workspace scaffolding and ID utilities are implemented with unit tests in-tree, but full validation is deferred until crates.io access is restored; `cargo test` cannot currently download dependencies.

## Context and Orientation

Current repository has only documentation; no Rust workspace exists. PROJECT_PLAN.md specifies a workspace split into `crates/core`, `crates/cli`, `crates/tui`, and optionally `crates/common`, with `td --version` working and SQLite migration scaffolding as part of Phase 0. ID rules (Phase 1) require lowercase normalization, regex `^[a-z0-9][a-z0-9_-]{2,63}$`, and reserved word rejection, plus base32 auto-generation with prefixes (`p_`, `s_`, etc.).

## Plan of Work

1) Create Cargo workspace at repository root listing member crates core/cli/tui/common; set edition 2021. Add a shared `rust-toolchain.toml` only if needed (avoid for now). Add a `.gitignore` entry for `target/` if absent.
2) Add crate skeletons:
   - `crates/common`: library exporting ID normalization/validation/generator utilities and an `IdError` enum. Include a reserved word list matching core commands (`list`, `add`, `rm`, `connect`, `exec`, `run`, `doctor`, `secret`, `config`, `push`, `pull`, `xfer`, `test`, `ui`). Provide unit tests for valid/invalid IDs and auto-generation length/prefix.
   - `crates/core`: library placeholder to compile; for now, just re-export the common ID utilities to ensure integration wiring is ready.
   - `crates/cli`: binary crate `td` using `clap` derive. Add a `--version` path (automatic via Clap) and a stub `run` function that currently only prints help when no subcommand is provided.
   - `crates/tui`: library placeholder compiling empty module with a TODO comment for future TUI setup.
3) Ensure `td --version` works: wire the CLI main to clap’s `CommandFactory`/`Parser` and no-op execution when no subcommand is passed. Keep dependencies minimal (`clap`, `once_cell`, `regex`, `rand`, `data-encoding`).
4) Tests: add unit tests in `crates/common` for `validate_id`, `normalize_id`, and `generate_id` rules (regex, reserved, lowercase, length and prefix).
5) Validation: run `cargo test` at workspace root to ensure compilation and tests pass.

## Concrete Steps

- Working directory: `/workspace/TeraDock`.
- Commands to run (expected later once code is in place):
  - `cargo test`
  - `cargo run -p td -- --version`

## Validation and Acceptance

- `cargo test` passes.
- `cargo run -p td -- --version` prints the binary name and version without panics.
- ID utility tests cover regex acceptance, reserved-word rejection, lowercase normalization, and generated ID prefix/length constraints.

## Idempotence and Recovery

- Workspace creation is additive; rerunning `cargo test` is safe. If dependency issues occur, run `cargo clean` and retry.

## Artifacts and Notes

- None yet.

## Interfaces and Dependencies

- Dependencies: `clap` (CLI parsing), `regex` (ID validation), `once_cell` (lazy static regex), `rand` + `data-encoding` (base32 ID generation). Expose public API:
  - `common::id::{normalize_id, validate_id, generate_id, IdError}`.

Update 2025-12-31 09:12Z: Documented the blocked `cargo test` attempt and captured the current outcome status until registry access is fixed.
