# AGENTS.md

This repository is maintained with AI-assisted development. This file is the
root contract for Codex and other coding agents working in TeraDock.

If this file conflicts with older assistant-specific notes, follow this file
for repository work unless the human maintainer gives explicit instructions.

## Default Rules

- Keep changes small and scoped.
- Prefer release-slice oriented work.
- Preserve CLI compatibility.
- Preserve safe metadata behavior.
- Treat session logs as sensitive.
- Read the relevant README, contributing, release, security, and internal
  design docs before editing behavior in that area.
- Do not widen a task into adjacent ConPTY, session logging, prune, release, or
  packaging work unless the task explicitly asks for it.

## Allowed

Agents may:

- Create a branch when instructed.
- Implement scoped changes.
- Add focused tests.
- Update docs and changelog.
- Run validation.
- Create commits.
- Prepare draft PRs when instructed.

## Requires Explicit Approval

Agents must not proceed without explicit approval for:

- Windows `auto -> conpty` promotion.
- Changing default session logging behavior.
- Secret masking policy changes.
- Automatic scheduled pruning.
- User data deletion or migration beyond explicitly requested prune behavior.
- Config schema breaking changes.
- CLI breaking changes.
- Real SSH server automated tests.
- Release tag creation.
- GitHub Release publication.
- PR merge.

## Session Logging Safety Boundary

- Interactive session logs are sensitive terminal transcripts. Anything shown
  during SSH, including passwords, tokens, prompt responses, pasted text,
  command output, or secrets, can be captured in log bodies.
- Session log metadata must remain small and safe. It must not store SSH auth
  args, full SSH command strings, private key paths, passwords, secrets, or
  tokens.
- `td session show` must stay metadata-first and only show transcript tail
  output when explicitly requested.
- `td session prune` must stay metadata-driven, path-validated, and
  confirmation-gated. Do not add automatic cleanup without explicit approval.
- Do not attach raw session logs to docs, issues, PRs, tests, screenshots, or
  release evidence unless reviewed and redacted.

## Windows Auto/ConPTY Boundary

- Windows `session.log.backend=auto` currently resolves to `no-log` for
  terminal-content logging.
- ConPTY logging is enabled only through explicit selection, such as
  `session.log.backend=conpty` with `session.log.enabled=true`, or a scoped CLI
  override such as `td connect <profile_id> --log-backend conpty`.
- The explicit ConPTY backend can be treated as `explicit_ready` where the docs
  record evidence, but it must not be promoted to `auto` without explicit
  maintainer approval and fresh evidence for the remaining manual-smoke gates.
- PowerShell Transcript remains explicit, degraded, and best-effort. Do not
  describe it as reliable SSH terminal-content logging.

## Validation

Before reporting completion, run:

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

If a command cannot be run, record the exact command and reason in the result
report.

Focused checks may be added for the touched area, but they do not replace the
required validation unless the maintainer explicitly accepts a narrower pass.

## Result Report

Every task should create:

```text
RESULT_TeraDock_<TASK>.md
```

Include:

- Summary.
- Scope and non-scope.
- Files changed.
- Existing docs consulted.
- Validation results.
- Safety boundaries preserved.
- Risks, skipped checks, or follow-up work.
- Commit hash when available after commit creation.

Use result reports to separate proven evidence from placeholders, historical
evidence, compatibility-only checks, and deferred manual smoke.

## Commit Rules

Use small conventional-style commits, e.g.

```text
feat(session): add prune json output
fix(conpty): prevent stale child on abort
docs(agent): add AI workflow contract
test(session): cover prune path safety
```

Do not include unrelated worktree changes. Do not stage user changes unless the
task explicitly asks for them.

## PR Rules

- Draft PRs only unless explicitly told otherwise.
- Include summary, scope, validation output, manual smoke status when
  applicable, security/logging impact, and unchanged safety boundaries.
- Do not mark a PR ready, merge a PR, create a release tag, or publish a GitHub
  Release unless explicitly instructed.
