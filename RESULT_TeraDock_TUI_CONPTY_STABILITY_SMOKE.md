# TeraDock TUI ConPTY Stability Smoke

Date: 2026-06-18 JST

Scope: Windows TUI `s` SSH sessions with explicit ConPTY session logging only.
This result does not promote Windows `auto -> conpty`.

## Phase 1 Review

Already proven by prior live smoke and current local rechecks:

- Explicit settings are accepted: `session.log.enabled=true` and
  `session.log.backend=conpty`.
- `td session doctor` reports `resolved backend: conpty`,
  `TUI logging: enabled for s-key SSH sessions`, `ConPTY backend:
  explicit_ready`, and `Auto selection: deferred`.
- Windows `auto` remains `no-log` through
  `windows_terminal_content_logging_requires_explicit_conpty`.
- The TUI `s` path suspends the TUI before SSH and resumes it after the SSH
  runner returns.
- The shared ConPTY event loop has separate input, output, child-wait, and
  startup-timeout paths.
- Startup timeout writes failed metadata on the ConPTY path.
- First Ctrl-C is forwarded to the remote PTY as `0x03`; a second Ctrl-C within
  2 seconds is the explicit emergency abort path.
- The output tee writes sanitized log bytes and flushes before writing the raw
  bytes back to stdout.
- Session metadata records ConPTY as `backend=conpty` with
  `content_capture=terminal_io`, `content_capture_reliable=true`, and
  `backend_warning=conpty_backend_is_explicit_and_not_selected_by_auto`.
- Current local `session list/show/path` verification succeeded for
  `sl_x7qxorxv` and `sl_mcx5u7jc`.
- Current local metadata safety scan over those two metadata files found no
  `auth_args`, `command`, `private_key_path`, `password`, `secret`, or `token`
  fields.

Unproven in this pass:

- A fresh interactive resize smoke.
- A fresh `seq 1 5000` large-output smoke.
- A fresh 60-second long-running command interrupted by one Ctrl-C with the
  exact `after_ctrl_c` marker.
- Fresh normal-exit and abort child-cleanup snapshots from the same stability
  run.
- Broader Windows Terminal, PowerShell, terminal-host, and environment
  coverage.

The automation shell used for this pass is not an interactive TTY. The required
setup commands and doctor check ran, but `td ui` exited with:

```text
Error: td ui requires an interactive TTY; interactive SSH sessions require a TTY
```

Because the TUI could not be driven interactively, no source-level terminal
logic fix was made from this pass.

## 1. Resize Smoke

Verdict: CONDITIONAL GO

Evidence:

- Source review confirms resize events are forwarded to ConPTY through the
  shared input bridge, and dimensions are clamped to at least `1x1`.
- No live resize session was run in this non-TTY pass.

Remaining check:

```sh
stty size || true
ls
echo resize_after
exit
```

Accept a documented limitation if resize display is imperfect but the terminal
remains usable, logging continues, `resize_after` is saved, TUI returns, and no
test child remains.

## 2. Large Output Smoke

Verdict: CONDITIONAL GO

Evidence:

- The ConPTY output tee reads chunks, writes sanitized bytes to the log,
  flushes, and then writes raw bytes to stdout.
- No fresh `seq 1 5000` live smoke was run in this non-TTY pass.

Remaining check:

```sh
seq 1 5000
exit
```

The saved log should contain the large output, the TUI should not freeze, and no
test child should remain.

## 3. Long-running Session Smoke

Verdict: CONDITIONAL GO

Evidence:

- Prior single-Ctrl-C smoke proved that one Ctrl-C can interrupt a remote
  command while keeping SSH and logging alive.
- The exact long-running command and `after_ctrl_c` marker were not rerun in
  this non-TTY pass.

Remaining check:

```sh
for i in $(seq 1 60); do date; sleep 1; done
```

Press Ctrl-C once, confirm the remote command stops and the SSH session remains
usable, then run:

```sh
echo after_ctrl_c
exit
```

## 4. Ctrl-C Inside Remote Command

Verdict: CONDITIONAL GO

Evidence:

- The implementation forwards the first Ctrl-C to the PTY as `0x03`.
- Prior live smoke recorded a GO for first-Ctrl-C remote interrupt and continued
  logging.
- This pass did not create a fresh session containing the exact
  `after_ctrl_c` marker.

GO condition for this exact smoke: the saved log contains `after_ctrl_c`,
metadata is `status=completed` with `exit_code=0`, and the TUI returns.

## 5. Normal Exit Child Cleanup

Verdict: CONDITIONAL GO

Evidence:

- Prior normal-exit ConPTY smoke recorded completed metadata and child cleanup.
- This pass did not create a fresh normal-exit TUI session.

Current process attribution:

- `Get-Process td,ssh,pwsh,powershell` showed one existing `ssh.exe`, but its
  start time predates this pass and it cannot be attributed to the attempted
  non-TTY TUI run.

## 6. Abort Child Cleanup

Verdict: GO for prior explicit double-Ctrl-C abort evidence; not rerun here.

Evidence:

- Prior double-Ctrl-C live smoke recorded `status=aborted`,
  `failure_phase=user_abort`, and
  `failure_reason=ctrl_c_double_press`, and found no test-derived `ssh.exe`
  child.
- The local session store used in this pass does not currently contain that
  older session id, so this pass did not independently recheck its metadata.

## 7. UTF-8/Japanese Output

Verdict: GO for prior explicit TUI evidence; not rerun here.

Evidence:

- Prior explicit TUI smoke recorded readable Japanese output in the saved
  ConPTY log.
- Source tests preserve UTF-8/Japanese text through the ConPTY log sanitizer.
- This pass did not run a fresh `echo "日本語テスト"` TUI session.

## 8. Session List/Show/Path Verification

Verdict: GO

Evidence from this pass:

- `td session list --json --limit 20` returned the current local ConPTY
  sessions `sl_x7qxorxv` and `sl_mcx5u7jc`.
- `td session show sl_x7qxorxv` reported `backend=conpty`,
  `status=completed_nonzero`, `exit_code=255`,
  `content_capture=terminal_io`, and `backend_status=explicit_ready`.
- `td session show sl_mcx5u7jc` reported `backend=conpty`, `status=failed`,
  `failure_phase=waiting_initial_output`, and
  `failure_reason=initial_output_timeout`.
- `td session path` returned a log path for both current local sessions.

## 9. Metadata Safety Verification

Verdict: GO for the current local metadata files.

Evidence from this pass:

- The two current local ConPTY metadata files were scanned for `auth_args`,
  `command`, `private_key_path`, `password`, `secret`, and `token`.
- The scan returned no matches.
- The auth-failure log tail contained readable failed-login terminal text; the
  metadata remained free of forbidden auth/secret fields.

Terminal transcript note:

- Log files remain sensitive. They may contain any displayed passwords, tokens,
  secrets, prompt responses, pasted text, or command output.

## 10. Auto Promotion Decision

Verdict: GO for keeping Windows `auto=no-log`; NO-GO for `auto -> conpty`
promotion in this pass.

Reason:

- Explicit ConPTY support is useful and has targeted GO evidence.
- Resize, large-output, long-running, broader cleanup, and broader Windows
  terminal coverage remain incomplete.
- The current diagnostic state is intentionally `explicit_ready` plus degraded
  overall status, not the production default backend.

