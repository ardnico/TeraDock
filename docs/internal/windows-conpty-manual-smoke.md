# Windows ConPTY Manual Smoke

This checklist is for the explicit Windows-only ConPTY proof of concept:

```powershell
.\target\release\td.exe session conpty-test <profile_id>
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
- Whether login succeeds.
- Any auth prompt behavior.

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
Get-Content <log_path>
Get-Content <metadata_path>
```

Confirm:

- Remote output is visible in the local terminal.
- The same remote output appears in the log file.
- Typed commands appear in the log only when the remote side echoes them.
- ANSI escape sequences are acceptable as preserved terminal bytes.
- `td session list`, `show`, and `path` work for the saved session.
- Metadata has `backend=conpty`.
- Metadata has the expected `exit_code`.
- Metadata has `content_capture=best_effort`.
- Metadata has `content_capture_reliable=false`.
- Metadata has `backend_warning=conpty_backend_is_experimental_poc`.
- Metadata does not include auth args, full command strings, private key paths,
  passwords, secrets, or tokens.

## Ctrl-C Check

Run a harmless remote command that can be interrupted:

```sh
sleep 30
```

Press `Ctrl-C`.

Confirm:

- The remote command is interrupted.
- The SSH session remains usable or exits in an understandable way.
- The local terminal accepts input after the test.
- The local terminal is not left in raw mode.
- A later `exit` returns to PowerShell.

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
- Metadata is saved only when the ConPTY child ran far enough to create a
  session log.
- The terminal is usable after failure.
- No child `ssh.exe` process or output thread remains after exit.

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
