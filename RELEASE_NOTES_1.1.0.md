# TeraDock 1.1.0

TeraDock 1.1.0 prepares explicit Windows ConPTY session logging for controlled
release. This release keeps Windows automatic terminal-content logging disabled:
`auto` remains `no-log`, and ConPTY must be selected explicitly.

## Highlights

- Added explicit Windows ConPTY session logging.
- Added TUI `s` integration for SSH profiles when
  `session.log.enabled=true` and `session.log.backend=conpty`.
- Added CLI session logging through explicit ConPTY selection, including
  `td connect <profile_id> --log-backend conpty`.
- Added `td session conpty-test <profile_id>` for focused Windows ConPTY smoke.
- Added session metadata inspection with `td session list`,
  `td session show`, and `td session path`.
- Added `td session doctor` diagnostics for ConPTY explicit readiness and
  Windows auto-selection deferral.

## Quick Start

```powershell
td config set session.log.enabled true
td config set session.log.backend conpty
td session doctor
td ui
```

In the TUI, select an SSH profile and press `s`. After the session returns:

```powershell
td session list
td session show <session_id>
td session path <session_id>
```

## Supported

- Windows explicit ConPTY session logging.
- TUI `s` SSH session logging with explicit ConPTY settings.
- CLI session logging through explicit ConPTY selection.
- `td session conpty-test <profile_id>`.
- `td session list`, `td session show`, `td session path`, and
  `td session doctor`.
- Safe session metadata.
- PowerShell Transcript remains available only as a degraded/best-effort
  explicit backend.

## Not Supported Or Not Default

- Windows `auto -> conpty`.
- Full terminal replay.
- Secret masking of terminal transcript bodies.
- Broad terminal-host guarantees.
- Automated real SSH integration tests.

On Windows, `auto` currently resolves to `no-log` for terminal-content logging.
Use `session.log.backend=conpty` explicitly to enable ConPTY logging.

## Security

ConPTY logs are terminal transcripts. Anything displayed in the terminal can be
captured, including passwords, tokens, prompt responses, pasted text, command
output, and secrets. Treat session logs as sensitive files.

Session metadata excludes auth args, full command strings, private key paths,
passwords, tokens, and secrets.

## Known Limitations

- Resize evidence is incomplete.
- Large output evidence is incomplete.
- Long-running session evidence is incomplete.
- Broader Windows terminal-host coverage is incomplete.
