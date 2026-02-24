# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.x     | :white_check_mark: |
| < 1.x   | :x:                |

## Reporting a Vulnerability

We take the security of rldyourterm seriously. If you have discovered a security vulnerability, we appreciate your help in disclosing it to us in a responsible manner.

### How to Report

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, please report them via GitHub Security Advisories:

1. Go to the [Security Advisories](https://github.com/rldyourmnd/rldyourterm/security/advisories) page
2. Click "Report a vulnerability"
3. Fill out the form with details about the vulnerability

### What to Include

Please include the following information:

- Type of vulnerability
- Full paths of source file(s) related to the vulnerability
- The location of the affected source code (tag/branch/commit)
- Any special configuration required to reproduce the issue
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the vulnerability

### Response Timeline

- **Initial Response**: Within 48 hours
- **Status Update**: Within 7 days
- **Resolution**: Depends on complexity, typically within 30 days

### Disclosure Policy

- We follow responsible disclosure practices
- We will credit you for your findings (unless you prefer to remain anonymous)
- We request that you do not disclose the vulnerability publicly until we have addressed it

## Security Best Practices

When using rldyourterm:

1. **Review installation scripts** before running them
2. **Use sudo only when necessary** - our scripts only require sudo for system packages
3. **Keep tools updated** - run installation scripts periodically to update
4. **Review configuration files** before applying them to your system
5. **Report suspicious code** - if you find anything concerning, please report it
6. **Run strict health checks** after changes:
   - Linux: `./scripts/health-check.sh --strict --summary`
   - macOS: `./scripts/health-check-macos.sh --strict`
   - Windows: `.\scripts\health-check-windows.ps1 -Strict`

## Known Security Considerations

- Some tools require API keys (e.g., for cloud services)
- Never commit API keys or secrets to the repository
- Configuration files may contain sensitive paths - review before sharing
- Release downloads can fail integrity checks when upstream checksums are missing;
  prefer official release assets and documented checksum values when pinning.

Thank you for helping keep rldyourterm secure! 🔐
