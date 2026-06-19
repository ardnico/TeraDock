# Coding Agents

- あなたはこのリポジトリの「実装担当エンジニア」です。
- 複雑な機能追加や大きめのリファクタを行うときは、必ず ExecPlan を使います。
- ExecPlan の書き方と要件は `.agent/PLANS.md` に従ってください。

# ExecPlans

- When writing complex features or significant refactors, use an ExecPlan (as described in .agent/PLANS.md) from design to implementation.
- 新しい機能 / 複雑な変更 / 2時間以上かかりそうな作業をする前に、
  `execplans/NNN-xxx.md` の形で ExecPlan を作成し、更新しながら作業してください。

  # AGENTS.md

This repository is maintained with AI-assisted development.

Agents may implement scoped changes, run validation, update documentation, create commits, and prepare draft pull requests when explicitly instructed.

## Default Rules

* Keep changes small and release-slice oriented.
* Prefer conservative fixes over broad rewrites.
* Do not widen feature scope without explicit approval.
* Preserve existing CLI compatibility unless the task explicitly requires a breaking change.
* Preserve safe metadata behavior.
* Treat session logs as sensitive data.

## Allowed Without Additional Approval

Agents may:

* Create a working branch for the assigned task.
* Modify source files required for the assigned scope.
* Add or update focused tests.
* Update README, CHANGELOG, release checklist, and internal docs when relevant.
* Run:

  * `cargo fmt --check`
  * `cargo test`
  * `cargo clippy --all-targets --all-features -- -D warnings`
  * `cargo build -p td --release --locked`
* Create a result report named `RESULT_TeraDock_<TASK>.md`.
* Create commits for completed scoped work.
* Prepare a draft pull request when instructed.

## Requires Explicit Approval

Agents must stop and ask before:

* Promoting Windows `auto -> conpty`.
* Changing default session logging behavior.
* Changing secret, password, token, or transcript masking policy.
* Deleting or migrating user data.
* Adding automatic scheduled pruning.
* Changing config schema compatibility.
* Removing existing CLI commands or changing their semantics.
* Adding real SSH server dependencies to automated tests.
* Creating a release tag.
* Publishing a GitHub Release.
* Merging a pull request.

## Session Logging Safety Rules

* Metadata must not store:

  * full SSH command strings
  * authentication arguments
  * private key paths
  * passwords
  * tokens
  * secrets
* Terminal transcript logs may contain displayed secrets.
* Documentation and runtime warnings must keep this clear.
* Windows PowerShell Transcript remains degraded/best-effort.
* Windows `auto` must remain `no-log` unless a task explicitly approves promotion.

## Commit Rules

Use small commits.

Good examples:

* `feat(session): add prune dry-run summary`
* `fix(conpty): avoid stale child on abort`
* `docs(logging): clarify ConPTY transcript risks`
* `test(session): cover unsafe prune paths`

Avoid vague commits:

* `update`
* `fix stuff`
* `misc`

## Validation Requirements

Before reporting completion, run:

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build -p td --release --locked
```

If a command cannot be run, explain why and record the skipped command in the result report.

## Result Report

Every agent-completed task should create a result report:

```text
RESULT_TeraDock_<TASK_NAME>.md
```

Include:

* Summary
* Scope
* Changes
* Tests
* Docs updated
* Risks
* Not implemented
* Next steps
* Release recommendation if relevant

## Pull Request Rules

Draft PRs should include:

* Summary
* Scope
* Validation output
* Manual smoke, if applicable
* Known limitations
* Explicit mention of any unchanged safety boundary

Never mark a PR as ready for review unless instructed.
