## Summary

-

## Scope

-

## Related issue

-

## Test results

Paste the commands that were run and the result:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Security/logging impact

- Does this affect secrets, passwords, tokens, SSH auth arguments, private key paths, operation logs, stdout/stderr display, or export/import data?
- If yes, describe the redaction or logging behavior.

## Documentation updated

-

## Breaking change

- [ ] No breaking change
- [ ] Breaking change, described above

## Checklist

- [ ] `cargo fmt --check`
- [ ] `cargo test`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] README/docs updated when behavior or user workflow changed
- [ ] No secrets, passwords, tokens, private keys, or full SSH auth arguments are logged
- [ ] TUI changes were manually smoke-tested in an interactive terminal
- [ ] Checked whether release notes need an update
