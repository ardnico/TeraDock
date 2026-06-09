# TeraDock

TeraDock is a CLI/TUI tool for managing connection profiles and safely running reusable command sets across SSH/Telnet/Serial targets.

The main workflow is simple: keep connection profiles in one local database, mark risky targets with a danger level, run a CommandSet against one or more SSH profiles, then inspect stdout, stderr, parsed output, and operation logs.

## Use Cases

- Check the state of multiple Linux servers with the same read-only commands.
- Manage access details for embedded devices, inspection equipment, lab hosts, and maintenance terminals.
- Label production or fragile targets as `high` or `critical` before running actions.
- Capture routine work as a CommandSet instead of retyping command sequences.
- Use the TUI to search, filter, mark profiles, run CommandSets, and review bulk results.

## Quick Start

Build from source:

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
td export -o teradock-export.json
td import --conflict rename teradock-export.json
```

CommandSets are currently created through `td init --with-samples`, import JSON, or direct database-backed tooling. The built-in `linux-basic-check` sample runs only read-only Linux commands: `uname -a`, `uptime`, `df -h`, `free -m`, and `systemctl --failed || true`.

## TUI Basics

Run `td ui`.

- `/` searches profiles.
- `T`, `g`, `D`, `[`, `]`, and `x` filter by type, group, danger, and tags.
- `Space` marks profiles for bulk execution.
- `s` opens an interactive SSH session for the selected SSH profile in the same terminal. TeraDock pauses the TUI, restores normal terminal mode while SSH runs, then returns to the TUI when the session exits.
- `r` runs the selected CommandSet on the selected profile.
- `R` runs the selected CommandSet on marked profiles.
- `1` to `4` switch stdout, stderr, parsed, and summary result tabs.
- `d` opens resolved settings details.
- `?` shows the full key help.

The status line explains why a run is not currently available, such as no selected profile, no CommandSet, or no marked profiles for bulk run.

Interactive SSH sessions require a TTY. If `td ui` is started with redirected stdin/stdout, TeraDock exits with a clear error instead of entering the TUI.

SSH sessions opened from the TUI are recorded in `op_logs` as `ssh_session` operations after the session exits or when process launch fails. Secrets, passwords, SSH auth arguments, and full command strings are not written to the session log metadata. Use `td recent` or `td recent --json` to review recently used interactive SSH profiles.

## Safety Model

Profiles have a danger level: `normal`, `high`, or `critical`. Critical profiles require explicit confirmation before connect, exec, run, transfer, and config apply operations. In the TUI, SSH sessions and single-profile CommandSet execution on critical profiles require typing the shown profile id, and bulk runs require typing the comma-separated critical ids.

Secrets are stored encrypted behind a master password. TeraDock does not print secret values in normal listing commands. Be careful with `td secret reveal` and with `td export --include-secrets`; exports without that flag include only secret metadata.

FTP transfer is treated as insecure and requires explicit opt-in. Prefer SSH-based `scp` or `sftp`.

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
- [Internal CommandSet Execution Boundary](docs/internal/commandset-execution-boundary.md)
- [Internal SSH Invocation Boundary](docs/internal/ssh-invocation-boundary.md)
