# TeraDock Windows Session Logging Result

## Summary

Added a minimal Windows interactive SSH session logging backend using PowerShell Transcript. Session logging remains disabled by default. When enabled with `session.log.backend=auto`, Windows now resolves to `powershell-transcript` when PowerShell, `ssh`, and the log directory are ready; otherwise it reports a no-log fallback reason.

## Changed Files

- `crates/core/src/session_log.rs`
- `crates/core/src/settings_registry.rs`
- `crates/cli/src/main.rs`
- `crates/tui/src/app.rs`
- `crates/tui/src/state.rs`
- `crates/tui/src/settings_ui.rs`
- `README.md`
- `docs/security.md`
- `docs/tui.md`
- `docs/internal/session-logging-design.md`
- `docs/internal/ssh-invocation-boundary.md`
- `RELEASE_CHECKLIST.md`
- `RESULT_TeraDock_WINDOWS_SESSION_LOGGING.md`

## Windows Backend Specification

- Backend name: `powershell-transcript`.
- PowerShell candidates: `powershell.exe`, `powershell`, `pwsh.exe`, `pwsh`.
- Windows `auto` uses PowerShell Transcript when PowerShell and `ssh` are found and the log directory is writable.
- The wrapper runs the existing resolved SSH executable and arguments with inherited stdin/stdout/stderr.
- The TUI still leaves raw mode and the alternate screen before launching the session, then restores the TUI after return.
- Metadata sidecars are written next to the log and are visible through `td session list`, `td session show`, and `td session path`.

## Constraints

- PowerShell transcript output is PowerShell-dependent.
- Terminal control sequences are not guaranteed to replay exactly.
- Every interactive prompt shape is not guaranteed to behave like a ConPTY recorder.
- Terminal output displayed during SSH can still contain passwords, tokens, secrets, or prompt responses and can be captured in the transcript.
- Metadata does not store SSH auth args, full command strings, private key paths, passwords, secrets, or tokens.
- ConPTY, tmux, terminal emulator launch, and portable-pty are intentionally not implemented in this slice.

## Backend Resolution

- Disabled: `resolved backend: disabled`, `Status: disabled`.
- `no-log`: `resolved backend: no-log`, `fallback reason: backend_no_log`.
- Linux/macOS `auto`: `script` when available, otherwise `no-log` with `script_unavailable`.
- Windows `auto`: `powershell-transcript` when ready.
- Windows missing PowerShell: `no-log` with `powershell_not_found`.
- Windows missing `ssh`: `no-log` with `ssh_not_found`.
- Log directory not writable: `no-log` with `log_dir_not_writable`.
- Explicit `powershell-transcript` outside Windows: error/not-ready with `unsupported_on_platform`.
- Explicit `powershell-transcript` on Windows without PowerShell: error/not-ready with `powershell_not_found`.

## Doctor Improvements

`td session doctor` now reports:

- enabled state
- backend setting
- resolved backend
- `script` command or note
- PowerShell command or note
- `ssh` command or note
- log directory existence and writability
- newest saved session log
- platform and platform support
- fallback reason
- status and hints

Observed Windows smoke output from `target\release\td.exe session doctor`:

- `enabled: true`
- `backend setting: auto`
- `resolved backend: powershell-transcript`
- `powershell command: C:\Program Files\PowerShell\7\pwsh.exe`
- `ssh command: C:\Windows\System32\OpenSSH\ssh.EXE`
- `log directory exists: true`
- `log directory writable: true`
- `platform: windows`
- `platform supported: true`
- `Status: ready`

## BIOS-Style Config UI Improvements

The settings diagnostics panel now shows enabled state, backend setting, resolved backend, platform, platform support, PowerShell readiness, `ssh` readiness, `script` readiness, log directory writability, fallback reason, and status. Saving settings reloads diagnostics before returning to the normal status message.

## Docs Updates

Updated README, security docs, TUI docs, internal session logging design, SSH invocation boundary notes, and the release checklist to document:

- default disabled session logging
- Linux/macOS `script` backend
- Windows PowerShell Transcript backend
- `td session doctor` readiness checks
- BIOS-style config UI controls
- transcript-body secret capture risk
- metadata exclusions for auth args, command strings, and private key paths
- Windows backend limitations

## Tests Run

- `cargo fmt --check` - passed
- `cargo test` - passed
- `cargo clippy --all-targets --all-features -- -D warnings` - passed
- `cargo build -p td --release --locked` - passed
- `cargo test -p tdcore session_log -- --nocapture` - passed before full validation
- `cargo test -p tui` - passed before full validation

## Windows Manual Smoke

Run with `target\release\td.exe` on Windows:

- `td.exe session doctor` - passed, backend ready as `powershell-transcript`.
- `td.exe session doctor --json` - passed, JSON includes PowerShell and `ssh` command fields.
- `td.exe session list` - passed, no saved SSH session logs were present.
- `td.exe config schema session.log.backend` - passed, allowed values are `auto`, `script`, `powershell-transcript`, `no-log`.
- `td.exe config get session.log.backend --resolved` - passed, current local setting resolves to `auto`.
- `td.exe config get session.log.enabled --resolved` - passed, current local setting resolves to `true`.
- `td.exe config ui` - non-interactive environment returned `td config ui requires an interactive TTY`.
- `td.exe ui` - non-interactive environment returned `td ui requires an interactive TTY; interactive SSH sessions require a TTY`.
- `td.exe session path <session_id>` - not run because there were no saved session logs.

## Unimplemented

- ConPTY backend.
- tmux integration.
- terminal emulator launch.
- portable-pty recorder.
- Automated tests that connect to a real SSH server.
- Full visual/manual TUI smoke in an interactive terminal.

## Next Stability Improvements

- Run a real interactive Windows SSH smoke against a controlled host and verify the transcript and metadata sidecar after exit.
- Add an authenticated/manual smoke transcript to release evidence for `td config ui`, `td ui`, `td session list`, and `td session path <session_id>`.
- Consider a future ConPTY backend if exact terminal behavior and broader prompt compatibility become required.
