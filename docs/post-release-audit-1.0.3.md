# TeraDock 1.0.3 Post-Release Audit

Observed on 2026-06-13 from the local `v1.0.3` checkout and the public GitHub Release.

## 1.0.3 Public State

- Current public release: `v1.0.3`.
- Local checkout: `HEAD` matches tag `v1.0.3`.
- GitHub Release: `v1.0.3`, published, not draft, not prerelease.
- Release body: generated GitHub changelog link from `v1.0.2` to `v1.0.3`.
- Cargo package versions in the workspace are still `0.1.0` for `td`, `tdcore`, `tui`, and `common`.
- The version mismatch affects package-derived artifact names for deb/rpm packages.

## Distribution

TeraDock 1.0.3 is distributed through GitHub Release assets, not crates.io.

Observed `v1.0.3` assets:

- `td-1.0.3-windows-x86_64-setup.exe`
- `td-1.0.3-linux-x86_64.tar.gz`
- `td_0.1.0-1_amd64.deb`
- `td-0.1.0-1.x86_64.rpm`
- `SHA256SUMS-linux-x86_64`
- `SHA256SUMS-windows-x86_64`

Actions artifacts from the release workflow are named:

- `linux-artifacts`
- `windows-artifacts`

## Major Features Provided In 1.0.3

- Local profile management for SSH, Telnet, and Serial targets.
- Danger levels with critical-profile typed confirmation.
- SSH `connect`, `exec`, and stored CommandSet `run`.
- Safe sample CommandSet installation through `td init --with-samples`.
- TUI profile browsing, search, filters, marked bulk runs, result tabs, and interactive SSH session launch with `s`.
- Recent interactive SSH session listing through `td recent` and `td recent --json`.
- Encrypted local secret storage behind a master password.
- Config sets, environment-scoped settings, client overrides, and SSH auth order.
- File transfer commands through SCP/SFTP and explicitly acknowledged FTP.
- SSH tunnel start/status/stop commands.
- Import/export for profiles, CommandSets, config sets, parser definitions, and secret metadata.
- Doctor checks for local dependencies.

## Implementation Alignment

The README and current implementation are broadly aligned for the core 1.0.3 feature set. The CLI source defines the documented profile, config, env, agent, doctor, init, exec, run, connect, recent, tunnel, test, push, pull, xfer, secret, export, import, and ui commands. TUI documentation matches the implemented search/filter/run/result/SSH-session workflow.

The main documentation mismatch is release identity:

- README, CHANGELOG, release notes, release checklist, and artifact validation docs still contain `0.1.0` or `v0.1.0` wording.
- GitHub Release and tag state show the public stable version as `1.0.3`.
- Cargo package metadata still reports `0.1.0`, producing deb/rpm assets with `0.1.0` names even in the `v1.0.3` release.
- `RELEASE_NOTES_0.1.0.md` exists, but there is no dedicated checked-in release notes file for `1.0.3`.

## Installation Entry Points

Recommended user paths:

- Download the Windows installer from the GitHub Release.
- Download the Linux tar.gz from the GitHub Release.
- Use the deb/rpm packages when the package name and version are acceptable for the environment.
- Build from source with `cargo build -p td --release --locked`.

After install:

```bash
td --help
td doctor
td init --with-samples
td recent --json
```

## Smoke Test Targets

Baseline automated checks:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

Release artifact smoke checks:

```bash
td --help
td doctor
td init --with-samples
td profile list
td config keys
td recent --json
```

Manual TUI smoke checks:

- `td ui` starts in an interactive terminal.
- Search, type/group/danger/tag filters, and pane navigation work.
- `r` runs the selected CommandSet on the selected SSH profile.
- `R` runs the selected CommandSet on marked SSH profiles.
- `s` opens an interactive SSH session for an SSH profile and returns to the TUI.
- Critical SSH profiles require typed confirmation.
- Non-TTY `td ui` exits with a clear error.

Security smoke checks:

- Logs and recent-session output do not expose secrets, passwords, tokens, SSH auth arguments, private key paths, or full command strings.
- FTP requires explicit insecure acknowledgement.
- Default export excludes decrypted secret values.

## Known Constraints

- TUI recent pane is not implemented.
- Terminal emulator launch is not implemented.
- tmux integration is not implemented.
- Transfer/tunnel SSH invocation full commonization is incomplete.
- `tdcore::cmdset_runner` still receives SSH path and auth args separately.
- Automated tests with real server connections are limited.
- The v1.0 line should prioritize stabilization over feature expansion.

## Known Risks

- Cargo package version `0.1.0` differs from public release version `1.0.3`.
- deb/rpm asset names currently reflect Cargo package metadata instead of the public release version.
- Release checklist and release artifact validation docs still describe a `v0.1.0` release path.
- Checked-in release notes are for `0.1.0`; GitHub `v1.0.3` release notes are generated and minimal.
- Real SSH, transfer, tunnel, serial, and terminal-specific behavior still rely partly on manual or controlled-environment validation.
- Public issue and PR intake did not exist before this post-release operations pass.

## Carried To Next Version Planning

- Decide whether Cargo package versions should be aligned with public release versions before the next patch or minor release.
- Refresh release checklist and artifact validation docs for the 1.0.x release line.
- Add dedicated release notes for future public patch releases.
- Add better local smoke test scripting that avoids real production hosts.
- Plan 1.1 candidates without implementing them in the 1.0.x stabilization pass.
