# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project uses semantic versioning.

## [0.1.0] - 2026-06-10

### Added

- Profile management for SSH, Telnet, and Serial connection profiles.
- CommandSet execution for reusable command sequences against SSH profiles.
- TUI profile browsing, search, filtering, marking, CommandSet execution, and result tabs.
- TUI interactive SSH sessions with `s`.
- Critical profile confirmation for high-risk operations.
- SSH session operation logs and `td recent`.
- Secret management with encrypted local storage.
- Config, environment, and config set features.
- File transfer commands, including SCP/SFTP and explicitly acknowledged FTP.
- SSH tunnel commands.
- Import/export support.
- Doctor command for dependency and environment checks.

### Changed

- SSH invocation construction moved into `tdcore::ssh` for shared CLI/TUI behavior.
- CLI and TUI CommandSet execution share `tdcore::cmdset_runner`.
- README and docs onboarding now emphasize `td init --with-samples`, safe CommandSets, TUI usage, and security boundaries.

### Security

- TUI SSH session logs avoid passwords, secret values, SSH auth arguments, private key paths, and full command strings.
- FTP transfers require both configuration opt-in and explicit insecure acknowledgement.
- Critical profiles require typed confirmation before sensitive operations.
- Default export excludes decrypted secret values unless `--include-secrets` is used after master password verification.

### Known Limitations

- TUI recent pane is not implemented.
- Terminal emulator launch is not implemented.
- tmux integration is not implemented.
- `tdcore::cmdset_runner` still receives the SSH path and auth args separately.
- Transfer and tunnel command shapes are not fully converted to `SshInvocation`.
- Real SSH server integration tests are not included in the automated test suite.
