# Getting Started

This guide gets you from an empty local database to a safe CommandSet run.

## 1. Build Or Install

From the repository root:

```bash
cargo build -p td --release
```

During development, examples can be run as:

```bash
cargo run -p td -- <command>
```

After installing or copying the binary, use `td <command>`.

## 2. Initialize Local Data

```bash
td init --with-samples
td doctor
```

`td init` creates the local config directory and SQLite database if needed. It does not delete or overwrite existing data. `--with-samples` installs `linux-basic-check` only when that CommandSet id is not already present.

## 3. Add A Profile

```bash
td profile add \
  --profile-id lab1 \
  --name "Lab server 1" \
  --host 192.0.2.10 \
  --user admin \
  --danger high \
  --group lab \
  --tag linux
```

Use `--danger critical` for production or fragile targets that should require explicit confirmation.

## 4. Run A CommandSet

```bash
td run lab1 linux-basic-check
td run lab1 linux-basic-check --json
```

The sample CommandSet is read-only and intended for Linux hosts.

## 5. Use The TUI

```bash
td ui
```

Search with `/`, mark profiles with `Space`, run the selected CommandSet with `r`, and run marked profiles with `R`.
