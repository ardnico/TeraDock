# TeraDock 0.1 Release Checklist

Use this checklist before tagging `v0.1.0`.

## Pre-release checks

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## CLI smoke tests

```bash
td --help
td doctor
td init --with-samples
td profile list
td config keys
td recent --json
```

## TUI smoke tests

- `td ui` starts in an interactive TTY.
- `/` search works.
- Type, group, danger, tag, and query filters work.
- `s` opens an SSH session for an SSH profile.
- A non-SSH profile does not open an SSH session.
- A critical profile requires typed confirmation.
- SSH session exit returns to the TUI and redraws the screen.
- Non-TTY `td ui` exits with a clear error.

## Security checks

- No password, token, or secret value appears in logs.
- No SSH auth args, private key paths, or full SSH command string appears in `ssh_session` metadata.
- FTP requires `allow_insecure_transfers=true` and `--i-know-its-insecure`.
- Critical confirmation works for connect, exec, run, transfer, config apply, and TUI SSH sessions.
- `td export` excludes decrypted secret values unless `--include-secrets` is explicitly used.

## Packaging checks

- Linux tar.gz artifact is built and named with version and target.
- deb artifact is built and named with version and target.
- rpm artifact is built and named with version and target.
- Windows installer artifact is built and named with version and target.
- Clean install test succeeds on at least one supported Linux target if possible.
- Clean install test succeeds on Windows if possible.
- Release workflow is reviewed before pushing the `v0.1.0` tag because it creates or updates a GitHub Release.

## Documentation checks

- README quick start works from a clean checkout.
- `docs/tui.md` reflects current keybindings.
- `docs/security.md` reflects current logging and security policy.
- `docs/internal/ssh-invocation-boundary.md` reflects the current SSH boundary.
- `docs/internal/commandset-execution-boundary.md` reflects the current CommandSet boundary.
- `CHANGELOG.md` is updated.
- `RELEASE_NOTES_0.1.0.md` is reviewed and ready to paste into GitHub Releases.

## Tag and release

```bash
git tag v0.1.0
git push origin v0.1.0
```

After the workflow completes, download and smoke-test the uploaded artifacts before announcing the release.
