# TeraDock v0.1 Release Candidate Prep Result

Date: 2026-06-10

## Changes

- Added release-facing documents: `CHANGELOG.md`, `RELEASE_CHECKLIST.md`, and `RELEASE_NOTES_0.1.0.md`.
- Aligned workspace and crate metadata for v0.1.0.
- Updated README with GitHub Release installation options, `td recent --json`, known limitations, and release document links.
- Updated TUI, security, and internal boundary docs with v0.1 release limitations.
- Updated CI clippy command to match the release gate: `--all-targets --all-features -- -D warnings`.

## Changed Files

- `.github/workflows/ci.yml`
- `Cargo.toml`
- `crates/cli/Cargo.toml`
- `crates/common/Cargo.toml`
- `crates/core/Cargo.toml`
- `crates/tui/Cargo.toml`
- `README.md`
- `docs/tui.md`
- `docs/security.md`
- `docs/internal/ssh-invocation-boundary.md`
- `docs/internal/commandset-execution-boundary.md`
- `CHANGELOG.md`
- `RELEASE_CHECKLIST.md`
- `RELEASE_NOTES_0.1.0.md`
- `RESULT_TeraDock_V0_1_RC_PREP.md`

## Version And Metadata

- Workspace version remains `0.1.0`.
- `td`, `tdcore`, `tui`, and `common` all report version `0.1.0` in `cargo metadata`.
- License is now consistently `Apache-2.0`, matching the repository `LICENSE`.
- Repository is set to `https://github.com/ardnico/TeraDock`.
- Authors, readme, keywords, categories, and package descriptions are set through workspace inheritance or package-specific descriptions.
- crates.io publishing is intentionally disabled with `publish = false` for this release candidate. The v0.1 path is GitHub Release artifacts, not crates.io.

## Changelog Summary

`CHANGELOG.md` follows a Keep a Changelog style and adds `## [0.1.0] - 2026-06-10`.

It covers added profile management, SSH/Telnet/Serial support, CommandSet execution, TUI workflows, TUI SSH sessions, critical confirmation, `td recent`, secrets, config/env/configset, transfer/tunnel, import/export, and doctor.

It also documents SSH boundary changes, README/docs onboarding updates, security notes, and known limitations.

## Release Checklist Summary

`RELEASE_CHECKLIST.md` covers:

- Pre-release Rust gates.
- CLI smoke tests.
- TUI smoke tests.
- Security checks.
- Packaging checks for tar.gz, deb, rpm, and Windows installer artifacts.
- Documentation checks.
- Tag and release steps.

## Release Notes Summary

`RELEASE_NOTES_0.1.0.md` is written for direct GitHub Release use.

It describes TeraDock as an early release, lists the target audience, highlights, installation options, quick start, known limitations, safety notes, and next roadmap. It avoids claiming production-ready status.

## README And Docs Updates

- README now states the GitHub Release artifact path and notes that crates.io publication is out of scope for v0.1 RC.
- README now includes known limitations and release document links.
- `docs/tui.md` now documents TUI limitations around recent sessions, current-terminal SSH, tmux, and real SSH server tests.
- `docs/security.md` now adds the release safety scope and controlled-host validation guidance.
- Internal SSH and CommandSet boundary docs now explicitly frame the remaining boundaries as v0.1 limitations.

## CI And Packaging

- CI already runs on `main` pushes and pull requests across `ubuntu-latest` and `windows-latest`.
- CI now runs clippy with `--all-targets --all-features -- -D warnings`.
- Release workflow already triggers on `v*` tags.
- Release workflow builds `td` with `cargo build -p td --release --locked`.
- Linux release workflow builds tar.gz, deb, and rpm artifacts.
- Windows release workflow builds an Inno Setup installer.
- Release workflow uploads artifacts to a GitHub Release.

Packaging artifacts were not built locally in this pass. The release binary build step was validated locally.

## Tests Run

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
cargo run -p td -- --help
cargo run -p td -- doctor
cargo run -p td -- recent --json
cargo run -p td -- ui
cargo run -p td -- init --with-samples
cargo run -p td -- profile list
cargo run -p td -- config keys
```

## Test Results

- `cargo fmt --check`: passed.
- `cargo test`: passed. 68 tests passed across workspace crates; doctest targets had no tests.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo build -p td --release --locked`: passed.
- `td --help`: passed and listed the expected command surface.
- `td doctor`: passed. On this Windows environment, OpenSSH `ssh`, `scp`, `sftp`, and `ftp` were found; `telnet` was missing; `SSH_AUTH_SOCK` was not set and produced the expected warning.
- `td recent --json`: passed and returned `[]`.
- Non-TTY `td ui`: passed the expected failure check with `td ui requires an interactive TTY; interactive SSH sessions require a TTY`.
- `td init --with-samples`: passed and reported `linux-basic-check` sample creation.
- `td profile list`: passed.
- `td config keys`: passed and listed `allow_insecure_transfers`, `ssh_auth_order`, `ssh.use_agent`, and `client_overrides`.

Note: an attempted temporary `APPDATA` override did not redirect TeraDock's config directory on this Windows environment. The smoke commands resolved the normal roaming app-data directory, so `td init --with-samples` may have installed the idempotent sample CommandSet in the local user database.

## Not Addressed

- No new runtime features were added.
- Terminal emulator launch was not implemented.
- tmux integration was not implemented.
- Transfer/tunnel were not refactored into full `SshInvocation` shapes.
- `tdcore::cmdset_runner` was not refactored to accept a core SSH invocation.
- Real SSH server integration tests were not added.
- Local deb/rpm/Inno installer builds were not executed.
- No GitHub tag was pushed and no GitHub Release was created.

## Human Checks Before v0.1 Release

- Review `RELEASE_NOTES_0.1.0.md` before using it as the GitHub Release body.
- Decide whether the tag workflow should publish directly or whether maintainers want a draft-release-only process.
- Run release workflow on a test tag or be ready to inspect the first `v0.1.0` workflow run closely.
- Download and smoke-test Linux tar.gz, deb, rpm, and Windows installer artifacts.
- Run `td ui` in a real interactive terminal and verify search, filters, `s`, critical confirmation, SSH return-to-TUI behavior, and non-SSH profile handling.
- Validate SSH connect, exec, run, transfer, tunnel, and config apply against controlled non-production hosts.
- Confirm whether `telnet` being unavailable on the Windows release test machine is acceptable or needs documentation in release notes.
