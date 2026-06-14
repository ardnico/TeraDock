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
3. Windows ConPTY session logging PoC behind an explicit `td session conpty-test <profile_id>` command, with source-level stabilization and a manual smoke template.
4. TUI recent pane.
5. Terminal emulator launch configuration.
6. tmux integration design.
7. Transfer/tunnel SSH invocation cleanup.
8. CommandSet runner boundary cleanup.
9. Better smoke test script.
10. Screenshots/GIF documentation.

## Not planned for 1.1

- Reliable/default Windows full SSH terminal-content logging. PowerShell Transcript remains explicit best-effort/degraded only.
- Automatic ConPTY backend selection or TUI integration before the PoC is manually proven with captured Windows smoke evidence.
- Web UI.
- Cloud sync.
- Remote server management daemon.
- Full Ansible replacement.
- Credential sharing service.

## Future session logging

- 1.1.x: Keep Windows `auto` on `no-log`, keep `powershell-transcript` explicit best-effort, surface capture warnings in doctor/show/config UI, and keep the ConPTY path explicit as `td session conpty-test <profile_id>`.
- 1.2: Stabilize the Windows ConPTY SSH logging proof of concept after manual smoke evidence. The current next step is to run `docs/internal/windows-conpty-manual-smoke.md` on controlled Windows SSH targets and review the saved metadata/logs before considering any broader integration.
- 1.3: Evaluate a production ConPTY backend for reliable Windows SSH terminal input/output capture.
