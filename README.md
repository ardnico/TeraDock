# TeraDock

TeraDock is a CLI/TUI tool for managing connection profiles and safely running reusable command sets across SSH/Telnet/Serial targets.

The main workflow is simple: keep connection profiles in one local database, mark risky targets with a danger level, run a CommandSet against one or more SSH profiles, then inspect stdout, stderr, parsed output, and operation logs.

Current stable version: **1.0.3**.

## Use Cases

- Check the state of multiple Linux servers with the same read-only commands.
- Manage access details for embedded devices, inspection equipment, lab hosts, and maintenance terminals.
- Label production or fragile targets as `high` or `critical` before running actions.
- Capture routine work as a CommandSet instead of retyping command sequences.
- Use the TUI to search, filter, mark profiles, run CommandSets, and review bulk results.

## Quick Start

Install from a GitHub Release artifact when available:

- Windows: `td-1.0.3-windows-x86_64-setup.exe`
- Linux portable archive: `td-1.0.3-linux-x86_64.tar.gz`
- Linux packages: `.deb` and `.rpm` release assets
- Checksums: `SHA256SUMS-linux-x86_64` and `SHA256SUMS-windows-x86_64`

The v1.0.3 release is distributed through GitHub Releases. crates.io publication is not part of this release path.
Before broad use, validate downloaded artifacts with the [release artifact validation guide](docs/release-artifact-validation.md).

To build from source:

```bash
cargo build -p td --release
cargo run -p td -- --help
```

Initialize local data and install the safe sample CommandSet:

```bash
cargo run -p td -- init --with-samples
cargo run -p td -- doctor
```

Add an SSH profile:

```bash
cargo run -p td -- profile add \
  --profile-id lab1 \
  --name "Lab server 1" \
  --host 192.0.2.10 \
  --user admin \
  --danger high \
  --group lab \
  --tag linux
```

Run the sample read-only CommandSet:

```bash
cargo run -p td -- run lab1 linux-basic-check
cargo run -p td -- run lab1 linux-basic-check --json
```

Open the TUI:

```bash
cargo run -p td -- ui
```

After installing the binary as `td`, drop the `cargo run -p td --` prefix.

## CLI Examples

```bash
td init --with-samples
td doctor
td profile list --group lab --tag linux
td profile show lab1
td exec lab1 --timeout-ms 5000 -- uname -a
td run lab1 linux-basic-check --json
td recent --limit 10
td recent --json
td config set session.log.enabled true
td session list
td session path <session_id>
td export -o teradock-export.json
td import --conflict rename teradock-export.json
```

CommandSets are currently created through `td init --with-samples`, import JSON, or direct database-backed tooling. The built-in `linux-basic-check` sample runs only read-only Linux commands: `uname -a`, `uptime`, `df -h`, `free -m`, and `systemctl --failed || true`.

## TUI Basics

Run `td ui`.

- `/` searches profiles.
- `T`, `g`, `D`, `[`, `]`, and `x` filter by type, group, danger, and tags.
- `C` clears filters.
- `Space` marks profiles for bulk execution.
- `s` opens an interactive SSH session for the selected SSH profile in the same terminal. TeraDock pauses the TUI, restores normal terminal mode while SSH runs, then returns to the TUI when the session exits.
- `c` opens the settings screen. Save changes there with `s`; session logging changes apply to the next SSH session opened with `s`.
- `r` runs the selected CommandSet on the selected profile.
- `R` runs the selected CommandSet on marked profiles.
- `1` to `4` switch stdout, stderr, parsed, and summary result tabs.
- `d` opens resolved settings details.
- `?` shows the full key help.

The status line explains why a run is not currently available, such as no selected profile, no CommandSet, or no marked profiles for bulk run.

Interactive SSH sessions require a TTY. If `td ui` is started with redirected stdin/stdout, TeraDock exits with a clear error instead of entering the TUI.

SSH sessions opened from the TUI are recorded in `op_logs` as `ssh_session` operations after the session exits or when process launch fails. Secrets, passwords, SSH auth arguments, and full command strings are not written to the session log metadata. Use `td recent` or `td recent --json` to review recently used interactive SSH profiles.

Interactive SSH terminal transcript logging is available for v1.1 preparation and is disabled by default:

```bash
./td session doctor
./td config ui
./td config set session.log.enabled true
./td config set session.log.backend auto
./td config set session.log.backend conpty
./td config get session.log.dir --resolved
./td session list
td session conpty-test <profile_id>
td connect <profile_id> --log-backend conpty
td session show <session_id>
td session path <session_id>
```

Use `td session doctor` to see whether logging is enabled, which backend will be used, backend status (`ready`, `degraded`, or `not_ready`), content-capture reliability, dependency availability, whether the log directory looks writable, and which saved session log is newest. On Windows it also prints the explicit ConPTY backend command, the PoC command, and the current ConPTY candidate label. Use `td config ui` for the BIOS-style settings screen outside the TUI, or press `c` inside `td ui`; the settings screen can change `session.log.enabled`, `session.log.backend`, and `session.log.dir` and shows the same readiness diagnostics.

When enabled on Linux/macOS, TeraDock uses the `script` backend when available and saves terminal logs plus metadata under `<data_dir>/session-logs` unless `session.log.dir` is configured.

On Windows, `auto` still resolves to `no-log` with `windows_terminal_content_logging_requires_explicit_conpty`. To capture SSH terminal I/O from the TUI `s` path on Windows, explicitly enable ConPTY and then open `td ui`:

```powershell
td config set session.log.enabled true
td config set session.log.backend conpty
td ui
```

The TUI `s` path uses ConPTY only when those saved settings are explicit and the selected profile is SSH. `td connect <profile_id> --log-backend conpty` can request the same backend for one CLI SSH connect. `powershell-transcript` is available only when explicitly configured and is marked best-effort/degraded because it may capture only PowerShell host transcript metadata, not SSH-side input/output. `td session conpty-test <profile_id>` remains available as a focused Windows ConPTY smoke command. ConPTY uses `portable-pty`; basic manual smoke has shown SSH login, visible remote output, saved log output, metadata, and `session list/show/path` compatibility, so the explicit candidate label is `experimental_ready`. It is still degraded and is not selected by `auto`. Terminal output shown during a captured session may include passwords, tokens, or secrets and can be written to the log file. Session log metadata excludes SSH auth args, full command strings, private key paths, passwords, secrets, and tokens. Saved settings affect SSH sessions started after the save.

## Safety Model

Profiles have a danger level: `normal`, `high`, or `critical`. Critical profiles require explicit confirmation before connect, exec, run, transfer, and config apply operations. In the TUI, SSH sessions and single-profile CommandSet execution on critical profiles require typing the shown profile id, and bulk runs require typing the comma-separated critical ids.

Secrets are stored encrypted behind a master password. TeraDock does not print secret values in normal listing commands. Be careful with `td secret reveal` and with `td export --include-secrets`; exports without that flag include only secret metadata.

FTP transfer is treated as insecure and requires explicit opt-in. Prefer SSH-based `scp` or `sftp`.

Interactive session logging is a separate terminal transcript feature, not `op_logs`. It is disabled by default because the transcript can capture any sensitive text displayed in the terminal. Session log metadata excludes SSH auth args, full command strings, private key paths, passwords, secrets, and tokens, but displayed terminal output is not masked.

## Import And Export

Use export/import to move local TeraDock data between machines, seed CommandSets, or back up profiles:

```bash
td export -o teradock-export.json
td import --conflict reject teradock-export.json
td import --conflict rename teradock-export.json
```

The export format includes profiles, CommandSets, parser definitions, config sets, and secret metadata. Secret values are excluded unless `--include-secrets` is used.

## Platform Notes

TeraDock is tested on Windows and Linux in CI. SSH actions require an external `ssh` client. File transfer features use `scp`, `sftp`, or explicitly allowed `ftp`. Serial support depends on local serial device names and permissions, which differ by OS.

Interactive session logging uses `script` on Linux/macOS. Windows SSH terminal-content logging requires explicit ConPTY selection: set `session.log.enabled=true` and `session.log.backend=conpty`, or use `td connect <profile_id> --log-backend conpty`. `auto` still does not choose ConPTY. The optional PowerShell Transcript backend is experimental best-effort and may miss SSH-side commands and output.

## Project Operations

- Report bugs with the GitHub bug report template. Remove secrets, passwords, tokens, private keys, and mask SSH host/user values before posting logs.
- Request features with the GitHub feature request template and check [Roadmap](ROADMAP.md) for the current 1.0.x and 1.1 scope.
- Security-sensitive issues should not include public details. See [Security Policy](SECURITY.md).
- Contributions should follow [Contributing](CONTRIBUTING.md).

## Known Limitations

- TUI recent-profile browsing is not implemented; use `td recent` or `td recent --json`.
- Terminal emulator launch and tmux integration are not implemented.
- ConPTY-based logging is an explicit Windows experimental backend; basic logging smoke has succeeded, but production/default `auto` integration is not implemented in the v1.1 candidate path.
- CommandSet execution still receives SSH path and auth args separately inside `tdcore::cmdset_runner`.
- Transfer and tunnel command shapes are not fully represented by `SshInvocation` yet.
- Automated tests do not include real SSH server integration tests.

## What TeraDock Is Not

- It is not a full replacement for configuration management tools such as Ansible.
- It is not a Web UI.
- It is not a password sharing tool.
- It is not a monitoring system by itself.

## More Documentation

- [Getting Started](docs/getting-started.md)
- [CommandSets](docs/commandsets.md)
- [TUI](docs/tui.md)
- [Security](docs/security.md)
- [Security Policy](SECURITY.md)
- [Contributing](CONTRIBUTING.md)
- [Roadmap](ROADMAP.md)
- [Post-Release Audit 1.0.3](docs/post-release-audit-1.0.3.md)
- [Release Artifact Validation](docs/release-artifact-validation.md)
- [Release Checklist](RELEASE_CHECKLIST.md)
- [Historical Release Notes 0.1.0](RELEASE_NOTES_0.1.0.md)
- [Changelog](CHANGELOG.md)
- [Internal CommandSet Execution Boundary](docs/internal/commandset-execution-boundary.md)
- [Internal SSH Invocation Boundary](docs/internal/ssh-invocation-boundary.md)
- [Internal Session Logging Design](docs/internal/session-logging-design.md)
