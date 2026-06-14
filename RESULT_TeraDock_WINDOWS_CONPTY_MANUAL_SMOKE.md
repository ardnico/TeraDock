# RESULT: Windows ConPTY Manual Smoke Stabilization

Date: 2026-06-14

## Manual Smoke Input

The smoke-result block provided in the task contained only the placeholder text
for what should be pasted. It did not include an actual Windows run command,
remote SSH commands, `td session show` output, metadata JSON, log body,
failure notes, Ctrl-C result, resize result, or UTF-8 result.

Because of that, all manual-smoke observations below are classified as
unverified. This pass does not claim that ConPTY is ready for `auto`, default
Windows logging, or TUI `s` integration.

## Phase 1 Classification

SSH connection:

- Login: unverified; no real SSH transcript was provided.
- Auth prompt: unverified.
- Bad host / auth failure: unverified.

Terminal I/O:

- Input: unverified by manual smoke.
- Remote output on screen: unverified by manual smoke.
- Remote output in log: unverified by manual smoke.
- Input command text in log: unverified; expected only when the remote side
  echoes input.
- ANSI escape sequences: unverified; current design preserves terminal bytes.

Exit:

- `exit` return: unverified by manual smoke.
- `exit_code` metadata: source path writes ConPTY exit code to metadata; manual
  confirmation is still required.
- Child process / output thread cleanup: hardened in this pass for input-loop
  error paths.

Ctrl-C:

- Remote process delivery: unverified by manual smoke.
- Local terminal recovery: raw-mode cleanup is hardened; manual confirmation is
  still required.

Resize:

- Display stability: unverified by manual smoke.
- Metadata/docs constraints: documented as a PoC constraint. Resize dimensions
  are now clamped to avoid forwarding `0x0`.

UTF-8/Japanese:

- Japanese output: unverified by manual smoke.
- Log encoding: documented as UTF-8 best-effort terminal bytes.

Security:

- Metadata secret exclusion: existing metadata tests cover absence of auth args,
  full command strings, private key paths, passwords, secrets, and tokens.
- Terminal log warning: documented. The log can contain anything displayed by
  the terminal, including secrets, echoed input, prompts, and command output.

## Phase 2 Verdict

CONDITIONAL GO: 制約付きで継続

This is a conditional decision for continuing the explicit CLI PoC stabilization
only. It is not a GO for `auto`, default Windows backend selection, or TUI
integration. The condition is that the next Windows run must fill
`docs/internal/windows-conpty-manual-smoke.md` with real evidence and must not
hit any No-Go criteria.

## Stabilization Changes

- Enter raw mode before launching the ConPTY child.
- On ConPTY input-loop failure, restore raw mode, kill/wait the child, close PTY
  handles, and join the output tee thread before returning.
- Write and flush the session log before writing the same chunk to local stdout.
- Clamp forwarded PTY resize dimensions to at least `1x1`.
- Convert ConPTY `u32` exit status into the current metadata `i32` range without
  negative wrapping.
- Add focused Windows-only tests for resize clamping and exit-code conversion.

## Changed Files

- `crates/cli/src/main.rs`
- `docs/internal/windows-conpty-manual-smoke.md`
- `docs/internal/windows-conpty-session-logging-design.md`
- `docs/security.md`
- `ROADMAP.md`
- `RELEASE_CHECKLIST.md`
- `RESULT_TeraDock_WINDOWS_CONPTY_MANUAL_SMOKE.md`

## Ctrl-C / Resize / UTF-8

- Ctrl-C: not manually verified in this turn; code still forwards `0x03` for
  `Ctrl-C` and now has stronger cleanup on input-loop errors.
- Resize: not manually verified in this turn; resize events are forwarded and
  dimensions are clamped.
- UTF-8/Japanese: not manually verified in this turn; logs remain best-effort
  terminal bytes.

## Failure Cases

Bad host, auth failure, and abort-at-prompt behavior were not provided in the
input and were not rerun automatically. They remain required manual checks.

## Test Results

Passed local validation:

- `cargo fmt --check`: passed.
- `cargo test`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo build -p td --release --locked`: passed.

Feasible non-interactive CLI checks:

- `.\target\release\td.exe session doctor`: passed; current Windows config
  reports `powershell-transcript` as degraded and still prints the explicit
  ConPTY PoC command.
- `.\target\release\td.exe session list`: passed against existing saved
  session metadata.
- `.\target\release\td.exe session show <existing_session_id>`: passed against
  existing saved session metadata.
- `.\target\release\td.exe session path <existing_session_id>`: passed against
  existing saved session metadata.

The existing saved session used for `list/show/path` compatibility had
`backend=powershell-transcript`, not `backend=conpty`, so it does not count as
ConPTY manual smoke evidence.

Windows manual smoke was not rerun because no concrete `<profile_id>` or
interactive controlled SSH transcript was provided in this task. Real-server SSH
smoke remains manual evidence, not an automated test.

## Why TUI Integration Still Waits

The TUI owns raw mode, alternate screen, mouse capture, redraw, and same-terminal
SSH suspend/restore behavior. The ConPTY PoC also owns raw mode and PTY I/O.
Combining those paths before the CLI PoC has real Windows smoke evidence would
increase the blast radius and make terminal recovery failures harder to isolate.

## Next Step

Run `docs/internal/windows-conpty-manual-smoke.md` on a controlled Windows SSH
profile and paste the full evidence: command, remote commands, `session show`,
metadata JSON, log body, failure cases, Ctrl-C, resize, and UTF-8 results.
