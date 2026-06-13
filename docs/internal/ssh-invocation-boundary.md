# SSH Invocation Boundary

This note records the current SSH responsibility boundary.

## Current Boundary

Shared SSH construction logic lives in `tdcore::ssh`:

- Load and validate the requested profile id.
- Reject non-SSH profiles before command construction.
- Resolve the SSH client from profile overrides, global overrides, or `PATH`.
- Load and validate `ssh_auth_order`.
- Detect agent/key availability for CLI/TUI warnings and auth behavior.
- Build SSH auth arguments.
- Build the common `ssh -p <port> <auth options> user@host` argument list.
- Build safe session metadata with caller-provided `source` and `mode`.

The shared builder returns an invocation object. It does not spawn external processes.

Interactive terminal transcript logging lives in `tdcore::session_log`. It decides whether a session should use the default-disabled no-log path, the Linux/macOS `script` backend, the Windows `powershell-transcript` backend, or a no-log fallback. It may build a wrapper command and metadata paths, but it does not own TUI raw mode or process spawning.

## CLI Responsibilities

CLI code keeps command-line behavior and user interaction:

- Parse arguments and format text/JSON output.
- Prompt for critical-profile confirmation.
- Print SSH auth hints and password-fallback warnings.
- Spawn `ssh` for `connect` and `exec`.
- Wrap interactive SSH `connect` with session logging when enabled and supported.
- Pass the core-built SSH client and auth args into CommandSet execution.
- Record operation logs for CLI operations.

## TUI Responsibilities

TUI code keeps UI and terminal behavior:

- Track selection, filters, panes, result tabs, status messages, and confirmation state.
- Convert a selected profile into an SSH session request.
- Spawn the interactive SSH process with inherited stdio.
- Suspend and resume the TUI terminal around interactive SSH sessions.
- Wrap interactive SSH sessions with session logging when enabled and supported.
- Record `ssh_session` results and launch failures using the core-built safe metadata.

Terminal suspend/resume stays in `crates/tui/src/app.rs` because it is coupled to raw mode, alternate screen state, mouse capture, redraw behavior, and the concrete TUI terminal backend. Core must stay usable from CLI and future non-terminal callers.

## Log Metadata Policy

SSH session metadata is intentionally small and safe:

```json
{
  "mode": "interactive",
  "source": "tui",
  "host": "example.com",
  "port": 22,
  "user": "user",
  "profile_type": "ssh"
}
```

The metadata must not include passwords, secret values, tokens, SSH auth arguments, full command strings, or private key paths. Launch failures may add a `launch_error` string, but not the command line that was attempted.

## Current Scope

The full core invocation builder is used by:

- TUI interactive SSH sessions.
- CLI SSH `connect`.
- CLI `exec`.
- CLI `run` setup before calling the existing CommandSet runner.

TUI CommandSet execution, transfer, tunnel, test, and config-apply paths now reuse the same core auth/client resolution helpers where practical, but their full command shapes remain separate.

For v0.1, this means transfer and tunnel are documented release limitations rather than release blockers.

## Future Cleanup Targets

- CommandSet: accept a core SSH invocation or narrower target/client/auth bundle directly in `tdcore::cmdset_runner`.
- Transfer: separate SSH auth/client construction from SCP/SFTP/FTP-specific command shapes.
- Tunnel: move tunnel-specific SSH argument construction behind a dedicated core helper.
- Terminal emulator launch: model as a caller-owned launch strategy around a core invocation.
- Tmux integration: model as a caller-owned launch strategy that never changes safe metadata policy.
