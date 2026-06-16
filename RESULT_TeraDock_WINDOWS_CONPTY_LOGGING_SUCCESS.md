# RESULT: Windows ConPTY Logging Success Evidence

Date: 2026-06-16

## Evidence Basis

This report records the local evidence available in this checkout and the
current Windows profile store. The successful ConPTY session was not rerun by
automation because real-server SSH smoke can require interactive auth and can
write sensitive terminal content to logs.

## Verification Environment

- Windows version: Microsoft Windows 11 Home, version 10.0.26200, build 26200.
- Architecture: ARM64 / ARM 64-bit processor.
- Shell used for review: PowerShell 7.5.4.
- Terminal emulator: not detected from environment variables in this Codex run.
- `ssh.exe` path: `C:\Windows\System32\OpenSSH\ssh.exe`.
- Target profile: `p_ojql3dws` (`nico@192.168.1.115:22`).
- Saved ConPTY session: `sl_dczccyww`.

## Run Command

PowerShell history contains the smoke command:

```powershell
.\target\release\td.exe session conpty-test p_ojql3dws
```

The full SSH invocation is intentionally not stored in session metadata.

Inspection commands run during this review:

```powershell
.\target\release\td.exe session doctor
.\target\release\td.exe session list --json
.\target\release\td.exe session list --limit 20
.\target\release\td.exe session show sl_dczccyww
.\target\release\td.exe session show sl_dczccyww --tail 120
.\target\release\td.exe session path sl_dczccyww
Get-Content C:\Users\leafs\AppData\Roaming\TeraDock\session-logs\sl_dczccyww.json
Get-Content C:\Users\leafs\AppData\Roaming\TeraDock\session-logs\sl_dczccyww.log -Tail 160
Get-Process td,ssh,pwsh,powershell -ErrorAction SilentlyContinue
```

## Commands Run On The SSH Host

The saved log shows these harmless remote interactions:

```sh
ll
df
logout
```

The purpose-built smoke commands `printf 'teradock-conpty-smoke\n'` and
`printf 'utf8: 日本語\n'` were not present in the saved log.

## Screen Display Result

The successful session reached an Ubuntu 22.04.5 LTS shell prompt and produced
normal remote output. The saved ConPTY log contains the remote MOTD, prompt,
the echoed `ll` and `df` commands, command output, `logout`, and the OpenSSH
connection-close line.

The current review verified the saved log and `td session show --tail` output.
It did not rerun the live terminal session.

## Log File Result

- Log path: `C:\Users\leafs\AppData\Roaming\TeraDock\session-logs\sl_dczccyww.log`.
- The log contains remote shell output, echoed remote commands, and normal
  session closure.
- Common terminal control output is sufficiently sanitized for `session show
  --tail` to be readable.

## Metadata Result

Metadata path:

```text
C:\Users\leafs\AppData\Roaming\TeraDock\session-logs\sl_dczccyww.json
```

Observed metadata:

```json
{
  "session_id": "sl_dczccyww",
  "profile_id": "p_ojql3dws",
  "user": "nico",
  "host": "192.168.1.115",
  "port": 22,
  "exit_code": 0,
  "backend": "conpty",
  "status": "completed",
  "content_capture": "best_effort",
  "content_capture_reliable": false,
  "backend_warning": "conpty_backend_is_experimental_poc"
}
```

The metadata does not store SSH auth args, full command strings, private key
paths, passwords, tokens, or secrets.

## Session List / Show / Path Result

`td session list --json` returned one saved ConPTY session with:

- `session_id=sl_dczccyww`
- `backend=conpty`
- `status=completed`
- `exit_code=0`
- `log_path` pointing to the `.log` file only

`td session show sl_dczccyww` displayed the same metadata and:

- `backend_status: degraded`
- `content_capture: best_effort`
- `content_capture_reliable: false`
- `backend_warning: conpty_backend_is_experimental_poc`

`td session path sl_dczccyww` returned the saved log path.

## Ctrl-C Result

No saved Ctrl-C aborted ConPTY session was present in the session log
directory. Ctrl-C cleanup is implemented and documented, but this success
evidence does not prove the abort path. It remains a required smoke item before
explicit stable backend promotion.

## Timeout Result

No saved startup-timeout ConPTY session was present in the session log
directory. The implementation has startup timeout support and failure metadata
paths, but this evidence set does not prove timeout behavior.

## Bad Host Result

No saved bad-host ConPTY session was present in the session log directory.
Bad-host failed metadata remains a required manual smoke item.

## Auth Failure Result

No saved auth-failure ConPTY session was present in the session log directory.
Auth failure behavior remains a required manual smoke item.

## UTF-8 / Japanese Result

The saved log contains Japanese month/day strings from the remote Ubuntu
environment and those are readable in the log. One MOTD time-zone fragment
rendered with mojibake (`火�ST`), and the purpose-built `printf 'utf8: 日本語\n'`
check was not present. UTF-8/Japanese is therefore acceptable for the observed
basic output, but not fully proven.

## Child Process Cleanup Result

The current process snapshot did not show a `td.exe` process from the saved
successful run. One `ssh.exe` process existed, but it started on 2026-06-14
before the saved success session and had `cmd.exe` as parent, so it was not
attributed to `sl_dczccyww`.

Clean abort/exit process checks should still be repeated from a clean process
snapshot during the next manual smoke pass.

## Verdict

`CONDITIONAL GO: basic logging works, but more edge cases remain`

This supports moving the explicit ConPTY candidate to an `experimental_ready`
label. It does not support `auto` selection, default Windows logging, or TUI
`s` path integration.
