# CommandSet Execution Boundary

This note records the current boundary after the productization pass.

## Current Boundary

Shared core logic lives in `tdcore::cmdset_runner`:

- Load the target profile and CommandSet.
- Enforce that `td run` currently supports SSH profiles only.
- Execute steps in stored order.
- Apply per-step `timeout_ms`.
- Apply parser specs through `tdcore::parser`.
- Apply `on_error=stop|continue`.
- Capture stdout, stderr, parsed output, exit codes, and durations.
- Update `profiles.last_used_at`.
- Append an `op_logs` row with CommandSet metadata.

CLI and TUI now call this same core function for CommandSet execution.

## Remaining CLI/TUI Responsibilities

CLI still owns:

- Argument parsing and text/JSON output formatting.
- Critical profile confirmation.
- SSH client resolution and SSH authentication option messages.
- Streaming step stdout/stderr to the terminal for non-JSON runs.

TUI still owns:

- Profile and CommandSet selection.
- Marked profile state for bulk runs.
- Critical confirmation input state.
- Result tab state and summary display.
- Command preview rendering with masked sensitive tokens.

## Known Follow-Ups

- Move SSH auth order parsing/building into core to remove the remaining duplication between CLI and TUI.
- Add first-class `td cmdset add/list/show/rm` commands instead of relying on samples, import JSON, or direct DB-backed tooling.
- Add a small executor abstraction so timeout tests can avoid spawning shell scripts.
- Consider recording timeout failures in `op_logs`; the current behavior matches the previous implementation and returns before logging.
- Add a profile-add interactive flow once the CLI validation shape is adjusted without weakening non-interactive errors.
