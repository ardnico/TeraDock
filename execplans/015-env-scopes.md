# Add env scopes, current env tracking, and resolved config merging

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This document follows the requirements in `.agent/PLANS.md` from the repository root and must be maintained accordingly.

## Purpose / Big Picture

Users need to define environment presets (like work/home) and switch between them while keeping a clear, predictable precedence for settings. After this change, `td env list/use/show/set` will manage env-scoped settings stored in the existing `settings` table, and `td config get --resolved` for a profile will merge `global` → `env` → `profile` → `command` overrides with the current env influencing the resolved value. The behavior is observable by setting an env-scoped value, switching the current env, and verifying that the resolved profile config changes.

## Progress

- [x] (2025-02-16 01:10Z) Review existing settings storage, CLI config commands, and where resolved settings are used.
- [x] (2025-02-16 01:35Z) Add env-aware scopes and current env helpers in `crates/core/src/settings.rs`, plus new helpers for listing envs and scoped settings.
- [x] (2025-02-16 01:50Z) Extend CLI with `td env list/use/show/set`, update config scope parsing to accept env scopes, and wire resolved lookups to include the current env.
- [ ] (2025-02-16 02:00Z) Validate behavior by demonstrating that switching env changes the resolved config for a profile.
- [ ] (2026-01-11 17:09Z) Attempted validation, but `cargo build -p td` failed (crates.io CONNECT 403), so CLI verification could not be run.
- [x] (2025-02-16 01:55Z) Update this ExecPlan with outcomes, surprises, and decisions.

## Surprises & Discoveries

- None.
- Validation is blocked because the `td` binary cannot be built in this environment (crates.io CONNECT 403).

## Decision Log

- Decision: Use the existing `settings` table for env scopes with `env:<name>` and `env.current` stored in global scope, as described in `PROJECT_PLAN.md`.
  Rationale: Aligns with the documented storage model and avoids schema changes.
  Date/Author: 2025-02-16 / Codex

## Outcomes & Retrospective

Implemented env scopes, current env tracking, and env-aware resolution in settings, plus CLI support for managing env presets. Manual validation is still outstanding because the CLI cannot be built in this environment (crates.io CONNECT 403). The change aligns with the documented precedence model and is ready for follow-up verification in a real profile environment.

## Context and Orientation

The core settings storage is implemented in `crates/core/src/settings.rs` and uses the `settings` table with `scope`, `key`, and `value` columns. Settings schema and scope validation live in `crates/core/src/settings_registry.rs`. The CLI entry point and subcommand wiring are in `crates/cli/src/main.rs`, including `td config get/set` and their scope parsing. This change must add env scopes (`env:<name>`), track the current env using a global setting key (`env.current`), and ensure resolved config lookups consider the current env between global and profile values.

## Plan of Work

First, extend the settings scope model in `crates/core/src/settings.rs` to include an `Env` scope, update parsing/serialization to accept `env:<name>`, and add helpers to get/set/clear the current env using the global scope key `env.current`. Add helper functions to list existing env names by querying distinct `settings.scope` values prefixed with `env:` and to list all settings for a specific scope to support `td env show`.

Next, update the resolved setting logic in `crates/core/src/settings.rs` so that resolution order is `command` (optional override provided by the caller) → `profile` → current `env` → `global`. Keep a convenience wrapper that preserves the existing signature used by the CLI while delegating to the new resolved function.

Then, update the CLI in `crates/cli/src/main.rs` to add a new `env` command with `list`, `use`, `show`, and `set` subcommands. Ensure `td env set <name>.<key> <value>` validates the setting key and uses env scope persistence. `td env use <name>` should update `env.current`. `td env list` should show existing env names and indicate the current env. `td env show <name>` should list env-scoped key/value pairs. Update config scope parsing and help text to accept `env:<name>`, and ensure `td config get --resolved --scope profile:<id>` uses the env-aware resolution.

Finally, validate by setting a value in two envs, switching the current env, and demonstrating that `td config get --resolved --scope profile:<id>` returns different results. Record observations and update the ExecPlan sections.

## Concrete Steps

1. From the repository root, edit `crates/core/src/settings.rs` to add env scopes and current env helpers, then implement env-aware resolution.
2. Edit `crates/cli/src/main.rs` to add `td env` subcommands and wire them into the command handler and scope parsing.
3. Run a manual validation sequence:

   - `td env set work.allow_insecure_transfers true`
   - `td env set home.allow_insecure_transfers false`
   - `td env use work`
   - `td config get allow_insecure_transfers --scope profile:<id> --resolved`
   - `td env use home`
   - `td config get allow_insecure_transfers --scope profile:<id> --resolved`

   Expect the resolved value to change with the current env. (If no profile exists, create a dummy profile or use an existing one.)

## Validation and Acceptance

The change is accepted when:

- `td env list/use/show/set` are recognized by the CLI and manipulate env-scoped settings stored as `env:<name>`.
- The current env is stored under the global setting key `env.current`.
- `td config get <key> --scope profile:<id> --resolved` incorporates the current env between profile and global values.
- A manual switch of the current env results in a different resolved value for a profile, as shown by CLI output.

## Idempotence and Recovery

All settings operations are idempotent; re-running `td env set` overwrites the same scope/key pair. If a mistake is made, re-run `td env set` with the correct value or switch the current env back with `td env use <name>`. No schema changes are introduced, so rollback is as simple as reverting code changes.

## Artifacts and Notes

No artifacts yet.

## Interfaces and Dependencies

Add/extend the following in `crates/core/src/settings.rs`:

    pub enum SettingScopeKind { Global, Env, Profile }
    pub enum SettingScope { Global, Env(String), Profile(String) }
    pub fn get_current_env(conn: &Connection) -> Result<Option<String>>
    pub fn set_current_env(conn: &Connection, name: &str) -> Result<()>
    pub fn clear_current_env(conn: &Connection) -> Result<()>
    pub fn list_env_names(conn: &Connection) -> Result<Vec<String>>
    pub fn list_settings_scoped(conn: &Connection, scope: &SettingScope) -> Result<Vec<(String, String)>>
    pub fn get_setting_resolved_with_override(conn: &Connection, scope: &SettingScope, key: &str, command_override: Option<&str>) -> Result<Option<String>>

The CLI should introduce a new `Commands::Env` variant with `EnvCommands::{List, Use, Show, Set}` and invoke these helpers.

Plan updates: Marked completed implementation steps and added an outcomes note to reflect the current state; left validation unchecked because it has not been run in this environment.
Update 2026-01-11 17:09Z: Logged validation attempt blocked by crates.io CONNECT 403 during `cargo build -p td`.
