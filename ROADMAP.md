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
3. TUI recent pane.
4. Terminal emulator launch configuration.
5. tmux integration design.
6. Transfer/tunnel SSH invocation cleanup.
7. CommandSet runner boundary cleanup.
8. Better smoke test script.
9. Screenshots/GIF documentation.

## Not planned for 1.1

- Reliable Windows full SSH terminal-content logging. PowerShell Transcript remains explicit best-effort/degraded only.
- ConPTY session logging implementation.
- Web UI.
- Cloud sync.
- Remote server management daemon.
- Full Ansible replacement.
- Credential sharing service.

## Future session logging

- 1.1.x: Keep Windows `auto` on `no-log`, keep `powershell-transcript` explicit best-effort, and surface capture warnings in doctor/show/config UI.
- 1.2: Build a Windows ConPTY SSH logging proof of concept.
- 1.3: Evaluate a production ConPTY backend for reliable Windows SSH terminal input/output capture.
