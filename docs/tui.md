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

This is separate from CommandSet execution. TeraDock temporarily leaves the TUI screen, restores the normal terminal mode, and starts `ssh -p <port> <auth options> user@host` with standard input, output, and error inherited from the current terminal. When the SSH process exits, the TUI returns and shows whether the session ended normally, with an exit code, or without an exit code.

If no profile is selected, the selected profile is not SSH, or the SSH client cannot be resolved from the profile/global overrides or `PATH`, the TUI stays open and shows a status message.

Critical profiles require typing the profile id before the SSH session opens.

Future extensions may add opening sessions in a new terminal window, terminal emulator selection, profile-specific terminal command overrides, tmux pane/window integration, SSH session history, and recently connected profile lists.

## Critical Confirmation

Critical profiles require typed confirmation. For a single profile, type the profile id. For a bulk run, type the comma-separated critical profile ids exactly as shown.

Press `Esc` to cancel a confirmation prompt.

## Results

Single runs populate stdout, stderr, and parsed tabs. Bulk runs also populate the summary tab with one row per profile. After a bulk run, stdout, stderr, and parsed tabs show the most recently executed profile.
