# Contributing

TeraDock is a local-first CLI/TUI tool for connection profiles, SSH-oriented CommandSets, and cautious operational workflows.

For the 1.0.x line, prioritize stabilization: bug fixes, documentation fixes, packaging fixes, regression fixes, and small safety improvements. Larger feature work should be proposed for 1.1 or later before implementation starts.

## Development Environment

- Rust stable toolchain.
- Git.
- Platform tools needed for the feature under test, such as `ssh`, `scp`, `sftp`, `telnet`, or serial device access.
- Optional packaging tools for release validation: Inno Setup on Windows, `cargo-deb`, and `cargo-generate-rpm`.

## Build

```bash
cargo build -p td --release --locked
```

For local development, use:

```bash
cargo run -p td -- --help
cargo run -p td -- doctor
```

## Test

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Run focused smoke checks when a change affects CLI workflows:

```bash
cargo run -p td -- --help
cargo run -p td -- doctor
cargo run -p td -- recent --json
```

TUI changes should also be smoke-tested manually in an interactive terminal with `td ui`.

## Release Before-Checks

Before release work, verify:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

Then follow `RELEASE_CHECKLIST.md` and `docs/release-artifact-validation.md`.

## Issue And PR Policy

- Use the issue templates for bug reports, feature requests, and documentation issues.
- Keep feature proposals scoped around the user problem and target release line.
- Keep pull requests reviewable. Avoid bundling unrelated refactors with documentation or bug fixes.
- Update README or docs when behavior, commands, configuration, logs, or safety expectations change.
- Include exact test results in the pull request.

## Security-Sensitive Data

Do not paste secrets, passwords, tokens, private keys, or unredacted connection details into issues, pull requests, docs, tests, screenshots, logs, fixtures, or examples.

If SSH connection details are needed, mask host and user values:

```text
host=<masked-host>
user=<masked-user>
```

Do not add logs that expose full SSH auth arguments, private key paths, decrypted secret values, or full commands containing sensitive arguments.

Interactive session logs are terminal transcripts and may contain anything displayed during SSH, including prompt responses and command output. Review and redact saved session logs before using them in bug reports, pull requests, screenshots, fixtures, or release evidence.

## Feature Scope

The 1.0.x line is for stabilization. Appropriate changes include:

- Bug fixes.
- Documentation fixes.
- Packaging fixes.
- Regression fixes.
- Small security or logging hardening.

Feature expansion should target 1.1 or later. Do not start large feature implementation, terminal emulator integration, tmux integration, Web UI work, cloud sync, or remote daemon work without a scoped roadmap issue.
