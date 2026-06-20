# RESULT_TeraDock_RELEASE_1_1_3_VERSION_CONSISTENCY

## Summary

Prepared v1.1.3 as a release consistency patch after the v1.1.2 tag was
already published before the version metadata fix.

This patch preserves the v1.1.2 session-log operations scope while aligning the
Cargo package versions, Cargo.lock workspace package entries, README
release-facing version text, changelog, release notes, and release binary
version output to 1.1.3.

## Why v1.1.3 was chosen instead of moving v1.1.2

The remote `v1.1.2` tag already exists and points to:

```text
4ff1ced999e6b8c18f30832967058a435c265776 refs/tags/v1.1.2
```

Moving, deleting, recreating, or force-pushing that published tag would rewrite
release history and could invalidate already-published release evidence or
downloaded artifacts. v1.1.3 is therefore the clean patch release that carries
the same v1.1.2 session-log operations scope with consistent package, binary,
README, changelog, and release-note metadata.

## Scope and non-scope

In scope:

- Update Cargo workspace/package versions from 1.1.2 to 1.1.3.
- Update Cargo.lock workspace package entries from 1.1.2 to 1.1.3.
- Ensure the release binary reports `td 1.1.3`.
- Update README stable version and artifact examples to 1.1.3.
- Add a changelog section and release notes for 1.1.3.

Out of scope and unchanged:

- Existing `v1.1.2`, `v1.1.1`, and `v1.1.0` changelog headings.
- Existing `v1.1.2` tag.
- Windows `auto -> conpty` selection.
- TUI/ConPTY behavior.
- Session logging backend selection.
- Prune/stats behavior and JSON schema.
- Secret masking, terminal replay, real SSH automated tests, GitHub Release
  publication, PR merge, or release tag creation.

## Files changed

- `Cargo.toml`
- `Cargo.lock`
- `crates/cli/Cargo.toml`
- `crates/common/Cargo.toml`
- `crates/core/Cargo.toml`
- `crates/tui/Cargo.toml`
- `README.md`
- `CHANGELOG.md`
- `RELEASE_NOTES_1.1.3.md`
- `RESULT_TeraDock_RELEASE_1_1_3_VERSION_CONSISTENCY.md`

Pre-existing worktree note: `RESULT_TeraDock_RELEASE_1_1_2_VERSION_FIX.md` was
already deleted before this task's edits and is not part of this release patch.

## Existing docs consulted

- `AGENTS.md`
- `CONTRIBUTING.md`
- `README.md`
- `CHANGELOG.md`
- `RELEASE_CHECKLIST.md`
- `docs/security.md`
- `docs/release-artifact-validation.md`
- `docs/internal/session-logging-design.md`
- `docs/internal/windows-conpty-session-logging-design.md`
- `RELEASE_NOTES_1.1.2.md`

## Version before/after

Before:

- Cargo workspace/package versions: `1.1.2`
- Cargo.lock workspace package entries for `common`, `td`, `tdcore`, and `tui`:
  `1.1.2`
- Release binary version state expected from package metadata: `td 1.1.2`

After:

- Cargo workspace/package versions: `1.1.3`
- Cargo.lock workspace package entries for `common`, `td`, `tdcore`, and `tui`:
  `1.1.3`
- Release binary version state: `td 1.1.3`

## README before/after

Before:

- Current stable version: `1.1.2`
- Windows example: `td-1.1.2-windows-x86_64-setup.exe`
- Linux portable archive example: `td-1.1.2-linux-x86_64.tar.gz`
- Release notes list started with `RELEASE_NOTES_1.1.2.md`

After:

- Current stable version: `1.1.3`
- Windows example: `td-1.1.3-windows-x86_64-setup.exe`
- Linux portable archive example: `td-1.1.3-linux-x86_64.tar.gz`
- README states v1.1.3 is a release consistency patch after v1.1.2.
- Release notes list starts with `RELEASE_NOTES_1.1.3.md`, while preserving the
  existing v1.1.2, v1.1.1, and v1.1.0 links.

## CHANGELOG changes

Added `## [1.1.3] - 2026-06-20` above the existing v1.1.2 heading.

The new section records:

- Version metadata alignment to 1.1.3 after the v1.1.2 tag was already
  published.
- v1.1.3 as a release consistency patch after v1.1.2.
- Preservation of the v1.1.2 session-log operations scope.
- No changes to Windows `auto`, TUI/ConPTY behavior, backend selection,
  prune/stats behavior, JSON schema, secret masking, terminal replay, or real
  SSH automated tests.

Existing `1.1.2`, `1.1.1`, and `1.1.0` release headings were preserved.

## td --version result

Command:

```powershell
.\target\release\td.exe --version
```

Result:

```text
td 1.1.3
```

## prune/stats smoke result

Commands:

```powershell
.\target\release\td.exe session prune --help
.\target\release\td.exe session stats --help
.\target\release\td.exe session prune --older-than 30d --dry-run --json
.\target\release\td.exe session stats --json
```

Results:

- `session prune --help`: PASS.
- `session stats --help`: PASS.
- `session prune --older-than 30d --dry-run --json`: PASS; dry-run JSON
  returned `dry_run: true`, `selected_sessions: 0`, `deleted_sessions: 0`,
  `failed_deletions: 0`, and `skipped_metadata: 0`.
- `session stats --json`: PASS; aggregate JSON returned `total_sessions: 2`,
  `total_log_bytes: 125`, `skipped_metadata: 0`, `by_backend.conpty: 2`,
  `by_status.completed_nonzero: 1`, and `by_status.failed: 1`.

The JSON smokes did not print terminal transcript bodies or full session
metadata.

## Validation results

Commands were executed through `rtk proxy` to follow the repository RTK rule;
the underlying validation commands are shown below.

| Command | Result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo test` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo build -p td --release --locked` | PASS |
| `git diff --check` | PASS |
| `.\target\release\td.exe --version` | PASS; `td 1.1.3` |
| `.\target\release\td.exe session prune --help` | PASS |
| `.\target\release\td.exe session stats --help` | PASS |
| `.\target\release\td.exe session prune --older-than 30d --dry-run --json` | PASS |
| `.\target\release\td.exe session stats --json` | PASS |

## Local/remote tag status

Recorded before commit creation and without changing tags.

```powershell
git rev-parse HEAD
```

```text
cba09bc89a961f1f5ad9f583013626ce21505b98
```

```powershell
git describe --tags --always --dirty
```

```text
v1.1.2-1-gcba09bc-dirty
```

```powershell
git tag --points-at HEAD
```

```text
<no output>
```

```powershell
git ls-remote --tags origin v1.1.2
```

```text
4ff1ced999e6b8c18f30832967058a435c265776 refs/tags/v1.1.2
```

```powershell
git ls-remote --tags origin v1.1.3
```

```text
<no output>
```

```powershell
git status --short
```

```text
 M CHANGELOG.md
 M Cargo.lock
 M Cargo.toml
 M README.md
 D RESULT_TeraDock_RELEASE_1_1_2_VERSION_FIX.md
 M crates/cli/Cargo.toml
 M crates/common/Cargo.toml
 M crates/core/Cargo.toml
 M crates/tui/Cargo.toml
?? RELEASE_NOTES_1.1.3.md
```

No local tag points at the recorded HEAD. Remote `v1.1.2` still points to the
published tag commit. Remote `v1.1.3` does not exist.

## Commit hash

Pending at report creation. The final commit is created after this report is
written, so the resulting commit hash is reported in the final task response.

## Safety boundaries preserved

- Did not move, delete, recreate, or push any tag.
- Did not publish a GitHub Release.
- Did not promote Windows `auto -> conpty`.
- Did not change TUI/ConPTY behavior.
- Did not change session logging backend selection.
- Did not change prune/stats behavior or JSON schema.
- Did not add secret masking.
- Did not add terminal replay.
- Did not add real SSH automated tests.
- Did not merge any PR.
- Did not attach raw session logs or transcript bodies to this report.

## Final verdict

CONDITIONAL GO: maintainer must create tag/release
