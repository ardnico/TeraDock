# Roadmap

## Current stable

- 1.0.3

## 1.0.x policy

- Bug fix.
- Docs fix.
- Packaging fix.
- Regression fix.
- No large feature expansion.

## 1.1 candidates

1. Interactive SSH session log saving with default-disabled Linux/macOS `script` backend.
2. Stability improvements after the session logging slice.
3. Windows ConPTY session logging as an explicit backend (`session.log.backend=conpty`, TUI `s`, `td connect --log-backend conpty`, and `td session conpty-test <profile_id>`), with normal TUI/Japanese, Ctrl-C, bad-host, and auth-failure smoke success and remaining resize, large-output, long-running, cleanup, and broader terminal validation.
4. TUI recent pane.
5. Terminal emulator launch configuration.
6. tmux integration design.
7. Transfer/tunnel SSH invocation cleanup.
8. CommandSet runner boundary cleanup.
9. Better smoke test script.
10. Screenshots/GIF documentation.

## Not planned for 1.1

- Reliable/default Windows full SSH terminal-content logging. PowerShell Transcript remains explicit best-effort/degraded only.
- Automatic ConPTY backend selection before the explicit ConPTY backend has resize, large-output, long-running, cleanup, UTF-8, and broader Windows smoke evidence.
- Web UI.
- Cloud sync.
- Remote server management daemon.
- Full Ansible replacement.
- Credential sharing service.

## Future session logging

- 1.1.x: Keep Windows `auto` on `no-log`, keep `powershell-transcript` explicit best-effort, surface capture warnings in doctor/show/config UI, and keep ConPTY explicit for `td connect`, TUI `s`, and `td session conpty-test <profile_id>`.
- 1.2: Keep the explicit ConPTY backend at `explicit_ready` after normal TUI/Japanese, Ctrl-C, bad-host, and auth-failure smoke, then collect resize, large-output, long-running, broader child-cleanup, and broader Windows terminal evidence before calling the explicit backend stable.
- 1.3: Evaluate a production ConPTY backend for reliable Windows SSH terminal input/output capture and only then consider `auto` selection.
