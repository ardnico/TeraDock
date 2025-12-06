# P2 extras: dashboard, badges, palette, sharing

This ExecPlan is a living document. Maintain it in accordance with .agent/PLANS.md and keep every section current.

## Purpose / Big Picture

Add the playful P2 features from PROJECT_PLAN.md section 5.3 so users can see lightweight usage insights (dashboard, badges, topology), share curated profiles, and trigger actions quickly via a command palette. The goal is observable, interactive behaviors in the GUI without breaking existing profile and settings flows.

## Progress

- [x] (2025-12-05 18:19Z) Drafted plan with scope and validation steps.
- [x] (2025-12-05 18:21Z) Implemented windows feature name fix so builds pick the correct `windows` crate features.
- [x] (2025-12-05 18:21Z) Added GUI dashboard tab with stats, achievements, and topology view derived from history and profiles.
- [x] (2025-12-05 18:21Z) Implemented shared profile export/import utilities and surfaced them in the GUI.
- [x] (2025-12-05 18:21Z) Added a command-palette style modal to run common actions quickly.
- [x] (2025-12-05 18:22Z) Updated PROJECT_PLAN.md 5.3 status and attempted formatting/build validation.

## Surprises & Discoveries

- Observation: `cargo build --release` still fails with crates.io CONNECT 403 while fetching dependencies.
  Evidence: build attempt returned curl 56 CONNECT 403 for index.crates.io during aes-gcm download.

## Decision Log

- Decision: Keep features additive inside the existing GUI rather than new binaries; reduces risk of regressions and keeps validation simple.
  Rationale: GUI already owns profile/history views and has state for config/paths, so extra UI fits naturally.
  Date/Author: 2025-12-05 / assistant

## Outcomes & Retrospective

- Dashboard tab now surfaces history-driven stats, achievements, and topology summaries without changing core storage.
- Sharing workflow exports/imports profile sets via a shared TOML file in the config directory.
- Command palette offers quick access to connect/edit/settings/dashboard actions.
- Validation attempted; build blocked by network 403 while downloading crates. Fmt passes; rerun build when network allows.

## Context and Orientation

- Core configuration and paths live in `crates/core/src/config.rs` (AppConfig, AppPaths) and are used by both CLI and GUI.
- GUI state and rendering live in `crates/gui/src/main.rs`, including tabs for Profiles, History, and Settings, profile CRUD, and history rendering.
- History data comes from `crates/core/src/history.rs`, which stores JSONL entries with profile id/name, timestamp, and success flag.
- PROJECT_PLAN.md section 5.3 lists playful P2 items to implement: usage dashboard, achievements/badges, topology view, shared profile distribution, and command palette UI.

## Plan of Work

1. Fix the `windows` dependency feature names in the workspace manifest to use underscores (`Win32_Foundation`, etc.) so the crate resolves.
2. Extend the GUI tab set with a new Dashboard tab summarizing history stats (totals, success rate, top profiles), achievements (e.g., first connect, streaks), and a simple topology summary by group/tag.
3. Add shared profile distribution: allow exporting selected or all profiles to a share file under the config directory and importing them back; reuse existing Profile serialization and SecretStore for passwords.
4. Implement a command-palette modal toggled from the GUI that lets users search actions (connect selected, edit selected, open settings, toggle dashboard) and execute them via keyboard-friendly UI.
5. Wire new functionality into state (saving/loading shared files, refreshing stats) and update PROJECT_PLAN.md 5.3 to reflect completion. Keep changes isolated and tested via `cargo fmt` and a build attempt.

## Concrete Steps

- Working directory: repository root `/workspace/TeraDock`.
- Edit `Cargo.toml` to correct `windows` feature names.
- Update `crates/gui/src/main.rs` to add Dashboard tab, stats/achievements/topology renderers, shared export/import helpers, and command palette state/handlers. Add any minimal structs needed for stats aggregation.
- If shared profile export/import needs paths, extend `AppPaths`/`AppConfig` minimally in `crates/core/src/config.rs` and use existing serialization APIs.
- Expose export/import in GUI settings or dashboard with buttons and success/error messages.
- Update PROJECT_PLAN.md 5.3 to mark items done and briefly describe implementations.
- Run `cargo fmt` and attempt `cargo build --release` (expect possible crates.io access warning, record it).

## Validation and Acceptance

- GUI shows a new Dashboard tab with totals, success rate, top profiles, achievements, and topology summary derived from history/profile data.
- Users can export selected/all profiles to a shared file and import them back from the same UI, with feedback on success/failure.
- Command palette modal opens (e.g., via button) and triggers at least connect/edit/settings/dashboard actions for the current selection.
- `cargo fmt` succeeds; build attempt runs (may fail due to network restrictions but should progress past manifest resolution errors).

## Idempotence and Recovery

- Export/import uses deterministic TOML files under the config directory; rerunning will overwrite safely. Stats recompute on each render from in-memory state.
- Command palette only mutates selection or triggers existing actions; closing it cancels without side effects.
- If build fails due to network, rerun after network is available; no persistent changes occur.

## Artifacts and Notes

- None yet.

## Interfaces and Dependencies

- Continue using existing `Profile`/`ProfileSet` serialization in `crates/core/src/profile.rs` for share files.
- Use `HistoryStore` from `crates/core/src/history.rs` for history data; no schema changes.
- GUI additions stay in `crates/gui/src/main.rs`; no new external crates anticipated.
