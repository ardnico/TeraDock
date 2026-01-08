# Doctor checks and SSH connect wiring

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Advance PROJECT_PLAN.md Phase 4-6 by adding `td doctor` to detect external clients (ssh/scp/sftp/telnet) and introducing the first `connect` path (SSH) with danger-level confirmation and operation logging. This establishes the environmental guardrails and the initial value path for connecting to hosts.

## Progress

- [x] (2026-01-06 23:31Z) Added `td doctor` with PATH-based discovery for ssh/scp/sftp/telnet and optional JSON output.
- [x] (2026-01-06 23:31Z) Implemented SSH `connect` command with critical danger confirmation, last-used tracking, and op_logs insertion of outcome/duration.
- [x] (2026-01-07 00:10Z) Added telnet `connect` support via system client with logging/last-used updates.
- [ ] (2026-01-06 23:32Z) Run `cargo test` (still blocked by crates.io CONNECT 403 in this environment; retry when registry is reachable).
- [ ] (2026-01-06 23:32Z) Extend connect support to serial once client selection and passthrough handling are designed.

## Surprises & Discoveries

- PATH scanning sufficed for client discovery; no extra dependency (like `which`) was necessary.
- Telnet/serial connect handling remains to be designed; SSH landed first to keep scope controlled.
- Cargo registry access is still blocked (CONNECT 403), so test runs cannot be validated locally yet.

## Decision Log

- Decision: Implement client discovery manually by walking PATH and PATHEXT instead of adding a new crate dependency.
  Rationale: Keeps the CLI lean and avoids pulling new crates while registry access is restricted.
  Date/Author: 2026-01-06 / assistant
- Decision: Gate `connect` on `danger_level=critical` with a strict “type yes” prompt before spawning ssh.
  Rationale: Aligns with PROJECT_PLAN.md Phase 5 guardrails without waiting for a broader confirmation framework.
  Date/Author: 2026-01-06 / assistant

## Outcomes & Retrospective

Doctor and SSH/telnet connect are now exposed in the CLI with logging and last-used updates; serial remains unimplemented. Validation is pending until crates.io becomes reachable for `cargo test`.

## Context and Orientation

Earlier phases delivered workspace scaffolding, ID rules, persistence, profiles, and secrets. PROJECT_PLAN.md Phase 4 calls for `doctor` to surface missing external clients; Phase 5/6 require guarded connect support starting with SSH. No telnet/serial handling existed before this plan.

## Plan of Work

1) Build a reusable PATH scanner in `tdcore::doctor` returning discovered client paths.
2) Wire `td doctor` CLI command with human-readable and JSON outputs.
3) Add `td connect <profile_id>` for SSH profiles: resolve ssh client, spawn inheriting stdio, confirm critical danger, update `last_used`, and log to `op_logs`.
4) Add telnet connect via system client (serial remains TODO).
5) Document remaining gaps (serial connect, tests blocked) and rerun tests when possible.

## Concrete Steps

- Add `tdcore::doctor::{check_clients, resolve_client}` with PATHEXT-aware PATH scanning.
- Expose `td doctor [--json]` in the CLI.
- Implement SSH connect: pick ssh from PATH, spawn `ssh user@host -p port`, log results to `op_logs`, and update `last_used_at`.
- Keep telnet/serial connect TODOs explicit for future work.
- Attempt `cargo test` once network access is available; record outcomes.

## Validation and Acceptance

- `td doctor` lists ssh/scp/sftp/telnet with paths or “MISSING”, and `--json` returns a structured report.
- `td connect <ssh_profile>` spawns ssh, confirms when danger=critical, updates last_used, and writes an op_logs row with duration/exit code.
- `td connect <telnet_profile>` spawns telnet <host> <port>, updates last_used, and logs outcome.
- Tests pass once registry access permits running them.

## Idempotence and Recovery

Doctor is read-only. Connect updates `last_used_at` and logs; rerunning is safe. If ssh is missing, the CLI errors without side effects. If logging fails, the user-facing error surfaces while leaving the DB intact.

## Artifacts and Notes

- None yet; add command output snippets later if behavior changes or for troubleshooting.

## Interfaces and Dependencies

- New modules: `tdcore::doctor`, `tdcore::oplog`.
- CLI: `td doctor [--json]`, `td connect <profile_id>` (SSH only; telnet/serial pending).
- Dependencies: reuses std PATH scanning; no new external crates added.
