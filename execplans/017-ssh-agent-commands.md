# Add SSH agent commands and doctor integration

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root and must be maintained accordingly.

## Purpose / Big Picture

Users need to see and manage their SSH agent state directly from TeraDock, including whether `SSH_AUTH_SOCK` is set, what keys are loaded, and the ability to add or clear keys with explicit confirmation. This change adds `td agent status/list/add/clear`, exposes agent status in `td doctor`, and registers an SSH agent config key so `td config get --resolved` includes it. A user can run `td agent status` to see agent availability, `td agent list` to view loaded keys, `td agent add ~/.ssh/id_ed25519` to add a key with confirmation, and `td doctor` to see agent warnings alongside client checks.

## Progress

- [x] (2025-02-15 01:00Z) Inspected CLI command structure, doctor report shape, and settings registry to decide where agent data should live and how to present it.
- [x] (2025-02-15 01:20Z) Added core SSH agent module, status/list parsing, and doctor integration, updating report structures and serialization.
- [x] (2025-02-15 01:35Z) Wired `td agent status/list/add/clear` CLI commands with confirmation prompts and output formatting.
- [x] (2025-02-15 01:40Z) Registered SSH agent config key in settings registry so resolved config commands recognize it.
- [ ] (2025-02-15 01:45Z) Validate behavior with manual CLI invocations and update progress logs.
- [ ] (2026-01-11 17:09Z) Attempted validation, but `cargo build -p td` failed (crates.io CONNECT 403), so the CLI could not be run.

## Surprises & Discoveries

- Observation: None yet.
  Evidence: No unexpected behavior encountered during implementation.
- Observation: Validation is blocked because the `td` binary cannot be built in this environment.
  Evidence: `cargo build -p td` fails downloading config.json from crates.io.

## Decision Log

- Decision: Centralize ssh-agent inspection in a new `tdcore::agent` module and surface it in doctor output.
  Rationale: Keeps CLI handlers thin and allows doctor + CLI to share the same status/list parsing behavior.
  Date/Author: 2025-02-15 / agent.

## Outcomes & Retrospective

- Pending validation; implementation complete but manual CLI checks still required. Validation is blocked here because the CLI cannot be built due to crates.io CONNECT 403.

## Context and Orientation

The CLI command definitions and handlers live in `crates/cli/src/main.rs`, with subcommands defined in the `Commands` enum and handled in `handle_*` functions. The doctor report is implemented in `crates/core/src/doctor.rs` and serialized to JSON for `td doctor --json`. Configuration settings and schema validation live in `crates/core/src/settings.rs` and `crates/core/src/settings_registry.rs`. The core library module list is in `crates/core/src/lib.rs`, and new modules must be added there. This task introduces a new core module `crates/core/src/agent.rs` to encapsulate SSH agent inspection and command invocation.

## Plan of Work

First, add `crates/core/src/agent.rs` with data structures that summarize agent state and helper functions to run `ssh-add -l` and parse its output into key summaries and counts. The status function should check `SSH_AUTH_SOCK`, run `ssh-add -l` when possible, and return a structured status object containing the auth socket value, key count, and any error message. Next, update `crates/core/src/doctor.rs` to include agent status in `DoctorReport` and emit warnings when the agent socket is missing or `ssh-add -l` fails. Then register a new config key such as `ssh.use_agent` in `crates/core/src/settings_registry.rs` so it appears in schema and resolved config output. Finally, wire `td agent status/list/add/clear` in `crates/cli/src/main.rs`, adding confirmation prompts for add/clear operations, ensuring `ssh-add` is invoked with the provided key path or `-D`, and printing friendly outputs or JSON when requested.

## Concrete Steps

1. Add a new file `crates/core/src/agent.rs` with an `AgentStatus` struct (serialized with `serde`) and helper functions to run `ssh-add -l` and summarize results.
2. Update `crates/core/src/lib.rs` to export the new `agent` module.
3. Extend `crates/core/src/doctor.rs` to include agent status in `DoctorReport` and add warnings based on the status.
4. Update `crates/core/src/settings_registry.rs` to register an SSH agent config key (for example `ssh.use_agent`), with allowed values and examples.
5. Add `Agent` subcommands in `crates/cli/src/main.rs`, including argument structs, command handlers, and confirmation prompts.
6. Manually exercise `td agent status`, `td agent list`, and `td agent clear` in the terminal to confirm output, updating this plan with observed behavior.

## Validation and Acceptance

Run the following from the repository root after implementation:

- `td agent status` prints the SSH auth socket state and a key count summary without crashing, even if no agent is running.
- `td agent list` runs `ssh-add -l` and prints key entries or a friendly message when no keys are loaded.
- `td agent add ~/.ssh/id_ed25519` prompts for confirmation before running `ssh-add` and reports success or errors.
- `td agent clear` performs a two-step confirmation before running `ssh-add -D`.
- `td doctor` includes agent status in both text and JSON output.
- `td config schema ssh.use_agent` shows the new key and allowed values, and `td config get ssh.use_agent --resolved` returns a value or empty when unset.

## Idempotence and Recovery

All steps are additive and safe to repeat. If `ssh-add` is not present or the agent is unavailable, the CLI should report the issue without modifying state. If a command fails halfway, retry by re-running the `td agent` command after fixing the environment (e.g., starting `ssh-agent` or setting `SSH_AUTH_SOCK`).

## Artifacts and Notes

Expected output examples (actual paths and counts vary):

    $ td agent status
    SSH_AUTH_SOCK: /tmp/ssh-XXXXXX/agent.123
    ssh-add: ok
    keys: 2

    $ td agent list
    256 SHA256:abc... user@example (ED25519)
    256 SHA256:def... user@example (ED25519)

    $ td agent clear
    About to remove all keys from ssh-agent.
    Type 'yes' to continue: yes
    Type 'clear' to confirm: clear
    ssh-agent keys cleared.

## Interfaces and Dependencies

In `crates/core/src/agent.rs`, define:

    pub struct AgentStatus {
        pub auth_sock: Option<String>,
        pub key_count: Option<usize>,
        pub keys: Vec<String>,
        pub error: Option<String>,
    }

    pub struct AgentList {
        pub keys: Vec<String>,
        pub raw: String,
        pub error: Option<String>,
    }

    pub fn status() -> AgentStatus
    pub fn list() -> AgentList
    pub fn run_add(key_path: &std::path::Path) -> std::io::Result<std::process::Output>
    pub fn run_clear() -> std::io::Result<std::process::Output>

These helpers should invoke the `ssh-add` binary in `PATH`.

Plan update note: marked implementation steps complete, added decision log, and noted pending manual validation.
Update 2026-01-11 17:09Z: Logged validation attempt blocked by crates.io CONNECT 403 during `cargo build -p td`.
