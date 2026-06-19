## Summary

-

## Agent Workflow

- [ ] I read and followed [AGENTS.md](https://github.com/ardnico/TeraDock/blob/main/AGENTS.md).
- [ ] This PR is scoped to the described release slice or task.
- [ ] I did not bundle unrelated refactors, packaging work, release work, or behavior changes.

## Scope

In scope:

-

Out of scope:

-

## Related Issue

-

## Safety Boundary Checklist

- [ ] I did not change Windows `auto -> conpty` behavior unless explicitly approved.
- [ ] I did not change default session logging behavior unless explicitly approved.
- [ ] I did not change secret masking policy unless explicitly approved.
- [ ] I did not add scheduled pruning, user data deletion, or migration beyond explicitly requested prune behavior.
- [ ] I did not introduce config schema or CLI breaking changes unless explicitly approved and described below.
- [ ] I did not add real SSH server automated tests unless explicitly approved.
- [ ] I did not create a release tag, publish a GitHub Release, mark a draft PR ready, or merge a PR.

## Session Logging Impact

- Does this affect terminal transcript capture, `td session show`, `td session prune`, session metadata, operation logs, stdout/stderr display, export/import data, or docs about those areas?
- If yes, describe the behavior and compatibility impact.
- [ ] I updated docs/security notes if terminal transcript behavior changed.

## Metadata And Log Sensitivity

- [ ] I did not store auth args, full SSH commands, private key paths, passwords, tokens, or secrets in metadata.
- [ ] I did not attach raw session logs, screenshots, fixtures, or release evidence unless reviewed and redacted.
- [ ] Any included command output, logs, or examples have secrets and sensitive host/user values removed.

## Validation

Paste the commands that were run and the result:

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

- [ ] I ran or documented why I could not run cargo fmt/test/clippy/build.

## Manual Smoke

- Required for interactive TUI, ConPTY, real SSH, terminal-host, packaging, or release-artifact behavior.
- Status:
  - [ ] Not applicable.
  - [ ] Completed and summarized above with redacted evidence.
  - [ ] Deferred or unavailable, with reason documented above.

## Documentation

- [ ] README/docs updated when behavior, commands, configuration, logs, or safety expectations changed.
- [ ] Security notes updated when logging, transcripts, metadata, or secret exposure expectations changed.

## Breaking Change Confirmation

- [ ] No breaking change.
- [ ] Breaking change, explicitly approved and described above.

## Release Note Needed?

- [ ] No release note needed.
- [ ] Release note updated.
- [ ] Release note needed but deferred, with reason documented above.
