# Add CLI import/export with secrets handling

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This document follows .agent/PLANS.md from the repository root and must be maintained in accordance with it.

## Purpose / Big Picture

Users need a reliable way to export their profiles, command sets, configs, and related secrets metadata from one environment and import them into another. After this change, `td export` will emit a JSON document describing those entities, defaulting to secret references instead of raw secrets, and `td import` will load that JSON into a database while preserving IDs and handling name conflicts deterministically (reject or rename). Users can see this working by exporting a database, wiping it, importing the JSON back, and observing the same IDs and conflict behavior.

## Progress

- [x] (2025-09-27 00:30Z) Inspect existing CLI, storage, and secrets code to understand how profiles, cmdsets, configs, and secrets are stored and exported today.
- [x] (2025-09-27 00:55Z) Added new core import/export module with schema types and JSON serialization for profiles/cmdsets/configs, including default secret references and optional raw secrets when `--include-secrets` is set.
- [x] (2025-09-27 01:10Z) Wired `td export` and `td import` CLI commands in `crates/cli/src/main.rs` to call the new module, parse flags, and implement conflict strategy (reject or rename) on import.
- [ ] (2025-09-27 01:15Z) Validate end-to-end: export JSON, wipe DB, re-import, verify preserved IDs and conflict behavior; document commands and results here.
- [ ] (2026-01-11 17:09Z) Attempted validation, but could not run `td` because `cargo build -p td` fails with crates.io CONNECT 403 in this environment.
- [ ] (2026-01-12 08:54Z) Attempted validation, but `cargo build -p td` failed (crates.io CONNECT 403), so the CLI could not be run.
- [x] (2026-01-12 22:59Z) Retried `cargo build -p td` to validate import/export; build still fails with crates.io CONNECT 403, so CLI validation remains blocked.

## Surprises & Discoveries

- Observation: Validation requires the `td` binary, but the build is blocked by crates.io CONNECT 403, preventing end-to-end testing here.
  Evidence: `cargo build -p td` fails downloading config.json from crates.io.

## Decision Log

- Decision: Use a single new module `crates/core/src/import_export.rs` for schema types and import/export helpers to keep CLI thin and data definitions centralized.
  Rationale: The change spans both CLI and persistence concerns; centralizing schema reduces duplication and keeps JSON encoding consistent.
  Date/Author: 2025-09-27 / Agent

## Outcomes & Retrospective

- Import/export functionality is implemented, but end-to-end validation is blocked in this environment due to build failures when fetching crates.io dependencies.

## Context and Orientation

The CLI entry point is `crates/cli/src/main.rs`, which defines subcommands and orchestrates core logic. The data model for profiles, command sets, configs, and secrets is in `crates/core` (exact files to locate during discovery). The new import/export schema will live in `crates/core/src/import_export.rs`, with types representing the export JSON document and functions to read/write from the database. Secrets exist as records with identifiers and values; by default exports should include only references (identifiers) and not raw values unless `--include-secrets` is passed.

A “profile” is a saved set of connection or environment settings. A “cmdset” (command set) is a named collection of commands. A “config” (configuration set) is a named configuration record. A “secret” is a stored sensitive value. The export schema should include profiles, command sets, and configs, each with references to any secrets they use, and optionally include the secret values when `--include-secrets` is requested.

## Plan of Work

First, inspect the existing CLI and core modules to learn how profiles, cmdsets, configs, and secrets are currently stored and serialized. Locate the storage interfaces, struct definitions, and any existing import/export logic. Capture these findings in this plan so a novice can follow them.

Next, create `crates/core/src/import_export.rs` with a clear JSON schema: a top-level export struct containing lists of profiles, command sets, configs, and optionally secrets. Implement serialization/deserialization via serde. Add functions to:

- Export: load data from the database, map it into the export schema, and serialize to JSON. By default include secret references; if `include_secrets` is true, embed secret values.
- Import: parse JSON, validate schema, and insert data into the database with preserved IDs. Implement conflict strategy: `reject` fails on any name/ID collision; `rename` appends a suffix to conflicting names while preserving IDs where allowed.

Then, wire `td export` and `td import` in `crates/cli/src/main.rs`. Add flags for `--include-secrets` and `--conflict` (reject/rename). Ensure `td export` writes JSON to stdout or file as currently designed in the CLI patterns, and `td import` reads from a provided path or stdin based on existing conventions.

Finally, validate with a manual scenario: create data, export to JSON, reset the database, import the JSON, and verify that IDs are preserved and conflicts are handled. Document exact commands and expected outputs in this plan.

## Concrete Steps

1. From `/workspace/TeraDock`, inspect the CLI and core modules:

   - `rg "export" crates/cli/src/main.rs`
   - `rg "import" crates/cli/src/main.rs`
   - `rg "profile" crates/core/src`
   - `rg "cmdset" crates/core/src`
   - `rg "config" crates/core/src`
   - `rg "secret" crates/core/src`

   Capture relevant findings (module paths, key structs, functions) in this plan.

2. Create `crates/core/src/import_export.rs` with:

   - `ExportDocument` struct (serde serializable) containing lists for profiles, cmdsets, configs, and optional secrets.
   - Helper structs for each exported entity with IDs, names, and secret references.
   - `export_to_json(storage, include_secrets) -> String` or similar function that returns JSON.
   - `import_from_json(storage, json, conflict_strategy) -> Result` that handles reject/rename conflicts and preserves IDs.

3. Update `crates/core/src/lib.rs` (or module tree) to export the new module.

4. Update `crates/cli/src/main.rs` to:

   - Add or modify `td export` and `td import` subcommands.
   - Parse `--include-secrets` and `--conflict` flags.
   - Call core import/export functions and handle errors.

5. Run formatting and tests as appropriate (likely `cargo fmt` and `cargo test` limited to relevant crates) and capture outputs.

## Validation and Acceptance

Manual validation scenario (document exact commands and outputs as performed):

1. Create data (profiles/cmdsets/configs) using existing CLI commands.
2. Run `td export --include-secrets > /tmp/export.json` (or file path as implemented) and confirm JSON output includes secret values.
3. Reset or remove the database (follow existing CLI or file deletion procedure discovered in code).
4. Run `td import --conflict reject /tmp/export.json` and confirm data is restored with the same IDs.
5. Create conflicting data, re-run import with `--conflict rename`, and confirm new names are renamed deterministically.

Acceptance: exporting produces valid JSON with expected entities; importing restores data with preserved IDs; conflicts are handled per strategy; no panics or partial imports without explicit error.

## Idempotence and Recovery

Export is read-only and safe to re-run. Import should be idempotent only when the target database is empty or when conflicts are handled deterministically. For `reject`, no changes should be applied after an error; use a transaction or rollback strategy. For `rename`, ensure names are adjusted in a repeatable way (e.g., suffix `-imported` or `-imported-<n>`). If import fails midway, rerun after fixing data or clearing the database.

## Artifacts and Notes

- None yet.

## Interfaces and Dependencies

In `crates/core/src/import_export.rs`, define:

  - `pub struct ExportDocument { pub profiles: Vec<ExportProfile>, pub cmdsets: Vec<ExportCmdset>, pub configs: Vec<ExportConfig>, pub secrets: Option<Vec<ExportSecret>> }`
  - `pub enum ConflictStrategy { Reject, Rename }`
  - `pub fn export_to_json(store: &Store, include_secrets: bool) -> Result<String>` (exact Store type to align with existing core storage interface)
  - `pub fn import_from_json(store: &Store, json: &str, strategy: ConflictStrategy) -> Result<()>`

Define `ExportProfile`, `ExportCmdset`, `ExportConfig`, and `ExportSecret` structs with fields that mirror existing data models, including IDs and any secret references.

Note: exact types and paths must be confirmed during discovery and recorded here.


Change Log: 2025-09-27 - Initial ExecPlan created for import/export CLI feature.
Update 2026-01-11 17:09Z: Recorded validation attempt blocked by crates.io CONNECT 403 during `cargo build -p td`.
Update 2026-01-12 22:59Z: Retried `cargo build -p td`; validation remains blocked by crates.io CONNECT 403.
