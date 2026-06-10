# TeraDock Artifact Validation Prep Result

Date: 2026-06-11

## Changes

- Added an artifact-only manual path to the release workflow with
  `workflow_dispatch`.
- Limited workflow write permission to the GitHub Release job; build and manual
  artifact-only runs keep read-only repository permission.
- Kept tag-based production release behavior, but added safeguards:
  - `*-rc*` tags are created as GitHub prereleases.
  - `*-test*` tags build artifacts but skip GitHub Release creation.
  - Manual `workflow_dispatch` runs build artifacts only and never create a
    GitHub Release.
- Added SHA-256 checksum generation for Linux and Windows artifacts.
- Added `docs/release-artifact-validation.md`.
- Expanded `RELEASE_CHECKLIST.md` with validation, release, and rollback gates.
- Updated README and `RELEASE_NOTES_0.1.0.md` to mention checksum assets,
  GitHub Release artifact distribution, and the validation guide.

No production `v0.1.0` tag was pushed. No GitHub Release was created.

## Changed Files

- `.github/workflows/release.yml`
- `README.md`
- `RELEASE_CHECKLIST.md`
- `RELEASE_NOTES_0.1.0.md`
- `docs/release-artifact-validation.md`
- `RESULT_TeraDock_ARTIFACT_VALIDATION_PREP.md`

## Release Workflow Investigation

- CI workflow: `.github/workflows/ci.yml`
  - Triggers on pushes to `main` and pull requests.
  - Runs on `ubuntu-latest` and `windows-latest`.
  - Runs `cargo fmt --all -- --check`,
    `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
    and `cargo test --workspace`.
- Release workflow: `.github/workflows/release.yml`
  - Before this pass, it triggered only on `v*` tags and always ran the GitHub
    Release job.
  - It builds `td` with `cargo build -p td --release --locked`.
  - Linux packaging uses `cargo deb`, `cargo generate-rpm`, and
    `packaging/linux/package-tar.sh`.
  - Windows packaging uses Inno Setup with `packaging/windows/td.iss`.
  - GitHub Release creation uses `softprops/action-gh-release@v2`.
  - It uses generated GitHub release notes rather than automatically reading
    `RELEASE_NOTES_0.1.0.md`.
- Cargo version:
  - Workspace package version is `0.1.0`.
  - `td`, `tdcore`, `tui`, and `common` are at `0.1.0`.
  - `crates/cli/Cargo.toml` has `publish = false`.
- Packaging metadata:
  - deb metadata lives in `crates/cli/Cargo.toml`.
  - rpm metadata lives in `crates/cli/Cargo.toml`.
  - tar.gz packaging lives in `packaging/linux/package-tar.sh`.
  - Windows installer metadata lives in `packaging/windows/td.iss`.
- Local tags observed during investigation:
  - `v1.0.1`
  - `v1.0.2`
  - No release/tag action was taken.

## Release Safety Findings

Before this pass:

- Any `v*` tag, including `v0.1.0-test.1`, would have created or updated a
  GitHub Release.
- RC tags were not automatically marked prerelease.
- There was no artifact-only manual workflow path.
- No checksum files were generated.

After this pass:

- `workflow_dispatch` builds artifacts only and uploads Actions artifacts.
- Build and manual artifact-only jobs use `contents: read`; only the guarded
  release job has `contents: write`.
- `v0.1.0-rc1` creates a prerelease.
- `v0.1.0-test.1` builds Actions artifacts and skips GitHub Release creation.
- `v0.1.0` still creates the production GitHub Release when intentionally
  pushed.
- A full production release can still be created by pushing a non-test,
  non-rc `v*` tag. The checklist now treats that as the final gated action.

## Test Tag / RC Tag / Workflow Dispatch Policy

Recommended order:

1. Use `workflow_dispatch` with `0.1.0-rc-artifact` for artifact-only
   validation.
2. Use `v0.1.0-rc1` only if the GitHub prerelease page and release asset upload
   behavior must be tested.
3. Use `v0.1.0-test.1` only when tag behavior must be verified without creating
   a GitHub Release.
4. Push `v0.1.0` only after artifact installation and smoke testing pass.

## Artifact Validation Procedure

The new validation guide covers:

- Validation strategy.
- Expected Linux and Windows artifacts.
- Checksum verification.
- Linux tar.gz smoke test.
- deb smoke test.
- rpm smoke test, including the no-`dnf` constraint.
- Windows installer smoke test.
- TUI manual smoke test.
- Security checks.
- Release go/no-go criteria.

## Checksum Support

Checksum support was added to the release workflow:

- Linux: `SHA256SUMS-linux-x86_64`
- Windows: `SHA256SUMS-windows-x86_64`

These files are uploaded in Actions artifacts and, for release tags that create
a GitHub Release, uploaded as release assets.

## RELEASE_CHECKLIST Updates

The checklist now includes:

- Link to `docs/release-artifact-validation.md`.
- `workflow_dispatch` artifact-only validation steps.
- RC tag prerelease validation steps.
- Test-tag behavior.
- GitHub Actions artifact confirmation steps.
- Production release pre-publish checks.
- Rollback and recovery policy.
- The v0.1 policy to fix forward with `v0.1.1` after public consumption rather
  than silently replacing release assets.

## RELEASE_NOTES Updates

`RELEASE_NOTES_0.1.0.md` now states:

- The release is distributed through GitHub Release artifacts.
- crates.io publication is not part of this release.
- Checksum files are expected release assets.
- deb/rpm packages are generated from `td` package metadata.
- Fresh install smoke testing should be completed before broad operational
  rollout.

## Tests Run

```bash
cargo fmt --check
git diff --check
python -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml', encoding='utf-8')); print('release.yml parse ok')"
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
cargo run -p td -- --help
cargo run -p td -- doctor
cargo run -p td -- recent --json
```

## Test Results

- `cargo fmt --check`: passed.
- `git diff --check`: passed.
- `release.yml` Python YAML parse check: passed.
- `cargo test`: passed. 68 tests passed across workspace crates; doctest
  targets had no tests.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo build -p td --release --locked`: passed.
- `td --help`: passed and listed the expected command surface.
- `td doctor`: passed. On this Windows environment, OpenSSH `ssh`, `scp`,
  `sftp`, and `ftp` were found; `telnet` was missing; `SSH_AUTH_SOCK` was not
  set and produced the expected warning.
- `td recent --json`: passed and returned `[]`.

Note: the CLI smoke commands were run with an `APPDATA` override, but prior
Windows validation in this repository showed that `directories::BaseDirs` may
still resolve the normal roaming app-data directory. This pass did not run
`td init --with-samples`; it ran only `--help`, `doctor`, and `recent --json`.

## Not Addressed

- No production `v0.1.0` tag was pushed.
- No GitHub Release was created.
- No deb/rpm package was built locally on this Windows machine.
- No Inno Setup installer was built locally.
- No artifact was installed from GitHub Actions because no workflow run was
  triggered from this environment.
- No real SSH server or production host smoke test was run.
- No TUI manual smoke test was run in this pass.
- No runtime feature work was performed.

## Human Steps Before v0.1.0 Release

1. Review the workflow diff and release docs.
2. Run the `Release` workflow manually with `workflow_dispatch` and version
   `0.1.0-rc-artifact`.
3. Download `linux-artifacts` and `windows-artifacts` from the workflow run.
4. Verify `SHA256SUMS-linux-x86_64` and `SHA256SUMS-windows-x86_64`.
5. Run Linux tar.gz, deb, rpm, and Windows installer smoke tests from
   `docs/release-artifact-validation.md`.
6. Run the TUI manual smoke test against disposable profiles and hosts.
7. Confirm README install instructions and release notes match the downloaded
   artifact names.
8. Optionally push `v0.1.0-rc1` if a GitHub prerelease page must be tested.
9. Push `v0.1.0` only after the above checks pass.
10. After the production workflow finishes, download the GitHub Release assets
    and repeat checksum and install smoke tests before announcing the release.
