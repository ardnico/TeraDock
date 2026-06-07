# CommandSets

A CommandSet is a stored sequence of command steps. TeraDock runs the steps in order against an SSH profile, captures stdout and stderr, applies optional parsers, records an operation log entry, and updates the profile's `last_used_at`.

## Built-In Sample

Install the sample with:

```bash
td init --with-samples
```

`linux-basic-check` contains read-only Linux commands:

```text
uname -a
uptime
df -h
free -m
systemctl --failed || true
```

The installer skips the sample when a CommandSet with the same id already exists.

## Step Fields

CommandSet data is stored in the SQLite `cmdsets` and `cmdsteps` tables and is included in import/export JSON.

- `cmd`: command string passed to SSH.
- `timeout_ms`: optional per-step timeout in milliseconds.
- `on_error`: `stop` stops at the first failing step; `continue` records the failure and runs the next step.
- `parser_spec`: `raw`, `json`, or `regex:<parser_id>`.

## Parsers

- `raw` keeps parsed output as an empty object.
- `json` parses stdout as JSON and falls back to an empty object when stdout is not valid JSON.
- `regex:<parser_id>` loads a regex parser definition from the `parsers` table and returns matching capture groups.

## Safer CommandSet Design

Prefer read-only commands for bulk runs:

```text
uname -a
uptime
df -h
journalctl -p err -n 20 --no-pager
```

Treat commands that restart services, modify files, erase data, change networking, or write firmware as dangerous. Put those in a separate CommandSet with clear naming, narrow profile filters, short timeouts, and `on_error=stop`.

## Bulk Run Notes

Bulk run in the TUI executes the selected CommandSet across marked profiles. Critical profiles require a typed confirmation. The summary tab shows per-profile success or failure; stdout, stderr, and parsed tabs show the most recently executed profile.

Use small read-only CommandSets first when validating new profile groups.
