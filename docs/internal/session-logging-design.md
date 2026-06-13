# Interactive Session Logging Design

This note defines the v1.1 candidate design for saving terminal output from interactive SSH sessions.

## Goal

- Save terminal display logs for interactive SSH sessions.
- Make saved sessions discoverable after the session ends.
- State clearly that terminal logs may contain secrets, passwords, tokens, or other sensitive output.

## Non-goals

- Complete secret masking.
- Identical PTY behavior on every operating system.
- Dependence on terminal-emulator-specific features.
- tmux integration.
- Web UI.
- Remote daemon.
- Integration with CommandSet output history.

## Security policy

- Session logging is disabled by default.
- Enabling session logging must produce clear warnings in docs and runtime status messages.
- If a password, secret, token, or sensitive prompt response is displayed in the terminal, it may be captured in the log file.
- Metadata must not include SSH auth arguments, full command strings, private key paths, passwords, secrets, or tokens.
- Log file permissions should be user-only when the OS supports that reliably.
- `docs/security.md` must describe the terminal-output risk and safe handling expectations.

## Current implementation constraints

TUI SSH sessions currently suspend raw mode, leave the alternate screen, disable mouse capture, show the cursor, and then spawn `ssh` with inherited stdin/stdout/stderr. After the child exits, the TUI re-enters raw mode and the alternate screen.

That inherited-stdio model is correct for interactive use, but TeraDock cannot capture stdout or stderr directly without becoming a terminal/PTY intermediary. The minimum safe path is to keep the existing terminal lifecycle and wrap the SSH command in an external recorder where supported.

## Backend candidates

### `script` command backend

Use the platform `script` utility to allocate a PTY and save the terminal transcript while running SSH.

Pros:
- Small implementation.
- Preserves an interactive terminal experience on Linux/macOS.
- Avoids adding a large PTY dependency for v1.1.

Cons:
- CLI flags differ across implementations.
- Usually unavailable on Windows.
- Captures displayed sensitive output without masking.

### portable-pty backend

Use a Rust PTY library and copy bytes between the user terminal and child process.

Pros:
- More control over capture and metadata.
- Potential route to Windows support later.

Cons:
- Larger implementation and test surface.
- Higher risk of breaking TUI terminal restore behavior.
- Not needed for the v1.1 minimum.

### no-log backend

Run SSH normally and record metadata explaining that no terminal log was saved.

Pros:
- Safe fallback when logging is disabled, unsupported, or unavailable.
- Keeps SSH usable when logging cannot be initialized.

Cons:
- No transcript is saved.

## v1.1 implementation decision

- Default: disabled.
- Linux/macOS: use the `script` backend when `session.log.backend=auto` or `script`.
- Windows: report unsupported and fall back to normal SSH without saving a session log.
- If `script` is unavailable or setup fails, continue the SSH session without logging when possible and record a no-log reason.
- Do not introduce portable-pty, tmux, terminal emulator launch, Web UI, remote daemon, or CommandSet output history integration in this slice.

## Data model

Session log metadata is a JSON sidecar file next to the terminal log:

- `session_id`
- `profile_id`
- `user`
- `host`
- `port`
- `started_at`
- `ended_at`
- `duration_ms`
- `exit_code`
- `backend`
- `log_path`
- `metadata_path`
- `status`
- `reason`

The metadata intentionally excludes SSH auth args, full command strings, private key paths, passwords, secrets, and tokens.

## CLI/TUI UX

- Configuration keys:
  - `session.log.enabled`
  - `session.log.dir`
  - `session.log.backend`
- Initial values:
  - `session.log.enabled=false`
  - `session.log.dir=<data_dir>/session-logs`
  - `session.log.backend=auto`
- TUI: pressing `s` opens an SSH session as before. When logging is enabled and supported, TeraDock saves the transcript and reports the session id after return.
- CLI: `td connect <profile_id>` can use the same logging path for SSH profiles.
- Reference commands:
  - `td session list`
  - `td session list --json`
  - `td session path <session_id>`
  - `td session show <session_id>`

`td session show` should default to metadata-oriented output and only show log excerpts when the caller explicitly asks for a tail length.

## op_logs integration

Existing `op_logs` remain operation event logs. They do not become terminal-output storage.

For interactive SSH session events, add only a small cross-reference:

```json
{
  "session_log_id": "sl_...",
  "session_log_saved": true
}
```

When no terminal log was saved:

```json
{
  "session_log_saved": false,
  "session_log_reason": "disabled"
}
```

Avoid storing `log_path` in `op_logs` unless there is a concrete need. The session log metadata file owns the local paths.
