# Security

## Responsible Disclosure

Use GitHub Security Advisories to report vulnerabilities:

- <https://github.com/rldyourmnd/rldyourterm/security/advisories>

## Security Baselines in This Repo

- Strict-mode scripts (`set -euo pipefail` / `Set-StrictMode`)
- User-local install paths where appropriate
- Cross-platform health-check validation
- Download integrity hardening in installers

## Operator Rules

- Never commit secrets/tokens.
- Review install script diffs before execution.
- Use least privilege (`sudo` only when needed).
- Re-run strict health checks after security-relevant changes.

## Security-Relevant Tools

- `semgrep`
- `ast-grep`
- `grepai` (for investigation/navigation)

## Policy Reference

- <https://github.com/rldyourmnd/rldyourterm/blob/main/SECURITY.md>
