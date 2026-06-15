# Windows ConPTY Manual Smoke

This checklist is for the explicit Windows-only ConPTY proof of concept:

```powershell
.\target\release\td.exe session conpty-test <profile_id>
```

For sanitized startup diagnostics:

```powershell
.\target\release\td.exe session conpty-test <profile_id> --debug
```

The startup timeout defaults to 10 seconds. Use this only for controlled
timeout/abort checks:

```powershell
.\target\release\td.exe session conpty-test <profile_id> --startup-timeout-sec 10
.\target\release\td.exe session conpty-test <profile_id> --debug --startup-timeout-sec 10
.\target\release\td.exe session conpty-test <profile_id> --debug --startup-timeout-sec 0
```

Do not use this checklist to promote ConPTY to `auto`, default session logging,
or the TUI `s` path. The goal is to collect enough manual evidence to decide
whether the PoC can continue.

## Prerequisites

- Windows 10/11 with ConPTY support.
- OpenSSH `ssh.exe` available through PATH or TeraDock client override.
- A controlled SSH profile that can be used for manual testing.
- A terminal that can be restored if the test fails.
- `cargo build -p td --release --locked` has completed.
- No passwords, tokens, private keys, or sensitive command output will be typed
  or displayed during the test unless the resulting logs can be securely
  reviewed and deleted.

## Profile Preparation

Create or choose an SSH profile:

```powershell
.\target\release\td.exe profile list
.\target\release\td.exe profile show <profile_id>
```

Confirm:

- `type` is `ssh`.
- The host is controlled test infrastructure.
- The profile does not require exposing sensitive auth material in terminal
  output.
- Critical profiles are tested only when the typed confirmation is expected.

## Run Command

Start the PoC:

```powershell
.\target\release\td.exe session conpty-test <profile_id>
```

Record:

- Full command used.
- Whether the warning about experimental ConPTY logging is shown.
- The printed log path.
- The startup phase lines through `Waiting for SSH output...`.
- Whether login succeeds.
- Any auth prompt behavior.
- Confirm normal output does not include `TRACE adding SYS env`, full
  environment variables, PATH dumps, auth arguments, full command strings,
  private key paths, passwords, tokens, or secrets.

Run the sanitized debug path:

```powershell
.\target\release\td.exe session conpty-test <profile_id> --debug
.\target\release\td.exe session conpty-test <profile_id> --debug --startup-timeout-sec 10
```

Record only these debug categories if they appear:

- selected profile id
- resolved SSH client path
- backend
- log path
- child spawn phase
- child spawned
- output reader started
- input bridge started
- child wait started
- startup timeout armed
- terminal query detected
- synthetic terminal response sent
- first output received or not
- startup watchdog killing child
- user abort received
- killing child
- child killed
- child exited
- dropping pty handles
- joining threads best-effort
- thread shutdown status
- writing aborted metadata
- metadata write result
- terminal restored
- exit phase or failure phase

Confirm debug output still does not include SSH auth args, a full command
string, private key paths, passwords, tokens, secrets, or a full environment
dump.

## Initial Output Timeout Check

If no ConPTY output appears for the configured startup timeout, TeraDock should
abort the child rather than waiting forever. With the default 10 seconds,
TeraDock should print:

```text
Error: no ConPTY output received within 10 seconds.
Aborting ConPTY child...
Session metadata saved with status=failed.
```

The command must return to PowerShell in about 10 seconds when no first output
byte is received. Treat a run that stays at `Waiting for SSH output...` longer
than the configured timeout as a failure.

If this happens, paste into the smoke report:

- The startup phase lines.
- Whether debug had reached `output reader started`, `input bridge started`,
  and `child wait started`.
- Whether debug printed `terminal query detected: cursor_position` and
  `synthetic terminal response sent: cursor_position`.
- Whether any `first output received: N bytes` debug line appeared.
- Whether `ssh.exe` remained running after abort or exit.
- Whether PowerShell accepts input immediately after the abort.
- Whether the terminal mode is restored.
- The saved metadata JSON.

Metadata should include:

- `status=failed`
- `failure_phase=waiting_initial_output`
- `failure_reason=initial_output_timeout`
- `content_capture=best_effort`
- `content_capture_reliable=false`
- `backend_warning=conpty_backend_is_experimental_poc`

## Commands To Type On The SSH Host

Use harmless commands whose output can be kept in a test log:

```sh
printf 'teradock-conpty-smoke\n'
printf 'utf8: 日本語\n'
printf 'ansi: \033[31mred\033[0m\n'
stty size || true
exit 7
```

If `exit 7` would disrupt the remote shell policy, use `exit` and record the
expected exit code instead.

## Confirmation Items

After the session exits:

```powershell
.\target\release\td.exe session list
.\target\release\td.exe session show <session_id>
.\target\release\td.exe session show <session_id> --tail 50
.\target\release\td.exe session path <session_id>
Get-Process td,ssh,pwsh,powershell -ErrorAction SilentlyContinue
Get-Content <log_path>
Get-Content <metadata_path>
```

Confirm:

- Remote output is visible in the local terminal.
- The same remote output appears in the log file.
- Typed commands appear in the log only when the remote side echoes them.
- ANSI escape sequences are acceptable as preserved terminal bytes.
- `td session list`, `show`, and `path` work for the saved session.
- `td session list` keeps the `log_path` column to a log path only; backend
  warnings or notes do not appear in that column.
- `td session show <session_id>` displays backend warnings and capture notes.
- Sessions without a log path show `<none>` in the list/show log path field.
- Metadata has `backend=conpty`.
- Metadata has the expected `exit_code`.
- Metadata has `content_capture=best_effort`.
- Metadata has `content_capture_reliable=false`.
- Metadata has `backend_warning=conpty_backend_is_experimental_poc`.
- Metadata does not include auth args, full command strings, private key paths,
  passwords, secrets, or tokens.

## Ctrl-C Check

For this PoC, Ctrl-C is TeraDock emergency abort, not a key to forward to the
SSH child. Run a command that can safely be killed or use a test profile that
stalls before first output:

```sh
sleep 30
```

Press `Ctrl-C`.

Confirm:

- TeraDock prints abort/shutdown diagnostics in `--debug` mode.
- The ConPTY child is killed.
- PowerShell returns without needing to close the terminal window.
- The local terminal accepts input after the test.
- The local terminal is not left in raw mode.
- `td session list` and `td session show <session_id>` can inspect the aborted
  session metadata.
- `Get-Process td,ssh,pwsh,powershell -ErrorAction SilentlyContinue` does not
  show a leftover `td.exe` or `ssh.exe` from the aborted ConPTY run. Existing
  parent PowerShell processes are expected.

Metadata should include:

- `status=aborted`
- `failure_phase=user_abort`
- `failure_reason=ctrl_c`
- `content_capture=best_effort`
- `content_capture_reliable=false`
- `backend_warning=conpty_backend_is_experimental_poc`

If `exit_code` is unavailable after abort, `null` is acceptable.

## Resize Check

While connected:

1. Make the terminal narrower.
2. Make the terminal wider.
3. Run:

```sh
stty size || true
printf 'resize-check\n'
```

Confirm:

- Display does not become unusable.
- New output remains visible.
- New output continues to be logged.

Known PoC constraint: resize is forwarded only when crossterm reports a resize
event, and the log is terminal bytes rather than a replayable terminal state.

## UTF-8 Check

Run:

```sh
printf 'Japanese: 日本語\n'
printf 'Symbols: ✓ Ω\n'
```

Confirm:

- The local terminal displays the text acceptably.
- The log file preserves the bytes well enough for review in a UTF-8-aware
  editor.
- Any mojibake is recorded with the terminal host, code page, and remote locale.

## Failure Case Check

Use controlled negative cases only:

- Bad host profile or temporary host override.
- Auth failure against a test account.
- User abort at auth prompt.

Confirm:

- The error is visible.
- The process exits or can be exited cleanly.
- Metadata is saved for spawn, timeout, and abort failures when the metadata
  sidecar can be written.
- Failure metadata includes `failure_phase` and `failure_reason`.
- The terminal is usable after failure.
- No child `ssh.exe` process or output thread remains after exit.
- Thread shutdown failures are reported only as sanitized debug/status text and
  do not prevent terminal restoration.
- Metadata does not include auth args, full command strings, private key paths,
  passwords, secrets, tokens, or a full environment dump.

## GO / NO-GO Criteria

GO:

- SSH login works.
- Remote output is visible locally.
- Remote output is written to the log.
- `exit` returns to PowerShell.
- `exit_code` is written to metadata.
- Metadata excludes secrets and SSH invocation details.
- `Ctrl-C` does not break the local terminal.

Conditional GO:

- Basic logging works, but resize, UTF-8, or some keys have documented
  constraints.
- The PoC remains explicit and Windows-only.
- `auto`, default backend selection, and TUI integration remain disabled.

No-Go:

- Remote output is not captured.
- Input is unstable.
- Auth prompts are unusable.
- Exit leaves the terminal broken.
- `Ctrl-C` makes the terminal unrecoverable.
- Metadata contains secrets, auth args, full command strings, or private key
  paths.
- Normal or debug output includes `TRACE adding SYS env` or a full environment
  dump.
