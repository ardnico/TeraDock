# TeraDock Session Logging v1.1 Prep Result

## Summary

Interactive SSH session terminal logging was designed and implemented as a v1.1 candidate feature.

This is not an `op_logs` expansion. `op_logs` remain operation event logs. The new feature saves terminal transcripts and sidecar metadata only when `session.log.enabled=true`.

## Changes

- Added `docs/internal/session-logging-design.md`.
- Added `tdcore::session_log` for config resolution, backend planning, session id/path generation, metadata writing, safe oplog references, and metadata listing/loading.
- Added config keys:
  - `session.log.enabled`
  - `session.log.dir`
  - `session.log.backend`
- Added `td session` CLI:
  - `td session list`
  - `td session list --json`
  - `td session path <session_id>`
  - `td session show <session_id>`
  - `td session show <session_id> --tail N`
- Wired TUI interactive SSH (`s`) through the session logging plan.
- Wired CLI `td connect <profile_id>` for SSH profiles through the same session logging plan.
- Added safe `op_logs` metadata references for interactive SSH session logs.
- Updated README, TUI docs, security docs, roadmap, contribution notes, release checklist, and SSH boundary docs.

## Changed Files

- `crates/core/src/session_log.rs`
- `crates/core/src/settings_registry.rs`
- `crates/core/src/paths.rs`
- `crates/core/src/lib.rs`
- `crates/tui/src/app.rs`
- `crates/tui/src/state.rs`
- `crates/cli/src/main.rs`
- `docs/internal/session-logging-design.md`
- `docs/internal/ssh-invocation-boundary.md`
- `docs/tui.md`
- `docs/security.md`
- `README.md`
- `ROADMAP.md`
- `CONTRIBUTING.md`
- `RELEASE_CHECKLIST.md`

## Session Logging Design

- Default is disabled.
- `session.log.enabled=true` is required before terminal transcripts are saved.
- Metadata is a JSON sidecar beside the terminal transcript.
- Session ids use the existing local id style with `sl_` prefix.
- Saved metadata includes session id, profile id, user, host, port, timestamps, duration, exit code, backend, log path, metadata path, and status.
- Metadata does not include SSH auth args, full command strings, private key paths, passwords, secrets, or tokens.

## Backend Policy

- Linux/macOS: use `script` when `session.log.backend=auto` or `script`.
- Windows: unsupported in this initial implementation; fallback to normal SSH.
- `session.log.backend=no-log` forces normal SSH without transcript saving.
- If `script` is unavailable or cannot be launched, TeraDock falls back to normal SSH when possible.
- TUI raw mode, alternate screen, mouse capture, and cursor restore remain owned by the TUI app layer.

## Added Config

```text
session.log.enabled = false
session.log.dir = <data_dir>/session-logs
session.log.backend = auto
```

`td config keys` includes these keys. `td config schema session.log.enabled`, `td config schema session.log.dir`, and `td config schema session.log.backend` expose descriptions and validation metadata. `td config get <key> --resolved` reports effective defaults for the session logging keys.

## Added CLI

```bash
td session list
td session list --json
td session path <session_id>
td session show <session_id>
td session show <session_id> --tail 50
```

`show` is metadata-first and does not print the full terminal transcript by default.

## Security Policy

- Session logging is default disabled.
- Runtime notices warn that displayed terminal output can contain secrets.
- Terminal transcripts are sensitive artifacts.
- TeraDock does not attempt complete masking of terminal output.
- Metadata excludes SSH auth args, full command strings, private key paths, passwords, secrets, and tokens.
- On Unix-like systems, TeraDock attempts user-only permissions for the session log directory and written files.

## op_logs Integration

TUI `ssh_session` metadata now includes:

```json
{
  "session_log_saved": true,
  "session_log_id": "sl_..."
}
```

or:

```json
{
  "session_log_saved": false,
  "session_log_reason": "disabled"
}
```

The log path is not copied into `op_logs`. It stays in the session metadata file.

CLI SSH `connect` uses the same safe reference fields in its operation metadata when session logging is evaluated.

## Tests Added

- Config registry tests for session logging keys and validators.
- Session log config resolution default test.
- No-log backend planning test.
- Script invocation construction test without executing `script`.
- Metadata write/list/load test without external SSH.
- Safe oplog reference metadata test.
- TUI oplog metadata tests for saved and disabled session log references.
- CLI parsing tests for `td session list` and `td session path`.
- CLI test for resolved static defaults of `session.log.enabled` and `session.log.backend`.

No external SSH server test was added. `script` execution is not required by unit tests.

## Test Results

Passed:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

## Not Implemented

- Windows PTY/session transcript backend.
- portable-pty backend.
- tmux integration.
- Terminal emulator launch.
- Web UI, remote daemon, cloud sync.
- Full secret masking of terminal output.
- CommandSet output history integration.
- TUI recent pane.

## Next Stability Improvements

- Add a small manual smoke protocol for Linux/macOS `script` backend behavior.
- Verify util-linux and macOS `script` flag compatibility on real hosts.
- Add a Windows-facing status message smoke check for unsupported fallback.
- Consider a future PTY abstraction only after the `script` backend is stable.
- Add cleanup/retention guidance for old session logs.
