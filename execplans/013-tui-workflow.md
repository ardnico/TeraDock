# Build full TUI workflow with action pane, confirmations, previews, and results

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This plan is maintained according to .agent/PLANS.md from the repository root.

## Purpose / Big Picture

Deliver a full-screen terminal UI that lets a user browse profiles, choose an action, preview the exact SSH command(s) that will run (with secrets masked), confirm dangerous operations, and review results (stdout, stderr, parsed output) without leaving the TUI. A user should be able to launch the TUI, navigate to a profile, run a CommandSet against it, and view the output in a dedicated results pane.

## Progress

- [x] (2025-02-14 01:10Z) Drafted initial plan with layout, state, and core integration details.
- [x] (2025-02-14 01:18Z) Added core list support for CommandSets via CmdSetStore::list.
- [x] (2025-02-14 01:28Z) Extended AppState with selection, action, confirmation, and result-tracking data.
- [x] (2025-02-14 01:38Z) Built multi-pane UI (profiles list, action pane with preview, results pane with tabs).
- [x] (2025-02-14 01:45Z) Wired input handling for navigation, confirmations, and executing runs.
- [ ] Validate run flow with command preview masking and results view updates.
- [x] (2026-01-12 22:59Z) Retried `cargo build -p tui` to validate the run flow; build still fails with crates.io CONNECT 403, so validation remains blocked.

## Surprises & Discoveries

- Observation: Validation is blocked because the TUI binary cannot be built in this environment.
  Evidence: `cargo build -p tui` fails downloading config.json from crates.io.

## Decision Log

- Decision: Use ratatui split layout with profiles on the left and action/results on the right.
  Rationale: Mirrors the requested “multi-pane layout” and keeps action/results visible during navigation.
  Date/Author: 2025-02-14 / agent

- Decision: Execute CommandSet steps via the same SSH invocation model as the CLI but inside the TUI event loop.
  Rationale: Provides parity with the core “run command” use case while keeping TUI self-contained.
  Date/Author: 2025-02-14 / agent

- Decision: Mask sensitive tokens in command previews using simple flag/key heuristics.
  Rationale: Avoids exposing secrets while keeping the preview readable without heavy parsing.
  Date/Author: 2025-02-14 / agent

## Outcomes & Retrospective

- Outcome: UI workflow is implemented, but validation is blocked because the TUI cannot be built in this environment (crates.io CONNECT 403).

## Context and Orientation

The existing TUI is in `crates/tui/src/` and currently renders a filtered list of profiles with a search mode. `crates/tui/src/app.rs` drives the event loop, `crates/tui/src/state.rs` holds UI state and profile filtering, and `crates/tui/src/ui.rs` renders a simple list. Core domain logic lives in `crates/core/src/`, especially `profile.rs` for profiles, `cmdset.rs` for CommandSet definitions and steps, `parser.rs` for parsing output, `doctor.rs` and `settings.rs` for SSH client resolution, and `db.rs` for SQLite connections. The CLI uses similar operations in `crates/cli/src/main.rs`, which can be used as behavioral reference for running CommandSets and handling dangerous profiles.

## Plan of Work

First, add a safe way to list CommandSets for the TUI. Prefer extending `tdcore::cmdset::CmdSetStore` with a `list()` method that returns all CommandSets ordered by name or ID. This keeps the TUI using the core store rather than raw SQL.

Next, expand `crates/tui/src/state.rs` to track selected profile index, selected CommandSet index, active pane, confirmation prompt state, and an optional result object that stores stdout, stderr, and parsed output. The state should also keep the raw CommandSet steps so the UI can show a command preview before execution.

Then update `crates/tui/src/ui.rs` to render a multi-pane layout. The main area should split horizontally: left for profiles, right for action + results. The right side should split vertically into an action pane (showing selected profile, selected CommandSet, step list, and command preview with masked secrets) and a results pane (tabs for stdout/stderr/parsed). Add a centered confirmation overlay for critical-danger actions.

After that, update `crates/tui/src/app.rs` to wire new keybindings: profile navigation, pane switching, CommandSet selection, run command action, confirmation accept/cancel, and result tab switching. The run action should execute CommandSet steps using SSH, capture outputs, parse with `tdcore::parser::parse_output`, and store results in state for display.

Finally, ensure the result viewer updates after a run, and the command preview masks secrets by replacing values of known sensitive flags or key/value pairs (for example, `--password`, `token=`, `secret=`). Use plain string parsing to avoid hiding non-sensitive data.

## Concrete Steps

1. In repository root, edit `crates/core/src/cmdset.rs` to add a `list()` method on `CmdSetStore`. Order by `name` then `cmdset_id` for a stable list.

2. In `crates/tui/src/state.rs`, add new structs/enums for panes and results, and include fields on `AppState` for selection, confirmations, and results. Add helpers for selecting profiles, command sets, and updating result tabs.

3. In `crates/tui/src/ui.rs`, implement layout using `ratatui::layout::Layout` with horizontal and vertical splits. Render:
   - Profiles list with selection highlight and counts.
   - Action pane showing selected profile metadata, selected CommandSet, step list, and command preview lines (masked).
   - Results pane with tabs (stdout/stderr/parsed) and the current content.
   - Confirmation overlay when a critical profile run is pending.

4. In `crates/tui/src/app.rs`, add key handling for:
   - Up/Down: move selection in the active pane.
   - Tab: cycle panes.
   - Enter or r: run CommandSet (prompt if danger is critical).
   - y/n/Esc: confirm or cancel danger prompt.
   - 1/2/3 (or left/right): switch result tabs.

5. Run a manual flow: launch TUI, pick a profile, select a CommandSet, run, and verify output appears in the results pane with stdout/stderr/parsed tabs.

## Validation and Acceptance

Run the TUI from the repository root using:

    cargo run -p tui

Acceptance is met when:

- The screen shows a left profiles list and right action/results panes.
- Selecting a profile and CommandSet updates the action pane preview.
- Running a CommandSet against a critical profile shows a confirmation prompt and only proceeds on confirmation.
- After a run, the results pane shows stdout, stderr, and parsed JSON tabs; switching tabs changes the content.

## Idempotence and Recovery

All changes are additive and safe to rerun. If execution fails (missing SSH client or unreachable host), the run should report the error in the results pane and allow the user to continue navigating. No database mutations are required beyond existing `touch_last_used` updates.

## Artifacts and Notes

Example expected action pane preview line:

    ssh -p 22 user@host "show version"  (secrets masked as **** if present)

Example results tab header:

    Results [stdout] [stderr] [parsed]

## Interfaces and Dependencies

- `tdcore::cmdset::CmdSetStore::list(&self) -> Result<Vec<CmdSet>>`
- `tdcore::parser::parse_output(spec: &ParserSpec, stdout: &str, def: Option<&ParserDefinition>) -> Result<serde_json::Value>`
- `tdcore::doctor::resolve_client_with_overrides` and `tdcore::settings::get_client_overrides` for SSH client resolution.

Plan updated on 2025-02-14: recorded implementation progress and added the masking decision after completing core/TUI wiring.
Update 2026-01-11 17:09Z: Logged validation attempt blocked by crates.io CONNECT 403 during `cargo build -p tui`.
Update 2026-01-12 22:59Z: Retried `cargo build -p tui`; validation remains blocked by crates.io CONNECT 403.
