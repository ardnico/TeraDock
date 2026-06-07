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

## Critical Confirmation

Critical profiles require typed confirmation. For a single profile, type the profile id. For a bulk run, type the comma-separated critical profile ids exactly as shown.

Press `Esc` to cancel a confirmation prompt.

## Results

Single runs populate stdout, stderr, and parsed tabs. Bulk runs also populate the summary tab with one row per profile. After a bulk run, stdout, stderr, and parsed tabs show the most recently executed profile.
