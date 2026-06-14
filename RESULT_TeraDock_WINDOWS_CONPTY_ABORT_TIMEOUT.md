# Windows ConPTY Abort / Timeout Stabilization Result

Date: 2026-06-14

## Summary

Stabilized the explicit Windows-only ConPTY PoC:

```powershell
td session conpty-test <profile_id>
td session conpty-test <profile_id> --debug
td session conpty-test <profile_id> --startup-timeout-sec 10
```

This pass did not promote ConPTY to `auto`, did not make it a default backend,
and did not integrate it with the TUI `s` path.

## Phase 1 Findings

After `Waiting for SSH output...`, the old path could hang in these places:

- The child had spawned but produced no ConPTY output bytes.
- The input bridge loop stayed alive while Ctrl-C was forwarded as `0x03` to
  SSH instead of treated as a parent abort.
- The child wait path used polling, but abort cleanup could still call blocking
  child wait.
- The output reader thread used a blocking PTY read and the parent used an
  unbounded `join()`.
- PTY handles were dropped only after child termination/failure paths reached
  cleanup, so a wedged child could keep the reader blocked.

Ctrl-C did not reliably return to PowerShell because the ConPTY PoC entered raw
mode and only treated Ctrl-C as local abort after the old initial-output timeout
warning. Before that point, and after output had appeared, Ctrl-C was forwarded
to the child. If the child or PTY bridge was wedged, forwarding Ctrl-C was not a
reliable parent-process escape hatch.

Thread join could wait forever on the output reader because it was blocked in
`read()` and the join had no timeout. Child cleanup was also too dependent on a
blocking wait after kill. Raw-mode restoration existed, but it was sequenced
after some cleanup that could block. `session list` already preferred
`metadata.log_path`, but malformed/old metadata or overlong fields could make a
warning-like value appear visually in the `log_path` column.

During validation, the old pre-fix run was still present as a stale
`target\release\td.exe session conpty-test p_ojql3dws` process with `ssh.exe`
and headless `conhost.exe` children. It locked `target\release\td.exe` and
blocked the first release build attempt. That stale process tree was stopped
before rerunning the build.

## Changes

- Added `--startup-timeout-sec <N>` to `td session conpty-test`.
- Default startup timeout is 10 seconds.
- `--startup-timeout-sec 0` disables the startup timeout.
- Ctrl-C in the ConPTY PoC is now always a TeraDock emergency abort.
- Timeout before the first output byte now aborts the child immediately instead
  of printing a warning and waiting forever.
- The runner now uses a cancellation signal plus separate output reader, input
  bridge, and child wait workers.
- Child wait uses `try_wait()` polling instead of an unbounded blocking wait.
- Shutdown uses bounded, best-effort joins and restores raw mode before slow
  output/wait cleanup can block the terminal.
- Debug output remains sanitized and includes startup, first-output, timeout,
  abort, kill, handle-close, metadata-write, and terminal-restore phases.
- `session list` now bounds fixed table cells and displays only real
  log-path-shaped values in the `log_path` column.
- `session show` remains the place for backend warnings and capture notes.
- Updated `docs/internal/windows-conpty-manual-smoke.md`.

## Emergency Abort

Ctrl-C now triggers:

- parent-side abort signal
- child kill through `portable-pty` child killer
- cancellation signal for input/wait workers
- PTY writer/master drop through input worker shutdown
- bounded wait/output joins
- raw-mode restore
- failure metadata write with `status=aborted`

Expected abort metadata:

```json
{
  "backend": "conpty",
  "status": "aborted",
  "failure_phase": "user_abort",
  "failure_reason": "ctrl_c",
  "content_capture": "best_effort",
  "content_capture_reliable": false,
  "backend_warning": "conpty_backend_is_experimental_poc"
}
```

`exit_code` may be `null` when the abort path cannot reliably acquire it.

## Startup Timeout

If no ConPTY output byte arrives within the configured timeout, TeraDock prints:

```text
Error: no ConPTY output received within 10 seconds.
Aborting ConPTY child...
Session metadata saved with status=failed.
```

Expected timeout metadata:

```json
{
  "backend": "conpty",
  "status": "failed",
  "failure_phase": "waiting_initial_output",
  "failure_reason": "initial_output_timeout"
}
```

## Thread Shutdown

- Output reader still reads to EOF on normal child exit so tail output is not
  dropped.
- Abort/timeout paths kill the child and request input/wait shutdown.
- Input bridge uses short polling and exits on cancellation.
- Child wait worker uses short polling and reports timeout if the child does not
  exit after shutdown request.
- Join waits are bounded and debug-only shutdown failures do not prevent terminal
  restore.
- Log chunks are flushed as they are written.

## Session List

`td session list` now:

- Displays `-` when there is no usable log path.
- Displays only absolute paths or `.log`-shaped relative paths in `log_path`.
- Keeps backend warnings and capture notes out of the `log_path` column.
- Truncates fixed-width table cells to reduce table breakage.

Non-SSH smoke confirmed the existing saved ConPTY session now displays:

```text
sl_5ryr7np3 ... completed exit 0   C:\Users\leafs\AppData\Roaming\TeraDock\session-logs\sl_5ryr7np3.log
```

## Manual Smoke

Not run automatically against the real SSH server. This was intentional: the
request prohibits automated real-server SSH tests, and ConPTY logs can capture
terminal secrets.

Updated manual smoke coverage now includes:

- Ctrl-C abort confirmation.
- Startup timeout confirmation.
- Failure metadata confirmation.
- `session list` log path display confirmation.
- `--debug` confirmation.
- PowerShell return after abort.
- Terminal mode recovery after abort/failure.

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

Notes:

- The first `cargo build -p td --release --locked` failed because the old stuck
  `td.exe` process locked `target\release\td.exe`.
- After stopping that stale `td.exe`, `ssh.exe`, and `conhost.exe` process tree,
  the release build passed.
- The non-SSH CLI smoke confirmed the new `--startup-timeout-sec` help text and
  the corrected `session list` log path column.

## Not Done

- No ConPTY `auto` or default-backend promotion.
- No TUI integration.
- No full terminal emulator.
- No automated real-server SSH test.
- No secret masking for terminal output.
- No storage of SSH auth args, full command strings, private key paths,
  passwords, tokens, secrets, or full environment dumps in metadata/debug output.

## Next Steps

1. Run `docs/internal/windows-conpty-manual-smoke.md` on a controlled Windows
   SSH profile.
2. Verify `--startup-timeout-sec 10` produces failed timeout metadata when the
   first byte never arrives.
3. Verify `--debug --startup-timeout-sec 0` plus Ctrl-C returns to PowerShell
   and writes aborted metadata.
4. Confirm no `ssh.exe` or headless `conhost.exe` children survive abort.
5. Only after that evidence, decide whether another explicit-CLI ConPTY hardening
   pass is needed. Do not promote to `auto` or integrate with TUI before that.
