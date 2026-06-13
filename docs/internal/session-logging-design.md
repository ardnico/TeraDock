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

### PowerShell Transcript backend

Use PowerShell `Start-Transcript` / `Stop-Transcript` to run the existing `ssh`
invocation and save the terminal transcript on Windows.

Pros:
- Small Windows implementation.
- Keeps the same inherited stdin/stdout/stderr interaction model.
- Avoids a ConPTY dependency in this slice.

Cons:
- Transcript format is PowerShell-dependent.
- Terminal control sequences are not guaranteed to replay exactly.
- Not every interactive prompt is guaranteed to behave like a ConPTY recorder.
- Captures displayed sensitive output without masking.

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
- Windows: use the `powershell-transcript` backend when `session.log.backend=auto` or `powershell-transcript`.
- Windows `auto` requires PowerShell, `ssh`, and a writable log directory. Missing PowerShell resolves to `no-log` with `powershell_not_found`; missing `ssh` resolves to `no-log` with `ssh_not_found`; an unwritable log directory resolves to `no-log` with `log_dir_not_writable`.
- Explicit `powershell-transcript` is unsupported outside Windows. On Windows, explicit `powershell-transcript` reports not-ready errors instead of silently opening an unlogged SSH session.
- If `script` is unavailable or setup fails under `auto`, continue the SSH session without logging when possible and record a no-log reason.
- Do not introduce ConPTY, portable-pty, tmux, terminal emulator launch, Web UI, remote daemon, or CommandSet output history integration in this slice.

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
- TUI settings: pressing `c` opens the settings screen. Saving there writes global settings and affects subsequent SSH sessions.
- CLI: `td connect <profile_id>` can use the same logging path for SSH profiles.
- CLI settings: `td config ui` opens the same BIOS-style settings screen outside `td ui`.
- Diagnostics: `td session doctor` reports enablement, backend setting, resolved backend, `script` availability, PowerShell availability, `ssh` availability, log directory state, newest saved session log, platform support, fallback reason, status, and hints.
- Reference commands:
  - `td session doctor`
  - `td config ui`
  - `td session list`
  - `td session list --json`
  - `td session path <session_id>`
  - `td session show <session_id>`

`td session show` should default to metadata-oriented output and only show log excerpts when the caller explicitly asks for a tail length.

The settings screen includes a diagnostics panel with the same core report. It shows enabled state, backend setting, resolved backend, platform, platform support, PowerShell/ssh readiness, log directory writability, fallback reason, and status. It is intentionally focused on global Session Logging settings first; profile/env settings can still override the effective value and are shown as source warnings rather than being edited from this screen.

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
