# Windows ConPTY Session Logging PoC Result

## Summary

Implemented a Windows-only ConPTY session logging proof of concept behind the explicit command:

```powershell
td session conpty-test <profile_id>
```

This is a PoC only. Windows `auto` still resolves to `no-log`, PowerShell Transcript remains best-effort/degraded, and the TUI `s` path is not integrated with ConPTY.

## Phase 1 Findings

- PowerShell Transcript did not capture SSH-side typed commands, remote shell output, or interactive terminal content after `ssh.exe` took over. The observed log contained only PowerShell transcript start/end host metadata.
- Existing metadata warnings are correct: `powershell-transcript` records `content_capture=best_effort`, `content_capture_reliable=false`, and host-only/empty captures are marked with `host_only_or_empty`.
- ConPTY PoC scope was limited to explicit CLI launch, SSH profile resolution, ConPTY child spawn, terminal I/O bridge, output tee to the session log file, metadata completion, exit code propagation, and `td session list/show/path` compatibility.
- TUI integration was intentionally avoided because the TUI owns raw-mode, alternate-screen, mouse-capture, and suspend/restore lifecycle. ConPTY terminal behavior should be manually proven before adding that risk.

## ConPTY Crate/API Selection

Selected `portable-pty = 0.9.0`.

Rationale:

- Provides a maintained cross-platform PTY API backed by Windows ConPTY.
- Can spawn `ssh.exe` under a PTY with `CommandBuilder`.
- Exposes a master reader/writer for output teeing and input forwarding.
- Exposes child `try_wait`/exit status and PTY resize.
- Keeps the PoC smaller than a direct Windows API wrapper.

## PoC CLI

- Added `td session conpty-test <profile_id>`.
- Windows only; non-Windows returns `unsupported`.
- Resolves `profile_id` through the existing SSH invocation boundary.
- Rejects non-SSH profiles.
- Preserves critical-profile confirmation before connecting.
- Does not store full command strings, SSH auth args, private key paths, passwords, secrets, or tokens in metadata.

## Session Metadata/Log Behavior

- Allocates files under the configured `session.log.dir`.
- Writes terminal output from ConPTY to `<session_id>.log`.
- Writes JSON metadata compatible with `td session list`, `td session show`, and `td session path`.
- Uses `backend=conpty`.
- Marks capture as `content_capture=best_effort`, `content_capture_reliable=false`, and `backend_warning=conpty_backend_is_experimental_poc`.
- Logs may contain secrets displayed in the terminal. PoC does not mask terminal output.

## Manual Smoke

Succeeded:

- Not run against a real SSH server in this automated implementation pass.

Failed:

- Not applicable; no controlled Windows SSH target/profile was supplied for interactive manual smoke.

Still required on Windows:

```powershell
.\target\release\td.exe session doctor
.\target\release\td.exe session conpty-test <profile_id>
.\target\release\td.exe session list
.\target\release\td.exe session show <session_id>
.\target\release\td.exe session path <session_id>
```

Manual checks to record:

- SSH login works.
- `pwd` / `ls` output is visible.
- The same output is present in the log file.
- `exit` returns to the local terminal.
- `exit_code` is written to metadata.
- Ctrl-C does not leave the terminal broken.
- Resize behavior is acceptable or documented as a limitation.
- UTF-8/Japanese output is not obviously corrupted.

## Ctrl-C, Resize, UTF-8 Status

- Ctrl-C: implemented as raw `0x03` forwarding to the ConPTY child, with a raw-mode guard restoring the local terminal.
- Resize: implemented for crossterm resize events by forwarding the new size to ConPTY.
- UTF-8/Japanese: log format is UTF-8 best-effort text; not manually verified against a real SSH target in this pass.

## Tests

Automated tests do not use a real SSH server.

Covered:

- `conpty` backend enum/config schema parsing.
- Windows explicit ConPTY diagnostics as experimental/degraded.
- Standard `td connect`/TUI planning does not use ConPTY and returns `conpty_backend_poc_only`.
- Non-Windows ConPTY is unsupported.
- `td session conpty-test <profile_id>` CLI parsing.
- Non-SSH profile rejection through the shared SSH invocation boundary.
- ConPTY metadata excludes auth args, full command strings, private key paths, passwords, secrets, and tokens.
- `td session show` reports ConPTY as degraded with the experimental backend warning.
- Minimal Windows key-event to PTY byte mapping for text, arrows, and Ctrl-C.

Validation commands run in this pass:

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
.\target\release\td.exe session doctor
```

Results:

- `cargo fmt --check`: passed.
- `cargo test`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo build -p td --release --locked`: passed.
- `.\target\release\td.exe session doctor`: passed. The local Windows config currently resolves to `powershell-transcript`, reports `Status: degraded`, and prints `ConPTY backend: experimental` plus `ConPTY PoC command: td session conpty-test <profile_id>`.

## Not Implemented

- No `auto -> conpty` promotion.
- No TUI `s` ConPTY integration.
- No full terminal emulator.
- No secret masking for terminal output.
- No asciinema/replay format.
- No automated real-server SSH integration test.

## Auto Backend Promotion Decision

Do not promote ConPTY to `auto` yet.

Required evidence before promotion:

- Successful manual smoke on Windows with multiple auth prompts and normal shell usage.
- Ctrl-C recovery evidence.
- Resize behavior evidence.
- UTF-8/Japanese output evidence.
- Failure-mode evidence for bad host, auth failure, and nonzero SSH exit.
- Review that metadata still excludes auth args, full commands, private key paths, and secret values.

## Next Stability Improvements

- Add a structured manual smoke template for ConPTY runs.
- Capture and document known terminal-key limitations beyond the minimal key mapping.
- Add stronger output-thread shutdown/error reporting.
- Consider an asciinema-compatible log format only after text logging is reliable.
- Evaluate whether the ConPTY input bridge should move into a reusable module before TUI integration.
