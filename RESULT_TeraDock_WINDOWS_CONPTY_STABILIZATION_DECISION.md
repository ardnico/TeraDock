# RESULT: Windows ConPTY Stabilization Decision

Date: 2026-06-16

## Decision

`CONDITIONAL GO: basic logging works, but more edge cases remain`

The explicit Windows ConPTY path can move from plain PoC wording to an
`experimental_ready` candidate label. It is still degraded. This is not approval
to select ConPTY from `auto`, make it the default Windows backend, or integrate
it into the TUI `s` path.

## Changes

- Updated `td session doctor` on Windows to print:
  - `ConPTY backend: experimental_ready`
  - `Reason: manual smoke succeeded, but TUI integration and broader Windows validation are pending.`
  - overall `Status: degraded`
- Updated explicit ConPTY diagnostics warning/hints in `tdcore`.
- Added a regression assertion that explicit ConPTY diagnostics mention
  `experimental_ready` while still not being used by the standard session plan.
- Added `RESULT_TeraDock_WINDOWS_CONPTY_LOGGING_SUCCESS.md`.
- Updated README, security docs, session logging design, Windows ConPTY design,
  manual smoke checklist, and roadmap.

## Manual Smoke Summary

Recorded success evidence:

- Saved session: `sl_dczccyww`
- Target profile: `p_ojql3dws`
- Backend: `conpty`
- Status: `completed`
- Exit code: `0`
- Verified commands: `session list`, `session show`, `session show --tail`,
  and `session path`
- Verified log content: remote Ubuntu login output, shell prompt, echoed remote
  commands, command output, logout, and OpenSSH close line
- Verified metadata boundary: no SSH auth args, full command strings, private
  key paths, passwords, tokens, or secrets in metadata

Not yet proven by saved evidence:

- Ctrl-C abort metadata and terminal recovery.
- Startup timeout failed metadata.
- Bad host failed metadata.
- Auth failure behavior.
- Purpose-built UTF-8/Japanese output.
- Clean child-process result from a fresh smoke run.
- Multiple Windows terminal environments.

## Promotion Criteria

Added promotion criteria to
`docs/internal/windows-conpty-session-logging-design.md` for:

- PoC -> explicit stable backend.
- Explicit stable backend -> auto backend.

The auto criteria are intentionally stricter and require multiple Windows
environments, Windows Terminal and PowerShell coverage, documented failure
modes, no known terminal restore bug, no known child leak, and TUI manual smoke.

## Status Model

Current state:

- `conpty-test`: experimental command.
- `session.log.backend=conpty`: `experimental_ready` candidate label, still
  degraded and not used by the standard plan.
- `auto`: still resolves to `no-log` on Windows.
- `powershell-transcript`: still explicit best-effort/degraded.

## Docs Updated

- `README.md`
- `docs/security.md`
- `docs/internal/session-logging-design.md`
- `docs/internal/windows-conpty-session-logging-design.md`
- `docs/internal/windows-conpty-manual-smoke.md`
- `ROADMAP.md`

Docs now state that ConPTY basic logging has succeeded, ConPTY is the primary
Windows backend candidate, PowerShell Transcript remains degraded/best-effort,
terminal output can contain secrets, and metadata stays limited to safe fields.

## Validation

Passed:

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

Additional non-interactive checks passed:

```text
.\target\release\td.exe session doctor
.\target\release\td.exe session list --limit 20
.\target\release\td.exe session show sl_dczccyww
.\target\release\td.exe session path sl_dczccyww
```

`td session doctor` now prints `ConPTY backend: experimental_ready` while the
overall status remains `degraded`.

## Why TUI Integration Still Waits

The TUI owns raw mode, alternate screen, mouse capture, redraw, and SSH
suspend/restore behavior. The ConPTY PoC also owns terminal raw mode and PTY
I/O. Combining those paths before failure-mode and cleanup smoke is complete
would increase the blast radius of terminal recovery bugs.

## Why Auto Promotion Still Waits

`auto` would affect normal Windows SSH sessions. The current evidence proves
basic logging on one profile, but does not yet prove Ctrl-C, timeout, bad host,
auth failure, UTF-8/Japanese edge cases, clean child cleanup, multiple Windows
terminal hosts, or TUI behavior. Therefore Windows `auto` remains `no-log`.

## Next Steps

1. Run the manual smoke checklist against a controlled profile with Ctrl-C,
   startup timeout, bad host, auth failure, and UTF-8/Japanese commands.
2. Capture a fresh process snapshot before and after abort/exit to prove no
   `td.exe` or `ssh.exe` child remains.
3. Repeat on Windows Terminal and plain PowerShell.
4. Only after explicit backend criteria pass, decide whether to wire ConPTY into
   normal explicit `session.log.backend=conpty` flows.
5. Keep TUI and `auto` out of scope until their stricter criteria are met.
