# Windows ConPTY PoC Stabilization Result

## Summary

Stabilized the explicit Windows-only ConPTY proof of concept:

```powershell
td session conpty-test <profile_id>
td session conpty-test <profile_id> --debug
```

This pass did not promote ConPTY to `auto`, did not make it a default backend,
and did not integrate it with the TUI `s` SSH-session path.

## Phase 1 Findings

- `TRACE adding SYS env` came from dependency tracing during `portable-pty`
  environment propagation while spawning the ConPTY child.
- It appeared in normal execution because CLI logging initialized a stderr
  tracing layer without an explicit filter, so TRACE events from dependencies
  were accepted.
- The likely hang points before visible SSH output were PTY open, reader/writer
  creation, raw-mode entry, child spawn, output reader startup, input bridge
  event polling, child wait, or a child that started but produced no bytes.
- Failure metadata previously lacked structured `failure_phase` and
  `failure_reason`, and ConPTY launch failures could return without a saved
  session metadata sidecar.
- The minimal scope was logging filter hardening, sanitized debug output,
  startup phase messages, initial-output timeout warning, bridge diagnostics,
  and ConPTY failure metadata.

## Changes

- Added `--debug` to `td session conpty-test <profile_id>`.
- Added `TERADOCK_DEBUG=1` support for the same sanitized debug path.
- Added tracing filters so normal CLI execution does not emit dependency TRACE
  output such as environment propagation dumps.
- Kept debug output sanitized:
  - profile id
  - resolved SSH client path
  - backend
  - log path
  - child spawn phase
  - output reader/input bridge/child wait startup
  - first output byte count
  - exit or failure phase
- Did not print SSH auth args, full command strings, private key paths,
  passwords, tokens, secrets, or full environment dumps.

## Startup And Timeout

Normal ConPTY startup now prints phase-oriented status before entering the
interactive bridge:

```text
ConPTY session logging PoC is experimental.
ConPTY is not selected by auto and is not integrated with the TUI.
Starting ConPTY SSH session...
Profile: <profile_id> (<user>@<host>:<port>)
Log path: <path>
Spawning <ssh-client-name>...
Waiting for SSH output...
```

If no ConPTY output byte is observed within 10 seconds, TeraDock warns:

```text
Warning: no ConPTY output received for 10 seconds.
SSH may be waiting for input, blocked, or the output bridge may be stuck.
Press Ctrl-C to abort.
```

The timeout path is warning-first. If the child later exits without any ConPTY
output, metadata is saved with:

```json
{
  "status": "failed",
  "failure_phase": "waiting_initial_output",
  "failure_reason": "initial_output_timeout",
  "content_capture": "best_effort",
  "content_capture_reliable": false,
  "backend_warning": "conpty_backend_is_experimental_poc"
}
```

After the initial-output timeout warning, pressing Ctrl-C aborts the local PoC
and saves metadata with:

```json
{
  "status": "aborted",
  "failure_phase": "user_abort",
  "failure_reason": "ctrl_c"
}
```

After initial output has appeared, Ctrl-C continues to be forwarded to the
ConPTY child as `0x03`.

## Metadata

Added optional metadata fields:

- `failure_phase`
- `failure_reason`

Added ConPTY failure metadata helpers for spawn, timeout, input bridge, output
bridge, child wait, raw-mode, PTY-open, log-create, and user-abort paths. The
same ConPTY metadata exclusions remain in force: no auth args, full command
strings, private key paths, passwords, secrets, tokens, or full environment.

`td session show <session_id>` now prints `failure_phase` and `failure_reason`
when present.

## Bridge Stabilization

- Output reader startup is visible in debug mode.
- First-output byte count is visible in debug mode.
- Output bytes are flushed to both stdout and the log file.
- Input bridge startup and child wait startup are visible in debug mode.
- Resize events still forward to the PTY.
- Expected EOF-style output reader shutdown errors are treated as normal end of
  stream.
- Raw mode is still guarded and restored on normal exit and failure cleanup.
- On input bridge failure or timeout abort, the child is killed/waited, PTY
  handles are dropped, and the output thread is joined.

## Manual Smoke

Updated `docs/internal/windows-conpty-manual-smoke.md` with:

- `--debug` execution.
- Initial-output-timeout evidence to paste.
- Ctrl-C abort metadata checks.
- `failure_phase` / `failure_reason` checks.
- Log body checks.
- Normal/debug trace and environment-dump absence checks.

Manual SSH smoke against a real profile was not run by this automated pass
because it can require interactive authentication and can capture sensitive
terminal output in session logs. The updated checklist should be used on a
controlled Windows SSH target.

## Validation

Passed:

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
.\target\release\td.exe session conpty-test --help
```

The help output includes:

```text
Usage: td.exe session conpty-test [OPTIONS] <PROFILE_ID>
      --debug  Print sanitized ConPTY PoC startup and bridge phase diagnostics
```

## Not Done

- No `auto -> conpty` promotion.
- No TUI integration.
- No full terminal emulator.
- No automated real-server SSH test.
- No storage of auth args, full command strings, private key paths, passwords,
  tokens, secrets, or full environment data in metadata/debug output.

## Next Steps

1. Run the updated manual smoke on a controlled Windows SSH target.
2. Capture both normal and `--debug` output when the initial-output hang
   reproduces.
3. Inspect saved metadata and log files for `failure_phase`,
   `failure_reason`, first-output behavior, and secret-free metadata.
4. Decide whether the next stabilization step should make timeout handling
   interactive, for example prompt to continue or abort after the 10-second
   warning.
