# Guard FTP transfers behind settings + runtime flag

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

The file `.agent/PLANS.md` in the repository root defines the ExecPlan format and maintenance rules; this plan must follow it.

## Purpose / Big Picture

Users should be able to run transfer commands with `--via ftp` only when they have explicitly enabled insecure transfers in settings and they explicitly confirm intent at runtime. If either approval is missing, FTP should be rejected with a clear error. When FTP is used, the operation log should clearly mark the transfer as insecure so audits can find it, and the CLI should warn about insecure usage. This change makes FTP support safe-by-default while still allowing deliberate use.

## Progress

- [x] (2025-02-14 21:55Z) Created initial ExecPlan for FTP guardrails and transfer refactor.
- [x] (2025-02-14 22:20Z) Added allow_insecure_transfers setting plumbing and config set CLI support.
- [x] (2025-02-14 22:30Z) Moved transfer execution into a dedicated CLI transfer module with FTP gating and logging markers.
- [x] (2025-02-14 22:35Z) Extended transfer enums, client overrides, and doctor checks to include FTP.
- [x] (2025-02-14 22:40Z) Updated CLI parsing tests for config set and insecure flag defaults.

## Surprises & Discoveries

- None observed.

## Decision Log

- Decision: Introduce a dedicated CLI transfer module to centralize transfer execution and FTP guard checks.
  Rationale: The user requirement calls out a new transfer module and the guardrails are best enforced in one place shared by push/pull/xfer/config apply.
  Date/Author: 2025-02-14 / assistant
- Decision: Use a batch-driven `ftp` invocation with a `TD_FTP_PASSWORD` environment variable for credentials.
  Rationale: FTP requires credentials, and the repo does not currently store a password in profile data; a simple env var keeps scope limited while enabling scripted transfers.
  Date/Author: 2025-02-14 / assistant

## Outcomes & Retrospective

- FTP transfers are now gated behind a settings flag and runtime acknowledgement, with insecure usage marked in op logs and warned in logs. Config and transfer CLI paths were updated to support the new setting and flag, and transfer execution was centralized in a new module.

## Context and Orientation

Transfers are currently implemented in `crates/cli/src/main.rs` inside `run_transfer_with_log` and `execute_transfer`, which orchestrate scp/sftp execution using helpers from `crates/core/src/transfer.rs`. Transfer CLI args are defined in `TransferArgs`, `XferArgs`, and `ConfigApplyArgs` within the same file. The settings system stores key/value strings in the `settings` table via `crates/core/src/settings.rs`. Client overrides and environment checks live in `crates/core/src/doctor.rs`, which currently knows about ssh/scp/sftp/telnet. Operation logs are recorded via `crates/core/src/oplog.rs` and are used in transfer flows to capture metadata.

FTP is planned in `EXTERNAL_DESIGN.md` and `PROJECT_PLAN.md`, including a double-lock requirement (setting + runtime flag) and an audit trail entry indicating insecure usage.

## Plan of Work

First, extend core settings with getters/setters for `allow_insecure_transfers`, and add a `td config set allow_insecure_transfers=true|false` CLI path to update it. Next, extend transfer enums and doctor/client override plumbing to include FTP so CLI can resolve the ftp client path when allowed. Then, move transfer execution logic out of `crates/cli/src/main.rs` into a new `crates/cli/src/transfer.rs` module. That module will enforce the FTP double-lock by reading the setting, checking the `--i-know-its-insecure` flag, and returning a clear error when approvals are missing. The module will also log a warning when FTP is used and include an `insecure: true` marker in the operation log metadata for transfers and config apply. Finally, update CLI flags (`--i-know-its-insecure`) on push/pull/xfer/config apply and adjust parsing tests for new arguments.

## Concrete Steps

1. Add `get_allow_insecure_transfers`, `set_allow_insecure_transfers`, and `clear_allow_insecure_transfers` in `crates/core/src/settings.rs`. Ensure callers can treat missing values as `false`.
2. Update `crates/core/src/doctor.rs` to include FTP in `ClientKind`, override fields, and client checks.
3. Extend `crates/core/src/transfer.rs` to include `TransferVia::Ftp` and helper methods to identify insecure transfers.
4. Create `crates/cli/src/transfer.rs` to contain transfer execution, FTP gating, and logging behavior. Move existing execute/run helpers from `main.rs` and update imports.
5. Update CLI args in `crates/cli/src/main.rs` to add `--i-know-its-insecure` for transfer and config apply commands, plus a new `config set` subcommand to set `allow_insecure_transfers`.
6. Update config/transfer parsing tests in `crates/cli/src/main.rs` to cover the new flags and config set command.

## Validation and Acceptance

- Running `td push ... --via ftp` should fail unless both `allow_insecure_transfers=true` is set via `td config set allow_insecure_transfers=true` and `--i-know-its-insecure` is provided.
- When both approvals are present, FTP execution should proceed (subject to external ftp client availability) and the operation log metadata should include `"insecure": true` with the transfer entry.
- `td config apply ... --via ftp` should follow the same gating rules and record `insecure: true` in its op log metadata when used.
- Updated CLI parsing tests should still pass.

## Idempotence and Recovery

Settings updates are safe to repeat: re-running `td config set allow_insecure_transfers=true` overwrites the setting. Transfer code changes are additive; if a transfer fails, no settings are mutated. If needed, revert by removing the FTP gate and restoring the transfer logic to `main.rs`.

## Artifacts and Notes

- None yet.

## Interfaces and Dependencies

In `crates/core/src/settings.rs`, define:

    pub fn get_allow_insecure_transfers(conn: &Connection) -> Result<bool>
    pub fn set_allow_insecure_transfers(conn: &Connection, allow: bool) -> Result<()>
    pub fn clear_allow_insecure_transfers(conn: &Connection) -> Result<()>

In `crates/core/src/transfer.rs`, extend `TransferVia` with `Ftp` and provide:

    pub fn is_insecure(&self) -> bool

In `crates/cli/src/transfer.rs`, define:

    pub struct TransferOutcome { ok: bool, exit_code: i32, duration_ms: i64, client_used: PathBuf, insecure: bool }
    pub fn run_transfer_with_log(..., insecure_flag: bool, op: &str) -> Result<()>
    pub fn execute_transfer(..., insecure_flag: bool) -> Result<TransferOutcome>

These functions should enforce FTP gating, include an `insecure` marker for FTP in op log metadata, and emit a warning log when FTP is used.

Plan update note (2025-02-14): Marked all steps complete, recorded the FTP credential handling decision, and summarized outcomes after implementing the changes.
