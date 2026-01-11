# Add td test connectivity checks

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Introduce `td test <profile_id>` so operators can quickly validate DNS resolution, TCP reachability, and (optionally) SSH BatchMode authentication without opening an interactive session. This gives an immediate, scriptable answer to “is this host reachable, and can my SSH credentials work?” with both text and JSON output, and it records the results in `op_logs` for later audit.

## Progress

- [x] (2026-01-12 12:15Z) Reviewed existing CLI/profile/doctor/oplog wiring to align test reporting and logging expectations.
- [x] (2026-01-12 12:34Z) Implemented core tester module for DNS/TCP/optional SSH BatchMode probes and structured report output.
- [x] (2026-01-12 12:40Z) Wired CLI `td test` command with JSON/text output and op_logs logging.
- [x] (2026-01-12 12:44Z) Updated CLI parsing tests for the new command and documented outcomes.
- [x] (2026-01-11 16:55Z) Restored CLI handler wiring for `td test` and op_logs reporting after detecting a missing dispatch path.
- [ ] (2026-01-12 12:50Z) Validate behavior against a reachable host (blocked: no known reachable host in this environment).

## Surprises & Discoveries

- Observation: No obvious reachable SSH host is configured in this sandbox, so manual connectivity validation is blocked.
  Evidence: Not attempted; would require a known reachable profile in the local database.
- Observation: The CLI dispatch for `td test` was missing despite the handler implementation, requiring a wiring fix.
  Evidence: `main` did not route `Commands::Test` before the fix.

## Decision Log

- Decision: Default `td test` to DNS + TCP checks, with SSH BatchMode explicitly gated behind a flag.
  Rationale: DNS/TCP are safe, quick diagnostics; SSH auth probing should be opt-in because it depends on SSH client availability and auth configuration.
  Date/Author: 2026-01-12 / assistant
- Decision: Record the full test report JSON in `op_logs.meta_json` and update `last_used_at` for the tested profile.
  Rationale: Aligns with existing operational logging and keeps profile recency consistent with other command handlers.
  Date/Author: 2026-01-12 / assistant

## Outcomes & Retrospective

The `td test` implementation is complete with DNS/TCP probes, optional SSH BatchMode checking, JSON/text output, and op_logs recording. Manual connectivity validation is still pending due to the lack of a known reachable host in this environment, but the CLI dispatch is now correctly wired.

## Context and Orientation

The CLI entry point is `crates/cli/src/main.rs`, which already hosts `td doctor`, `td exec`, and `td run` handlers along with op_logs insertion via `tdcore::oplog`. Profile data lives in `crates/core/src/profile.rs`, and client resolution is centralized in `tdcore::doctor`. Logging writes to the `op_logs` table defined in `crates/core/src/db.rs`. The new connectivity logic will live in `crates/core/src/tester.rs` so the CLI can focus on wiring, output formatting, and logging.

## Plan of Work

Implement a new `tdcore::tester` module that exposes a `run_profile_test` helper returning a structured report. The report should include profile identifiers, host/port, a list of checks (DNS, TCP, optional SSH BatchMode), per-check status, durations, and an overall `ok` boolean. DNS should resolve `host:port` to socket addresses. TCP should attempt a connect with a short timeout and report the selected address. The SSH BatchMode probe should run `ssh -o BatchMode=yes` with a short ConnectTimeout and a simple `exit 0` command when the CLI flag is set.

In the CLI, add a `test` subcommand that accepts `profile_id`, `--json`, and `--ssh` (to enable BatchMode). Fetch the profile, guard against unsupported profile types (serial), resolve the SSH client when `--ssh` is used, build the tester options, execute the test, log to `op_logs` with `op=test`, and emit either JSON (the report) or a human-friendly summary aligned with doctor-style output.

## Concrete Steps

- Add `crates/core/src/tester.rs` with:
  - `TestCheck` and `TestReport` structs (serde-serializable).
  - `SshBatchCommand` and `TestOptions` inputs to control the optional SSH probe and timeouts.
  - `run_profile_test(&Profile, &TestOptions) -> TestReport` that executes DNS, TCP, and optional SSH checks in order.
- Export the module in `crates/core/src/lib.rs`.
- Update `crates/cli/src/main.rs`:
  - Add `Commands::Test` to CLI args.
  - Implement `handle_test` to build options, call `tdcore::tester::run_profile_test`, log to `op_logs`, and print JSON/text output.
  - Add a parsing test for the new command in the CLI tests section.
- Run a manual test against a reachable host if possible (e.g., an existing SSH profile) and record the outcome or limitations.

## Validation and Acceptance

- `td test <profile_id>` prints DNS and TCP results; `--json` emits a JSON report with `ok`, `profile_id`, `profile_type`, `host`, `port`, `duration_ms`, and a `checks` array of objects containing `name`, `ok`, `skipped`, `duration_ms`, `detail`, and optional `exit_code` and `data`.
- `td test <profile_id> --ssh` runs the BatchMode probe for SSH profiles, reports its status, and includes the SSH check in JSON.
- Each invocation writes an `op_logs` row with `op=test`, `ok` derived from the report, `duration_ms` populated, `exit_code` set when the SSH probe runs, and `meta_json` containing the serialized report.

## Idempotence and Recovery

The test command is read-only. Re-running it is safe; failures produce structured output and do not mutate profile state beyond `last_used_at` updates and op_logs entries.

## Artifacts and Notes

- Capture any manual test output snippets here once executed.

## Interfaces and Dependencies

- New module: `tdcore::tester` with `run_profile_test`, `TestOptions`, `TestReport`, `TestCheck`, and `SshBatchCommand`.
- CLI additions: `td test <profile_id> [--json] [--ssh]` in `crates/cli/src/main.rs`.
- Dependencies: reuse `std::net` for DNS/TCP, `std::process::Command` for SSH BatchMode; no new crates.

Update 2026-01-12 12:50Z: Marked implementation tasks complete, logged the decision to store reports in op_logs and update last_used_at, and noted the remaining manual validation gap.
