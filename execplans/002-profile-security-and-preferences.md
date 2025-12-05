# Profile editing, encrypted passwords, SSH forwarding, and UI preferences

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. Maintain this file in accordance with `.agent/PLANS.md` from the repository root.

## Purpose / Big Picture

Enable users to manage connection profiles directly from the GUI, including editing core fields, SSH forwarding, and access passwords stored encrypted at rest. Expand settings so users can pick color themes, fonts, and text sizes, and capture any further extension ideas in `PROJECT_PLAN.md`. The result should let a novice launch the GUI, edit a profile, save encrypted credentials and forwarding rules, adjust appearance, and connect using the updated data.

## Progress

- [x] (2025-05-09 00:15Z) Drafted ExecPlan with scope covering profile editing, encryption, SSH forwarding, and UI preferences.
- [x] (2025-05-09 00:16Z) Add core support for encrypted secret storage and extend profile/config models.
- [x] (2025-05-09 00:17Z) Update command builder to honor stored passwords and SSH forwarding rules.
- [x] (2025-05-09 00:18Z) Implement GUI profile editor with password handling, forwarding inputs, and save flow.
- [x] (2025-05-09 00:19Z) Implement settings UI for theme/font/size, persist preferences, and apply at runtime.
- [ ] (2025-05-09 00:20Z) Update PROJECT_PLAN.md with extension ideas and validate via tests/manual checks (ideas added; tests blocked by crates.io 403 on cargo test).

## Surprises & Discoveries

- Observation: `cargo test` failed to download the crates.io index (403) while fetching new dependencies, blocking automated validation.
  Evidence: `cargo test` aborted with "failed to download from https://index.crates.io/config.json" due to CONNECT 403.

## Decision Log

- Decision: Use an application-local AES-256-GCM key stored under the config directory to encrypt/decrypt saved passwords, encoding ciphertexts as base64 strings inside profile records.
  Rationale: Avoid plaintext storage while keeping implementation self-contained and cross-platform without extra OS dependencies.
  Date/Author: 2025-05-09 / Assistant

## Outcomes & Retrospective

To be updated after implementation and validation are complete.

## Context and Orientation

Current workspace contains core models for profiles (`crates/core/src/profile.rs`), config handling (`crates/core/src/config.rs`), command building (`crates/core/src/command.rs`), and GUI launcher (`crates/gui/src/main.rs`). Profiles are read-only in the GUI aside from pin toggling. Settings tab only edits the Tera Term path. There is no password storage or SSH forwarding support, and no UI theming controls. This plan adds encrypted password storage, editable profile forms (including forwarding rules and other fields), enhanced settings for appearance, and updates to persist and apply these choices.

## Plan of Work

Describe, in prose, the sequence of edits and additions. For each edit, name the file and location (function, module) and what to insert or change. Keep it concrete and minimal.

1. Extend core models:
   - In `crates/core/src/profile.rs`, add fields for encrypted password text and SSH forwarding rules (a structured list) with serde defaults. Provide helper methods for applying forwarding arguments.
   - In `crates/core/src/config.rs`, add UI preference fields (theme selection, font family, text size) with defaults and normalization so older settings files remain valid. Add helper to persist these preferences.
   - Introduce `crates/core/src/secrets.rs` for AES-256-GCM encryption/decryption, generating a key file inside the config directory. Surface through `ttcore` lib exports.
   - Update `crates/core/src/error.rs` to include crypto errors.

2. Command building:
   - In `crates/core/src/command.rs`, thread optional decrypted password and SSH forwarding arguments into the generated command-line arguments for Tera Term. Keep backward compatibility for profiles without these fields.

3. GUI profile editor:
   - In `crates/gui/src/main.rs`, expand state to hold an editable form for the selected profile, including fields for name/host/port/protocol/user/group/tags/description/macro path/color, password input, and SSH forwarding entries. Provide add/remove controls and validation, persisting changes back to the profiles file using the core `ProfileSet::save` and secret store for encryption.
   - Add connection path to decrypt stored passwords when launching Tera Term.

4. Settings and theming:
   - Extend the Settings tab to edit and persist UI preferences (theme light/dark/system, font family choice, and base text size). Apply preferences immediately via egui style updates when changed.

5. Documentation update:
   - Append extension candidates discovered during implementation to `PROJECT_PLAN.md` under an appropriate section.

6. Validation:
   - Run `cargo fmt` and `cargo test`. If new functionality requires manual checks, describe commands/flows to exercise profile editing, password encryption/decryption, SSH forwarding args, and theme switching.

## Concrete Steps

- Add dependencies to `Cargo.toml` (workspace and relevant crates) for AES-GCM encryption, base64 encoding, and random key generation.
- Implement `secrets.rs` with key management and encrypt/decrypt helpers; export via `ttcore`.
- Update profile model with new fields and serde defaults, plus helper for forwarding arg rendering.
- Update config model with UI preference struct and normalization; adjust load/save defaults.
- Adjust command builder to include password and forwarding arguments when present.
- Enhance GUI state and UI: editable profile form, password handling (encrypt on save, decrypt on connect), forwarding editor, settings tab for appearance, and immediate style application.
- Update `PROJECT_PLAN.md` with extension ideas noted during work.
- Run formatting and tests; record results and any manual validation outcomes.

## Validation and Acceptance

- `cargo fmt` shows no diff.
- `cargo test` passes.
- Manual GUI check: user can select a profile, edit fields (including adding forwarding and entering a password), save, restart app, and see persisted encrypted password and forwarding entries reflected; connect uses the stored password. Theme/font/size selections apply immediately and persist after restart.

## Idempotence and Recovery

Changes are additive and default-backed. If encryption key file is missing or corrupted, regenerating it will invalidate stored ciphertexts; mitigate by handling decryption errors gracefully and allowing password re-entry. Profile and settings saves create parent directories as needed. Re-running validation commands is safe.

## Artifacts and Notes

Add short transcripts or observations here after running commands or manual validation during implementation.

## Interfaces and Dependencies

- New dependency: AES-256-GCM (`aes-gcm`), random key generation (`rand_core`/`rand`), and base64 for ciphertext encoding.
- New module: `ttcore::secrets::SecretStore` exposing `encrypt(&self, plaintext: &[u8]) -> Result<String>` and `decrypt(&self, b64: &str) -> Result<String>` plus `new(key_path: impl Into<PathBuf>)` that ensures key creation.
- Profile additions: `password: Option<String>` (encrypted text), `ssh_forwardings: Vec<SshForwarding>` where `SshForwarding { direction: ForwardDirection, local_host: Option<String>, local_port: u16, remote_host: String, remote_port: u16 }` with serde defaults.
- Config additions: `ui: UiPreferences { theme: ThemePreference, font_family: String, text_size: f32 }` with defaults and normalization. Apply in GUI settings tab.

Revision note (2025-05-09 00:15Z): Initial draft capturing goals and planned edits.
