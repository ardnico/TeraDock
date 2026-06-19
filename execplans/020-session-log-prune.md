# Session log prune for v1.1.1 stabilization

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document follows `.agent/PLANS.md`.

## Purpose / Big Picture

TeraDock can save interactive SSH terminal transcripts as session logs. Those logs are intentionally local and opt-in, but they can contain sensitive terminal output and currently accumulate until the user removes files by hand. This change adds `td session prune` so an operator can preview and then remove old saved session metadata and the matching terminal log files by age or by keeping only the newest sessions. The visible behavior is that `td session prune --older-than 30d --dry-run` prints exactly what would be removed without deleting anything, and `td session prune --older-than 30d --yes` removes only validated session metadata plus the corresponding validated log file.

## Progress

- [x] (2026-06-19 07:20 JST) Read repository instructions in `.agent/AGENTS.md` and `.agent/PLANS.md`.
- [x] (2026-06-19 07:25 JST) Investigated existing session log directory, metadata schema, list/show/path behavior, session id generation, status handling, and docs.
- [x] (2026-06-19 07:55 JST) Add core session prune planning, safe path validation, and delete application logic.
- [x] (2026-06-19 08:00 JST) Add `td session prune` CLI with `--older-than`, `--keep-last`, `--dry-run`, and `--yes`.
- [x] (2026-06-19 08:10 JST) Add focused tests for dry-run planning, age selection, keep-last selection, malformed and unsafe metadata skips, missing logs, non-success statuses, and summary counts.
- [x] (2026-06-19 08:25 JST) Update README, `docs/security.md`, `RELEASE_CHECKLIST.md`, and `CHANGELOG.md`.
- [x] (2026-06-19 08:30 JST) Create `RESULT_TeraDock_SESSION_PRUNE_V1_1_1.md`.
- [x] (2026-06-19 08:45 JST) Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo build -p td --release --locked`.
- [x] (2026-06-19 09:10 JST) Add `RELEASE_NOTES_1.1.1.md`, direct regression coverage for traversal and orphan-log safety, and always print the skipped metadata count in prune summaries.
- [x] (2026-06-19 09:25 JST) Rerun `cargo fmt --check`, `cargo test -p tdcore prune_`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build -p td --release --locked`, `.\target\release\td.exe session prune --help`, and `git diff --check` after the final additions.

## Surprises & Discoveries

- Observation: Existing `list_session_logs_in_dir` silently ignores unreadable or malformed JSON metadata, which is correct for listing but insufficient for cleanup evidence because prune must report skipped unsafe entries.
  Evidence: `crates/core/src/session_log.rs` scans `.json` files, reads them with `fs::read_to_string`, and only pushes entries when `serde_json::from_str::<SessionLogMetadata>` succeeds.
- Observation: Failed and aborted ConPTY sessions already produce metadata, sometimes with no log file.
  Evidence: `complete_conpty_failure_session` uses `require_log_file: false`, and the existing `writes_conpty_double_ctrl_c_abort_metadata_without_log_file` test asserts `metadata.log_path == None`.
- Observation: The focused prune tests pass before the broader validation gates.
  Evidence: `cargo test -p tdcore prune_` passed 11 tests with 67 filtered out.
- Observation: The full validation gate passes after adding the CLI parser test.
  Evidence: `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo build -p td --release --locked` all exited 0 after the final release-note and safety-test additions.

## Decision Log

- Decision: Prune will be metadata-driven and will not touch orphan `.log` files that have no valid metadata in the initial implementation.
  Rationale: This matches the requested scope and avoids broad deletion behavior before there is a separate orphan-log audit path.
  Date/Author: 2026-06-19 / Codex
- Decision: When `--older-than` and `--keep-last` are combined, a session must satisfy both criteria to be deleted.
  Rationale: This is the more conservative interpretation because it deletes fewer files and preserves the newest sessions even if they are old.
  Date/Author: 2026-06-19 / Codex
- Decision: Non-dry-run deletion requires `--yes`; no interactive prompt will be implemented for this first version.
  Rationale: The requested examples use `--yes`, and requiring an explicit flag keeps automation predictable while ensuring the default command deletes nothing.
  Date/Author: 2026-06-19 / Codex
- Decision: Prune will validate both the actual metadata file path and the metadata-recorded `metadata_path` and `log_path` before deleting anything.
  Rationale: The user explicitly called out path traversal, paths outside the log directory, and Windows canonicalization. Validating the pair before deletion prevents metadata from causing deletion outside the configured session log directory.
  Date/Author: 2026-06-19 / Codex

## Outcomes & Retrospective

Core prune planning/deletion, CLI command, focused tests, documentation, release notes, and result reporting are complete. All required validation gates passed. The implemented behavior is a v1.1.1 stabilization candidate and does not promote Windows `auto -> conpty`.

## Context and Orientation

The session log implementation lives in `crates/core/src/session_log.rs`. It resolves the configured directory with `configured_session_log_dir`, which defaults to the TeraDock config directory plus `session-logs` when `session.log.dir` is unset. New sessions allocate an id with `generate_id("sl_")` and create two sibling paths: `<session_id>.log` and `<session_id>.json`. The JSON metadata schema is `SessionLogMetadata`, including `session_id`, target profile fields, timestamps, duration, `exit_code`, `backend`, optional `log_path`, `metadata_path`, `status`, optional failure fields, and optional capture status fields. Successful zero-exit sessions use `status="completed"`, nonzero exits use `status="completed_nonzero"`, and failure paths can use `status="failed"` or `status="aborted"`.

The CLI entry point is `crates/cli/src/main.rs`. The `SessionCommands` enum currently contains `doctor`, `conpty-test`, `list`, `show`, and `path`. `td session list` loads valid metadata from the configured log directory and sorts newest first by `started_at`. `td session show` reads one metadata JSON by validated id and can print a sanitized tail from the log path. `td session path` prints the metadata `log_path` if present.

The security docs already state that terminal transcripts may contain secrets and that Windows `auto` remains `no-log` unless ConPTY is selected explicitly. This plan must not promote Windows `auto -> conpty`, must not add secret masking, and must not add full terminal replay.

## Plan of Work

Add a metadata-driven prune API to `crates/core/src/session_log.rs`. The API will scan `.json` metadata files in the configured log directory, record skipped unreadable or malformed metadata, validate path safety for each parsed session, select candidates by age and/or keep-last, calculate planned bytes from the actual metadata file and existing log file, and apply deletion by deleting the log first and the metadata second. The log-first order keeps metadata visible if log deletion fails.

Add a `Prune(SessionPruneArgs)` variant to `SessionCommands` in `crates/cli/src/main.rs`. The handler will parse age values like `30d`, reject zero `--keep-last`, build a prune plan, print candidate paths for dry-runs, require `--yes` for actual deletion, and print sessions matched/deleted, planned bytes, skipped metadata, and failed deletions.

Update README and `docs/security.md` with the new prune commands and the reminder that session logs are sensitive. Update `RELEASE_CHECKLIST.md` and `CHANGELOG.md` to include v1.1.1 cleanup scope while preserving the explicit statement that Windows `auto` remains unchanged.

Create `RESULT_TeraDock_SESSION_PRUNE_V1_1_1.md` with the current investigation, implemented behavior, safety design, tests, docs, open gaps, and release recommendation.

## Concrete Steps

Work from `C:\Users\leafs\work\git\TeraDock`.

Run these validation gates after implementation:

    cargo fmt --check
    cargo test
    cargo clippy --all-targets --all-features -- -D warnings
    cargo build -p td --release --locked

Manual examples after build:

    .\target\release\td.exe session prune --older-than 30d --dry-run
    .\target\release\td.exe session prune --older-than 30d --yes
    .\target\release\td.exe session prune --keep-last 100 --dry-run
    .\target\release\td.exe session prune --keep-last 100 --yes

## Validation and Acceptance

The core tests must prove that dry-run planning does not delete files, age selection chooses old sessions only, keep-last preserves the newest N sessions, malformed metadata is skipped, paths outside the log directory are skipped, missing log files do not crash, failed/aborted/completed_nonzero sessions are eligible, and summary counts match the planned candidates and bytes.

The CLI must refuse to delete when neither `--dry-run` nor `--yes` is supplied for a non-empty plan. It must print candidate metadata and log paths in dry-run mode. With `--yes`, it must delete only validated metadata files and their validated corresponding logs. It must never delete files outside the canonical session log directory.

## Idempotence and Recovery

Dry-run is read-only and can be repeated safely. Actual deletion is intentionally irreversible, so it requires `--yes`. If deletion fails for a log file, metadata is left in place so the session remains visible to `td session list/show`. If metadata deletion fails after log deletion succeeds, the failure is reported and the user can rerun prune after fixing permissions; missing logs are not treated as a crash.

## Artifacts and Notes

The final result artifact will be `RESULT_TeraDock_SESSION_PRUNE_V1_1_1.md`. It will include the validation command outcomes.

## Interfaces and Dependencies

In `crates/core/src/session_log.rs`, define public prune structs and functions near the existing list/show helpers:

    pub struct SessionPruneCriteria { pub older_than_ms: Option<i64>, pub keep_last: Option<usize>, pub now_ms: i64 }
    pub struct SessionPrunePlan { ... }
    pub fn plan_session_prune(conn: &Connection, criteria: SessionPruneCriteria) -> Result<SessionPrunePlan>
    pub fn plan_session_prune_in_dir(dir: &Path, criteria: SessionPruneCriteria) -> Result<SessionPrunePlan>
    pub fn apply_session_prune_plan(plan: &SessionPrunePlan) -> SessionPruneApplyReport

The implementation should use only the Rust standard library and existing crate dependencies. No new terminal backend, auto backend promotion, secret masking, or terminal replay logic belongs in this plan.
