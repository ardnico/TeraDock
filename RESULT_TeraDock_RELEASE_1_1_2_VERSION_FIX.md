# TeraDock v1.1.2 Version Metadata Fix

## Summary

Aligned the release-final version and release-facing documentation for the
v1.1.2 session-log operations slice.

This is not a feature change. No session-log backend, TUI/ConPTY behavior,
prune policy, stats behavior, secret masking, terminal replay, tag, or GitHub
Release operation was changed.

## Version Policy

For this repository, the GitHub release tag and CLI binary version should match.
For a `v1.1.2` release:

```text
td --version => td 1.1.2
```

Cargo package versions for the workspace packages were aligned to `1.1.2`.

## Scope And Non-Scope

In scope:

- Cargo/package version metadata.
- Cargo.lock package entries for workspace packages.
- README current stable version and artifact examples.
- CHANGELOG release headings for `1.1.2`, `1.1.1`, and `1.1.0`.
- Result report for this fix.

Out of scope and unchanged:

- Windows `auto -> conpty`.
- TUI/ConPTY behavior.
- Session logging backend selection.
- Prune deletion policy.
- `td session prune --json` schema.
- `td session stats` behavior.
- Secret masking.
- Terminal replay.
- Real SSH automated tests.
- Release tag creation, deletion, or movement.
- GitHub Release publication.
- PR merge.

## Files Changed

- `Cargo.toml`
- `crates/cli/Cargo.toml`
- `crates/core/Cargo.toml`
- `crates/common/Cargo.toml`
- `crates/tui/Cargo.toml`
- `Cargo.lock`
- `README.md`
- `CHANGELOG.md`
- `RESULT_TeraDock_RELEASE_1_1_2_VERSION_FIX.md`

Consulted and left unchanged because their v1.1.2 scope already matched the
session-log operations release:

- `RELEASE_NOTES_1.1.2.md`
- `RELEASE_NOTES_1.1.1.md`
- `RELEASE_NOTES_1.1.0.md`
- `RELEASE_CHECKLIST.md`

Other consulted docs:

- `AGENTS.md`
- `C:\Users\leafs\.codex\RTK.md`
- `CONTRIBUTING.md`
- `docs/security.md`
- `docs/internal/codex-workflow.md`
- `docs/internal/session-logging-design.md`
- `docs/internal/windows-conpty-session-logging-design.md`
- `docs/release-artifact-validation.md`
- `RESULT_TeraDock_RELEASE_1_1_2_FINAL_CHECK.md` existing untracked local report.

## Cargo Package Versions

Before:

- Workspace package version: `0.1.0`
- `td`: `0.1.0`
- `tdcore`: `0.1.0`
- `common`: `0.1.0`
- `tui`: `0.1.0`
- `Cargo.lock` workspace package entries: `0.1.0`

After:

- Workspace package version: `1.1.2`
- `td`: `1.1.2`
- `tdcore`: `1.1.2`
- `common`: `1.1.2`
- `tui`: `1.1.2`
- `Cargo.lock` workspace package entries: `1.1.2`

Note: unrelated dependency package versions in `Cargo.lock`, such as
`num-conv 0.1.0`, were not changed.

## README Version

Before:

- `Current stable version: **1.0.3**.`
- Windows artifact example: `td-1.0.3-windows-x86_64-setup.exe`
- Linux portable artifact example: `td-1.0.3-linux-x86_64.tar.gz`
- Distribution sentence referred to `v1.0.3`.

After:

- `Current stable version: **1.1.2**.`
- Windows artifact example: `td-1.1.2-windows-x86_64-setup.exe`
- Linux portable artifact example: `td-1.1.2-linux-x86_64.tar.gz`
- Distribution sentence refers to `v1.1.2`.
- README documentation links include release notes for `1.1.2`, `1.1.1`,
  and `1.1.0`.

## CHANGELOG Headings

Before:

- `## [1.1.2] - Unreleased`
- `## [1.1.1] - Unreleased`
- `## [1.1.0] - Unreleased`

After:

- `## [1.1.2] - 2026-06-20`
- `## [1.1.1] - 2026-06-18`
- `## [1.1.0] - 2026-06-18`

Dates were selected from the local tag creator dates:

```text
v1.1.0 2026-06-18 45e9876
v1.1.1 2026-06-18 45168eb
v1.1.2 2026-06-20 4ff1ced
```

## td --version Result

Command:

```powershell
.\target\release\td.exe --version
```

Result:

```text
td 1.1.2
```

## Prune And Stats Smoke

Command:

```powershell
.\target\release\td.exe session prune --help
```

Result: passed. Help includes `--older-than`, `--keep-last`, `--dry-run`,
`--yes`, and `--json`.

Command:

```powershell
.\target\release\td.exe session stats --help
```

Result: passed. Help includes `--json`.

Command:

```powershell
.\target\release\td.exe session prune --older-than 30d --dry-run --json
```

Result: passed.

```json
{
  "criteria": {
    "keep_last": null,
    "older_than": "30d"
  },
  "deleted_sessions": 0,
  "dry_run": true,
  "failed_deletions": 0,
  "planned_bytes": 0,
  "selected_sessions": 0,
  "sessions": [],
  "skipped_metadata": 0
}
```

Command:

```powershell
.\target\release\td.exe session stats --json
```

Result: passed.

```json
{
  "by_backend": {
    "conpty": 2
  },
  "by_status": {
    "completed_nonzero": 1,
    "failed": 1
  },
  "log_directory": "C:\\Users\\leafs\\AppData\\Roaming\\TeraDock\\session-logs",
  "newest_session": {
    "session_id": "sl_x7qxorxv",
    "started_at": "2026-06-18T12:22:19Z"
  },
  "oldest_session": {
    "session_id": "sl_mcx5u7jc",
    "started_at": "2026-06-18T12:21:07Z"
  },
  "skipped_metadata": 0,
  "total_log_bytes": 125,
  "total_sessions": 2
}
```

## Validation Results

Passed:

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
git diff --check
.\target\release\td.exe --version
.\target\release\td.exe session prune --help
.\target\release\td.exe session stats --help
.\target\release\td.exe session prune --older-than 30d --dry-run --json
.\target\release\td.exe session stats --json
```

Observed summaries:

- `cargo test`: 159 passed.
- `cargo clippy`: no issues found.
- `cargo build -p td --release --locked`: release build completed.
- `td --version`: `td 1.1.2`.

## Tag And Remote Tag Status

Task-start status before edits:

```powershell
git rev-parse HEAD
```

```text
4ff1ced999e6b8c18f30832967058a435c265776
```

```powershell
git describe --tags --always --dirty
```

```text
v1.1.2
```

```powershell
git tag --points-at HEAD
```

```text
v1.1.2
```

```powershell
git status --short
```

```text
?? RESULT_TeraDock_RELEASE_1_1_2_FINAL_CHECK.md
```

Remote tag check was safe and succeeded:

```powershell
git ls-remote --tags origin v1.1.2
```

```text
4ff1ced999e6b8c18f30832967058a435c265776	refs/tags/v1.1.2
```

After edits and before commit:

```text
git rev-parse HEAD                 => 4ff1ced999e6b8c18f30832967058a435c265776
git describe --tags --always --dirty => v1.1.2-dirty
git tag --points-at HEAD             => v1.1.2
git ls-remote --tags origin v1.1.2   => 4ff1ced999e6b8c18f30832967058a435c265776 refs/tags/v1.1.2
```

This task did not create, delete, move, or push tags. Because the requested
commit is created after the existing `v1.1.2` tag, maintainers still need to
decide tag handling before treating the current fix commit as the final
`v1.1.2` release point.

## Safety Boundaries Preserved

- Windows `auto` still does not select ConPTY by default.
- Explicit Windows ConPTY logging remains opt-in.
- TUI/ConPTY behavior was not edited.
- Session logging backend selection was not edited.
- `td session prune` deletion policy was not edited.
- `td session prune --json` schema was not edited.
- `td session stats` behavior was not edited.
- Secret masking and terminal replay were not added.
- No raw session logs were attached to this report.
- No real SSH automated tests were added.
- No release tags or GitHub Releases were created, moved, deleted, or published.

## Final Verdict

`CONDITIONAL GO: decide tag handling`

Reason: version metadata, README release-facing docs, CHANGELOG headings,
`td --version`, prune smoke, stats smoke, and full validation now pass for the
fix. The existing local and remote `v1.1.2` tag point at
`4ff1ced999e6b8c18f30832967058a435c265776`, which predates this fix commit.
Tag handling was explicitly forbidden in this task, so release finalization
requires a maintainer decision outside this change.

