# TeraDock TUI ConPTY Stability Decision

Date: 2026-06-18 JST

## Smoke Summary

Explicit ConPTY setup and diagnostics were verified:

```powershell
.\target\release\td.exe config set session.log.enabled true
.\target\release\td.exe config set session.log.backend conpty
.\target\release\td.exe session doctor
```

Observed doctor state:

- `enabled: true`
- `backend setting: conpty`
- `resolved backend: conpty`
- `TUI logging: enabled for s-key SSH sessions`
- `ConPTY backend: explicit_ready`
- `Auto selection: deferred`
- `Status: degraded`

The automation shell was not an interactive TTY, so `.\target\release\td.exe
ui` exited with:

```text
Error: td ui requires an interactive TTY; interactive SSH sessions require a TTY
```

No fresh interactive resize, large-output, long-running, or exact
`after_ctrl_c` TUI smoke was created in this pass. Existing local saved
sessions `sl_x7qxorxv` and `sl_mcx5u7jc` were rechecked with
`session list/show/path`, log tails, and metadata safety scans.

Verdict summary:

- Resize smoke: `CONDITIONAL GO`
- Large output smoke: `CONDITIONAL GO`
- Long-running session smoke: `CONDITIONAL GO`
- Ctrl-C inside remote command: `CONDITIONAL GO`
- Normal exit child cleanup: `CONDITIONAL GO`
- Abort child cleanup: `GO` for prior explicit double-Ctrl-C evidence; not
  rerun in this pass.
- UTF-8/Japanese output: `GO` for prior explicit TUI evidence; not rerun in
  this pass.
- Session list/show/path verification: `GO`
- Metadata safety verification: `GO` for current local metadata files.
- Auto promotion decision: `GO` for keeping Windows `auto=no-log`; `NO-GO` for
  `auto -> conpty` promotion.

## Fixes Made

- No ConPTY launcher, event-loop, Ctrl-C, resize, output tee, metadata schema,
  or child-cleanup logic was changed.
- Updated ConPTY doctor/settings reason text to reflect current evidence:
  explicit TUI logging, Japanese output, Ctrl-C, bad-host, and auth-failure
  smokes have succeeded; resize, large-output, long-running, cleanup, and
  broader terminal coverage remain.
- Updated README, TUI docs, security docs, internal ConPTY design docs, manual
  smoke docs, roadmap, and release checklist.
- Added `RESULT_TeraDock_TUI_CONPTY_STABILITY_SMOKE.md`.

## Test Results

All required validation gates passed:

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

The first `cargo fmt --check` found only a rustfmt wrapping difference in the
updated diagnostic hint. `cargo fmt` was applied, and the required
`cargo fmt --check` gate then passed.

## Explicit ConPTY Support Verdict

Verdict: `CONDITIONAL GO`

Explicit Windows ConPTY TUI logging remains supported as an explicit,
operator-selected backend at `explicit_ready`. It is suitable to keep exposed
for controlled Windows TUI `s`, `td connect --log-backend conpty`, and
`td session conpty-test <profile_id>` usage with the documented transcript
security warning.

This is not a production/default backend verdict. The remaining stability
matrix still needs fresh manual Windows TTY evidence for resize, large output,
long-running sessions, exact `after_ctrl_c`, and broader cleanup snapshots.

## Remaining Limitations

- This pass could not drive `td ui` interactively because the automation shell
  is not a TTY.
- Resize forwarding is implemented but fresh live resize evidence is still
  missing.
- Large-output backpressure has source-level support through chunked teeing but
  still needs a fresh `seq 1 5000` TUI log.
- Long-running command interruption still needs a fresh exact-marker run:
  `after_ctrl_c`.
- The ConPTY log is a terminal transcript, not a full terminal replay.
- Terminal transcript logs may contain displayed passwords, tokens, secrets,
  prompt responses, pasted text, and command output.
- Metadata excludes auth args, full command strings, private key paths,
  passwords, secrets, and tokens, but logs are still sensitive.
- Automated tests still do not use a real SSH server.

## Auto Promotion Decision

Decision: keep Windows `auto=no-log`.

Do not promote `auto -> conpty` in this release slice. Auto promotion remains
deferred because the remaining evidence requires broader terminal-host,
failure-mode, resize, large-output, long-running, and cleanup coverage.

## Next Release Recommendation

Release with explicit ConPTY only; keep auto=no-log

