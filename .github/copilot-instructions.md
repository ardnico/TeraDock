# Copilot Instructions

TeraDock is a Rust CLI/TUI tool for local connection profiles, SSH-oriented
CommandSets, interactive terminal workflows, and cautious operational use.

## Source Of Truth

- Follow [AGENTS.md](../AGENTS.md) as the root contract for AI-assisted work.
- For Codex-style assigned tasks, also follow
  [docs/internal/codex-workflow.md](../docs/internal/codex-workflow.md).
- Keep changes small, scoped, and compatible with existing CLI behavior.
- Read the relevant README, contributing, release, security, and internal design
  docs before changing behavior in that area.

## Approval Boundaries

Do not proceed without explicit maintainer approval for the boundaries listed in
`AGENTS.md`, especially:

- Windows `auto -> conpty` promotion.
- Default session logging behavior changes.
- Secret masking policy changes.
- Scheduled pruning or user data deletion/migration.
- Breaking config or CLI changes.
- Real SSH server automated tests.
- Release tag creation, GitHub Release publication, or PR merge.

## Session Logging Safety

Interactive session logs are sensitive terminal transcripts. Do not paste raw
session logs into issues, PRs, docs, screenshots, tests, fixtures, or release
evidence unless reviewed and redacted.

Session metadata must remain small and safe. Do not store auth args, full SSH
commands, private key paths, passwords, tokens, or secrets in metadata.

`td session show` should stay metadata-first. Transcript tail output should only
be shown when explicitly requested. `td session prune` should stay
metadata-driven, path-validated, and confirmation-gated.

## Windows Auto/ConPTY Boundary

On Windows, `session.log.backend=auto` currently resolves to `no-log` for
terminal-content logging. ConPTY logging is explicit only, such as
`session.log.backend=conpty` with logging enabled or
`td connect <profile_id> --log-backend conpty`.

PowerShell Transcript remains explicit, degraded, and best-effort. Do not
describe it as reliable SSH terminal-content logging.

## Validation

Before reporting completion, run or document why you could not run:

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

Focused checks may be added for the touched area, but they do not replace the
required validation unless the maintainer explicitly accepts a narrower pass.
