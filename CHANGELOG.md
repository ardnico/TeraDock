# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project uses semantic versioning.

## [1.1.2] - Unreleased

### Added

- `td session prune --json` for machine-readable prune dry-run and deletion summaries.
- `td session stats` for read-only aggregate saved-session log counts, byte totals, backend/status distribution, skipped metadata count, and oldest/newest session ids.
- `td session stats --json` for automation-friendly aggregate session-log statistics.

### Security

- JSON output does not include terminal transcript bodies or dump full session metadata.
- `td session prune --json` reports summary counts and safe candidate/action fields without SSH auth arguments, full command strings, private key paths, passwords, tokens, or secrets.
- `td session stats --json` reports aggregate counts only; it does not read or print terminal transcript bodies, dump full metadata, or reveal auth arguments, full command strings, private key paths, passwords, tokens, or secrets.
- Malformed or unsafe metadata is skipped and counted.
- Session logs remain sensitive local terminal transcripts.

### Unchanged

- Windows `auto` remains unchanged and does not select ConPTY by default.
- Explicit Windows ConPTY logging remains opt-in.
- TUI/ConPTY behavior is unchanged.
- Prune deletion policy is unchanged.
- PowerShell Transcript remains degraded and best-effort.
- Secret masking and terminal replay are not implemented.

## [1.1.1] - Unreleased

### Added

- `td session prune` for metadata-driven session log retention cleanup.
- `td session prune --older-than <age>` with age values such as `30d`.
- `td session prune --keep-last <count>` to retain the newest saved sessions.
- `td session prune --dry-run` to preview selected metadata/log paths and planned bytes without deleting files.
- `td session prune --yes` as the explicit confirmation required for deletion.

### Security

- Session log cleanup validates metadata and log paths before deletion and skips unreadable, malformed, traversal, or out-of-directory metadata.
- Session logs remain sensitive local transcript files; users should dry-run and prune old logs regularly.
- Windows `auto` remains unchanged and does not select ConPTY by default.

## [1.1.0] - Unreleased

### Added

- Explicit Windows ConPTY session logging for controlled terminal transcript capture.
- TUI `s` integration for SSH profiles when `session.log.enabled=true` and `session.log.backend=conpty`.
- CLI session logging path through explicit ConPTY selection, including `td connect <profile_id> --log-backend conpty`.
- `td session conpty-test <profile_id>` for focused Windows ConPTY smoke checks.
- Session metadata plus `td session list`, `td session show`, and `td session path` support for saved session logs.
- `td session doctor` diagnostics for ConPTY explicit readiness, TUI logging readiness, and Windows auto-selection deferral.

### Changed

- Windows `auto` remains `no-log` for terminal-content logging; use `session.log.backend=conpty` explicitly to enable ConPTY.
- PowerShell Transcript remains an explicit degraded/best-effort backend, not reliable SSH terminal-content logging.

### Security

- Terminal transcripts may contain displayed secrets, including passwords, tokens, prompt responses, pasted text, command output, and other sensitive values.
- Session metadata excludes auth args, full command strings, private key paths, passwords, tokens, and secrets.

### Known Limitations

- Resize evidence is incomplete.
- Large output evidence is incomplete.
- Long-running session evidence is incomplete.
- Broader Windows terminal-host coverage is incomplete.
- Full terminal replay is not supported.
- Secret masking of terminal transcript bodies is not implemented.
- Automated real SSH integration tests are not included.

## [0.1.0] - 2026-06-10

### Added

- Profile management for SSH, Telnet, and Serial connection profiles.
- CommandSet execution for reusable command sequences against SSH profiles.
- TUI profile browsing, search, filtering, marking, CommandSet execution, and result tabs.
- TUI interactive SSH sessions with `s`.
- Critical profile confirmation for high-risk operations.
- SSH session operation logs and `td recent`.
- Secret management with encrypted local storage.
- Config, environment, and config set features.
- File transfer commands, including SCP/SFTP and explicitly acknowledged FTP.
- SSH tunnel commands.
- Import/export support.
- Doctor command for dependency and environment checks.

### Changed

- SSH invocation construction moved into `tdcore::ssh` for shared CLI/TUI behavior.
- CLI and TUI CommandSet execution share `tdcore::cmdset_runner`.
- README and docs onboarding now emphasize `td init --with-samples`, safe CommandSets, TUI usage, and security boundaries.

### Security

- TUI SSH session logs avoid passwords, secret values, SSH auth arguments, private key paths, and full command strings.
- FTP transfers require both configuration opt-in and explicit insecure acknowledgement.
- Critical profiles require typed confirmation before sensitive operations.
- Default export excludes decrypted secret values unless `--include-secrets` is used after master password verification.

### Known Limitations

- TUI recent pane is not implemented.
- Terminal emulator launch is not implemented.
- tmux integration is not implemented.
- `tdcore::cmdset_runner` still receives the SSH path and auth args separately.
- Transfer and tunnel command shapes are not fully converted to `SshInvocation`.
- Real SSH server integration tests are not included in the automated test suite.
