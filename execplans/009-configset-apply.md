# Add ConfigSet storage and config apply CLI

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

PLANS.md is stored at `.agent/PLANS.md` in the repository root. This ExecPlan must be maintained in accordance with that document.

## Purpose / Big Picture

Users need to store named ConfigSets that list local config files and apply them to SSH profiles, including backup and dry-run planning. After this change, `td configset add/list/show/rm` manages ConfigSet metadata and files, and `td config apply` can plan or apply per-file changes over SSH with optional backups and chmod. The feature is proven by running `td config apply --plan` to show planned changes without modifying files and by applying a config set with `--backup` to create `.bak.<ts>` copies on the remote host.

## Progress

- [x] (2025-02-14 00:00Z) Create ExecPlan for ConfigSet storage + config apply work.
- [x] (2025-02-14 00:40Z) Implement core ConfigSet storage module and exports.
- [x] (2025-02-14 00:55Z) Implement CLI commands for ConfigSet add/list/show/rm.
- [x] (2025-02-14 01:15Z) Implement `td config apply` with plan/backup/apply flow and logging.
- [x] (2025-02-14 01:25Z) Update CLI parsing tests for new config commands.

## Surprises & Discoveries

- Observation: None yet.
  Evidence: N/A.

## Decision Log

- Decision: Use a new `td configset` command group for storage management while keeping `td config apply` under `td config`.
  Rationale: The existing `config` command already handles client overrides; adding apply there preserves the documented `td config apply` UX while keeping storage operations discoverable.
  Date/Author: 2025-02-14 / agent.
- Decision: Use the local `sha256sum` command for hashing instead of adding a new Rust dependency.
  Rationale: The repository currently avoids a hashing crate; invoking `sha256sum` keeps changes minimal and matches the remote hashing approach.
  Date/Author: 2025-02-14 / agent.

## Outcomes & Retrospective

ConfigSet storage now exists in core with list/get/insert/delete operations, and the CLI can add/list/show/rm config sets plus apply them with plan/backup behavior. The implementation adds SHA-256 checks for changed detection and remote home resolution for `~/` destinations. Remaining work is running full integration tests against real SSH endpoints when available.

## Context and Orientation

The SQLite schema already includes `configsets` and `configfiles` tables in `crates/core/src/db.rs`. There is no storage module for them yet, so create `crates/core/src/configset.rs` and export it in `crates/core/src/lib.rs`. The CLI is implemented in `crates/cli/src/main.rs`, with existing command handlers such as `handle_profile`, `handle_config`, and `handle_run`. Transfers use `tdcore::transfer` helpers; SSH execution is done directly via `std::process::Command` in the CLI. Operation logging uses `tdcore::oplog::log_operation` and the `op_logs` table.

Config apply behavior is specified in `PROJECT_PLAN.md` section 9.1: for each config file, resolve `~/` to the remote home, check existence/hash when needed, optionally back up, upload to a temp file, move into place, and apply mode. Plan mode must not modify remote state.

## Plan of Work

First, add a ConfigSet storage module in `crates/core/src/configset.rs`. Define data structures for config sets and files, including a `ConfigFileWhen` enum to represent `always`, `missing`, or `changed` behavior. Implement a `ConfigSetStore` with methods to insert a config set (including files), list config sets, fetch a config set with its files, and delete a config set. Use ID normalization and validation via `common::id` as done in `profile.rs` and `secret.rs`.

Next, extend the CLI in `crates/cli/src/main.rs` with a `configset` command group. Add args for `add` (name, optional config_id, optional hooks_cmdset_id, and repeated `--file` specs), `list`, `show`, and `rm`. Parse file specs into `NewConfigFile` values and call the store. For `show`, print the config set and file list as pretty JSON.

Finally, add `td config apply` under the existing `config` command. The handler should load the profile (SSH only), confirm danger for critical profiles, load the config set and files, and for each file resolve `~/` using a remote `$HOME` lookup. For `when=changed`, compute the local SHA-256 hash and compare against the remote hash fetched via SSH `sha256sum`; for `when=missing`, only apply if the remote file is absent. If `--plan` is set, only display per-file actions; otherwise, perform backups when `--backup` is set, upload the local file to a temp destination using scp, move it into place, and chmod if a mode is specified. Log a `config_apply` op with metadata (config_id, plan, backup, counts) and update profile `last_used_at`.

## Concrete Steps

1. Create `crates/core/src/configset.rs` with the new data types and store methods. Export the module in `crates/core/src/lib.rs`.
2. Add any needed dependencies (e.g., SHA-256) to the workspace and CLI crate for local hashing.
3. Update `crates/cli/src/main.rs`:
   - Add `ConfigSetCommands` and `ConfigSetAddArgs` plus parsing helpers for `--file` specs.
   - Add `ConfigApplyArgs` under `ConfigCommands`.
   - Implement `handle_configset` and `handle_config_apply` helpers.
4. Update or add CLI parsing tests for the new subcommands.
5. Run formatting or minimal checks if needed.

## Validation and Acceptance

- `td configset add --name "dotfiles" --file src=./.bashrc,dest=~/.bashrc,mode=644,when=changed` stores a config set and prints the config ID.
- `td configset list` shows the stored config set.
- `td configset show <config_id>` prints JSON including file entries.
- `td config apply <profile_id> <config_id> --plan` shows per-file planned actions without modifying remote files.
- `td config apply <profile_id> <config_id> --backup` transfers files, creates `.bak.<ts>` backups when files exist, and applies modes.

## Idempotence and Recovery

Config set insertion is additive; re-running with the same ID should either error from SQLite constraints or be avoided by choosing a new ID. `td config apply --plan` is always safe. For apply runs, backups are only created when explicitly requested, and failures after upload can be retried since the temp file is written with a timestamped suffix.

## Artifacts and Notes

Expected `--file` spec format (repeatable):

    --file src=./dotfiles/.bashrc,dest=~/.bashrc,mode=644,when=changed

Example plan output (illustrative):

    PLAN apply: ./dotfiles/.bashrc -> /home/user/.bashrc (changed)

## Interfaces and Dependencies

In `crates/core/src/configset.rs`, define:

    pub enum ConfigFileWhen { Always, Missing, Changed }
    pub struct ConfigSetStore { conn: Connection }
    pub struct NewConfigSet { pub config_id: Option<String>, pub name: String, pub hooks_cmdset_id: Option<String>, pub files: Vec<NewConfigFile> }
    pub struct NewConfigFile { pub src: String, pub dest: String, pub mode: Option<String>, pub when: ConfigFileWhen }
    pub struct ConfigSetDetails { pub config: ConfigSet, pub files: Vec<ConfigFile> }

The CLI should use the `sha256sum` command for local hashing and should use existing SSH invocation patterns from `handle_exec`/`handle_run` for remote checks and operations.

Plan update note: Updated Progress, Outcomes, and dependencies to reflect completed implementation steps, CLI test additions, and the choice to use `sha256sum` for hashing.
