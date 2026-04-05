# Security Policy

## Reporting Vulnerabilities

If you discover a security vulnerability, please report it responsibly.

**Do not open a public issue.**

Contact: `hoangvananhnghia@gmail.com`

Include: description, affected component, reproduction steps, and potential impact.

### Response Timeline

- Acknowledgment: 24 hours
- Initial assessment: 7 days
- Fix released: 30 days
- Public disclosure: after fix is available

### Scope

In scope: credential exposure, authentication bypass, code injection, unsafe dependencies, privilege escalation, data leakage.

Out of scope: social engineering, phishing, physical security, denial of service, third-party service outages.

## Security Model

### Credential Handling

Luma does not store credentials. It reads from existing tool configurations:

- Anthropic: macOS Keychain or `~/.claude/.credentials.json`
- OpenAI: `~/.codex/auth.json`

Credentials are loaded into memory for HTTP requests. They are not written to disk, logged, or included in error messages. Memory is not currently zeroed on drop.

### OAuth Tokens

OAuth tokens are held in memory for the duration of the session. Expired tokens are refreshed automatically using the refresh token from the credential source.

### Communication

All API communication uses HTTPS with system certificate validation via `reqwest`. Authentication is passed in HTTP headers, never in query parameters. No sensitive data is logged.

### Command Execution

The bash tool executes commands via `bash -c`. A blocklist prevents destructive commands (`rm -rf /`, `git push --force`, `git reset --hard`, `dd`, `mkfs`). Commands are not sanitized beyond this check — the agent controls what commands are run.

### File Access

Read, write, and edit operations act only on paths specified by the agent. There is no sandboxing beyond the filesystem permissions of the running user.

### Debug Logging

Logging is disabled by default. When enabled via `LUMA_DEBUG=1`, logs are written to `/tmp/luma.log`. Credentials and API payloads are not logged.

## Dependencies

Luma uses standard Rust ecosystem crates: `tokio`, `reqwest`, `serde`, `anyhow`, `smallvec`, `regex`, `globset`. Run `cargo audit` to check for known advisories.

## Known Limitations

- Credentials held in memory are not zeroed on drop
- No sandboxing for file or command execution
- No audit logging beyond optional debug log
- No compliance certifications (SOC 2, HIPAA, PCI DSS)

## Privacy

No telemetry, usage tracking, or analytics. All data stays local except API calls to Anthropic and OpenAI.
