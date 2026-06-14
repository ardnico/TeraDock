# Windows ConPTY Session Logging Design

## Goal

- Save interactive SSH terminal input/output on Windows.
- Preserve the normal same-terminal SSH experience while teeing terminal bytes to a session log file.
- Keep metadata small and safe: no SSH auth args, full command strings, private key paths, passwords, secrets, or tokens.

## Why PowerShell Transcript Is Insufficient

PowerShell Transcript records the PowerShell host transcript. It is not a PTY recorder for an external interactive console program such as `ssh.exe`.

Observed limitations:

- External interactive console I/O may not be fully captured.
- SSH-side commands and remote shell output can be absent.
- A saved log can contain only transcript start/end metadata.
- The result can look like logging succeeded while the SSH terminal content is missing.

For this reason, `powershell-transcript` must remain an explicit best-effort/degraded backend, and Windows `auto` must not select it as the default terminal-content logging path.

## Candidate Implementation

Use the Windows ConPTY / Pseudo Console API to run `ssh.exe` under a pseudo console controlled by TeraDock. The current PoC chooses `portable-pty` because it gives the smallest maintained Rust API for spawning `ssh.exe` under the native Windows ConPTY implementation, taking a reader/writer for terminal I/O, resizing the pseudo console, and waiting for the child exit status.

Current PoC implementation pieces:

- Spawn `ssh.exe` as a ConPTY child process through `portable-pty`.
- Read key events from the user's current terminal in raw mode and write a minimal byte translation into ConPTY.
- Pump pseudo console output back to the user's terminal and tee the same bytes to the session log file.
- Preserve exit code propagation from `ssh.exe`.
- Handle terminal resize events by calling the ConPTY resize API when crossterm reports a resize.
- Handle Ctrl-C as a raw `0x03` byte sent to the child and rely on a raw-mode guard to restore the local terminal.
- Write initial logs as UTF-8 best-effort text; ANSI escape sequences may be preserved.
- Keep TUI integration out of scope until the PoC succeeds in manual smoke.

## Stabilization Status

The 2026-06-14 stabilization pass did not include pasted manual smoke output.
The implementation was therefore hardened only where source inspection showed
low-risk PoC improvements:

- Raw mode is entered before launching the ConPTY child so setup failures do not
  leave a running child with an unprepared input bridge.
- Input-loop errors now restore raw mode, terminate/wait for the child, close
  PTY handles, and join the output tee thread before returning.
- Output tee writes and flushes the log before writing to the local display for
  each chunk, so a display write failure does not discard bytes that were
  already read from ConPTY.
- Resize dimensions are clamped to at least `1x1` before forwarding to ConPTY.
- Windows ConPTY exit codes are converted into the current metadata `i32`
  range without wrapping large `u32` values negative.

Manual evidence is still required before treating ConPTY as more than an
explicit PoC. Use `docs/internal/windows-conpty-manual-smoke.md` to collect the
next run.

## Phase 1 Findings

- PowerShell Transcript does not capture the missing content: SSH-side typed commands, remote shell output, interactive prompt I/O, and other terminal content after `ssh.exe` takes over can be absent. The saved file can contain only PowerShell transcript start/end metadata.
- Existing metadata warning behavior is correct: explicit `powershell-transcript` is marked best-effort/degraded, `content_capture_reliable=false`, and host-only/empty logs are annotated with `host_only_or_empty`.
- The ConPTY PoC implementation scope is a Windows-only explicit CLI, SSH invocation reuse from profile id, ConPTY child spawn, terminal input/output bridge, output tee to the session log file, metadata completion, exit-code propagation, and `td session list/show/path` compatibility.
- TUI integration is avoided because the existing TUI owns raw-mode, alternate-screen, mouse capture, and same-terminal suspend/restore behavior. Mixing that with an unproven PTY bridge would increase the blast radius before the Windows terminal behavior is manually proven.

## PoC Success Criteria

- `ssh.exe` can be launched under ConPTY.
- User can type commands interactively.
- Remote shell output is visible in the current terminal.
- The same terminal output is written to a session log file.
- SSH exit code is propagated.
- Ctrl-C does not leave the terminal broken.
- Resize handling is documented and the PoC forwards resize events when crossterm reports them.
- UTF-8/Japanese output is not obviously corrupted.

No success criterion is considered satisfied by source inspection alone. The
criteria require a manual Windows run transcript that includes the command,
typed remote commands, `td session show`, metadata JSON, log text, Ctrl-C,
resize, UTF-8, and controlled failure-case observations.

## PoC No-Go Criteria

- Input/output is unreliable.
- Ctrl-C breaks the terminal.
- SSH authentication prompt becomes unusable.
- Captured output is only host/process metadata.
- Implementation requires a full terminal emulator layer.
- Metadata requires storing secret/auth/full command data.

## Backend Status Model

- `disabled`: session logging is off.
- `not_ready`: the selected backend cannot run on the current platform or is missing required dependencies.
- `degraded`: the backend is available but best-effort, experimental, or not reliable enough to treat as the production terminal-content path.
- `ready`: the backend is available and considered suitable for the supported platform.

PowerShell Transcript remains `degraded`. The ConPTY PoC is also treated as experimental/degraded. Windows `auto` still resolves to `no-log`; it does not choose ConPTY.

## Log Format

- Initial ConPTY logs are UTF-8 best-effort text.
- ANSI escape sequences may be preserved.
- Perfect replay is not guaranteed.
- Future versions may consider asciinema-compatible output.

## Risks

- Implementation complexity is materially higher than the `script` or PowerShell wrapper backends.
- Windows version and terminal-host differences can affect behavior.
- Resize handling can be fragile.
- Ctrl-C and control-sequence behavior can diverge from direct `ssh.exe`.
- Secrets, passwords, tokens, prompt responses, pasted text, and remote command output displayed in the terminal can be logged.
- Automated tests are difficult because a real interactive SSH server should not be required.
- The PoC log is a byte transcript, not a full terminal replay. ANSI escape
  sequences may remain in the file and resize does not rewrite earlier screen
  state.

## Non-goals

- Production/default backend promotion during the PoC.
- Full terminal emulator.
- Perfect replay format.
- Secret masking.
- Storing SSH auth args, full command strings, or private key paths in metadata.
- Automatic `auto -> conpty` selection.
- TUI integration before manual smoke evidence.

## Suggested Roadmap

- v1.1.x: Keep Windows `auto` on `no-log`; keep `powershell-transcript` explicit best-effort/degraded; expose ConPTY only as `td session conpty-test <profile_id>`; surface metadata, doctor, show, and config UI warnings.
- v1.2: Stabilize the ConPTY proof of concept for `ssh.exe` with manual smoke evidence for terminal I/O teeing, resize handling, Ctrl-C behavior, exit code propagation, UTF-8/Japanese output, and log metadata.
- v1.3: Evaluate productionizing ConPTY as the reliable Windows SSH terminal-content backend.
