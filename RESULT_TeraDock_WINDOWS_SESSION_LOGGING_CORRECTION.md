# TeraDock Windows Session Logging Correction

## Phase 1 Findings

- PowerShell Transcript records the PowerShell host transcript. It is not a PTY recorder for external interactive `ssh.exe`, so SSH-side commands and remote shell output can be missing.
- The previous Windows `auto` path resolved to `powershell-transcript` when PowerShell and `ssh` were available, and diagnostics marked it `ready`. That could make host-only logs look successfully captured.
- Existing metadata did not distinguish reliable terminal-content capture from best-effort host transcript capture.
- `td session show` did not surface a capture warning when a saved PowerShell transcript appeared host-only or empty.
- ConPTY is the correct future design boundary for reliable Windows SSH terminal input/output capture, but it is intentionally not implemented in this slice.

## Changes

- Changed Windows `session.log.backend=auto` to resolve to `no-log`.
- Added fallback reason `windows_terminal_content_logging_requires_conpty`.
- Kept explicit `session.log.backend=powershell-transcript` on Windows, but marked it `degraded` and best-effort.
- Added diagnostics fields for content-capture reliability and warnings.
- Added PowerShell Transcript metadata:
  - `content_capture=best_effort`
  - `content_capture_reliable=false`
  - `backend_warning=powershell_transcript_may_not_capture_interactive_ssh_io`
- Added host-only/empty transcript detection for PowerShell logs.
- Added `content_capture_status=host_only_or_empty` and `content_capture_warning` when no SSH terminal content appears to have been captured.
- Updated `td session show` to print capture fields and the host-only warning.
- Updated the BIOS-style settings diagnostics panel with status, capture reliability, warning, and a human-readable ConPTY reason.

## PowerShell Transcript Re-evaluation

`powershell-transcript` remains available only when the user explicitly selects it. It is now treated as experimental best-effort because it may capture only PowerShell transcript headers and host metadata, not interactive SSH input/output.

## Auto Backend Resolution

- Linux/macOS `auto`: still uses `script` when available.
- Windows `auto`: now resolves to `no-log`.
- Windows `auto` fallback reason: `windows_terminal_content_logging_requires_conpty`.

## Docs Updated

- `README.md`
- `docs/security.md`
- `docs/tui.md`
- `docs/internal/session-logging-design.md`
- `docs/internal/ssh-invocation-boundary.md`
- `docs/internal/windows-conpty-session-logging-design.md`
- `ROADMAP.md`
- `RELEASE_CHECKLIST.md`

## ConPTY Design Summary

Added `docs/internal/windows-conpty-session-logging-design.md`. It defines the future Windows backend as a ConPTY/Pseudo Console wrapper around `ssh.exe`, teeing terminal I/O to the user terminal and a log file while handling resize, Ctrl-C, encoding, and exit code propagation.

## Tests And Validation

Passed:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

Focused coverage added:

- Windows `auto` resolves to `no-log` with ConPTY fallback reason.
- Windows `auto` does not depend on PowerShell or `ssh`.
- Explicit PowerShell Transcript reports `degraded` and `best_effort`.
- PowerShell metadata records `content_capture_reliable=false`.
- Host-only PowerShell transcripts get `content_capture_status=host_only_or_empty`.
- `td session show` capture lines include the host-only warning.
- Config UI displays the ConPTY fallback reason in human-readable form.

## Not Implemented

- ConPTY backend implementation.
- Full terminal emulator or replay format.
- Secret masking for terminal output.
- Real SSH server integration tests.

## Next Steps

- Build a ConPTY proof of concept for Windows SSH terminal logging.
- Validate resize, Ctrl-C, UTF-8/Windows encoding, and exit code behavior.
- Decide whether ConPTY can become the production Windows backend in a later release.
