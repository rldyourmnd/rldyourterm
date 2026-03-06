# Security Policy

## Supported Versions

The project currently maintains the latest `main` branch for security fixes.

| Version | Supported |
| --- | --- |
| `main` | :white_check_mark: |
| older snapshots/tags | :x: |

## Reporting a Vulnerability

Please do **not** open a public issue for sensitive vulnerabilities.

1. Open a private report through GitHub Security Advisories ("Report a vulnerability").
2. Include:
   - affected commit/tag,
   - reproduction steps,
   - impact assessment,
   - suggested remediation (if available).
3. We will acknowledge receipt within **72 hours**.
4. We will provide a remediation timeline after triage and coordinate disclosure.

## Disclosure Process

- Critical issues are prioritized immediately.
- A fix will be prepared on a private branch and validated against quality gates.
- Public disclosure is coordinated after a fix is available.

## Scope

This policy covers:
- runtime crash/escape paths,
- PTY/shell interaction boundaries,
- clipboard/input/output processing,
- dependency and CI supply-chain security.

## Known Advisory Exceptions

- `RUSTSEC-2024-0436` (`paste`, informational/unmaintained) is currently accepted as a **temporary transitive risk** via the `wgpu-hal -> metal` macOS dependency path.
- The ignore is tracked in `.cargo/audit.toml` with rationale.
- This exception must be re-evaluated on each dependency refresh and removed as soon as an upstream-maintained path is available.
