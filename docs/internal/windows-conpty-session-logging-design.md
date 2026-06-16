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
- Treat Ctrl-C as a parent-side emergency abort for the PoC and rely on a raw-mode guard to restore the local terminal.
- Write initial logs as UTF-8 best-effort text after stripping or normalizing common terminal control sequences.
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

The 2026-06-16 evidence pass found one successful saved ConPTY session
(`sl_dczccyww`) from profile `p_ojql3dws`. That session proves the basic path:
SSH login produced visible remote output, the output was saved to the session
log, metadata was written with `backend=conpty`, `status=completed`, and
`exit_code=0`, and `td session list/show/path` could inspect the session.

This is enough to call the explicit candidate `experimental_ready`, but not
enough to call it `ready`. Ctrl-C abort, startup timeout, bad host, auth
failure, UTF-8/Japanese edge cases, child cleanup in a clean process snapshot,
and broader Windows terminal coverage still need recorded evidence.

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

Source inspection alone is not evidence. As of 2026-06-16, the successful
`sl_dczccyww` smoke satisfies only the basic login/output/log/metadata/list
items. The remaining criteria require manual Windows run evidence that includes
Ctrl-C, resize, UTF-8/Japanese, bad host, auth failure, timeout, and clean child
process observations.

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
- `experimental_ready`: ConPTY-only candidate label for the explicit backend after basic manual smoke has succeeded, while the overall diagnostic status remains `degraded`.
- `ready`: the backend is available and considered suitable for the supported platform.

PowerShell Transcript remains `degraded`. The ConPTY PoC command remains experimental. The explicit `session.log.backend=conpty` candidate can be described as `experimental_ready`, but diagnostics still report `Status: degraded` until broader Windows and TUI evidence exists. Windows `auto` still resolves to `no-log`; it does not choose ConPTY.

Expected doctor shape:

```text
ConPTY backend: experimental_ready
ConPTY PoC command: td session conpty-test <profile_id>
Reason: manual smoke succeeded, but TUI integration and broader Windows validation are pending.
Status: degraded
```

## Promotion Criteria: PoC -> Explicit Stable Backend

ConPTY can be treated as stable for explicit `session.log.backend=conpty` only
after all of the following are recorded:

- SSH login works with a normal OpenSSH profile.
- Remote shell output is visible in the current terminal.
- Remote shell output is saved to the session log.
- User-entered commands are captured at an acceptable level.
- `exit` returns to the local shell.
- SSH exit code is recorded.
- Ctrl-C abort returns to the local shell.
- Child `ssh.exe` does not remain after abort/exit.
- Startup timeout produces failed metadata.
- Bad host produces failed metadata.
- Auth failure does not corrupt the terminal.
- UTF-8/Japanese output is acceptable.
- Metadata excludes auth args, full command strings, private key paths, passwords, tokens, and secrets.

## Promotion Criteria: Explicit Backend -> Auto Backend

Auto promotion requires more evidence than explicit backend stabilization:

- Multiple Windows environments verified.
- Windows Terminal and PowerShell tested.
- At least one failure mode per category is documented.
- Docs clearly warn about captured terminal secrets.
- No known terminal restore bug.
- No known child process leak.
- TUI integration has passed manual smoke.

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
- v1.2: Move the explicit ConPTY candidate from `experimental_ready` to explicit stable only after Ctrl-C, timeout, bad host, auth failure, UTF-8/Japanese, cleanup, and broader Windows terminal evidence is recorded.
- v1.3: Evaluate productionizing ConPTY as the reliable Windows SSH terminal-content backend and only then consider `auto` selection.
