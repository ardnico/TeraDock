# Windows ConPTY Event Loop Stabilization Result

Date: 2026-06-14

## Summary

Stabilized the explicit Windows-only ConPTY PoC:

```powershell
td session conpty-test <profile_id>
td session conpty-test <profile_id> --debug --startup-timeout-sec 10
```

This pass did not promote ConPTY to `auto`, did not make it a default backend,
and did not integrate it with the TUI.

## 2026-06-15 Follow-Up

The real stalled run wrote only four bytes to the log before hanging:
`ESC [ 6 n`. This is a cursor-position terminal query, not an SSH banner or
prompt. The previous event loop treated any first byte as first output, so this
control-only sequence disabled startup timeout. The child then waited for a
terminal response that TeraDock was not forwarding through the crossterm event
input path.

Fixes added in this follow-up:

- Detect startup-visible output separately from terminal control-only output.
- Keep startup timeout armed when the only output is `ESC[6n` or similar
  control traffic.
- Detect `ESC[6n` and send a synthetic cursor-position response back into the
  PTY.
- Detect `ESC[5n` and send a synthetic device-status-ok response.
- Make the startup timer a watchdog that kills the child directly if no
  startup-visible output arrives before the deadline.
- Add a main-loop `recv_timeout` fallback so an empty event stream cannot wait
  forever.
- Add tests for cursor-position query detection, split query parsing, and
  visible prompt detection.

## Phase 1 Findings

- The previous main loop could wait on three independent paths: child wait
  polling, input/control polling, and an AtomicBool-based initial-output check.
  It did not receive first output as a real event.
- Startup timeout could be weakened by cleanup ordering: timeout was detected,
  but cleanup then depended on worker shutdown and child wait timing.
- Ctrl-C abort could leave `ssh.exe` briefly visible because child kill was not
  followed by a long enough child-wait confirmation. The old best-effort wait
  was 1 second, while the wait thread did not report shutdown timeout until 2
  seconds.
- Output reader could mark first output in an AtomicBool, but it could not send
  `FirstOutput` to the main loop.
- Input bridge startup was debug-visible, but Ctrl-C was reported as a local
  bridge message rather than a main-loop `UserAbort` decision.
- `session list` already tried to filter warning-like log paths, but no-log
  display used `-`; the requested stable value is now `<none>`.

## Changes

- Added a `ConptyEvent` event path for:
  - `FirstOutput`
  - `OutputChunk`
  - `ChildExited`
  - `StartupTimeout`
  - `UserAbort`
  - `OutputError`
  - `InputError`
- Added a startup timer worker that sends `StartupTimeout` to the main loop.
- Changed output reader startup to send `FirstOutput` on the first non-empty
  read and `OutputChunk` for each chunk.
- Changed child wait to send `ChildExited` with the sanitized exit code.
- Changed input bridge Ctrl-C handling to send `UserAbort`.
- Centralized timeout, abort, output error, input error, and child wait error
  decisions in the main event loop.
- Switched cleanup diagnostics to:
  - `debug: user abort received`
  - `debug: killing child`
  - `debug: child killed`
  - `debug: dropping pty handles`
  - `debug: joining threads best-effort`
  - `debug: terminal restored`
- Added debug metadata write result reporting for success and failure paths.
- Increased child wait cleanup confirmation to 3 seconds and warn when child
  exit cannot be confirmed.
- Changed missing or invalid `log_path` display to `<none>`.
- Updated `docs/internal/windows-conpty-manual-smoke.md`.

## Startup Timeout

Default startup timeout remains 10 seconds. `--startup-timeout-sec <N>` still
overrides it, and `--startup-timeout-sec 0` disables it.

If no first output byte arrives before the timeout, the main loop now receives
`StartupTimeout`, kills the child, requests worker shutdown, drops PTY handles,
flushes/finishes the log path best-effort, restores the terminal, writes
metadata, and returns to PowerShell.

Expected metadata:

```json
{
  "backend": "conpty",
  "status": "failed",
  "failure_phase": "waiting_initial_output",
  "failure_reason": "initial_output_timeout"
}
```

## Ctrl-C Cleanup

Ctrl-C is still treated as a TeraDock emergency abort for this PoC. The input
bridge sends `UserAbort`; the main loop kills the child, signals shutdown,
best-effort joins workers, restores terminal mode, writes aborted metadata, and
warns if child exit cannot be confirmed.

Expected metadata:

```json
{
  "backend": "conpty",
  "status": "aborted",
  "failure_phase": "user_abort",
  "failure_reason": "ctrl_c"
}
```

## Session List

`td session list` now keeps `log_path` to a path-shaped value only. Backend
warnings and capture notes remain in `td session show`. Missing or invalid log
paths render as `<none>`.

## Debug Output

`--debug` now covers child spawn, output reader start, input bridge start,
child wait start, startup timeout arming, first output, user abort, child kill,
child exit, thread shutdown status, terminal restore, and metadata write
result. It still does not print auth args, full command strings, private key
paths, full environment dumps, passwords, tokens, or secrets.

## Manual Smoke

Real SSH manual smoke was not run automatically. This is intentional because
the request prohibits automated real-server SSH tests and ConPTY logs can
capture terminal secrets.

Updated manual smoke now includes:

- `--debug --startup-timeout-sec 10`
- confirmation that no-output startup returns in about 10 seconds
- Ctrl-C abort process checks with `Get-Process td,ssh,pwsh,powershell`
- failed/aborted metadata checks through `td session show`
- `td session list` layout and `log_path` checks

## Validation

Passed:

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
.\target\release\td.exe session conpty-test --help
.\target\release\td.exe session list --limit 5
```

The local non-SSH list smoke showed the `log_path` column containing only the
saved log path for the latest ConPTY metadata row. Real SSH smoke was not run.

2026-06-15 follow-up validation:

- Rebuilt `target\release\td.exe`.
- Reproduced the stale stuck process/log pattern from the reported command.
- Confirmed the original stalled log contained only `ESC[6n`.
- Confirmed the fixed build responds to the cursor-position query and captures
  Ubuntu MOTD plus a remote shell prompt in the ConPTY log.
- The automated/non-interactive smoke was then stopped manually because it had
  reached the remote shell prompt and was waiting for input.

## Not Done

- No ConPTY `auto` or default-backend promotion.
- No TUI integration.
- No full terminal emulator.
- No automated real-server SSH test.
- No PowerShell Transcript reliability change.
- No storage of auth args, full command strings, private key paths, passwords,
  tokens, secrets, or full environment dumps.

## Next-Step Judgment

Next step is manual Windows smoke against a controlled SSH profile. If
`--debug --startup-timeout-sec 10` still stays at `Waiting for SSH output...`
for more than 10 seconds, collect the debug lines and session metadata from
this event-loop build before widening scope.
