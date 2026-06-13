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

Use the Windows ConPTY / Pseudo Console API to run `ssh.exe` under a pseudo console controlled by TeraDock.

Likely implementation pieces:

- Evaluate Rust crate options such as `portable-pty`, `conpty`, or a small direct Windows API wrapper.
- Spawn `ssh.exe` as a ConPTY child process.
- Pump stdin from the user's terminal into the pseudo console.
- Pump pseudo console output back to the user's terminal and tee the same bytes to the session log file.
- Preserve exit code propagation from `ssh.exe`.
- Handle terminal resize events and call the ConPTY resize API.
- Handle Ctrl-C and other control input without breaking the user's terminal state.
- Define UTF-8 / Windows encoding behavior and document what bytes are written to the log.
- Keep raw terminal restore and alternate-screen behavior owned by the CLI/TUI launch layer.

## Risks

- Implementation complexity is materially higher than the `script` or PowerShell wrapper backends.
- Windows version and terminal-host differences can affect behavior.
- Resize handling can be fragile.
- Ctrl-C and control-sequence behavior can diverge from direct `ssh.exe`.
- Secrets, passwords, tokens, prompt responses, pasted text, and remote command output displayed in the terminal can be logged.
- Automated tests are difficult because a real interactive SSH server should not be required.

## Non-goals

- v1.1 immediate implementation.
- Full terminal emulator.
- Perfect replay format.
- Secret masking.
- Storing SSH auth args, full command strings, or private key paths in metadata.

## Suggested Roadmap

- v1.1.x: Keep Windows `auto` on `no-log`; keep `powershell-transcript` explicit best-effort/degraded; surface metadata, doctor, show, and config UI warnings.
- v1.2: Build a ConPTY proof of concept for `ssh.exe` with terminal I/O teeing, resize handling, Ctrl-C behavior, exit code propagation, and log metadata.
- v1.3: Evaluate productionizing ConPTY as the reliable Windows SSH terminal-content backend.
