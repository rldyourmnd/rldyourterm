# Contributing and Governance

## Contribution Rules

- Keep changes scoped and verifiable.
- Update scripts, docs, and wiki together when behavior changes.
- Prefer deterministic install and validation patterns.
- Avoid destructive operations in shared branches.

## PR Checklist

- [ ] Platform install flow still works for touched files.
- [ ] Relevant dry-run command passes.
- [ ] Relevant health-check passes.
- [ ] `CHANGELOG.md` updated under `[Unreleased]`.
- [ ] Wiki/docs links and commands are still valid.

## Validation Matrix

- Linux: `./scripts/install.sh --dry-run` and `./scripts/health-check.sh --summary`
- macOS: `./scripts/macos/install.sh --dry-run` and `./scripts/health-check-macos.sh --summary`
- Windows: `.\scripts\install-windows.ps1 -DryRun` and `.\scripts\health-check-windows.ps1 -Summary`

## Policy References

- Contributing: <https://github.com/rldyourmnd/rldyourterm/blob/main/CONTRIBUTING.md>
- Security: <https://github.com/rldyourmnd/rldyourterm/blob/main/SECURITY.md>
- Code of Conduct: <https://github.com/rldyourmnd/rldyourterm/blob/main/CODE_OF_CONDUCT.md>
