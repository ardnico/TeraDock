# TeraDock Settings UI And Session Log Diagnostics

## Phase 1 Findings

- Settings are stored in the local SQLite `settings` table under scoped keys: `global`, `env:<name>`, and `profile:<profile_id>`.
- The settings registry already exposes UI-ready schema data: key, description, value type, allowed values, examples, danger flag, and supported scopes.
- Session logging can appear not to work when logging is disabled, `session.log.backend=no-log`, Windows fallback is active, `script` is unavailable, the log directory cannot be created/written, or no SSH session has completed yet.
- The smallest change range was `tdcore::session_log` for shared diagnostics, `td` CLI for new commands, and `tui` for the settings screen plus `td ui` entry route.
- This pass intentionally did not redesign the backend, add a terminal emulator launcher, add tmux integration, add portable-pty, make Windows logging fully supported, or expand editing across all config scopes.

## Changes

- Added reusable session logging diagnostics in `tdcore::session_log`.
- Added `td session doctor` and `td session doctor --json`.
- Added `td config ui`, a BIOS-style settings UI focused on Session Logging.
- Added a diagnostics panel inside the settings UI.
- Added `c` from `td ui` to open settings and return with refreshed state.
- Moved TUI clear-filters from `c` to `C`.
- Updated docs and release checklist for the new settings and diagnostics flow.

## Changed Files

- `crates/core/src/session_log.rs`
- `crates/cli/src/main.rs`
- `crates/tui/src/settings_ui.rs`
- `crates/tui/src/app.rs`
- `crates/tui/src/state.rs`
- `crates/tui/src/ui.rs`
- `crates/tui/src/lib.rs`
- `README.md`
- `docs/tui.md`
- `docs/security.md`
- `docs/internal/session-logging-design.md`
- `RELEASE_CHECKLIST.md`
- `RESULT_TeraDock_SETTINGS_UI_AND_SESSION_LOG_DIAG.md`

## Added CLI

```bash
td session doctor
td session doctor --json
td config ui
```

`td session doctor` reports enablement, backend setting, resolved backend, `script` command status, log directory existence/writability, newest saved session log, platform support, fallback reason, status, and hints.

## Settings UI

`td config ui` opens a ratatui settings screen. The first category is Session Logging and includes:

- `session.log.enabled`
- `session.log.backend`
- `session.log.dir`

Controls:

- `Up`/`Down`: move
- `Left`/`Right`: cycle enum values
- `Space`: toggle booleans
- `Enter`: edit strings/paths
- `s`: save global settings
- `r`: reload/discard unsaved changes
- `d`: refresh diagnostics
- `?`: help
- `q`/`Esc`: exit, with confirmation if dirty

The UI shows the effective source as `default`, `global`, `env`, or `profile`. It saves global settings only and warns when a profile/env override is currently winning.

## `td ui` Route

- `c` opens the settings screen from the main TUI.
- After returning, the main TUI refreshes state and shows whether session logging is enabled or disabled.
- Saved settings affect the next SSH session opened with `s`.
- `C` now clears filters.

## Diagnostics Panel

The settings screen shows the same core diagnostics as `td session doctor`:

- Logging status
- Resolved backend
- `script` command status/path
- Log directory
- Writability
- Latest saved session log
- Fallback reason
- Hints

## Observable Causes For Missing Logs

The new diagnostics make these causes explicit:

- Logging is disabled.
- Backend is configured as `no-log`.
- Windows fallback is active.
- `script` is missing on Linux/macOS.
- Log directory is missing or not writable.
- No saved session metadata exists yet.
- Platform/backend resolves to a no-log fallback.

## Docs Updated

- README TUI/session logging docs.
- Detailed TUI keybinding and settings docs.
- Security notes for transcript sensitivity and metadata boundaries.
- Internal session logging design.
- Release checklist smoke and documentation checks.

## Tests And Validation

Passed:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

Focused coverage added:

- `td session doctor` CLI parse.
- `td config ui` CLI parse.
- Disabled diagnostics.
- `no-log` backend diagnostics.
- Settings UI boolean toggle.
- Settings UI enum cycle.
- Settings UI dirty/reload/save behavior.
- Settings UI key-release filtering.
- `td ui` `c` action routing.

## Manual Smoke

Non-interactive release-binary smoke:

```text
target\release\td.exe session doctor
```

Result on this Windows host:

- `enabled: true`
- `backend setting: auto`
- `resolved backend: no-log`
- `script command: not checked because Windows session logging is unsupported`
- `log directory exists: false`
- `log directory writable: true`
- `last session log: none`
- `fallback reason: unsupported_on_windows`
- `Status: not_ready`

```text
target\release\td.exe config get session.log.enabled --resolved
session.log.enabled=true
```

```text
target\release\td.exe session list --json
[]
```

Interactive manual smoke not executed in this automated pass:

- `td config ui` interactive navigation/save.
- `td ui` -> `c` -> settings -> return.
- `td ui` -> `s` against a real SSH server.
- `td session path <session_id>` and file/metadata existence after a real SSH session.
- Verifying `op_logs` after a real SSH session.

Those require an operator TTY and a controlled SSH target. No real-server SSH connection was added to automated tests.

## Not Implemented

- Windows terminal transcript capture.
- Terminal emulator launch.
- tmux integration.
- portable-pty backend.
- Full profile/env/global editing in the settings UI.
- Automated SSH server integration tests.

## Next Stability Improvements

- Add a Linux/macOS CI or scripted smoke path that can mock or provide `script`.
- Add a safe fixture around session metadata/path creation without real SSH.
- Add a settings UI path for clearing a global value back to default.
- Add optional profile/env scope editing after the global-only flow settles.
- Improve doctor wording for missing-but-creatable log directories.
