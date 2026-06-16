# TeraDock ConPTY Explicit Backend Integration Result

Date: 2026-06-16

## Summary

ConPTY was promoted from a PoC-only command to an explicit Windows session
logging backend. It is still experimental/degraded and is not selected by
`auto`.

The explicit backend is available through:

- `session.log.enabled=true` plus `session.log.backend=conpty`
- `td connect <profile_id> --log-backend conpty`
- TUI `s` for SSH profiles when the saved settings explicitly select `conpty`
- `td session conpty-test <profile_id>` for focused smoke/debug runs

## Phase 1 Findings

- Reusable PoC pieces: the stabilized ConPTY event loop, startup timeout,
  Ctrl-C abort path, child cleanup, terminal I/O tee, sanitizer, exit-code
  conversion, and existing session metadata/list/show/path storage.
- TUI `s` integration risk: the TUI owns raw mode, alternate screen, mouse
  capture, and suspend/resume, so integration must stay outside the TUI render
  loop and call the shared runner only after suspension.
- CLI `connect` integration risk: `connect` supports SSH/Telnet/Serial, so
  `--log-backend conpty` must stay SSH-only and must not affect non-SSH paths.
- `auto` is still avoided because the broader Windows evidence for Ctrl-C,
  bad host, auth failure, UTF-8/Japanese, child cleanup, and terminal-host
  coverage is not complete.
- Minimal change scope: core backend resolution/metadata, shared ConPTY runner,
  explicit CLI/TUI call sites, doctor/config diagnostics, docs, tests, and this
  result report.

## Backend Resolution

- Windows:
  - `auto` -> `no-log`
  - fallback reason:
    `windows_terminal_content_logging_requires_explicit_conpty`
  - `conpty` -> explicit ConPTY backend, status `degraded`,
    reliability label `experimental_ready`
  - `powershell-transcript` -> explicit degraded/best-effort
  - `script` -> unsupported
  - `no-log` -> no logging
- Linux/macOS:
  - `auto` -> `script` when available
  - `conpty` -> unsupported
  - `powershell-transcript` -> unsupported
  - `script` -> script backend
  - `no-log` -> no logging

## Code Changes

- Moved the Windows ConPTY runner into `tdcore::conpty` so CLI and TUI call the
  same event loop and sanitizer.
- Added `SessionLogPlan::Conpty`.
- Added `plan_for_explicit_backend_with_ssh` for one-shot CLI backend override.
- Added `td connect <profile_id> --log-backend conpty`.
- Added TUI `s` ConPTY execution when the saved settings explicitly select it.
- Kept Windows `auto` on `no-log`; no default promotion was added.

## Doctor And Config UI

- `session.log.backend` schema includes `conpty`.
- `td session doctor` reports explicit ConPTY as resolved backend `conpty`,
  content capture reliability `experimental_ready`, warning text, and
  `Status: degraded`.
- Windows `auto` still reports `resolved backend: no-log`.
- BIOS-style config UI cycles `conpty` through the registered schema and shows
  the same diagnostics rows.

## Metadata

ConPTY metadata now includes:

```json
{
  "backend": "conpty",
  "content_capture": "terminal_io",
  "content_capture_reliable": true,
  "backend_status": "experimental_ready",
  "backend_warning": "conpty_backend_is_explicit_and_not_selected_by_auto"
}
```

Metadata still excludes SSH auth args, full command strings, private key paths,
passwords, tokens, and secrets.

## Docs Updated

- `README.md`
- `docs/security.md`
- `docs/tui.md`
- `docs/internal/session-logging-design.md`
- `docs/internal/windows-conpty-session-logging-design.md`
- `docs/internal/windows-conpty-manual-smoke.md`
- `ROADMAP.md`
- `RELEASE_CHECKLIST.md`

## Tests

Passed:

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo build -p td --release --locked`

Additional focused coverage added:

- `session.log.backend=conpty` explicit Windows plan
- Windows `auto` remains `no-log`
- non-Windows `conpty` remains unsupported
- doctor/config UI explicit ConPTY diagnostics
- CLI `connect --log-backend conpty` parsing and rejection of other values
- TUI `s` plan chooses ConPTY only for explicit backend
- TUI `s` plan does not choose ConPTY for Windows `auto`
- ConPTY metadata excludes sensitive invocation fields

## Windows Manual Smoke

Not executed in this automated implementation turn. No real SSH server session
or TUI interactive smoke was started automatically.

Non-invasive check executed:

- `.\target\release\td.exe session doctor`

Observed local config was still `powershell-transcript`; doctor reported
`Status: degraded` and printed the new ConPTY explicit config hint:

- `ConPTY backend: experimental_ready`
- `ConPTY explicit config: td config set session.log.backend conpty`
- `Reason: manual smoke succeeded, but auto selection is still deferred.`

Required manual smoke remains documented in
`docs/internal/windows-conpty-manual-smoke.md`.

## Why Auto Is Not Promoted

ConPTY has basic successful logging evidence, but not enough breadth for
automatic selection. Before `auto -> conpty`, the project still needs recorded
Windows evidence for Ctrl-C abort, startup timeout, bad host, auth failure,
UTF-8/Japanese output, child cleanup, broader terminal hosts, and TUI return
after both normal exit and abort.

## Next Stability Work

- Run the updated Windows manual smoke checklist with a controlled SSH profile.
- Record TUI `s` exit and Ctrl-C return evidence.
- Record bad host/auth failure metadata.
- Record UTF-8/Japanese output evidence.
- Confirm no orphaned `ssh.exe` remains after normal exit, timeout, or abort.
- Keep ConPTY explicit until those results are reviewed.
