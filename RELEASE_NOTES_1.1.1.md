# TeraDock v1.1.1

v1.1.1 is a stabilization release for session logging.

## Added

* Added `td session prune` for explicit cleanup of saved session logs and metadata.
* Added dry-run support:

  * `td session prune --older-than 30d --dry-run`
  * `td session prune --keep-last 100 --dry-run`
* Added explicit deletion confirmation via `--yes`.
* Added retention options:

  * `--older-than <duration>`
  * `--keep-last <count>`

## Safety

Session pruning is metadata-driven and conservative.

* Malformed or unreadable metadata is skipped.
* Paths outside the session log directory are skipped.
* Path traversal candidates are skipped.
* Orphan log-only files are not removed in this release.
* Dry-run is available and recommended before deletion.

## Unchanged

* Windows `auto` still does not select ConPTY.
* Explicit Windows ConPTY logging remains opt-in.
* PowerShell Transcript remains degraded/best-effort.
* Terminal transcript logs may contain displayed secrets.
* Metadata avoids storing auth args, full SSH command strings, private key paths, passwords, tokens, and secrets.
