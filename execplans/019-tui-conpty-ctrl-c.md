# Improve Windows TUI ConPTY Ctrl-C Behavior

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This plan follows `.agent/PLANS.md`. It is self-contained for this repository and describes how to change explicit Windows ConPTY session logging so the first `Ctrl-C` interrupts the remote process instead of aborting the TeraDock session, while a second quick `Ctrl-C` remains an emergency abort.

## Purpose / Big Picture

Windows users can explicitly enable ConPTY logging for `td ui`, press `s` on an SSH profile, run a long remote command such as `sleep 30`, press `Ctrl-C` once, and keep the SSH session alive. After the first `Ctrl-C`, the remote shell should return and allow another command such as `echo after-ctrl-c`, then `exit` should complete the session with `status=completed` and `exit_code=0`. If the terminal does not respond, pressing `Ctrl-C` a second time within two seconds aborts the TeraDock ConPTY run, kills the child, restores the TUI, and writes aborted metadata with `failure_phase=user_abort` and `failure_reason=ctrl_c_double_press`.

This plan must not promote Windows `auto` to ConPTY. The backend remains explicit only.

## Progress

- [x] (2026-06-17 JST) Read repository instructions and confirmed `.agent/PLANS.md` requires a living ExecPlan for this cross-cutting TUI/ConPTY change.
- [x] (2026-06-17 JST) Inspected current TUI suspend/resume path in `crates/tui/src/app.rs` and ConPTY input loop in `crates/core/src/conpty.rs`.
- [x] (2026-06-17 JST) Identified that `key_event_to_pty_bytes` already maps `Ctrl-C` to byte `0x03`, but `run_conpty_input_bridge` intercepts that key first and turns it into `ConptyEvent::UserAbort`.
- [x] (2026-06-17 JST) Implemented first-Ctrl-C forwarding and second-Ctrl-C abort in `crates/core/src/conpty.rs`.
- [x] (2026-06-17 JST) Added focused unit tests for the Ctrl-C policy and metadata expectations.
- [x] (2026-06-17 JST) Updated TUI/ConPTY documentation and created `RESULT_TeraDock_TUI_CONPTY_CTRL_C_BEHAVIOR.md`.
- [x] (2026-06-17 JST) Aligned emergency abort metadata with the terminal-exit fix request by using `failure_reason=ctrl_c_double_press` and creating `RESULT_TeraDock_TUI_CONPTY_CTRL_C_TERMINAL_EXIT_FIX.md`.
- [x] (2026-06-17 JST) Ran `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo build -p td --release --locked`; all passed.

## Surprises & Discoveries

- Observation: The byte mapping for remote interrupt already exists.
  Evidence: `crates/core/src/conpty.rs` has `key_event_to_pty_bytes`, which maps `KeyCode::Char('c')` with `KeyModifiers::CONTROL` to `Some(vec![0x03])`.

- Observation: The mapping is bypassed for Ctrl-C.
  Evidence: `run_conpty_input_bridge` checks `key_event_is_ctrl_c` first, sets `cancel=true`, sends `ConptyEvent::UserAbort`, and returns before the writer sees `0x03`.

- Observation: TUI suspension is not the direct cause of the abort.
  Evidence: `crates/tui/src/app.rs` disables raw mode and leaves the alternate screen before calling the ConPTY runner; the runner itself enters raw mode and reads the key event while the TUI is suspended.

## Decision Log

- Decision: Keep the fix inside the shared ConPTY input bridge rather than special-casing the TUI caller.
  Rationale: `td session conpty-test`, explicit `td connect --log-backend conpty`, and TUI `s` all use `run_conpty_ssh_child`, so the input behavior should be consistent and tested once.
  Date/Author: 2026-06-17 / Codex

- Decision: Use the requested two-stage policy as the default: first Ctrl-C forwards `0x03`, second Ctrl-C within two seconds aborts.
  Rationale: This preserves the normal remote interrupt behavior while retaining a direct escape hatch if the remote side or ConPTY bridge stops responding.
  Date/Author: 2026-06-17 / Codex

- Decision: Do not add a new `auto` backend path or any real SSH automated test.
  Rationale: The user explicitly prohibited `auto -> conpty` promotion and real SSH automated tests; this change can be covered with unit tests plus manual smoke instructions.
  Date/Author: 2026-06-17 / Codex

## Outcomes & Retrospective

Completion update 2026-06-17: The source change is complete. The first Ctrl-C now writes and flushes `0x03` without setting the cancel flag; a second Ctrl-C within 2 seconds reuses the existing `UserAbort` path and records `failure_reason=ctrl_c_double_press`. Source-level tests prove the Ctrl-C policy and metadata expectations, docs describe Ideal/Acceptable/Failure smoke classifications, and the result reports separate implemented behavior from manual evidence still requiring a controlled Windows SSH run. Automated validation passed. The remaining work is manual operator smoke on a controlled Windows SSH profile to collect live terminal, metadata, log, and child-process evidence.

## Context and Orientation

The TUI entry point is `crates/tui/src/app.rs`. The `s` key builds an SSH session command and calls `run_interactive_ssh_session`. That function suspends the TUI by disabling raw mode, leaving the alternate screen, disabling mouse capture, and showing the cursor. For explicit Windows ConPTY session logging, `run_conpty_logged_ssh_session` calls `tdcore::conpty::run_conpty_ssh_child`.

The shared ConPTY runner is `crates/core/src/conpty.rs`. It spawns `ssh.exe` under `portable-pty`, starts an output thread that reads from the pseudo terminal and writes to the local terminal and log, starts an input thread that reads crossterm key events from the local terminal and writes bytes into the pseudo terminal, starts a wait thread for child status, and uses events to coordinate shutdown. A pseudo terminal, or PTY, is a terminal-like interface that lets TeraDock sit between the user's terminal and the SSH child.

Session metadata is written by `crates/core/src/session_log.rs`. Successful ConPTY completion uses `complete_conpty_session`, which maps `exit_code=0` to `status=completed` and nonzero exits to `status=completed_nonzero`. Double-Ctrl-C emergency aborts use `complete_conpty_failure_session` with `status=aborted`, `failure_phase=user_abort`, and `failure_reason=ctrl_c_double_press`.

## Plan of Work

In `crates/core/src/conpty.rs`, replace the current unconditional Ctrl-C abort inside `run_conpty_input_bridge` with a small policy helper. The helper records the last forwarded Ctrl-C time. When the first Ctrl-C is received, it writes `0x03` to the ConPTY writer, flushes, emits debug lines that contain no command, auth, path, secret, or environment data, and continues the loop without setting `cancel`. When another Ctrl-C arrives within two seconds, it sets `cancel=true`, sends `ConptyEvent::UserAbort`, and returns. If more than two seconds have passed, treat the new key as another first Ctrl-C and forward it. The `UserAbort` path records `failure_reason=ctrl_c_double_press`.

The existing `ConptyEvent::UserAbort` path should remain the emergency path. It should still kill the child, restore raw mode through `RawModeGuard`, join threads best-effort, and surface aborted metadata through the existing caller logic. The output reader and child wait should continue after a forwarded Ctrl-C, so child exit after remote `exit` remains a normal completion.

Add unit-testable helpers for the Ctrl-C policy if direct input bridge testing would require real terminal input. Keep tests independent of real SSH. Add tests proving that `Ctrl-C` maps to `0x03`, that the first Ctrl-C policy forwards, that a quick second Ctrl-C aborts, that a delayed Ctrl-C forwards again, and that ConPTY metadata for a forwarded interrupt followed by normal exit remains completed.

Update `docs/internal/windows-tui-conpty-manual-smoke.md`, `docs/internal/windows-conpty-session-logging-design.md`, `docs/tui.md`, and `RELEASE_CHECKLIST.md` to describe the first Ctrl-C remote interrupt, second Ctrl-C emergency abort, metadata expectations, child cleanup expectations, and Ideal/Acceptable/Failure classifications. Also sync directly related stale references in `docs/internal/windows-conpty-manual-smoke.md`, `docs/security.md`, and `ROADMAP.md` because the Ctrl-C behavior lives in the shared ConPTY runner. Create `RESULT_TeraDock_TUI_CONPTY_CTRL_C_BEHAVIOR.md` with investigation findings, implementation summary, validation results, auto-promotion rationale, and remaining failure cases.

## Concrete Steps

Work from `C:\Users\leafs\work\git\TeraDock`.

Edit source and docs with scoped patches. Then run:

    cargo fmt --check
    cargo test
    cargo clippy --all-targets --all-features -- -D warnings
    cargo build -p td --release --locked

Manual smoke remains a controlled Windows SSH run and should not be automated:

    td config set session.log.enabled true
    td config set session.log.backend conpty
    td ui

In the TUI, choose an SSH profile, press `s`, and run:

    sleep 30
    # press Ctrl-C once
    echo after-ctrl-c
    exit

The expected manual observation is that the remote shell returns after one Ctrl-C, `after-ctrl-c` appears in the log, the TUI returns after `exit`, metadata has `status=completed` and `exit_code=0`, and no test `ssh.exe` child remains.

For emergency abort manual smoke, run `sleep 30` and press Ctrl-C twice within two seconds. The expected observation is TUI return, metadata has `status=aborted`, `failure_phase=user_abort`, `failure_reason=ctrl_c_double_press`, and no test `ssh.exe` child remains.

## Validation and Acceptance

Acceptance is source-level and manual-smoke ready in this pass. Unit tests must pass without needing a real SSH server. The four required cargo commands must complete successfully. The manual smoke instructions must clearly distinguish implemented expected behavior from evidence that still has to be collected by an operator on a controlled Windows SSH profile.

The implementation is accepted when source inspection shows that the first Ctrl-C is written and flushed to the ConPTY writer as `0x03`, no abort flag is set for that first key, the event loop continues, and the existing child exit path still writes completed metadata. It is also accepted when the quick second Ctrl-C still reaches the existing `UserAbort` path and writes aborted metadata through existing session-log functions.

## Idempotence and Recovery

The code and docs edits are ordinary source changes and can be reapplied or reverted by file. The manual smoke should use a controlled profile and should not type secrets. If the terminal mode becomes unusable during manual smoke, close and reopen the terminal or use the documented recovery command for that shell, then check for leftover `td` or `ssh` processes from the test before retrying.

## Artifacts and Notes

The requested final result report is `RESULT_TeraDock_TUI_CONPTY_CTRL_C_TERMINAL_EXIT_FIX.md`; `RESULT_TeraDock_TUI_CONPTY_CTRL_C_BEHAVIOR.md` remains as the broader behavior report. The reports should state that `auto` remains deferred because this pass only addresses Ctrl-C behavior and does not provide the full failure-case matrix required for default backend selection.

## Interfaces and Dependencies

The shared API stays `tdcore::conpty::run_conpty_ssh_child(executable, args, log_path, options)`. No new public CLI flag is required for this pass. `ConptyRunOptions` remains the place for debug and timeout behavior unless a later change needs an explicit policy override. Debug output must only use generic messages such as `debug: ctrl-c received`, `debug: ctrl-c forwarded to conpty child`, and `debug: session continues after ctrl-c`; it must not include full commands, auth args, private key paths, passwords, tokens, secrets, or full environment.

Revision note 2026-06-17: Created the plan after source investigation showed the current abort comes from the ConPTY input bridge intercepting Ctrl-C before the existing `0x03` key mapping can write to the PTY.

Revision note 2026-06-17: Updated the plan after implementation and validation. The code, tests, docs, and result report are complete; live Windows SSH smoke remains an evidence-gathering task, not an automated test.

Revision note 2026-06-17: Updated the plan for the terminal-exit fix request. Emergency abort metadata now uses `failure_reason=ctrl_c_double_press`, and the specifically requested result artifact is part of the deliverable.
