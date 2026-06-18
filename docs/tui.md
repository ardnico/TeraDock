# TUI

Start the terminal UI with:

```bash
td ui
```

The left pane lists profiles. The right pane shows the selected profile, selected CommandSet, command preview, and results.

## Navigation

- `/`: search profiles.
- `Tab`: cycle panes.
- `Up`/`Down` or `k`/`j`: move selection.
- `T`: cycle profile type filter.
- `g`: cycle group filter.
- `D`: cycle danger filter.
- `[` and `]`: move the tag cursor.
- `x`: toggle the focused tag filter.
- `C`: clear filters.

## Settings

- `c`: open the settings screen from `td ui`.

The settings screen lists Session Logging settings first and shows the effective value source (`default`, `global`, `env`, or `profile`). It saves global settings only. If a profile or env override is winning, the screen warns that a global edit may not change the selected context.

Use `Space` to toggle booleans, `Left`/`Right` to cycle enum values, `Enter` to edit strings and paths, `s` to save, `r` to reload and discard unsaved changes, `d` to refresh diagnostics, `?` for help, and `q`/`Esc` to exit. Unsaved changes are held in memory until `s` is pressed. After saving, session logging changes apply to the next SSH session opened with `s`.

## Running CommandSets

- `r` or `Enter`: run the selected CommandSet on the selected profile.
- `Space`: mark or unmark a profile.
- `R`: run the selected CommandSet on marked profiles.
- `1` to `4`: switch stdout, stderr, parsed, and summary tabs.

The status line explains the next available action or why a run cannot start. Common reasons are no matching profile, no CommandSet, a non-SSH profile, or no marked profiles for bulk run.

## Interactive SSH Sessions

- `s`: open an interactive SSH session for the selected SSH profile.

This is separate from CommandSet execution. TeraDock temporarily leaves the TUI screen, restores the normal terminal mode, and starts a shared core-built `ssh -p <port> <auth options> user@host` invocation with standard input, output, and error inherited from the current terminal. When the SSH process exits, the TUI returns and shows whether the session ended normally, with an exit code, or without an exit code.

If SSH cannot be launched, the TUI returns and shows the launch error. If SSH exits without an exit code, for example after signal termination on platforms that report it that way, the TUI shows that explicitly. The TUI clears and redraws after returning from SSH so resize changes during the session do not leave stale screen content.

If no profile is selected, the selected profile is not SSH, or the SSH client cannot be resolved from the profile/global overrides or `PATH`, the TUI stays open and shows a status message.

Critical profiles require typing the profile id before the SSH session opens.

Interactive SSH sessions require a TTY. Running `td ui` with redirected input or output, such as `td ui < input.txt`, exits with a clear error instead of entering raw mode.

Each TUI SSH session attempt is written to `op_logs` as `op = ssh_session` after the session exits or after launch failure. The log row includes the profile id, SSH client path, success flag, exit code when available, duration, and shared core-built metadata such as `mode = interactive`, `source = tui`, host, port, user, and profile type. Passwords, secret values, SSH auth arguments, and full command strings are not logged.

## Interactive Session Logs

Interactive session logging saves the terminal transcript from an interactive SSH session. This is separate from `op_logs`: `op_logs` record operation events, while session logs record terminal output.

Session logging is disabled by default:

```bash
td session doctor
td config ui
td config get session.log.enabled --resolved
td config set session.log.enabled true
td config set session.log.backend auto
td config set session.log.backend conpty
td config get session.log.dir --resolved
```

Defaults:

- `session.log.enabled=false`
- `session.log.dir=<data_dir>/session-logs`
- `session.log.backend=auto`

When logging is enabled, pressing `s` still uses the same TUI suspend/resume lifecycle. TeraDock leaves raw mode and the alternate screen before starting the logged SSH session, then returns to the TUI after the session exits.

Linux/macOS use the `script` backend when available. Windows `auto` resolves to `no-log` with `windows_terminal_content_logging_requires_explicit_conpty`; ConPTY is not selected by `auto`. To capture SSH terminal I/O from the TUI `s` path on Windows, save the explicit ConPTY settings and then open an SSH profile with `s`:

```powershell
td config set session.log.enabled true
td config set session.log.backend conpty
td ui
```

Explicit `conpty` uses the shared Windows ConPTY runner and is `explicit_ready` for manually selected Windows TUI `s` SSH sessions. During a ConPTY SSH session, the first `Ctrl-C` is forwarded to the remote PTY as a process interrupt so the SSH session can continue; live smoke has verified that connection state and log capture continue after a single Ctrl-C. Pressing `Ctrl-C` again within 2 seconds takes the TeraDock emergency abort path; live smoke has verified aborted metadata and child cleanup for that explicit path. Normal TUI logging, Japanese output, single-Ctrl-C remote interrupt, double-Ctrl-C emergency abort, bad-host metadata, and auth-failure metadata have succeeded, but the overall diagnostics remain degraded and Windows `auto` remains deferred until resize, large-output, long-running, broader cleanup, and broader Windows terminal evidence is recorded. Explicit `powershell-transcript` remains available on Windows as a best-effort/degraded backend and may miss SSH-side input/output. If an explicit backend is selected and is not ready, TeraDock reports the backend error instead of silently opening an unlogged SSH session.

Use `td session doctor` or the settings diagnostics panel to check whether logging is enabled, the backend setting, resolved backend, TUI logging status, dependency availability, log directory existence and writability, platform support, fallback reason, ConPTY backend position, auto-selection state, warning, status, and the newest saved session log. The BIOS-style settings screen can toggle `session.log.enabled`, cycle `session.log.backend`, edit `session.log.dir`, and refreshes diagnostics after save.

Saved sessions can be inspected from the CLI:

```bash
td session doctor
td session list
td session list --json
td session show <session_id>
td session show <session_id> --tail 50
td session path <session_id>
```

`td session show` displays metadata by default. It does not print the full terminal log unless `--tail N` is provided.

Session metadata includes the session id, profile id, user, host, port, start/end times, duration, exit code, backend, status, log path, metadata path, and capture reliability/status warnings. It does not include SSH auth arguments, full command strings, private key paths, passwords, secrets, or tokens.

The terminal log itself can still contain any sensitive information displayed during the SSH session. If a password, token, secret, private value, or command output appears on screen, it may be captured in the transcript. On Windows, ConPTY logs SSH terminal I/O when explicitly selected, and PowerShell Transcript is explicit best-effort only; it may capture only host transcript metadata and omit remote commands/output.

If a ConPTY SSH session does not respond after a forwarded `Ctrl-C`, press `Ctrl-C` again within 2 seconds to abort TeraDock and return to the TUI. If the terminal still looks broken after returning from SSH, run `reset` on Unix-like shells when available, or close and reopen the terminal on Windows. If a child process appears to remain after a failed ConPTY run, inspect and stop only the specific leftover `td` or `ssh` process from that test before starting another TUI session.

When a TUI SSH session writes to `op_logs`, the row includes only `session_log_saved`, `session_log_id` when a log was saved, or `session_log_reason` when no log was saved. The log path is kept in the session metadata rather than copied into `op_logs`.

Use `td recent`, `td recent --limit 10`, or `td recent --json` to list recently used interactive SSH profiles from the CLI. TUI recent-profile panes are not part of the current UI.

## Critical Confirmation

Critical profiles require typed confirmation. For a single profile, type the profile id. For a bulk run, type the comma-separated critical profile ids exactly as shown.

Press `Esc` to cancel a confirmation prompt.

## Results

Single runs populate stdout, stderr, and parsed tabs. Bulk runs also populate the summary tab with one row per profile. After a bulk run, stdout, stderr, and parsed tabs show the most recently executed profile.

## Known Limitations

- Recent SSH sessions are available through `td recent`, not a TUI pane.
- Interactive SSH opens in the current terminal only; terminal emulator launch is not implemented.
- Windows full SSH terminal-content logging is implemented only as the explicit ConPTY backend; `auto` still resolves to no-log.
- tmux integration is not implemented.
- The automated test suite does not connect to a real SSH server.
