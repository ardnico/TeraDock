# TeraDock 0.1 Release Checklist

Use this checklist before tagging `v0.1.0`. Do not push the production tag or
publish the production GitHub Release until artifact validation is complete.

## 1. Pre-release checks

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

Optional local CLI smoke checks:

```bash
cargo run -p td -- --help
cargo run -p td -- doctor
cargo run -p td -- recent --json
```

## 2. Release workflow validation path

Preferred artifact-only validation:

1. Open the GitHub Actions `Release` workflow.
2. Run `workflow_dispatch`.
3. Use a version label such as `0.1.0-rc-artifact`.
4. Confirm the workflow uploads `linux-artifacts` and `windows-artifacts`.
5. Confirm no GitHub Release is created by the manual run.

Optional RC validation:

```bash
git tag v0.1.0-rc1
git push origin v0.1.0-rc1
```

The release workflow treats `*-rc*` tags as GitHub prereleases. Delete the RC
tag and prerelease only if they were created for validation and are no longer
useful.

Test-tag behavior:

- Tags containing `-test` build artifacts but skip GitHub Release creation.
- Prefer `workflow_dispatch` over test tags unless tag-specific behavior must be
  verified.

Production release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Only run the production tag after all checklist items pass.

## 3. Artifact checks

Follow [Release Artifact Validation](docs/release-artifact-validation.md).

Expected Actions artifacts:

- `linux-artifacts`
- `windows-artifacts`

Expected files:

- `td-<version>-linux-x86_64.tar.gz`
- Debian package from `cargo deb`
- RPM package from `cargo generate-rpm`
- `td-<version>-windows-x86_64-setup.exe`
- `SHA256SUMS-linux-x86_64`
- `SHA256SUMS-windows-x86_64`

Confirm:

- Linux tar.gz artifact is built and named with version and target.
- deb artifact is built and named with package version and architecture.
- rpm artifact is built and named with package version, release, and
  architecture.
- Windows installer artifact is built and named with version and target.
- Checksum files are present and match downloaded artifacts.
- Clean install test succeeds on at least one supported Linux target if
  possible.
- Clean install test succeeds on Windows if possible.

## 4. CLI smoke tests

Run after extracting or installing each artifact:

```bash
td --help
td doctor
td init --with-samples
td profile list
td config keys
td recent --json
td session list --json
```

## 5. TUI smoke tests

- `td ui` starts in an interactive TTY.
- `/` search works.
- Type, group, danger, tag, and query filters work.
- Details view opens.
- `s` opens an SSH session for an SSH profile.
- With `session.log.enabled=false`, `s` opens SSH without saving a terminal
  transcript.
- With `session.log.enabled=true` on Linux/macOS, `s` uses `script` when
  available and `td session list` shows the saved metadata after return.
- On Windows, enabled session logging reports unsupported and falls back to a
  normal SSH session.
- A non-SSH profile does not open an SSH session.
- A critical profile requires typed confirmation.
- SSH session exit returns to the TUI and redraws the screen.
- Non-TTY `td ui` exits with a clear error.

## 6. Security checks

- No password, token, or secret value appears in logs.
- No SSH auth args, private key paths, or full SSH command string appears in
  `ssh_session` metadata.
- Interactive session log metadata excludes SSH auth args, private key paths,
  full SSH command strings, passwords, secrets, and tokens.
- Interactive terminal transcripts are treated as sensitive because terminal
  output displayed during SSH can be captured.
- `td recent --json` does not expose excessive credential or invocation data.
- `td session show <session_id>` does not dump the full terminal log unless
  `--tail N` is explicitly provided.
- FTP requires `allow_insecure_transfers=true` and `--i-know-its-insecure`.
- Critical confirmation works for connect, exec, run, transfer, config apply,
  and TUI SSH sessions.
- `td export` excludes decrypted secret values unless `--include-secrets` is
  explicitly used.

## 7. Documentation checks

- README quick start works from a clean checkout.
- README install instructions match actual release artifacts.
- `docs/release-artifact-validation.md` matches the workflow behavior.
- `docs/tui.md` reflects current keybindings.
- `docs/security.md` reflects current logging and security policy.
- `docs/internal/session-logging-design.md` reflects current session logging
  backend and security decisions.
- `docs/internal/ssh-invocation-boundary.md` reflects the current SSH boundary.
- `docs/internal/commandset-execution-boundary.md` reflects the current
  CommandSet boundary.
- `CHANGELOG.md` is updated.
- `RELEASE_NOTES_0.1.0.md` is reviewed and ready for the GitHub Release body.
- Known limitations are current and do not overstate production readiness.

## 8. Before publishing the production GitHub Release

- Confirm the `v0.1.0` workflow run completed on Linux and Windows.
- Download all Release assets from GitHub, not only Actions artifacts.
- Re-run checksum verification on downloaded assets.
- Re-run install smoke tests on fresh Linux and Windows environments.
- Confirm release notes, README, and actual asset names match.
- Confirm GitHub Release is not accidentally marked prerelease for `v0.1.0`.
- Confirm no unresolved no-go criteria remain.

## 9. Rollback and recovery

Use conservative recovery for v0.1:

- If a validation tag is wrong, delete the local and remote validation tag.
- If an RC prerelease is wrong, delete the prerelease and validation tag.
- If the production `v0.1.0` tag was pushed but the release workflow failed
  before public use, delete the tag only after confirming no one consumed it.
- If the production release is published and users may have downloaded assets,
  do not silently replace artifacts. Document the issue in the release notes and
  fix forward with `v0.1.1`.
- If a limitation is found after release but artifacts are usable, add it to the
  release notes and plan the fix.

For v0.1, prefer re-tagging only before public consumption. After public
consumption, prefer a new patch release instead of replacing assets in place.
