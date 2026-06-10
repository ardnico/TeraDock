# TeraDock 0.1.0

TeraDock 0.1.0 is an early release of a local-first CLI/TUI tool for managing connection profiles and safely running reusable CommandSets across SSH-oriented operations.

Feedback is welcome, especially around onboarding, packaging, and day-to-day TUI workflows.

This release is distributed through GitHub Release artifacts. It is not
published to crates.io.

## Who is this for?

TeraDock is for operators, developers, and lab or maintenance teams who need a small local tool to keep connection profiles, mark risky targets, run repeated SSH checks, and inspect results without building a larger automation stack.

## Highlights

- Manage SSH, Telnet, and Serial profiles in a local SQLite database.
- Initialize a safe read-only sample CommandSet with `td init --with-samples`.
- Run SSH CommandSets from the CLI or TUI.
- Browse, search, filter, mark, and run profiles in the TUI.
- Open an interactive SSH session from the TUI with `s`.
- Require typed confirmation for critical profiles.
- Store secrets encrypted behind a master password.
- Review recent TUI SSH sessions with `td recent`.
- Use import/export for profile and CommandSet backup or transfer.
- Build Linux tar.gz/deb/rpm artifacts and a Windows installer from the release workflow.

## Installation

Download the artifact for your platform from the GitHub Release:

- Windows: `td-0.1.0-windows-x86_64-setup.exe`
- Linux portable archive: `td-0.1.0-linux-x86_64.tar.gz`
- Debian package: `.deb` generated from the `td` package metadata
- RPM package: `.rpm` generated from the `td` package metadata
- Checksums: `SHA256SUMS-linux-x86_64` and `SHA256SUMS-windows-x86_64`

You can also build from source:

```bash
cargo build -p td --release
```

The resulting binary is at `target/release/td` or `target/release/td.exe`.

## Quick start

```bash
td init --with-samples
td doctor
td profile add --profile-id lab1 --name "Lab server 1" --host 192.0.2.10 --user admin --danger high --group lab --tag linux
td run lab1 linux-basic-check
td ui
```

Inside the TUI, use `/` to search, `Space` to mark profiles, `r` to run the selected CommandSet, `R` to run against marked profiles, and `s` to open an interactive SSH session for the selected SSH profile.

## Known limitations

- TUI recent pane is not implemented; use `td recent` or `td recent --json`.
- Terminal emulator launch is not implemented.
- tmux integration is not implemented.
- `tdcore::cmdset_runner` still receives the SSH path and auth args separately.
- Transfer and tunnel command shapes are not fully converted to `SshInvocation`.
- Real SSH server integration tests are not included in the automated test suite.
- Telnet and Serial are scoped primarily to connection workflows in this early release.
- Fresh install smoke testing of release artifacts should be completed before broad operational rollout.

## Safety notes

- Critical profiles require typed confirmation before sensitive operations.
- TUI SSH session logs intentionally omit passwords, secret values, SSH auth args, private key paths, and full command strings.
- FTP is insecure and requires both configuration opt-in and an explicit `--i-know-its-insecure` flag.
- Default exports exclude decrypted secret values. Treat `td export --include-secrets` output as highly sensitive.

## Next roadmap

- Add a TUI recent-session pane.
- Continue moving transfer, tunnel, and CommandSet boundaries toward shared SSH invocation structures.
- Add first-class CommandSet authoring commands.
- Expand artifact smoke testing across fresh Windows and Linux environments.
- Add controlled integration coverage that does not require real production SSH hosts.
