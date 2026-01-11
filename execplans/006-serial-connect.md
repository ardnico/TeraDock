# Serial connect passthrough for v0.1

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Enable PROJECT_PLAN.md Phase 15’s serial connect path so users can select a serial profile and get a raw passthrough session (stdin to device, device to stdout) using system TTYs. After this change, `td connect <serial_profile>` should open the configured serial device at the configured baud rate, log the operation, and return cleanly on EOF or error while keeping secrets out of logs.

## Progress

- [x] (2026-01-10 01:58Z) Added serial connect implementation in the CLI with raw-mode passthrough and op_log entries.
- [x] (2026-01-10 01:58Z) Added `serialport` and `crossterm` workspace dependencies to support serial I/O and raw terminal mode.
- [ ] (2026-01-10 01:58Z) Run `cargo test` (blocked by crates.io CONNECT 403 in this environment; retry when registry is reachable).
- [ ] (2026-01-10 15:56Z) Retried `cargo test`; crates.io CONNECT 403 persists, so workspace validation remains blocked.
- [ ] (2026-01-11 04:22Z) Retried `cargo test`; crates.io CONNECT 403 persists (failed to download data-encoding), so workspace validation remains blocked.
- [ ] (2026-01-11 16:55Z) Retried `cargo test`; crates.io CONNECT 403 persists (failed to download config.json).
- [ ] (2026-01-11 17:09Z) Retried `cargo test`; crates.io CONNECT 403 persists (failed to download config.json).

## Surprises & Discoveries

- Cargo registry access is currently blocked again (CONNECT tunnel 403), so new dependencies cannot be fetched and tests cannot be executed in this environment.
- Serialport IO requires a short timeout to keep the read thread responsive so it can observe the shutdown flag.
- Reattempted `cargo test` on 2026-01-10 still failed with CONNECT 403 while fetching the crates.io index.
- Retried `cargo test` on 2026-01-11 and still hit CONNECT 403 while downloading the crates.io index (data-encoding).
- Retried `cargo test` on 2026-01-11 and still hit CONNECT 403 while downloading config.json from crates.io.

## Decision Log

- Decision: Map `profiles.host` to the serial device path and `profiles.port` to the baud rate for v0.1 serial connect.
  Rationale: The existing schema does not include serial-specific fields, so reusing host/port keeps the change minimal while still exposing a controllable serial configuration.
  Date/Author: 2026-01-10 / assistant

## Outcomes & Retrospective

Serial connect is implemented with raw-mode passthrough, but workspace tests are blocked until crates.io registry access is restored.

## Context and Orientation

Serial profiles are already modeled by `ProfileType::Serial` in `crates/core/src/profile.rs`, but `td connect` currently returns an error for serial profiles. SSH and telnet connect use system clients and log to `op_logs`. The SQLite schema stores a `host` string and `port` number for all profiles; for serial this plan interprets `host` as the device path (e.g., `/dev/ttyUSB0` or `COM3`) and `port` as the baud rate (e.g., `9600`). The CLI owns the connect logic in `crates/cli/src/main.rs`.

## Plan of Work

Implement serial connect entirely in the CLI. Add dependencies on `serialport` for opening the device and `crossterm` for raw terminal mode. Add a small raw-mode guard to ensure raw mode is disabled on exit. Create a `run_serial_session` helper that spawns a thread to read from the serial device and write to stdout while the main thread reads stdin and writes to the serial port. Use a shared atomic flag to stop the reader thread when the stdin loop ends. Update `handle_connect` to route serial profiles to the new `connect_serial` function and record op_log entries with `client_used` set to a descriptive `serialport:<device>` string.

## Concrete Steps

- Working directory: `/workspace/TeraDock`.
- Add workspace dependencies in `Cargo.toml` and `crates/cli/Cargo.toml` for `serialport` and `crossterm`.
- In `crates/cli/src/main.rs`, add:
  - `RawModeGuard` that calls `enable_raw_mode()` on entry and `disable_raw_mode()` on drop.
  - `connect_serial(store, profile)` that opens the device with `serialport::new(&profile.host, profile.port as u32)` and runs `run_serial_session` while capturing duration and logging to `op_logs`.
  - `run_serial_session` that starts a reader thread (`port.try_clone()`) and loops over stdin writes until EOF or error.
- Update the `Connect` command help text to mention serial.
- Run `cargo test` at the repository root and capture output.

## Validation and Acceptance

- `td connect <serial_profile>` opens the device at the specified baud rate and echoes serial output to stdout while sending stdin to the device.
- When the session ends (stdin EOF or error), raw terminal mode is restored.
- `op_logs` records a `connect` row with `client_used` set to `serialport:<device>` and `ok` reflecting the session result.
- `cargo test` passes once registry access is available.

## Idempotence and Recovery

Serial connect does not mutate persistent state other than `last_used_at` and `op_logs`. Re-running is safe. If raw mode is left enabled unexpectedly, rerun the CLI and ensure the guard drops cleanly; otherwise, reset the terminal with `stty sane` on Unix or restart the shell on Windows.

## Artifacts and Notes

- None yet; add a short session transcript once testing is possible.

## Interfaces and Dependencies

- New dependencies: `serialport` and `crossterm` in the workspace and CLI crate.
- CLI additions in `crates/cli/src/main.rs`:
  - `connect_serial(store: &ProfileStore, profile: Profile) -> Result<()>`
  - `run_serial_session(port: &mut Box<dyn serialport::SerialPort>) -> Result<()>`
  - `RawModeGuard` for terminal state management.

Update 2026-01-11 04:22Z: Retried `cargo test` and recorded the ongoing crates.io CONNECT 403 failure in Progress and Surprises.
Update 2026-01-11 17:09Z: Retried `cargo test`; registry access remains blocked (CONNECT 403).
