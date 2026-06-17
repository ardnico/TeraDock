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
td config ui
td recent --json
td session doctor
td session list --json
```

Windows ConPTY explicit backend smoke, only on a controlled Windows SSH profile:

```powershell
.\target\release\td.exe config set session.log.enabled true
.\target\release\td.exe config set session.log.backend conpty
.\target\release\td.exe session doctor
.\target\release\td.exe session conpty-test <profile_id>
.\target\release\td.exe connect <profile_id> --log-backend conpty
.\target\release\td.exe ui
.\target\release\td.exe session list
.\target\release\td.exe session show <session_id>
.\target\release\td.exe session path <session_id>
```

Follow [Windows ConPTY Manual Smoke](docs/internal/windows-conpty-manual-smoke.md).
Confirm SSH login, terminal output display, output capture in the log,
`exit_code` in metadata, Ctrl-C recovery, resize behavior, UTF-8 behavior, and
controlled failure behavior. This check does not promote ConPTY to `auto`.

## 5. TUI smoke tests

- `td ui` starts in an interactive TTY.
- `/` search works.
- Type, group, danger, tag, and query filters work.
- Details view opens.
- `c` opens settings, `s` saves settings there, and returning to `td ui`
  refreshes state.
- `s` opens an SSH session for an SSH profile.
- With `session.log.enabled=false`, `s` opens SSH without saving a terminal
  transcript.
- `td session doctor` reports disabled/ready/degraded/not_ready status, backend
  resolution, `script`, PowerShell, and `ssh` availability, log directory
  state, newest saved session log, platform support, fallback reason, capture
  reliability, and warning.
- With `session.log.enabled=true` on Linux/macOS, `s` uses `script` when
  available and `td session list` shows the saved metadata after return.
- On Windows, enabled `auto` session logging resolves to `no-log` with
  `windows_terminal_content_logging_requires_explicit_conpty`.
- On Windows, explicit `powershell-transcript` reports `degraded`,
  `content_capture=best_effort`, and warns that interactive SSH input/output
  may not be captured. Missing PowerShell or `ssh` for that explicit backend
  reports `powershell_not_found` or `ssh_not_found`.
- On Windows, ConPTY remains explicit and experimental. With
  `session.log.enabled=true` and `session.log.backend=conpty`, TUI `s` uses
  ConPTY for SSH profiles, returns to the TUI after `exit`, and leaves
  `session list/show/path` usable. It is not selected by `auto`.
- `td session doctor` and the settings diagnostics panel show `TUI logging:
  enabled for s-key SSH sessions` only when the resolved backend can be used
  for TUI `s` sessions; unsupported explicit backends show a not-ready status.
- If a TUI/ConPTY smoke breaks terminal mode, recover with `Ctrl-C`, `reset`
  where available, or by reopening the terminal, then confirm no leftover
  `td` or `ssh` child from that run remains.
- Host-only or empty PowerShell transcripts add `content_capture_status` and
  `content_capture_warning`, and `td session show <session_id>` displays the
  warning.
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
- Windows PowerShell Transcript is not treated as reliable SSH terminal-content
  logging; full Windows support requires a ConPTY backend.
- Windows ConPTY PoC metadata excludes SSH auth args, private key paths, full
  SSH command strings, passwords, secrets, and tokens. Treat ConPTY log files as
  sensitive terminal transcripts because displayed output and echoed input can
  be captured.
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
- `docs/internal/windows-conpty-session-logging-design.md` reflects the planned
  Windows full terminal-content backend.
- `docs/internal/windows-conpty-manual-smoke.md` reflects current ConPTY PoC
  GO/NO-GO criteria and known constraints.
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
