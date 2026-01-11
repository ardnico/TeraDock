# Add CI matrix and release packaging for Windows and Linux

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This plan follows `.agent/PLANS.md` from the repository root and must be maintained in accordance with it.

## Purpose / Big Picture

After this change, contributors can rely on a GitHub Actions CI workflow that runs formatting, linting, and tests on Windows and Linux for every main and pull request update, and a tagged release will automatically publish Windows (Inno Setup) and Linux (`cargo-deb` and `tar.gz`) artifacts. The user-visible outcome is a reproducible CI signal and downloadable installers and archives from GitHub Releases.

## Progress

- [x] (2025-02-14 00:00Z) Read existing repository structure and constraints for packaging and CI.
- [x] (2025-02-14 00:30Z) Added packaging configuration files for Windows Inno Setup and Linux tarball/deb metadata.
- [x] (2025-02-14 00:35Z) Added GitHub Actions CI workflow with matrix for fmt/clippy/test.
- [x] (2025-02-14 00:45Z) Added GitHub Actions release workflow that builds and publishes Windows/Linux artifacts on tag.
- [x] (2025-02-14 00:50Z) Reviewed workflow syntax and documented expected outputs in the plan.

## Surprises & Discoveries

- Observation: No existing `.github/workflows` directory or packaging scripts are present in the repository.
  Evidence: `find .github -maxdepth 2 -type f` returned only `.github/copilot-instructions.md`.

## Decision Log

- Decision: Introduce a new release workflow triggered by tags in addition to the main CI workflow.
  Rationale: The requirement includes both CI on main/PRs and release artifacts on tag, which are distinct triggers and permissions.
  Date/Author: 2025-02-14 / ChatGPT

## Outcomes & Retrospective

The repository now contains CI and release workflows plus packaging scripts/configuration for Windows and Linux. The next step for maintainers is to tag a release (for example `v0.1.0`) to verify artifacts are attached in GitHub Releases, and to confirm `cargo deb -p td` and the tarball script succeed locally after building.

## Context and Orientation

The repo is a Rust workspace with a CLI package at `crates/cli/Cargo.toml` that builds the `td` binary. Packaging configuration for `cargo-deb` must live under that package, while Windows installer configuration for Inno Setup will live under a new packaging directory (for example `packaging/windows/td.iss`). GitHub Actions workflow files live under `.github/workflows/` and will be added from scratch. The release process must build a release binary (`target/release/td` or `td.exe`), then package it using `cargo-deb` for Linux, `tar.gz` for Linux, and Inno Setup for Windows.

## Plan of Work

First, add Linux packaging metadata under `crates/cli/Cargo.toml` using the `package.metadata.deb` table, specifying at least maintainer, section, and assets for the `td` binary, so `cargo deb -p td` works. Then add a Linux tarball script under a new `packaging/linux/package-tar.sh` that stages the `td` binary into a versioned directory and produces a gzip tarball under `dist/`.

Next, add a Windows Inno Setup script at `packaging/windows/td.iss` that packages `target\\release\\td.exe`, installs into `{autopf}` under `TeraDock`, and creates a user config directory (for example `{userappdata}\\TeraDock`) with uninstall cleanup.

Then, add `.github/workflows/ci.yml` to run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` on a matrix of `ubuntu-latest` and `windows-latest` for pushes to `main` and PRs.

Finally, add `.github/workflows/release.yml` that triggers on tags (for example `v*`). It will build release binaries, generate `cargo-deb` and `tar.gz` on Linux, generate the Inno Setup installer on Windows, and upload artifacts to GitHub Releases using a release action. Ensure the workflow names the artifacts predictably so users can identify platform builds.

## Concrete Steps

1. Edit `crates/cli/Cargo.toml` to add `package.metadata.deb` entries and any missing package metadata needed for `cargo-deb`.
2. Create `packaging/linux/package-tar.sh` with a version argument and instructions to stage the binary and emit `dist/td-<version>-linux-x86_64.tar.gz`.
3. Create `packaging/windows/td.iss` for Inno Setup with parameters for version and output naming.
4. Add `.github/workflows/ci.yml` with a two-OS matrix and fmt/clippy/test steps.
5. Add `.github/workflows/release.yml` that builds, packages, and uploads artifacts on tags.

Expected transcript (illustrative, run from repository root):

  $ cargo fmt --all -- --check
  $ cargo clippy --workspace --all-targets -- -D warnings
  $ cargo test --workspace

## Validation and Acceptance

Acceptance is met when:

- CI workflow runs on pushes to `main` and pull requests, showing fmt, clippy, and test jobs for both Windows and Linux in GitHub Actions.
- Tagging a release (e.g., `v0.1.0`) triggers the release workflow and attaches a `.exe` installer, `.deb`, and `.tar.gz` to the GitHub Release.
- `cargo deb -p td` succeeds locally after the metadata changes.
- `packaging/linux/package-tar.sh <version>` produces a tarball containing the `td` binary.

## Idempotence and Recovery

All scripts and workflows are additive and safe to rerun. If packaging fails, rebuild with `cargo build -p td --release` and rerun the packaging commands. Re-running `cargo deb` or the tar script overwrites artifacts in `dist/`.

## Artifacts and Notes

Key files to be created:

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `packaging/windows/td.iss`
- `packaging/linux/package-tar.sh`

## Interfaces and Dependencies

- `cargo-deb` (installed via `cargo install cargo-deb`) will be used on Linux to generate `.deb` using metadata in `crates/cli/Cargo.toml`.
- Inno Setup (`iscc`) will be used on Windows to generate the installer from `packaging/windows/td.iss`.
- GitHub Actions will use `dtolnay/rust-toolchain@stable` and `softprops/action-gh-release@v2` (or equivalent) to build and publish artifacts.

Plan change log: Initial plan authored to cover CI matrix and release packaging requirements.

Plan change log: Marked the plan as complete after implementing workflows and packaging files to keep the living document in sync with repository state.
