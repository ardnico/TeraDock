# Secrets encryption, master password gating, and CLI plumbing

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This plan follows `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Deliver the Phase 3 capabilities from PROJECT_PLAN.md: secrets are always stored encrypted with XChaCha20-Poly1305 using a key derived from a user-supplied master password via Argon2id. The CLI gains `td secret set-master/add/list/reveal/rm`, with reveal allowed only when a master password is configured and supplied interactively (non-echo). All secret values avoid logs, and only metadata appears in listings.

## Progress

- [x] (2025-12-31 06:50Z) Draft ExecPlan for secrets, master password, and CLI wiring.
- [x] (2025-12-31 07:10Z) Implement core crypto helpers (Argon2id derivation, XChaCha20-Poly1305 with AAD on secret_id/kind) with unit tests.
- [x] (2025-12-31 07:20Z) Persist master state in `settings` (salt, kdf params, check token) and expose load/set helpers.
- [x] (2025-12-31 07:30Z) Implement `SecretStore` (add/list/delete/reveal) enforcing master presence and ID rules with in-memory DB tests.
- [x] (2025-12-31 07:40Z) Wire CLI `secret` subcommands with non-echo password prompts; logging avoids secret material.
- [x] (2025-12-31 09:15Z) Added CLI argument parsing tests for profile/secret commands to guard CLI surfaces.
- [ ] (2025-12-31 09:05Z) Run `cargo test` and document results (blocked by crates.io access: CONNECT 403; retry when registry reachable).

## Surprises & Discoveries

- Cargo registry access is still blocked (CONNECT 403) when attempting `cargo test`, preventing dependency download; testing remains pending until access is available.
- Latest retry on 2025-12-31 confirms the same crates.io 403 behavior, so workspace tests cannot be executed yet.

## Decision Log

- Decision: Leave secret metadata persistence out of v0.1 despite placeholder `meta` on `NewSecret` because the DB schema in PROJECT_PLAN.md lacks a target column; avoid inventing storage outside the plan.
  Rationale: Keeps schema aligned with agreed plan and prevents silent drift that would require migrations later.
  Date/Author: 2025-12-31 / assistant

## Outcomes & Retrospective

Core crypto, master password handling, and CLI wiring are implemented with added CLI parsing tests, but full validation is blocked by crates.io access (CONNECT 403). Next action is to rerun `cargo test` once registry connectivity is restored.

## Context and Orientation

The workspace already implements Phase 0-2 basics: workspace scaffolding, config/log/db paths, SQLite schema including `secrets`, and profile CRUD with CLI integration. There is no secret encryption, master password handling, or CLI commands for secrets. PROJECT_PLAN.md Section 4 specifies Argon2id-derived keys and XChaCha20-Poly1305 encryption with AAD containing `secret_id` and `kind`, master-password-gated `secret reveal`, and no plaintext display in logs. Settings are stored in the SQLite `settings` table (key/value TEXT).

## Plan of Work

Add a `crypto` module to `tdcore` that performs Argon2id derivation into a 32-byte key using stored parameters and encrypts/decrypts with XChaCha20-Poly1305, including AAD built from secret metadata. Create helpers to generate and persist master state: random salt, default Argon2id parameters, and an encrypted check token stored in `settings` so future password entries can be verified without exposing secrets. Build a `SecretStore` that enforces master presence for add/reveal operations, normalizes/validates `secret_id` using existing ID rules with prefix `s_` auto-generation, and stores ciphertext/nonce/timestamps in the `secrets` table. Expose methods to check whether a master is set, set a new master (fail if already set for safety), load a master key from user input, and perform CRUD operations that never return ciphertext to callers except when revealing a secret explicitly.

Update the CLI to add a `secret` subcommand group with `set-master`, `add`, `list`, `reveal`, and `rm`. Use `rpassword` to capture master password and secret values without echo. `set-master` prompts for confirmation; `reveal` prompts for the master password each time and prints only the plaintext value. Ensure tracing logs never include secret material (only IDs/kinds). Keep `list` output metadata-only. Propagate errors when master is missing to guide the user to set it first.

## Concrete Steps

- Working directory: `/workspace/TeraDock`.
- Add workspace dependencies: `argon2`, `chacha20poly1305` with XChaCha20-Poly1305, `base64`, `rand_core` (or `rand`), `rpassword`, `zeroize` if needed for key wiping.
- Implement `tdcore::crypto` with:
  - `KdfParams { mem_cost_kib, iterations, parallelism }` defaults; `derive_key(password, salt, params) -> [u8; 32]` using Argon2id.
  - `seal(key, nonce, aad, plaintext)` / `open(key, nonce, aad, ciphertext)` wrappers using XChaCha20-Poly1305.
  - Helpers to produce random salt/nonce and an encrypted check token.
- Add a lightweight `settings` helper for get/set string values on a `Connection`.
- Implement `tdcore::secret::{NewSecret, SecretMetadata, SecretStore, MasterState}` providing:
  - `is_master_set`, `set_master(password)`, `load_master(password)` (verifies check token), erroring if unset.
  - `add(master, input)`, `list() -> Vec<SecretMetadata>`, `reveal(master, secret_id) -> String`, `delete(secret_id)`.
- Wire CLI `secret` commands in `crates/cli/src/main.rs` with rpassword prompts and JSON parsing of values when needed; avoid tracing secret plaintext.
- Tests:
  - Crypto: encrypt/decrypt round-trip; AAD mismatch fails.
  - Master workflow: cannot add/reveal without master; set/load succeeds; reveal returns original value.
  - Secret CRUD against in-memory DB.
- Run `cargo test`.

## Validation and Acceptance

Feature is acceptable when `cargo test` passes and the following manual flows succeed:
- `td secret set-master` prompts twice and records master state (second run refuses to overwrite).
- `td secret add --label db --kind password` prompts for master and secret value, then prints the generated secret_id.
- `td secret list` shows metadata without plaintext.
- `td secret reveal <id>` prompts for master and prints the plaintext once; logs do not contain the value.
- `td secret rm <id>` removes the entry and subsequent `reveal` fails with not found.

## Idempotence and Recovery

Master creation is one-time to avoid accidental key loss; resetting would require manual DB cleanup (out of scope). Secret add/delete operations are idempotent w.r.t. DB constraints; repeated `set-master` attempts error early without modifying data. Tests use in-memory DBs to avoid altering real user data. If migration or encryption errors occur, delete the DB and rerun to reapply migrations.

## Artifacts and Notes

Capture any notable test output or error cases in updates to this plan if encountered.

## Interfaces and Dependencies

- New modules: `crates/core/src/crypto.rs`, `crates/core/src/settings.rs`, `crates/core/src/secret.rs`.
- Public API: `secret::{SecretStore, MasterState, NewSecret, SecretMetadata}` with methods `is_master_set`, `set_master`, `load_master`, `add`, `list`, `reveal`, `delete`.
- Dependencies: `argon2` (Argon2id derivation), `chacha20poly1305` (XChaCha20-Poly1305 AEAD), `rand`/`rand_core` for salts/nonces, `base64` for settings encoding, `rpassword` for CLI prompts, `zeroize` optional for key wiping.

---

Update 2025-12-31 09:16Z: Recorded CLI parsing test addition and the continued crates.io blockade preventing full `cargo test` execution.
