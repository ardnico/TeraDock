# SSH run commandset execution with parsing

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Users can execute a stored CommandSet against an SSH profile using `td run <profile_id> <cmdset_id>`. Each step runs in order, respects per-step timeouts, stops on failures unless configured to continue, and emits the fixed JSON schema when `--json` is requested. This makes multi-step automation possible without manual SSH scripting, and it is visible by running `td run` and observing per-step output or structured JSON.

## Progress

- [x] (2026-01-11 05:58Z) Drafted and implemented CommandSet storage types, parser handling, and CLI `td run` execution path.
- [x] (2026-01-11 06:51Z) Added unit tests for CommandSet loading and parser behavior.
- [ ] (2026-01-12 08:54Z) Retried `cargo test`; crates.io CONNECT 403 persists (failed to download config.json).

## Surprises & Discoveries

- Retried `cargo test` on 2026-01-11 and still hit CONNECT 403 while downloading the crates.io index (data-encoding).
- Retried `cargo test` on 2026-01-11 and still hit CONNECT 403 while downloading config.json from crates.io.

## Decision Log

- Decision: Represent parser output as a JSON object with `steps` inside the top-level `parsed` field for `td run --json`.
  Rationale: The fixed schema demands a single `parsed` field; nesting step details preserves per-step data without breaking the top-level contract.
  Date/Author: 2026-01-11 / assistant

## Outcomes & Retrospective

CommandSet execution via `td run` is now implemented for SSH profiles with per-step parsing and timeout handling. Validation remains blocked by registry access for tests.

## Context and Orientation

CommandSet data already exists in the SQLite schema (`cmdsets`, `cmdsteps`, `parsers`) defined in `crates/core/src/db.rs`, but there is no code to load those records or execute them. The CLI in `crates/cli/src/main.rs` currently exposes `td exec` for one-off commands. This plan adds core types for CommandSets and parsers in `crates/core/src/cmdset.rs` and `crates/core/src/parser.rs`, then wires a new `td run` command that reads CommandSets, executes each step over SSH, and logs the run in `op_logs`.

## Plan of Work

First, add core models and store helpers that load CommandSets, steps, and parser definitions from SQLite. Next, implement parser handling for `raw`, `json`, and `regex:<parser_id>` specs, where regex definitions are stored in the `parsers` table. Then, add a new CLI `td run` command that resolves the SSH client, runs steps in order with per-step timeouts, applies parsers, respects `on_error` behavior, and outputs either streamed stdout/stderr or a JSON summary that contains per-step data within `parsed.steps`. Finally, log the run in `op_logs` and update the profile’s `last_used_at`.

## Concrete Steps

- Working directory: `/workspace/TeraDock`.
- Add `crates/core/src/cmdset.rs` with `CmdSet`, `CmdStep`, `CmdSetStore`, and `StepOnError` parsing.
- Add `crates/core/src/parser.rs` with `ParserSpec`, `ParserDefinition`, and `parse_output` using `regex`.
- Update `crates/core/src/lib.rs` and `crates/core/Cargo.toml` to expose and depend on the new modules.
- Add a new CLI `Run` command in `crates/cli/src/main.rs` that executes CommandSets over SSH and emits JSON when `--json` is specified.
- Run `cargo test` from the repository root and record the outcome.

## Validation and Acceptance

- `td run <profile_id> <cmdset_id> --json` returns an object containing `ok`, `exit_code`, `stdout`, `stderr`, `duration_ms`, and `parsed.steps` with per-step results.
- `td run <profile_id> <cmdset_id>` streams command output and returns an error when a step fails with `on_error=stop`.
- `op_logs` has a `run` entry with duration and exit code, and the profile’s `last_used_at` is updated.
- `cargo test` passes once registry access is available.

## Idempotence and Recovery

Reading CommandSets is read-only; repeated runs are safe and only update `last_used_at` plus append a `run` row in `op_logs`. If a run fails partway, rerun after correcting the CommandSet or target host.

## Artifacts and Notes

- None yet.

## Interfaces and Dependencies

- New modules:
  - `crates/core/src/cmdset.rs` with `CmdSetStore::get`, `CmdSetStore::list_steps`, and `CmdSetStore::get_parser`.
  - `crates/core/src/parser.rs` with `ParserSpec::parse` and `parse_output`.
- CLI additions:
  - `td run <profile_id> <cmdset_id> [--json]`.
- Dependencies:
  - `regex` added to `crates/core`.

Update 2026-01-11 06:00Z: Retried `cargo test` and recorded the ongoing crates.io CONNECT 403 failure in Progress and Surprises.
Update 2026-01-11 06:51Z: Added CommandSet/parser unit tests to the plan Progress.
Update 2026-01-11 17:09Z: Retried `cargo test`; registry access remains blocked (CONNECT 403).
