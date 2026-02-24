# Operations Runbook

## Daily Verification

- Linux: `./scripts/health-check.sh --summary`
- macOS: `./scripts/health-check-macos.sh --summary`
- Windows: `.\scripts\health-check-windows.ps1 -Summary`
- Linux health-check now includes recent user-journal runtime diagnostics for:
  - GNOME compositor freeze signatures,
  - rldyourterm runtime warning signatures.

## Pre-Release Gate

1. Run strict checks:
   - Linux: `./scripts/health-check.sh --strict --summary`
   - macOS: `./scripts/health-check-macos.sh --strict`
   - Windows: `.\scripts\health-check-windows.ps1 -Strict`
2. Ensure CI checks are green.
3. Confirm docs/wiki/changelog updates are included.

## Change Workflow

1. Implement script/config/doc changes.
2. Run platform health checks.
3. Update `CHANGELOG.md` under `[Unreleased]`.
4. If behavior changed, update wiki pages in the same branch.

## Runtime Incident Triage (Linux rldyourterm)

```bash
rldyourterm-stable --mode stable
rldyourterm-stable --mode stable
rldyourterm-stable --mode minimal
rldyourterm-stable --mode wayland
rldyourterm-stable --mode software
```

## Wiki Publish Workflow

```bash
./scripts/publish-wiki.sh
```

Requirements:

- `gh` authenticated
- push permission to `<repo>.wiki.git`

## Canonical Runbooks

- Health checks: <https://github.com/rldyourmnd/rldyourterm/blob/main/docs/operations/health-check.md>
- Troubleshooting: <https://github.com/rldyourmnd/rldyourterm/blob/main/docs/operations/troubleshooting.md>
- Upgrade/rollback: <https://github.com/rldyourmnd/rldyourterm/blob/main/docs/operations/upgrade-and-rollback.md>
