# Security Policy

## Supported Versions

| Version | Supported |
| --- | --- |
| 1.0.x | Yes |
| Earlier versions | No |

## Reporting A Vulnerability

Do not post sensitive vulnerability details in a public issue.

For now, please avoid posting sensitive details publicly and open a minimal issue requesting private coordination. Include only the affected version, platform, and a short non-sensitive description until a private channel is established.

## Do Not Share Secrets Publicly

Never paste these into issues, pull requests, discussions, screenshots, logs, examples, or fixtures:

- Passwords.
- Tokens.
- Private keys.
- Secret values from `td secret reveal`.
- Full SSH auth arguments.
- Unmasked hostnames, usernames, or internal addresses when they identify a real environment.
- Export files produced with `td export --include-secrets`.

If connection context is required, mask it:

```text
host=<masked-host>
user=<masked-user>
profile_id=<sanitized-profile-id>
```

## Logs And Output

Generally acceptable after review and redaction:

- TeraDock version.
- Operating system and terminal.
- Sanitized command names.
- Sanitized profile IDs.
- Non-sensitive exit codes and error summaries.

Do not include:

- Passwords, tokens, private keys, or decrypted secret values.
- Full SSH command lines or auth arguments.
- Private key paths.
- Raw stdout/stderr that contains credentials, customer data, internal addresses, or other sensitive data.
- Operation logs that identify real hosts or users without masking.

## SSH, Sessions, And Operation Logs

TeraDock records operation metadata for workflows such as CommandSet runs and TUI SSH sessions. This metadata must stay small and safe. It should not include passwords, secret values, tokens, full SSH auth arguments, private key paths, or full command strings.

Before sharing `td recent`, JSON output, screenshots, or operation log excerpts, review and redact host, user, profile, path, and environment-specific values.

## FTP

FTP is insecure because it does not protect credentials or file contents in transit. Prefer SSH-based `scp` or `sftp`. TeraDock treats FTP as an explicitly acknowledged insecure path.

Do not share FTP credentials or session output publicly.
