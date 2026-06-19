# RESULT_TeraDock_AGENT_WORKFLOW

## Summary

Added a root AI-agent workflow contract and Codex workflow guide for scoped,
safe work in TeraDock. This was a docs-only change. No implementation behavior,
Windows `auto -> conpty` selection, session logging behavior, or prune behavior
was changed.

## Phase 1 Findings

Existing development rules:

- `CONTRIBUTING.md` prioritizes stabilization, scoped release work, reviewable
  PRs, docs updates for behavior/safety changes, and exact test results.
- `README.md`, `RELEASE_CHECKLIST.md`, and `docs/security.md` treat terminal
  transcript logs as sensitive and require redaction before sharing logs.
- `.github/pull_request_template.md` asks for summary, scope, test results,
  security/logging impact, docs updates, and breaking-change confirmation.
- GitHub issue templates require reporters to remove secrets, passwords,
  tokens, private keys, unmasked host/user values, and full SSH auth arguments.

Release validation commands already documented:

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

Session logging safety boundary:

- Session logging is disabled by default.
- Session logs are local terminal transcripts and may contain displayed
  passwords, tokens, prompts, pasted text, command output, or secrets.
- Session metadata must not include SSH auth args, full command strings, private
  key paths, passwords, secrets, or tokens.
- `td session show` is metadata-first and log body output is explicit through
  tail options.
- `td session prune` is metadata-driven, validates paths before deletion, uses
  dry-run for preview, and requires `--yes` for actual deletion.

Windows auto/ConPTY boundary:

- Windows `session.log.backend=auto` remains `no-log` for terminal-content
  logging.
- ConPTY is an explicit Windows backend only, selected through
  `session.log.backend=conpty` with logging enabled or a scoped CLI override.
- The explicit backend is documented as `explicit_ready` with degraded overall
  diagnostics until remaining manual smoke evidence is complete.
- PowerShell Transcript remains explicit degraded/best-effort and is not
  reliable SSH terminal-content logging.

Existing documented prohibitions and non-goals:

- Do not promote Windows `auto -> conpty` in the current release slices.
- Do not claim full terminal replay.
- Do not add secret masking of terminal transcript bodies as an implicit
  behavior.
- Do not add automated real SSH server integration tests without a deliberate
  scope decision.
- Do not paste raw session logs or unredacted connection details into docs,
  issues, PRs, screenshots, tests, fixtures, or release evidence.
- Do not create production release tags or publish GitHub Releases before the
  release checklist and artifact validation are complete.

## Added Files

- `AGENTS.md`
- `docs/internal/codex-workflow.md`
- `RESULT_TeraDock_AGENT_WORKFLOW.md`

## Updated Files

- `README.md`

## Relationship To Existing Docs

- `AGENTS.md` consolidates the AI-agent contract at the repo root and points
  agents at existing contributing, release, security, and internal design docs.
- `docs/internal/codex-workflow.md` provides a task prompt template, release
  slice workflow, manual smoke handling, validation failure handling, and stop
  conditions.
- `README.md` now links to `AGENTS.md` and the Codex workflow guide from project
  operations and the documentation index.
- `CHANGELOG.md` was inspected but not changed because this is contributor
  workflow documentation, not runtime/user-facing TeraDock behavior.

## Allowed Agent Scope Documented

Agents may:

- Create a branch when instructed.
- Implement scoped changes.
- Add focused tests.
- Update docs and changelog.
- Run validation.
- Create commits.
- Prepare draft PRs when instructed.

## Explicit Approval Required

The new contract requires explicit approval before:

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

## Validation Results

All required validation passed:

```powershell
cargo fmt --check
```

Passed.

```powershell
cargo test
```

Passed. Reported unit/doc test groups all succeeded, including the visible
5 + 43 + 78 + 24 test groups and zero-test doc groups.

```powershell
cargo clippy --all-targets --all-features -- -D warnings
```

Passed.

```powershell
cargo build -p td --release --locked
```

Passed.

## Commit Hash

Pending until commit creation. The exact commit hash is reported in the final
assistant completion message because a commit cannot include its own final SHA
without changing that SHA.

## Next Codex-Friendly Work Candidates

- Add a short PR template note linking to `AGENTS.md`.
- Refresh `.github/copilot-instructions.md` so it no longer contains
  non-TeraDock project guidance.
- Add a focused docs pass for v1.1.1 release notes and prune examples.
- Create a release-slice issue template for manual ConPTY smoke evidence.
