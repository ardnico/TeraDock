# Windows ConPTY Manual Smoke

This checklist is for the explicit Windows-only ConPTY backend and its focused
PoC command:

```powershell
.\target\release\td.exe session conpty-test <profile_id>
.\target\release\td.exe config set session.log.enabled true
.\target\release\td.exe config set session.log.backend conpty
.\target\release\td.exe connect <profile_id> --log-backend conpty
.\target\release\td.exe ui
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

Do not use this checklist to promote ConPTY to `auto` or default session
logging. The goal is to collect enough manual evidence to decide whether the
explicit backend can move from `explicit_ready` to stable.

## Recorded Evidence

2026-06-16 local review found one successful ConPTY CLI session. The detailed
evidence is recorded in `RESULT_TeraDock_WINDOWS_CONPTY_LOGGING_SUCCESS.md`.
2026-06-17 operator smoke reported successful TUI `s` ConPTY logging with
Japanese output. The TUI-specific evidence is recorded in
`RESULT_TeraDock_TUI_CONPTY_LOGGING_SUCCESS.md`, and the TUI edge-case
checklist lives in `docs/internal/windows-tui-conpty-manual-smoke.md`.

Verified from that session:

- SSH login produced remote shell output in the saved ConPTY log.
- The log contains a remote prompt, echoed commands, command output, logout,
  and the connection-close line.
- `td session list`, `td session show`, `td session show --tail`, and
  `td session path` can inspect the saved ConPTY session.
- With `session.log.enabled=true` and `session.log.backend=conpty`, TUI `s`
  can save SSH command history, command output, and Japanese output, then
  return to the TUI.
- Metadata contains safe fields only and does not store auth args, full SSH
  command strings, private key paths, passwords, tokens, or secrets.

Still required before explicit stable backend promotion:

- Ctrl-C remote interrupt evidence from a clean process snapshot.
- Ctrl-C emergency abort evidence from a clean process snapshot.
- Startup timeout failed metadata.
- Bad host failed metadata.
- Auth failure behavior.
- TUI `s` Ctrl-C return-to-screen evidence.
- Broader Windows terminal coverage.

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
- Whether the warning about explicit ConPTY logging and captured terminal
  secrets is shown.
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
- `content_capture=terminal_io`
- `content_capture_reliable=true`
- `backend_status=explicit_ready`
- `backend_warning=conpty_backend_is_explicit_and_not_selected_by_auto`

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
- Common ANSI color, cursor, title, and line-editing escape sequences are
  stripped or normalized in the saved log. The live terminal may still receive
  raw terminal control bytes.
- `td session list`, `show`, and `path` work for the saved session.
- `td session list` keeps the `log_path` column to a log path only; backend
  warnings or notes do not appear in that column.
- `td session show <session_id>` displays backend warnings and capture notes.
- Sessions without a log path show `<none>` in the list/show log path field.
- Metadata has `backend=conpty`.
- Metadata has the expected `exit_code`.
- Metadata has `content_capture=terminal_io`.
- Metadata has `content_capture_reliable=true`.
- Metadata has `backend_status=explicit_ready`.
- Metadata has `backend_warning=conpty_backend_is_explicit_and_not_selected_by_auto`.
- Metadata does not include auth args, full command strings, private key paths,
  passwords, secrets, or tokens.

## Explicit Backend Checks

Verify doctor/config UI before running from normal paths:

```powershell
.\target\release\td.exe config set session.log.enabled true
.\target\release\td.exe config set session.log.backend conpty
.\target\release\td.exe session doctor
.\target\release\td.exe config ui
```

Confirm:

- `backend setting: conpty`
- `resolved backend: conpty`
- `TUI logging: enabled for s-key SSH sessions`
- `ConPTY backend: explicit_ready`
- `Auto selection: deferred`
- warning says ConPTY is explicit and failure cases still require evidence.
- `Status: degraded`
- Diagnostics mention that ConPTY is explicit and not selected by `auto`.

Verify CLI connect:

```powershell
.\target\release\td.exe connect <profile_id> --log-backend conpty
```

Verify TUI:

```powershell
.\target\release\td.exe ui
```

In the TUI, select an SSH profile and press `s`. Confirm remote output is
visible, output is saved, Japanese output is preserved, `exit` returns to the
TUI, and `session list/show/path` can inspect the saved ConPTY session. Follow
the focused TUI checklist in `docs/internal/windows-tui-conpty-manual-smoke.md`
for Ctrl-C, bad host, auth failure, resize, and large output.

If a ConPTY session does not respond after the first forwarded `Ctrl-C`, press
`Ctrl-C` again within 2 seconds to use the emergency abort path. If the TUI
screen or terminal mode is still not restored, reopen the terminal if necessary,
and check for leftover `td` or `ssh` processes from the test before retrying.
Record the recovery steps as part of the smoke evidence.

## Ctrl-C Check

For the shared ConPTY runner, the first Ctrl-C is forwarded to the SSH child as
`0x03`. A second Ctrl-C within 2 seconds is the TeraDock emergency abort. Run a
command that can safely be interrupted:

```sh
sleep 30
```

Press `Ctrl-C` once, then if the remote shell returns:

```sh
echo after-ctrl-c
exit
```

Confirm:

- The remote process stops and the SSH session remains usable.
- The saved log contains `sleep 30` and `after-ctrl-c`.
- Metadata has `status=completed` and `exit_code=0`.
- PowerShell returns after remote `exit`.
- The local terminal accepts input after the test.
- The local terminal is not left in raw mode.
- `Get-Process td,ssh,pwsh,powershell -ErrorAction SilentlyContinue` does not
  show a leftover `td.exe` or `ssh.exe` from the completed ConPTY run.

For emergency abort, run `sleep 30` again and press `Ctrl-C` twice within 2
seconds.

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
- `failure_reason=ctrl_c_double_press`
- `content_capture=terminal_io`
- `content_capture_reliable=true`
- `backend_status=explicit_ready`
- `backend_warning=conpty_backend_is_explicit_and_not_selected_by_auto`

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
- `auto` and default backend selection remain disabled.

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
