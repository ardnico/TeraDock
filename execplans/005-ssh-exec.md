# SSH exec command with timeout and JSON output

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Deliver PROJECT_PLAN.md Phase 7’s first slice: a non-interactive `exec` command for SSH profiles that honors danger-level confirmation, resolves the SSH client from PATH, supports a timeout, captures stdout/stderr, and emits the fixed JSON schema for tool integration.

## Progress

- [x] (2026-01-06 23:50Z) Added CLI `td exec` for SSH profiles with optional `--timeout-ms` and `--json` output.
- [x] (2026-01-06 23:50Z) Wired op_logs insertion, last-used tracking, and critical danger confirmation for exec.
- [x] (2026-01-08 14:46Z) Fixed SSH invocation to avoid sending a stray `--` as the remote command and aligned client resolution with profile/global overrides.
- [x] (2026-01-06 23:51Z) Run `cargo test` (blocked by crates.io access / read-only sandbox; retry when registry reachable).
- [x] (2026-01-11 16:17Z) Extend parsing/structured `parsed` field with parser specs for `td exec --json` now that CommandSet/parser support exists.
- [x] (2026-01-11 03:39Z) Added basic JSON stdout parsing for `td exec --json`, populating `parsed` when stdout is valid JSON.
- [x] (2026-01-09 15:49Z) Ran `cargo test`; all workspace tests passed once registry access was available.

## Surprises & Discoveries

- Timeout support required an extra crate (`wait-timeout`) since std lacks a portable timeout on child processes.
- Sandbox remains read-only with restricted network; no test execution possible yet.
- SSH treats `--` after the host as part of the remote command, so passing it by default caused an unexpected `--` command on the remote side; removed to keep commands intact.
- Registry access was restored by 2026-01-09, allowing `cargo test` to pass successfully.

## Decision Log

- Decision: Use PATH-based SSH client resolution (shared with doctor) rather than embedding an SSH implementation.
  Rationale: Aligns with project plan to rely on system clients and keeps scope lean.
  Date/Author: 2026-01-06 / assistant
- Decision: Keep SSH exec invocation free of an extra `--` delimiter and honor profile/global client overrides before falling back to PATH.
  Rationale: Prevents an unintended `--` remote command and matches the client resolution order defined in PROJECT_PLAN.md.
  Date/Author: 2026-01-08 / assistant
- Decision: Populate `parsed` with stdout JSON when available for `td exec --json`, leaving it empty otherwise.
  Rationale: Provides immediate structured output for JSON-producing commands without waiting on full CommandSet/parser plumbing.
  Date/Author: 2026-01-11 / assistant
- Decision: Add optional parser specs (`raw/json/regex:<id>`) to `td exec --json`.
  Rationale: Aligns exec parsing with CommandSet parser support and closes the remaining parsing integration item in this plan.
  Date/Author: 2026-01-11 / assistant

## Outcomes & Retrospective

`td exec` is now available for SSH profiles with JSON output, logging, and parser-spec support. Execution semantics can be tightened further (e.g., richer metadata) if needed.

## Context and Orientation

Connect and doctor are already present. This plan adds the first non-interactive execution path, preceding full CommandSet/run orchestration in Phase 7.

## Plan of Work

1) Add timeout-capable process runner using `wait-timeout`.
2) Wire `td exec <profile_id> -- <cmd...>` with JSON output, danger confirmation, and op_log entry.
3) Defer parser/CommandSet integration to a future plan.

## Concrete Steps

- Add `wait-timeout` dependency.
- Resolve SSH client via `doctor::resolve_client`, spawn with `--` and captured stdio, and enforce optional timeout.
- Serialize result to the fixed JSON schema (`ok`, `exit_code`, `stdout`, `stderr`, `duration_ms`, `parsed`).
- Update op_logs and last_used_at for the profile.

## Validation and Acceptance

- `td exec --json <ssh_profile> -- echo hi` outputs the expected JSON schema.
- `td exec --json --parser regex:<id> <ssh_profile> -- <cmd>` applies the referenced parser definition.
- Critical profiles prompt for confirmation before executing.
- op_logs receives an `exec` entry with exit code/duration.
- Tests pass when running `cargo test` at the workspace root.

## Idempotence and Recovery

Exec is stateless aside from logging and last_used updates; reruns are safe. On timeout, the process is killed and a clear error returned.

## Artifacts and Notes

- None yet; capture sample outputs once tests can run.

## Interfaces and Dependencies

- CLI: `td exec <profile_id> [--timeout-ms N] [--json] [--parser <spec>] -- <cmd...>`
- Core helpers reused: `doctor::resolve_client`, `profile::touch_last_used`, `oplog::log_operation`.
- New dependency: `wait-timeout` for child process timeout handling.

Update 2026-01-09 15:49Z: Recorded successful workspace tests now that registry access is available and updated progress/validation notes.
Update 2026-01-11 16:17Z: Added parser-spec support for `td exec --json` and marked the parsing integration item complete.
