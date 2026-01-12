# Implement SSH forward storage, tunnel management, and session tracking

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This plan follows the requirements in `.agent/PLANS.md` from the repository root and must be maintained accordingly.

## Purpose / Big Picture

Users need a way to persist SSH forward definitions, start long-running SSH tunnels that apply those forwards, and list/stop those tunnel sessions later. After this change, a user can define forwards tied to a profile, run `td tunnel start` to launch an SSH tunnel in the background, see its status and PID, and stop it; dead tunnels are detected and cleaned up. The behavior is observable through CLI output and the ability to verify SSH processes exist or have been removed.

## Progress

- [x] (2026-01-11 15:53Z) Reviewed repository structure, database schema, and CLI command layout for adding tunnel/forward/session support.
- [x] (2026-01-11 16:01Z) Implemented SSH forward storage, validation, and normalization rules in core.
- [x] (2026-01-11 16:01Z) Added session tracking schema and core store, plus CLI tunnel start/stop/status wiring.
- [x] (2026-01-11 16:55Z) Restored CLI dispatch for `td tunnel` commands and added process termination handling for tunnel stop.
- [ ] (2026-01-11 16:01Z) Validate tunnel lifecycle manually and record any observed output in this plan.
- [x] (2026-01-11 16:01Z) Updated the ExecPlan with implementation outcomes and decisions.

## Surprises & Discoveries

- Observation: Existing database migrations already go up to schema v3 and include the `ssh_forwards` table but no session table.
  Evidence: `crates/core/src/db.rs` defines migrations through `PRAGMA user_version = 3`.
- Observation: Tunnel CLI dispatch paths were missing, so `td tunnel start/status/stop` did not execute.
  Evidence: No `Commands::Tunnel` branch in the CLI match prior to wiring.
- Observation: Validation is blocked because the `td` binary cannot be built in this environment.
  Evidence: `cargo build -p td` fails downloading config.json from crates.io.

## Decision Log

- Decision: Store tunnel sessions in a dedicated `sessions` table with JSON-encoded forward names and PID so dead processes can be detected and cleaned up.
  Rationale: The project plan calls for session tracking with PID and forward list; a dedicated table keeps session-specific data independent of operational logs.
  Date/Author: 2026-01-11 / agent
- Decision: Use OS tooling (`kill -0` on Unix, `tasklist` on Windows) to check PID liveness when cleaning tunnel sessions.
  Rationale: Avoids pulling in new dependencies while still allowing dead-session cleanup in common environments.
  Date/Author: 2026-01-11 / agent

## Outcomes & Retrospective

- Implemented forward storage/validation, session tracking, and tunnel CLI commands. The system now records tunnel sessions with PID and forwards, and `tunnel status` prunes dead sessions before listing. CLI dispatch has been restored so tunnel commands execute and stop sessions via PID termination.

Remaining: manual validation run and transcript capture, plus any adjustments discovered during live SSH tunnel checks. Validation is blocked here because the CLI cannot be built due to crates.io CONNECT 403.

## Context and Orientation

The core database schema is defined in `crates/core/src/db.rs`. It already creates an `ssh_forwards` table with `profile_id`, `name`, `kind`, `listen`, and `dest` columns, but the rest of the codebase does not expose this data. The CLI entry point is `crates/cli/src/main.rs`, which defines top-level subcommands and handlers. SSH command construction is done in `crates/cli/src/main.rs` using helper functions such as `resolve_client_for` and `ssh_auth_context`. Profiles are stored and fetched via `crates/core/src/profile.rs` using `ProfileStore`. The project plan in `PROJECT_PLAN.md` defines forward rules (listen port normalization, destination host requirement, and dynamic forwards without a destination) and session tracking expectations.

Terms: A "forward" is a saved SSH port forwarding rule (local, remote, or dynamic). A "tunnel" is a long-running SSH process that applies one or more forwards and keeps the connection open. A "session" is the persisted record of a running tunnel, including its PID and applied forwards.

## Plan of Work

First, implement a core module `crates/core/src/tunnel.rs` that defines forward data structures, validation/normalization rules, and a store layer for CRUD on `ssh_forwards`. The rules will normalize a port-only `listen` value to `127.0.0.1:<port>`, require an explicit host in `dest` for local/remote forwards, and allow dynamic forwards to omit `dest` (stored as an empty string). The store will enforce per-profile uniqueness of forward names.

Next, extend the database migrations in `crates/core/src/db.rs` with a new schema version that adds a `sessions` table including `session_id`, `kind`, `profile_id`, `pid`, `started_at`, and `forwards_json`. Add a `SessionStore` in `tunnel.rs` to create/list/stop sessions, and a helper to check whether the stored PID is alive and prune dead sessions.

Then, update `crates/cli/src/main.rs` to add a `tunnel` command with `start`, `status`, and `stop` subcommands. The `start` command will load the profile and forward definitions, construct the SSH command with `-N` and the appropriate `-L/-R/-D` flags, spawn the process, and record the session in the new `sessions` table. The `status` command will list current sessions, checking for dead PIDs and removing them. The `stop` command will find the session by ID, attempt to terminate the PID, and remove the session record.

Finally, run manual validation by starting a tunnel, listing sessions, and stopping it, noting expected CLI output and PID state. Update this ExecPlan with actual outcomes, decisions, and surprises.

## Concrete Steps

1. Add `crates/core/src/tunnel.rs` with forward and session store types, parsing/normalization helpers, and public APIs used by the CLI.
2. Update `crates/core/src/lib.rs` to export the new module.
3. Add a schema migration in `crates/core/src/db.rs` for the `sessions` table.
4. Extend `crates/cli/src/main.rs` with `tunnel` subcommands and handlers that use the new core APIs.
5. Run manual commands from the repo root (expected to be run by the developer during validation):

   - `td tunnel start <profile_id> --forward <name>`
   - `td tunnel status`
   - `td tunnel stop <session_id>`

   Observe that status lists a PID after start and that the session disappears after stop.

## Validation and Acceptance

Validation is successful when a user can start a tunnel for an SSH profile using stored forward definitions, see the tunnel listed with a PID, and stop it, after which status no longer shows the session. If a PID is already dead, `tunnel status` should remove it from the list and report the cleanup. The CLI should reject invalid forward definitions: port-only listen values are normalized, destinations must include a host for local/remote forwards, and dynamic forwards must not require a destination.

## Idempotence and Recovery

Schema migrations are safe to run multiple times via the existing `user_version` gating. Creating a tunnel session twice should produce two entries; stopping a non-existent or already-dead session should return a clear error and leave the system in a consistent state. If an SSH process fails to start, no session should be persisted.

## Artifacts and Notes

If needed, include a short transcript in this section showing `td tunnel status` output before and after stopping a session.

## Interfaces and Dependencies

In `crates/core/src/tunnel.rs`, define:

    pub enum ForwardKind { Local, Remote, Dynamic }
    pub struct Forward { pub id: i64, pub profile_id: String, pub name: String, pub kind: ForwardKind, pub listen: String, pub dest: Option<String> }
    pub struct NewForward { pub profile_id: String, pub name: String, pub kind: ForwardKind, pub listen: String, pub dest: Option<String> }
    pub struct ForwardStore { pub fn insert(&self, input: NewForward) -> Result<Forward>; pub fn list_for_profile(&self, profile_id: &str) -> Result<Vec<Forward>>; pub fn get_by_name(&self, profile_id: &str, name: &str) -> Result<Option<Forward>>; pub fn remove(&self, profile_id: &str, name: &str) -> Result<()> }

    pub enum SessionKind { Tunnel }
    pub struct Session { pub session_id: String, pub kind: SessionKind, pub profile_id: String, pub pid: Option<u32>, pub started_at: i64, pub forwards: Vec<String> }
    pub struct NewSession { pub kind: SessionKind, pub profile_id: String, pub pid: Option<u32>, pub forwards: Vec<String> }
    pub struct SessionStore { pub fn insert(&self, input: NewSession) -> Result<Session>; pub fn list(&self) -> Result<Vec<Session>>; pub fn remove(&self, session_id: &str) -> Result<()>; pub fn cleanup_dead(&self) -> Result<Vec<Session>> }

The CLI should use `SessionStore` and `ForwardStore` and rely on existing SSH client resolution and authentication helpers in `crates/cli/src/main.rs`.

---

Change note: Initial ExecPlan created from repo research and project plan requirements.
Change note: Updated progress, decisions, and outcomes after implementing tunnel/forward/session support.
Update 2026-01-11 17:09Z: Logged validation attempt blocked by crates.io CONNECT 403 during `cargo build -p td`.
