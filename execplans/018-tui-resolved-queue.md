# Extend TUI with resolved settings view, critical typing confirm, and bulk run

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root and must be maintained accordingly.

## Purpose / Big Picture

TUI operators need a fast, safe workflow that matches the project plan: a resolved configuration view per profile, safer confirmation for critical operations, and the ability to run a CommandSet across multiple selected profiles with a clear summary. After this change, a user can open details to see resolved settings (global/env/profile/command), must type a profile identifier to confirm critical runs, and can mark multiple profiles to execute a CommandSet in bulk with a results summary, all without leaving the TUI.

## Progress

- [x] (2026-01-12 23:30Z) Created the ExecPlan for TUI resolved view, confirm typing, and bulk run updates.
- [x] (2026-01-13 15:07Z) Implemented core helper to return resolved settings details for a profile.
- [x] (2026-01-13 15:07Z) Extended TUI state to track details view, confirmation input, marked profiles, and bulk run summary.
- [x] (2026-01-13 15:07Z) Updated TUI input handling and rendering for details pane, help overlay, critical typing confirm, and bulk run results.
- [ ] Validate behavior by running the TUI and documenting observed results or blockers.

## Surprises & Discoveries

- None yet.

## Decision Log

- Decision: Represent resolved setting details as a core struct containing per-scope values and a resolved source indicator.
  Rationale: The TUI should use core APIs, and a single structured payload keeps rendering logic simple and consistent with settings resolution rules.
  Date/Author: 2026-01-12 / assistant
- Decision: For bulk runs with critical profiles, require a typed confirmation string listing the critical profile IDs.
  Rationale: The project plan mandates typed confirmation for critical operations; requiring explicit IDs scales the safety rule to bulk actions without silent bypass.
  Date/Author: 2026-01-12 / assistant

## Outcomes & Retrospective

- Pending implementation.

## Context and Orientation

The TUI lives in `crates/tui/src/` with state in `state.rs`, input handling in `app.rs`, and rendering in `ui.rs`. The resolved configuration logic lives in `crates/core/src/settings.rs` and the settings registry in `crates/core/src/settings_registry.rs`. The project plan section “Phase 17（TUI）を拡張” requires keybindings, resolved view, critical typed confirmation, and multi-select run summary. The TUI currently supports CommandSet execution for a single profile and a simple results pane; it does not yet provide resolved settings or bulk run summaries.

## Plan of Work

First, add a core helper in `crates/core/src/settings.rs` that returns resolved setting details for a profile. This helper should enumerate known setting keys from `settings_registry`, fetch values for global/env/profile/command scopes, and compute the resolved value plus its source. This keeps the TUI calling core APIs instead of re-implementing resolution.

Next, extend `crates/tui/src/state.rs` to track: whether the details view is open, the rendered resolved settings lines, a scroll offset for details, a set of marked profile IDs for bulk run, a bulk run summary structure, and a confirmation input buffer for critical actions. Add helper methods to toggle details, update the details data when the selected profile changes, toggle marks, run the CommandSet across multiple profiles, and enforce typed confirmation for critical profiles.

Then, update `crates/tui/src/app.rs` to wire new keybindings: `d` to toggle details, `?` to toggle help overlay, `Space` to mark/unmark profiles, `R` to run a bulk CommandSet for marked profiles, and typed confirmation input (including backspace/enter) for critical actions. Preserve search mode and results tab switching.

Finally, update `crates/tui/src/ui.rs` to render the details pane (resolved settings) when toggled, to show marks in the profiles list, to show a bulk run summary in the results pane, and to present a typed confirmation overlay for critical actions.

## Concrete Steps

1. In `crates/core/src/settings.rs`, add a `ResolvedSettingDetail` struct and a helper like `resolve_settings_for_profile` that returns per-key values for command/profile/env/global plus the resolved source.
2. In `crates/tui/src/state.rs`, add state for details view, confirmation input, marked profiles, and bulk run summary. Implement helper methods for toggling details, updating details on profile changes, and executing bulk runs with typed confirmation.
3. In `crates/tui/src/app.rs`, update key handling to support details/help toggles, typed confirm input, profile marking, and bulk run invocation.
4. In `crates/tui/src/ui.rs`, render the resolved details pane, confirmation overlay with typed input, profile mark indicators, and bulk run summary tab content.
5. Run `cargo run -p tui` from the repository root and document observations or blockers in this plan.

## Validation and Acceptance

- Toggling details shows resolved settings with per-scope values and a resolved source for the selected profile.
- Critical profile runs require typing the profile ID (or critical IDs in bulk) before execution proceeds.
- Marking multiple profiles and running a CommandSet produces a summary view listing success/failure per profile.
- The TUI remains keyboard-driven with `/` search, `d` details, `Space` mark, `R` bulk run, and `?` help.

## Idempotence and Recovery

The details view and marking are in-memory only. Bulk run execution is repeatable and logs each run through existing core logging. If a bulk run fails for a profile, remaining profiles should still execute and the summary should record the failure.

## Artifacts and Notes

Record any TUI run transcripts or screenshots here if available.

## Interfaces and Dependencies

Add to `crates/core/src/settings.rs`:

    pub enum ResolvedSettingSource { Command, Profile, Env, Global }
    pub struct ResolvedSettingDetail { pub key: String, pub command_value: Option<String>, pub profile_value: Option<String>, pub env_value: Option<String>, pub global_value: Option<String>, pub resolved_value: Option<String>, pub resolved_source: Option<ResolvedSettingSource> }
    pub fn resolve_settings_for_profile(conn: &Connection, profile_id: &str, command_overrides: Option<&std::collections::HashMap<String, String>>) -> Result<Vec<ResolvedSettingDetail>>

The TUI should use these helpers to render the resolved settings view without re-implementing settings resolution logic.
