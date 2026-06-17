# TeraDock TUI Session Logging Integration Result

Date: 2026-06-17

## Summary

TUI `s` interactive SSH sessions now use the shared session logging plan path already used by the CLI session logging work. The integration remains limited to `session.log.enabled=true` plus a backend that resolves explicitly for the current platform. Windows ConPTY is used only when `session.log.backend=conpty`; Windows `auto` still resolves to `no-log`.

## Phase 1 Findings

- Current TUI `s` path: `crates/tui/src/app.rs` maps `s` to `OpenSshSession`, then `AppState::build_ssh_session_command()` builds a shared `tdcore::ssh::SshInvocation` with source `tui` and mode `interactive`.
- Logging boundary: `crates/tui/src/state.rs` attaches `tdcore::session_log::SessionLogPlan` to the TUI SSH session command, and `crates/tui/src/app.rs` dispatches plain/script/PowerShell Transcript/ConPTY execution after suspending the TUI terminal.
- Fragile lifecycle area: anything after raw-mode disable and alternate-screen exit must not skip terminal restore. The risky section was the suspended block before backend execution and notice flushing.
- Minimum ConPTY TUI scope: keep the existing TUI suspend/resume path, call the shared ConPTY runner only for explicit Windows `conpty`, write metadata through `complete_conpty_session` / `complete_conpty_failure_session`, and record only safe `op_logs` references.
- Not touched: terminal emulator launch, tmux, SSH server integration tests, `auto -> conpty`, secret masking of terminal transcript bodies, and broader transfer/tunnel SSH invocation cleanup.

## Backend Selection

- `session.log.enabled=false` -> normal SSH, no terminal log.
- Linux/macOS `auto` or `script` -> `script` when available.
- Windows `auto` -> `no-log` with `windows_terminal_content_logging_requires_explicit_conpty`.
- Windows explicit `conpty` -> ConPTY runner for TUI `s`.
- Windows explicit `powershell-transcript` -> degraded/best-effort PowerShell Transcript.
- Unsupported explicit backend -> TUI status reports a backend setup/launch failure instead of silently running an unlogged session.

## Changes Made

- Hardened `run_interactive_ssh_session()` so the session body runs while suspended, then the TUI restore path is always attempted before returning the session result or error.
- Updated TUI success/failure status messages to include saved session IDs when metadata/logs are written, for example `SSH session ended: exit 0, log saved: sl_xxxxx`.
- Added `tui_integration` to session logging diagnostics and surfaced it in `td session doctor` and the BIOS-style settings diagnostics panel as `TUI logging`.
- Kept `op_logs` integration limited to safe cross-reference fields: `session_log_saved`, `session_log_id`, or `session_log_reason`.
- Updated README, TUI docs, security docs, internal session logging design, Windows ConPTY design, Windows manual smoke checklist, roadmap, and release checklist.

## Metadata And Security

TUI session metadata continues to include safe target/session fields such as session id, profile id, user, host, port, timestamps, duration, exit code, backend, log path, metadata path, status, capture reliability, and backend warnings. It does not store full command strings, auth args, private key paths, passwords, tokens, or secrets.

Terminal transcript bodies can still contain any password, token, secret, pasted text, prompt response, or command output displayed by the terminal. Runtime warnings and docs continue to state this.

## Auto Promotion Decision

No `auto -> conpty` promotion was made. Windows `auto` remains `no-log` because ConPTY still needs broader Ctrl-C, timeout, bad host, auth failure, UTF-8/Japanese, child cleanup, TUI return, and terminal-host evidence before default selection is safe.

## Validation

- `cargo fmt --check`: passed.
- `cargo test`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo build -p td --release --locked`: passed.
- `.\target\release\td.exe session doctor`: passed; output included `TUI logging: enabled for s-key SSH sessions` with the current local PowerShell Transcript config.

## Manual Smoke

Real SSH manual smoke was not run in this automated pass. The updated smoke path is:

```powershell
td config set session.log.enabled true
td config set session.log.backend conpty
td session doctor
td ui
```

Then select an SSH profile, press `s`, run harmless remote commands such as `pwd`, `ls`, `echo 日本語テスト`, exit, and verify `td session list`, `td session show <session_id>`, `td session path <session_id>`, the log body, metadata, Ctrl-C recovery, and absence of leftover `ssh` children.

## Remaining Work

- Run full Windows TUI ConPTY manual smoke against controlled SSH infrastructure.
- Record Ctrl-C abort, startup timeout, bad host, auth failure, UTF-8/Japanese, resize, and clean child-process evidence.
- Improve failure-status wording after more real ConPTY failure samples are collected.
- Re-evaluate explicit ConPTY stability only after the above evidence exists; consider `auto` only in a later, separate promotion decision.
