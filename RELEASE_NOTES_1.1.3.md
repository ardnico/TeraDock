# TeraDock v1.1.3

v1.1.3 is a release consistency patch after v1.1.2.

The remote v1.1.2 tag was already published before the package and binary
version metadata fix, so this patch advances the release-facing version state
to 1.1.3 instead of moving or replacing the existing v1.1.2 tag.

## Fixed

* Updated Cargo workspace/package versions and Cargo.lock workspace package
  entries to 1.1.3.
* Ensured the packaged binary reports `td 1.1.3`.
* Updated README stable-version and artifact examples to 1.1.3.

## Preserved scope

* Preserves the v1.1.2 session-log operations scope: `td session prune --json`,
  `td session stats`, and `td session stats --json`.
* Does not change prune/stats behavior or JSON schema.
* Does not change Windows `auto -> conpty`, TUI/ConPTY behavior, or session
  logging backend selection.
* Does not add secret masking, terminal replay, or real SSH automated tests.
