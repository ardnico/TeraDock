# RESULT: TeraDock session prune JSON output

## Summary

Added `td session prune --json` for dry-run and confirmed deletion workflows.
Human output is unchanged when `--json` is not specified.

Supported command shapes:

- `td session prune --older-than 30d --dry-run --json`
- `td session prune --keep-last 100 --dry-run --json`
- `td session prune --older-than 30d --yes --json`
- `td session prune --keep-last 100 --yes --json`

## Scope

In scope:

- CLI flag parsing for `td session prune --json`.
- Machine-readable prune summaries built from the existing prune plan/apply API.
- Focused CLI tests for dry-run JSON, keep-last/actual JSON, skipped metadata count, failed deletion details, and forbidden field avoidance.
- README, security docs, changelog, and release checklist updates.

Out of scope:

- Windows `auto -> conpty` promotion.
- TUI or ConPTY behavior changes.
- Session logging backend selection changes.
- Prune deletion policy changes.
- Orphan log-only cleanup.
- Secret masking.
- Terminal replay.
- Real SSH automated tests.
- Release tag or GitHub Release publication.

## Phase 1 Findings

Current prune human output:

- Dry-run prints `Session prune dry-run`, log directory, selected session count, planned byte count, `failed deletions: 0`, skipped metadata count, and candidate session metadata/log paths.
- Confirmed deletion prints `Session prune`, log directory, selected session count, planned byte count, deleted count, failed deletion count, skipped metadata count, and failure details when deletion fails.
- Without `--yes`, existing refusal behavior is preserved.

JSON summary fields selected:

- `dry_run`
- `criteria.older_than`
- `criteria.keep_last`
- `selected_sessions`
- `deleted_sessions`
- `planned_bytes`
- `skipped_metadata`
- `failed_deletions`
- `sessions`
- Optional `requires_confirmation`
- Optional `failures`

Dry-run and actual deletion share:

- Criteria.
- Selected session count.
- Deleted session count.
- Planned bytes.
- Skipped metadata count.
- Failed deletion count.
- Per-session action status.

Path handling:

- Dry-run JSON includes the same validated metadata/log candidate paths already shown by human dry-run output.
- Confirmed deletion JSON does not include candidate paths by default.
- Deletion failure details include only the validated path involved in the failed deletion attempt.
- Skipped unsafe/malformed metadata is counted; raw metadata is not dumped.

Existing JSON consistency:

- Existing CLI JSON commands use `serde_json::to_string_pretty`.
- `td session prune --json` uses the same pretty JSON output style.
- It does not serialize full `SessionLogMetadata`; it emits a narrower prune-specific payload.

## JSON Schema

Dry-run payload:

```json
{
  "dry_run": true,
  "criteria": {
    "older_than": "30d",
    "keep_last": null
  },
  "selected_sessions": 1,
  "deleted_sessions": 0,
  "planned_bytes": 4096,
  "skipped_metadata": 0,
  "failed_deletions": 0,
  "sessions": [
    {
      "session_id": "sl_old",
      "started_at": "1970-01-01T00:00:01Z",
      "status": "completed",
      "backend": "conpty",
      "metadata_path": "session-logs/sl_old.json",
      "log_path": "session-logs/sl_old.log",
      "planned_bytes": 4096,
      "action": "would_delete"
    }
  ]
}
```

Confirmed deletion payload:

```json
{
  "dry_run": false,
  "criteria": {
    "older_than": null,
    "keep_last": 100
  },
  "selected_sessions": 1,
  "deleted_sessions": 1,
  "planned_bytes": 4096,
  "skipped_metadata": 0,
  "failed_deletions": 0,
  "sessions": [
    {
      "session_id": "sl_old",
      "action": "deleted"
    }
  ]
}
```

Failure detail payload adds:

```json
{
  "failures": [
    {
      "session_id": "sl_failed",
      "operation": "remove_log",
      "path": "session-logs/sl_failed.log",
      "error": "access denied"
    }
  ]
}
```

## Safety Behavior

- JSON output does not include terminal transcript bodies.
- JSON output does not dump full session metadata.
- JSON output does not include SSH auth args, full SSH command strings, private key paths, passwords, tokens, or secrets.
- Existing prune planning and deletion APIs remain responsible for metadata parsing, path validation, candidate selection, and deletion ordering.
- Unsafe or malformed metadata remains skipped.
- Missing log behavior is unchanged.
- Human output remains unchanged without `--json`.
- Windows `session.log.backend=auto` remains unchanged and does not select ConPTY.

## Files Changed

- `crates/cli/src/main.rs`
- `README.md`
- `docs/security.md`
- `CHANGELOG.md`
- `RELEASE_CHECKLIST.md`
- `RESULT_TeraDock_SESSION_PRUNE_JSON.md`

## Existing Docs Consulted

- `AGENTS.md`
- `CONTRIBUTING.md`
- `SECURITY.md`
- `README.md`
- `docs/security.md`
- `docs/internal/codex-workflow.md`
- `docs/internal/session-logging-design.md`
- `CHANGELOG.md`
- `RELEASE_CHECKLIST.md`
- `RELEASE_NOTES_1.1.1.md`

## Tests

Focused checks:

- `cargo test -p td prune` passed: 3 passed, 0 failed.
- `cargo test -p tdcore prune_` passed: 11 passed, 0 failed.
- `.\target\release\td.exe session prune --help` passed and shows `--json`.
- `git diff --check` passed.

Required validation:

- `cargo fmt --check` passed.
- `cargo test` passed.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo build -p td --release --locked` passed.

## Docs Updated

- `README.md` documents dry-run JSON examples and automation summaries.
- `docs/security.md` documents JSON safety boundaries and transcript exclusion.
- `CHANGELOG.md` records `td session prune --json`.
- `RELEASE_CHECKLIST.md` adds JSON prune validation expectations and release commands.

## Commit Hash

Pending before commit creation. The final commit hash is reported in the task completion message.

## Not Implemented

- No Windows `auto -> conpty` promotion.
- No TUI or ConPTY behavior changes.
- No session logging backend selection changes.
- No prune deletion policy expansion.
- No orphan log-only cleanup.
- No secret masking.
- No terminal replay.
- No real SSH automated tests.
- No release tag or GitHub Release.

## Next Steps

- Optional release-candidate smoke can run the documented JSON prune commands against a sanitized disposable session-log directory.
