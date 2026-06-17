# Windows TUI ConPTY Manual Smoke

This checklist is for Windows `td ui` with explicit ConPTY session logging:

```powershell
td config set session.log.enabled true
td config set session.log.backend conpty
td session doctor
td ui
```

Do not use this checklist to promote `auto -> conpty`. The current goal is to
prove explicit ConPTY stability for TUI `s` SSH sessions and to collect the
failure-case evidence required before any later auto-selection decision.

## Current Evidence

The 2026-06-17 operator smoke reported the normal TUI path as successful:

- Windows explicit settings: `session.log.enabled=true`,
  `session.log.backend=conpty`.
- `td ui` opened, an SSH profile was selected, and `s` started the session.
- SSH connected.
- Remote command history and command output were saved to the ConPTY log.
- Japanese output was saved.
- The TUI returned after `exit`.
- `td session list`, `td session show`, and `td session path` confirmed the
  saved session.

This is a `GO` for the explicit normal path only:

```text
GO: explicit conpty backend works for TUI normal SSH session
```

`auto` remains deferred until the edge cases below have recorded evidence.

## Normal Exit Code and Cleanup

Use this case when validating exit-code propagation and child cleanup for a
controlled SSH profile:

```powershell
td config set session.log.enabled true
td config set session.log.backend conpty
td ui
```

In the TUI, select the SSH profile and press `s`. On the remote shell, run:

```sh
echo normal-exit-test
exit
```

After the TUI returns, run:

```powershell
td session list
td session show <session_id>
td session path <session_id>
Get-Process td,ssh,pwsh,powershell -ErrorAction SilentlyContinue
```

Expected:

- Metadata has `backend=conpty`.
- Metadata has `status=completed`.
- Metadata has `exit_code=0`.
- Metadata has `content_capture=terminal_io`.
- Metadata has `content_capture_reliable=true`.
- The saved log contains `normal-exit-test`.
- No extra `ssh.exe` child from the test remains.
- Metadata excludes auth args, full command strings, private key paths,
  passwords, secrets, and tokens.

## Normal Path

In the TUI, select a controlled SSH profile and press `s`. Run:

```sh
pwd
ls
df -h
echo "日本語テスト"
exit
```

Expected:

- SSH connects.
- Each command is visible locally.
- Command history and command results are present in the saved log.
- Japanese output is readable in the saved log and in `td session show --tail`.
- TUI returns after `exit`.
- `td session list`, `td session show <session_id>`, and
  `td session path <session_id>` work.
- Metadata has `backend=conpty`, `status=completed`, `exit_code`,
  `log_path`, `content_capture=terminal_io`,
  `content_capture_reliable=true`, `backend_status=explicit_ready`, and
  `backend_warning=conpty_backend_is_explicit_and_not_selected_by_auto`.
- Metadata excludes auth args, full command strings, private key paths,
  passwords, secrets, and tokens.

## Ctrl-C

Run:

```sh
sleep 30
```

Press `Ctrl-C`, then if the session remains usable:

```sh
echo after-ctrl-c
exit
```

Expected:

- The remote process stops, or TeraDock safely aborts the ConPTY child.
- The local terminal and TUI are not left in a broken state.
- TUI returns or the terminal can be recovered without closing the whole app.
- `ssh.exe` from the test does not remain after abort/exit.
- Saved metadata is inspectable when the metadata sidecar can be written.

## Bad Host

Use a controlled profile whose host does not exist or is unreachable.

Expected:

- Timeout or SSH error is visible.
- Metadata has `status=failed` when the failure is caught by the ConPTY runner.
- TUI returns.
- No child `ssh.exe` from the test remains.

## Auth Failure

Use a controlled profile/account that fails authentication. Do not type real
passwords, tokens, or private values into the terminal during this test.

Expected:

- Prompt or failed-auth output is visible.
- Metadata has `status=failed` or `status=completed_nonzero`.
- TUI returns.
- Terminal mode is not broken.
- Metadata still excludes auth args, full commands, private key paths,
  passwords, secrets, and tokens.

## Resize

While SSH is connected, resize the terminal narrower and wider, then run:

```sh
stty size || true
echo resize-check
exit
```

Expected:

- Display does not become unusable.
- Logging continues after resize.
- TUI returns after `exit`.

## Large Output

Run:

```sh
seq 1 1000
exit
```

Expected:

- Output is displayed.
- Output is saved to the log.
- TeraDock does not freeze.
- TUI returns after `exit`.

## Inspection Commands

After every case:

```powershell
td session list
td session show <session_id>
td session show <session_id> --tail 50
td session path <session_id>
Get-Process td,ssh,pwsh,powershell -ErrorAction SilentlyContinue
```

Record only safe fields. Do not paste auth args, full SSH command strings,
private key paths, passwords, secrets, tokens, or sensitive terminal output into
reports.

## Auto Gate

Auto selection remains blocked until the normal path and all failure cases have
recorded evidence from controlled Windows runs:

- Ctrl-C.
- Startup timeout.
- Bad host.
- Auth failure.
- Resize.
- Large output.
- Long-running session.
- Clean child-process snapshot after normal exit and failure.
