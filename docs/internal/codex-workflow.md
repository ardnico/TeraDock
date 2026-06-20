# Codex Workflow

This document explains how to assign TeraDock work to Codex or another coding
agent while keeping release scope, session logging safety, and validation
discipline explicit.

## How To Assign A Task

Use small, release-slice oriented prompts. A good task names the target version
or feature slice, the files or behavior in scope, the forbidden boundaries, the
required validation, and whether Codex may commit or prepare a draft PR.

Prefer one deliverable per task:

- A focused bug fix plus test and result report.
- A documentation update plus validation.
- A smoke-evidence review plus result report.
- A narrow release checklist update.

Avoid asking for broad modernization, cleanup, and feature work in the same
prompt. If the task may require design, ask Codex to write or update an
ExecPlan first.

## Task Prompt Template

```text
Target:
<short repo area, issue, release slice, or file list>

Goal:
<specific behavior, doc, test, report, or decision to produce>

Scope:
- <allowed file/behavior area>
- <allowed test/doc area>

Forbidden:
- Do not promote Windows auto -> conpty.
- Do not change default session logging behavior.
- Do not change prune behavior unless the task is specifically about prune.
- Do not store auth args, full command strings, private key paths, passwords,
  secrets, or tokens in metadata.
- Do not add real SSH server automated tests.
- Do not create release tags, publish GitHub Releases, or merge PRs.

Validation:
Run:
- cargo fmt --check
- cargo test
- cargo clippy --all-targets --all-features -- -D warnings
- cargo build -p td --release --locked

Commit:
Create a small conventional-style commit if validation passes.

Stop condition:
Stop and report if the task requires a forbidden change, larger design decision,
unavailable manual smoke environment, or failing validation that cannot be fixed
within the requested scope.
```

## Release Slice Workflow

1. Read `AGENTS.md`, `CONTRIBUTING.md`, `RELEASE_CHECKLIST.md`,
   `CHANGELOG.md`, `docs/security.md`, and the relevant `docs/internal/*`
   design or smoke document.
2. Confirm whether the task is for v1.1 explicit ConPTY, v1.1.1 session prune,
   v1.1.2 session-log operations, packaging, docs, or another clearly named
   slice.
3. Keep edits inside that slice. Do not move adjacent deferred work into scope.
4. Add focused tests for code changes and update docs when behavior, commands,
   logging, configuration, or safety expectations change.
5. Create `RESULT_TeraDock_<TASK>.md` with files changed, validation, safety
   boundaries, and follow-up work.
6. Run required validation.
7. Commit only the task files with a small conventional-style message.
8. Prepare a draft PR only when instructed.

## Manual Smoke Handling

Manual smoke is required for interactive TUI, ConPTY, real SSH, terminal-host,
packaging, and release-artifact behavior that automated tests cannot prove.

When handling manual smoke:

- Use controlled profiles and sanitized output.
- Treat saved session logs as sensitive local files.
- Record only safe fields in reports.
- Keep live evidence, historical evidence, compatibility-only checks, and
  placeholders separate.
- If the current shell is not an interactive TTY, do not retry `td ui` in the
  same non-TTY path. Mark the check as manual or conditional and explain why.
- Do not add real SSH server dependencies to automated tests without explicit
  approval.

## If Validation Fails

If validation fails:

1. Capture the failing command and the relevant error lines.
2. Determine whether the failure is caused by the current task, the local
   environment, or pre-existing unrelated state.
3. Fix task-caused failures when the fix stays inside scope.
4. Do not widen into unrelated refactors or behavior changes to make validation
   pass.
5. Record remaining failures in `RESULT_TeraDock_<TASK>.md`.
6. Do not create a completion commit that claims validation passed when it did
   not.

## If A Larger Design Issue Appears

Stop and report instead of improvising a large design change when the task
reveals:

- Windows `auto -> conpty` promotion pressure.
- A need to change default session logging behavior.
- New secret masking policy requirements.
- User data deletion or migration outside explicitly requested prune behavior.
- Config schema or CLI breaking changes.
- Real SSH server test infrastructure needs.
- Release tag or GitHub Release publication decisions.

For larger implementation work, propose a small next slice or write an ExecPlan
for maintainer review before editing behavior.

## Commit And PR Expectations

Use conventional-style commit messages:

```text
docs(agent): add Codex workflow contract
fix(conpty): prevent stale child on abort
test(session): cover prune path safety
```

Draft PRs should include:

- Summary.
- Scope and non-scope.
- Validation output.
- Manual smoke status, if applicable.
- Security/logging impact.
- Explicit unchanged boundaries, especially Windows `auto -> conpty`,
  session logging defaults, safe metadata, and prune behavior.

Never mark a PR ready, merge a PR, create a release tag, or publish a GitHub
Release unless the maintainer explicitly says to do that.
