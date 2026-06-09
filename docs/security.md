# Security Notes

TeraDock is local-first: profiles, CommandSets, settings, operation logs, and encrypted secrets live in a local SQLite database under the TeraDock config directory.

## Secrets

Secrets are encrypted with a master password. Listing commands show metadata only. Revealing a secret requires the master password:

```bash
td secret set-master
td secret add --kind password --label device-login
td secret reveal <secret_id>
```

Do not put raw passwords, tokens, or private keys in profile notes, CommandSet commands, README examples, export fixtures, or operation logs.

## Critical Profiles

Use `--danger critical` for production, safety-sensitive, or fragile targets. Critical profiles require explicit confirmation before connect, exec, run, transfer, and config apply operations. In the TUI, interactive SSH sessions and single-profile CommandSet runs require typing the profile id; bulk runs require typing the listed critical ids exactly.

## FTP

FTP is insecure because it does not protect credentials or file contents in transit. Prefer SSH-based `scp` or `sftp`. TeraDock requires explicit opt-in before FTP transfers can run.

## Import And Export

Default exports include secret metadata but not secret values:

```bash
td export -o teradock-export.json
```

`td export --include-secrets` writes decrypted secret values into the export JSON after master password verification. Treat that file as highly sensitive and remove it when it is no longer needed.

Review imported files before loading them. Import can add profiles, CommandSets, parser definitions, config sets, and secret metadata.

## Operation Logs

TeraDock records operation type, profile id, client used, success/failure, exit code, duration, and small metadata such as CommandSet id. TUI interactive SSH sessions are recorded as `ssh_session` after the SSH process exits or after process launch failure.

Operation logs must stay free of secrets. Passwords, secret values, tokens, SSH auth arguments, and full command strings are not written to TUI SSH session log metadata. Command stdout and stderr are shown to the caller but are not currently stored in `op_logs`.
