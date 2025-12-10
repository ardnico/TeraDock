# SSH-first launcher and Windows OpenSSH integration

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this document according to `.agent/PLANS.md`.

## Purpose / Big Picture

Rework TeraDock into a Windows OpenSSH-first launcher that can open sessions in Windows Terminal or the default console without depending on Tera Term. Users should be able to select a profile and get a working `ssh` command with appropriate prompts for dangerous environments, from both the CLI and GUI. The installer and defaults must reflect the SSH-first positioning.

## Progress

- [x] (2025-05-07 00:20Z) Created ExecPlan and captured scope.
- [x] (2025-05-07 01:10Z) Restructured core profile/client model with `ClientKind`, new config paths, and command builder for SSH clients with fallback logic.
- [x] (2025-05-07 01:30Z) Updated CLI/GUI call sites to rely on SSH command builder, kept confirmations/history, and surfaced client kind plus SSH/WT path settings.
- [x] (2025-05-07 01:45Z) Refreshed defaults/tests to match SSH-first behavior and validated with workspace tests.

## Surprises & Discoveries

- Observation: The GUI source contained duplicated helper methods and stale profile management code, which caused compile-time duplication errors when adding new fields.
  Evidence: Cleaning up extra `persist_form`/`add_profile`/`delete_selected` implementations resolved E0592 errors during `cargo test`.

## Decision Log

- Decision: Keep a legacy `ClientKind::TeraTerm` variant for compatibility, but default to SSH-based clients and strip Tera Term assumptions from UI/installer copy.
  Rationale: Allows existing configs to keep working while enabling SSH-first behavior without blocking users lacking Tera Term.
  Date/Author: 2025-05-07 / assistant

## Outcomes & Retrospective

Core, CLI, and GUI now build SSH-focused command specs with `ClientKind` controlling Windows Terminal or plain ssh launches while keeping a legacy Tera Term path available. Default profiles target Windows Terminal SSH, settings expose ssh/wt/legacy paths, tests cover Windows Terminal command generation and fallback to plain ssh, and the installer surfaces an OpenSSH presence warning instead of implying a Tera Term dependency.

## Context and Orientation

The workspace has `crates/core` for profile/config/command logic, `crates/cli` for the CLI launcher, and `crates/gui` for the egui-based app. `crates/core/src/command.rs` currently builds Tera Term arguments from `Profile` and `AppConfig` (which includes only a `tera_term_path`). Profiles live in `crates/core/src/profile.rs`; configuration defaults and paths are in `crates/core/src/config.rs`. The installer script sits at `installer/setup.iss`. The goal is to add SSH client kinds (`WindowsTerminalSsh`, `PlainSsh`) and route command generation accordingly, while updating UI/CLI and defaults.

## Plan of Work

Introduce a `ClientKind` enum in `crates/core/src/profile.rs` with defaults to SSH-first values and profile serialization support. Extend `AppConfig` in `crates/core/src/config.rs` to include SSH and Windows Terminal paths plus any detection/fallback helpers; keep legacy Tera Term path optional for compatibility. Rewrite `build_command` in `crates/core/src/command.rs` to dispatch based on `ClientKind`, generating `CommandSpec` for Windows Terminal (wt new-tab ssh …), plain ssh (cmd /c start … ssh …), or legacy Tera Term. Preserve forwarding, user, port, danger title, and extra args mapping where sensible. Add unit tests covering command generation for each client kind and fallback behavior.

Update CLI (`crates/cli/src/main.rs`) to use the new command builder without Tera Term terminology, ensuring dry-run output and history entries still work. Adjust GUI (`crates/gui/src/main.rs`) to expose client kind selection in profile editing, surface SSH/Windows Terminal path inputs in Settings, remove Tera Term copy, and ensure dangerous connection confirmation works with the new clients. Refresh default profiles (`config/default_profiles.toml`) to include client kinds aligned with SSH, dropping Tera Term-specific fields/macros. Update the installer script (`installer/setup.iss`) to remove Tera Term assumptions and mention SSH/Windows Terminal expectations.

## Concrete Steps

- Modify core profile/config/command modules to add `ClientKind`, new paths, and SSH command builder logic with fallback; add targeted unit tests in `crates/core/tests`.
- Refactor CLI to rely on the new core APIs and adjust messaging for SSH-first behavior.
- Refine GUI forms and settings to handle client kinds and SSH path configuration while pruning Tera Term-specific inputs.
- Update defaults and installer copy; run `cargo fmt` and `cargo test` from repo root.

## Validation and Acceptance

- `cargo test` succeeds across the workspace, including new command-building tests.
- CLI `--dry-run` shows correctly composed ssh/wt commands for sample profiles without mentioning Tera Term.
- GUI presents client kind options and SSH/WT path settings; dangerous connections still show confirmation before execution.
- Installer script text reflects SSH-first requirements (OpenSSH presence warning) and no longer mentions Tera Term dependency.

## Idempotence and Recovery

Changes are additive and configuration-driven. Re-running the steps should regenerate defaults and rebuild without side effects. Profiles/configs remain TOML so users can manually adjust client kinds/paths if detection fails. Legacy Tera Term support remains available through the dedicated client kind without forcing SSH users to configure it.

## Artifacts and Notes

(Include key snippets or outputs as work proceeds.)

## Interfaces and Dependencies

- `crates/core/src/profile.rs`: define `ClientKind` enum (WindowsTerminalSsh, PlainSsh, TeraTerm) with serde support and default to SSH-focused variant.
- `crates/core/src/command.rs`: expose `build_command(profile, config, password)` returning `CommandSpec` for the chosen client; include helper for fallback when Windows Terminal path is unavailable.
- `crates/core/src/config.rs`: add `ssh_path`, `windows_terminal_path`, and optional `tera_term_path`; normalize defaults; helper to describe availability for UI.
