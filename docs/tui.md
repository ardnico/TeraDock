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
- `c`: clear filters.

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

Use `td recent`, `td recent --limit 10`, or `td recent --json` to list recently used interactive SSH profiles from the CLI. TUI recent-profile panes are not part of the current UI.

## Critical Confirmation

Critical profiles require typed confirmation. For a single profile, type the profile id. For a bulk run, type the comma-separated critical profile ids exactly as shown.

Press `Esc` to cancel a confirmation prompt.

## Results

Single runs populate stdout, stderr, and parsed tabs. Bulk runs also populate the summary tab with one row per profile. After a bulk run, stdout, stderr, and parsed tabs show the most recently executed profile.

## Known Limitations

- Recent SSH sessions are available through `td recent`, not a TUI pane.
- Interactive SSH opens in the current terminal only; terminal emulator launch is not implemented.
- tmux integration is not implemented.
- The automated test suite does not connect to a real SSH server.
