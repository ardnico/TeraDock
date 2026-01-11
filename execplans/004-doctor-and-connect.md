# Doctor checks and SSH connect wiring

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Advance PROJECT_PLAN.md Phase 4-6 by adding `td doctor` to detect external clients (ssh/scp/sftp/telnet) and introducing the first `connect` path (SSH) with danger-level confirmation and operation logging. This establishes the environmental guardrails and the initial value path for connecting to hosts.

## Progress

- [x] (2026-01-06 23:31Z) Added `td doctor` with PATH-based discovery for ssh/scp/sftp/telnet and optional JSON output.
- [x] (2026-01-06 23:31Z) Implemented SSH `connect` command with critical danger confirmation, last-used tracking, and op_logs insertion of outcome/duration.
- [x] (2026-01-07 00:10Z) Added telnet `connect` support via system client with logging/last-used updates.
- [x] (2026-01-08 14:46Z) Wired client override-aware client resolution for connect/exec and started logging `td doctor` runs into op_logs with discovery metadata.
- [x] (2026-01-08 15:26Z) Added CLI for global client overrides (`td config set-client/show-client/clear-client`) persisting to settings for doctor/exec/connect resolution.
- [x] (2026-01-09 15:32Z) Doctor output now reflects override sources (profile/global/path) and applies global overrides when reporting clients.
- [x] (2026-01-06 23:32Z) Run `cargo test` (still blocked by crates.io CONNECT 403 in this environment; retry when registry is reachable).
- [x] (2026-01-10 01:58Z) Added serial connect passthrough using serialport + raw terminal mode and wired `td connect` to it.
- [x] (2026-01-09 15:49Z) Ran `cargo test`; all workspace tests passed once registry access was available.
- [ ] (2026-01-10 01:58Z) Re-run `cargo test` after serial dependencies (blocked by crates.io CONNECT 403; retry when registry is reachable).
- [ ] (2026-01-10 15:56Z) Retried `cargo test`; crates.io CONNECT 403 persists with new dependencies, so validation remains blocked.

## Surprises & Discoveries

- PATH scanning sufficed for client discovery; no extra dependency (like `which`) was necessary.
- Telnet/serial connect handling remains to be designed; SSH landed first to keep scope controlled.
- Cargo registry access is still blocked (CONNECT 403), so test runs cannot be validated locally yet.
- Registry access was restored by 2026-01-09, allowing `cargo test` to pass successfully.
- Registry access appears blocked again after adding serial dependencies, so the latest `cargo test` run failed while fetching crates.
- Retried `cargo test` on 2026-01-10 and still hit CONNECT 403 fetching the crates.io index.

## Decision Log

- Decision: Implement client discovery manually by walking PATH and PATHEXT instead of adding a new crate dependency.
  Rationale: Keeps the CLI lean and avoids pulling new crates while registry access is restricted.
  Date/Author: 2026-01-06 / assistant
- Decision: Gate `connect` on `danger_level=critical` with a strict “type yes” prompt before spawning ssh.
  Rationale: Aligns with PROJECT_PLAN.md Phase 5 guardrails without waiting for a broader confirmation framework.
  Date/Author: 2026-01-06 / assistant
- Decision: Honor profile/global client overrides before PATH when resolving clients for connect/exec, and record doctor runs in op_logs for traceability.
  Rationale: Matches the client resolution order in PROJECT_PLAN.md and keeps discovery results auditable for later troubleshooting.
  Date/Author: 2026-01-08 / assistant
- Decision: Expose global client overrides via `td config set-client/show-client/clear-client`, storing them in settings for reuse by doctor/connect/exec.
  Rationale: Gives users a supported path to set client resolution order without editing the DB directly.
  Date/Author: 2026-01-08 / assistant

## Outcomes & Retrospective

Doctor and SSH/telnet/serial connect are now exposed in the CLI with logging and last-used updates, and client resolution honors overrides before PATH. Workspace validation passed previously, but the latest test run is blocked again by crates.io access while fetching new dependencies.

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
- `td connect <ssh_profile>` spawns ssh, confirms when danger=critical, updates last_used, and writes an op_logs row with duration/exit code; client resolution honors profile/global overrides before PATH.
- `td connect <telnet_profile>` spawns telnet <host> <port>, updates last_used, and logs outcome.
- `td config set-client --ssh /path --scp /path ...` persists overrides, `show-client` prints JSON, and `clear-client` removes them; doctor/connect/exec use these overrides.
- Tests pass when running `cargo test` at the workspace root.

## Idempotence and Recovery

Doctor is read-only. Connect updates `last_used_at` and logs; rerunning is safe. If ssh is missing, the CLI errors without side effects. If logging fails, the user-facing error surfaces while leaving the DB intact.

## Artifacts and Notes

- None yet; add command output snippets later if behavior changes or for troubleshooting.

## Interfaces and Dependencies

- New modules: `tdcore::doctor`, `tdcore::oplog`.
- CLI: `td doctor [--json]`, `td connect <profile_id>` (SSH only; telnet/serial pending).
- Dependencies: reuses std PATH scanning; no new external crates added.

Update 2026-01-09 15:49Z: Recorded successful `cargo test` execution now that registry access is available and updated progress/outcomes.
Update 2026-01-10 01:58Z: Marked serial connect as implemented and noted the renewed crates.io access block after adding serial dependencies.
