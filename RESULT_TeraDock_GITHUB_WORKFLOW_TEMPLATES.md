# RESULT_TeraDock_GITHUB_WORKFLOW_TEMPLATES

## Summary

Aligned GitHub-facing development workflow files with the post-v1.1.1
AI-agent contract. This was a docs/template-only change. No implementation
behavior, Windows `auto -> conpty` behavior, session logging behavior, prune
behavior, release tag, GitHub Release, or PR merge was changed.

## Phase 1 Findings

Existing PR template gaps:

- It did not link to `AGENTS.md`.
- It asked for scope and test results, but did not make the release-slice
  boundary explicit.
- It did not list the approval-only boundaries from `AGENTS.md`, especially
  Windows `auto -> conpty`, session logging defaults, masking policy, scheduled
  pruning, breaking CLI/config changes, real SSH automated tests, release tags,
  GitHub Releases, or PR merge.
- It mentioned security/logging generally, but did not separate terminal
  transcript behavior, session metadata safety, raw session log handling, and
  redacted evidence.
- It omitted `cargo build -p td --release --locked` from the validation command
  block.
- Manual smoke, breaking-change approval, and release-note decisions were
  present only weakly or not as explicit sections.

Existing issue template gaps:

- `bug_report.yml` already required version, OS, install method, command,
  expected behavior, actual behavior, redacted logs, and a sensitive-data
  confirmation.
- It did not separately capture terminal host and shell.
- It did not provide safe fields for a saved session id or redacted
  `td session doctor` output when reporting session logging, ConPTY, prune, or
  saved-session issues.
- There was no release-slice task template for assigning scoped maintainer or
  AI-agent work with objective, scope, forbidden changes, validation, manual
  smoke, docs, result report, and commit/PR expectations.

Copilot/Codex instruction fit:

- `.github/copilot-instructions.md` existed but described an unrelated
  FastAPI/Jinja household task-board project and included persona/tone guidance
  unrelated to TeraDock.
- It was not aligned with TeraDock's Rust CLI/TUI architecture, session logging
  safety boundary, Windows auto/ConPTY boundary, or required validation gates.

Duplication with `AGENTS.md`:

- The PR template now mirrors the approval-only safety checklist because PR
  authors need visible confirmation fields.
- The Copilot instructions intentionally stay short and point to `AGENTS.md`
  and `docs/internal/codex-workflow.md` instead of copying the full contract.
- The release-slice issue template includes the forbidden-change checklist
  because issue authors need to declare task boundaries before work starts.

Minimal change scope:

- Update `.github/pull_request_template.md`.
- Extend `.github/ISSUE_TEMPLATE/bug_report.yml`.
- Add `.github/ISSUE_TEMPLATE/release_slice_task.yml`.
- Replace `.github/copilot-instructions.md` with a concise TeraDock-specific
  guide.
- Do not modify Rust code, runtime behavior, release artifacts, or existing
  README/CONTRIBUTING/AGENTS workflow text.

## Changed Files

- `.github/pull_request_template.md`
- `.github/ISSUE_TEMPLATE/bug_report.yml`
- `.github/ISSUE_TEMPLATE/release_slice_task.yml`
- `.github/copilot-instructions.md`
- `RESULT_TeraDock_GITHUB_WORKFLOW_TEMPLATES.md`

## PR Template Changes

- Added a direct link to `AGENTS.md`.
- Added scoped-work and unrelated-change confirmations.
- Added explicit safety boundary checklist items for:
  - Windows `auto -> conpty`.
  - Default session logging behavior.
  - Secret masking policy.
  - Scheduled pruning and user data deletion/migration.
  - Breaking config or CLI changes.
  - Real SSH server automated tests.
  - Release tags, GitHub Releases, ready PRs, and PR merge.
- Added separate sections for session logging impact and metadata/log
  sensitivity.
- Added the required metadata safety checklist item:
  `I did not store auth args, full SSH commands, private key paths, passwords,
  tokens, or secrets in metadata.`
- Added the required docs/security note checklist item for terminal transcript
  behavior changes.
- Expanded validation to include:

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

- Added manual smoke, documentation, breaking-change, and release-note sections.

## Issue Template Changes

- Updated `bug_report.yml` with:
  - Separate terminal and shell field.
  - Safe session id field.
  - Redacted `td session doctor` output field for relevant issues.
- Added `release_slice_task.yml` with:
  - Objective.
  - Scope and non-scope.
  - Forbidden changes.
  - Validation.
  - Manual smoke.
  - Docs.
  - Result report.
  - Commit/PR expectation.
  - Safety confirmation.

## Copilot Instructions Changes

- Removed unrelated FastAPI/Jinja/Python household-task-board guidance.
- Replaced it with a concise TeraDock-specific guide covering:
  - TeraDock project identity as a Rust CLI/TUI.
  - `AGENTS.md` and `docs/internal/codex-workflow.md` as source-of-truth docs.
  - Approval boundaries.
  - Session logging safety.
  - Windows auto/ConPTY boundary.
  - Required validation commands.

## AGENTS.md Relationship

- `AGENTS.md` remains the root AI-agent contract.
- The GitHub PR and issue templates expose the contract at the contribution
  boundary so authors can confirm scope, validation, logging sensitivity, and
  approval-only safety constraints before review.
- `.github/copilot-instructions.md` acts as a short GitHub Copilot entrypoint,
  not a competing or complete replacement for `AGENTS.md`.

## Existing Docs Consulted

- `AGENTS.md`
- `docs/internal/codex-workflow.md`
- `.github/pull_request_template.md`
- `.github/ISSUE_TEMPLATE/bug_report.yml`
- `.github/ISSUE_TEMPLATE/feature_request.yml`
- `.github/ISSUE_TEMPLATE/documentation.yml`
- `.github/copilot-instructions.md`
- `CONTRIBUTING.md`
- `README.md`

## Validation Results

YAML validation:

```powershell
python - .github\ISSUE_TEMPLATE\bug_report.yml .github\ISSUE_TEMPLATE\documentation.yml .github\ISSUE_TEMPLATE\feature_request.yml .github\ISSUE_TEMPLATE\release_slice_task.yml
```

Passed. All four issue template YAML files parsed with PyYAML.

Required validation:

```powershell
cargo fmt --check
```

Passed.

```powershell
cargo test
```

Passed. Visible test groups reported 5, 43, 78, and 24 tests passed, with no
failures. Doc-test groups also reported no failures.

```powershell
cargo clippy --all-targets --all-features -- -D warnings
```

Passed.

```powershell
cargo build -p td --release --locked
```

Passed.

## Safety Boundaries Preserved

- No Rust implementation files were changed.
- No Windows `auto -> conpty` behavior was changed.
- No default session logging behavior was changed.
- No secret masking policy was changed.
- No prune behavior, scheduled pruning, user data deletion, or migration was
  changed.
- No config schema or CLI behavior was changed.
- No real SSH automated tests were added.
- No release tag, GitHub Release, PR creation, ready marking, merge, or
  publishing action was performed.

## Risks, Skipped Checks, And Follow-Up Work

- Manual smoke is not applicable because this change only updates GitHub
  Markdown/YAML templates.
- GitHub issue forms were checked as YAML, but GitHub's hosted issue-form
  renderer was not opened in this local validation.
- Follow-up candidates suitable for Codex:
  - Add a `.github/ISSUE_TEMPLATE/config.yml` if the maintainer wants template
    chooser ordering or blank-issue policy.
  - Add a future release-slice template variant specifically for ConPTY manual
    smoke evidence collection.
  - Review whether README's displayed current stable version should be updated
    in a separate release-docs slice.

## Commit Hash

Pending until commit creation. The exact commit hash is reported in the final
assistant completion message because a commit cannot include its own final SHA
without changing that SHA.
