# TeraDock Productization Result

## Implemented Changes

- Reframed README around TeraDock's primary value: safe reusable CommandSet execution across managed connection profiles.
- Added `td init` and `td init --with-samples`.
- Added an idempotent `linux-basic-check` sample CommandSet installer.
- Added core CommandSet creation types and store insertion support.
- Added `tdcore::cmdset_runner` so CLI and TUI share CommandSet execution behavior for step order, timeout, parser application, `on_error`, `op_logs`, and `last_used_at`.
- Updated CLI `td run` to call the shared core runner and to apply existing SSH auth-order settings.
- Updated TUI run paths to call the shared core runner.
- Improved TUI status/help text for missing profile, missing CommandSet, no marked profiles, critical confirmation, and bulk summaries.
- Added docs for getting started, CommandSets, TUI, security, and the internal execution boundary.
- Fixed small existing test/clippy issues that blocked the requested verification commands.

## Changed Files

- `README.md`
- `Cargo.lock`
- `crates/cli/src/main.rs`
- `crates/cli/src/transfer.rs`
- `crates/core/Cargo.toml`
- `crates/core/src/cmdset.rs`
- `crates/core/src/cmdset_runner.rs`
- `crates/core/src/db.rs`
- `crates/core/src/doctor.rs`
- `crates/core/src/error.rs`
- `crates/core/src/import_export.rs`
- `crates/core/src/lib.rs`
- `crates/core/src/oplog.rs`
- `crates/core/src/parser.rs`
- `crates/core/src/profile.rs`
- `crates/core/src/secret.rs`
- `crates/core/src/tester.rs`
- `crates/core/src/transfer.rs`
- `crates/core/src/tunnel.rs`
- `crates/tui/Cargo.toml`
- `crates/tui/src/state.rs`
- `crates/tui/src/ui.rs`
- `docs/getting-started.md`
- `docs/commandsets.md`
- `docs/tui.md`
- `docs/security.md`
- `docs/internal/commandset-execution-boundary.md`

## Added Commands And Options

- `td init`
- `td init --with-samples`

`--with-samples` creates `linux-basic-check` only when that CommandSet id does not already exist. Existing data is not overwritten.

## README And Docs Updates

- README now includes quick start, CLI examples, TUI essentials, danger level behavior, import/export, secret notes, platform notes, and "What TeraDock is not".
- `docs/commandsets.md` covers CommandSet purpose, safe examples, dangerous command handling, timeout, `on_error`, parser specs, and bulk run cautions.
- `docs/security.md` covers secret handling, critical confirmation, FTP risk, import/export cautions, and operation log contents.
- `docs/internal/commandset-execution-boundary.md` documents what moved into core and what remains in CLI/TUI.

## Tests Run

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Test Results

- `cargo fmt --check`: passed.
- `cargo test`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.

## New Or Updated Test Coverage

- CLI parse test for `td init --with-samples`.
- Sample CommandSet idempotency test.
- CommandSet insertion tests.
- Core CommandSet runner tests for successful parser application, `on_error=stop`, and `on_error=continue`.
- Existing tunnel and doctor tests adjusted so they pass on the current Windows environment.

## Unresolved Constraints

- `td profile add --interactive` was not implemented in this pass. It is documented as a follow-up because adding it cleanly requires changing current clap-required fields without weakening non-interactive validation.
- First-class `td cmdset add/list/show/rm` commands are still missing; initial CommandSet onboarding now uses `td init --with-samples` and import/export.
- SSH auth-order parsing/building is still duplicated between CLI and TUI; CommandSet execution itself is now shared.
- Timeout failures still return before writing an `op_logs` row, matching the previous behavior.

## Next Recommended Work

- Add `td cmdset add/list/show/rm` with JSON or repeatable step arguments.
- Add `td profile add --interactive` with clear validation and no secret capture.
- Move SSH auth option construction into core.
- Add a test executor abstraction for timeout behavior without shell-script based fake SSH.
- Consider storing a compact timeout/failure op log entry for failed CommandSet startup or timeout paths.

## Breaking Changes

No intentional breaking changes. Existing commands and DB schema are preserved.
