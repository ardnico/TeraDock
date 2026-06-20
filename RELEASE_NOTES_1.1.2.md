# TeraDock v1.1.2

v1.1.2 is a session-log operations release.

## Added

* Added `td session prune --json` for machine-readable prune dry-run and deletion summaries.
* Added `td session stats` for read-only saved-session aggregation.
* Added `td session stats --json` for automation-friendly session-log statistics.

## Session stats

`td session stats` reports:

* total saved sessions
* total terminal-log bytes
* skipped metadata count
* backend distribution
* status distribution
* oldest session
* newest session

`td session stats` is read-only. It does not delete, modify, or rewrite session files.

## Safety

* JSON output does not include terminal transcript bodies.
* JSON output does not dump full session metadata.
* Metadata safety boundaries remain unchanged.
* Session logs remain sensitive local terminal transcripts.
* Malformed or unsafe metadata is skipped and counted.

## Unchanged

* Windows `auto` still does not select ConPTY.
* Explicit Windows ConPTY logging remains opt-in.
* TUI/ConPTY behavior is unchanged.
* Prune deletion policy is unchanged.
* PowerShell Transcript remains degraded/best-effort.
* Secret masking and terminal replay are not implemented.
