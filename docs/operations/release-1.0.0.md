# Release 1.0.0

## 2026-02-24

- Completed full rldyourterm rebrand in project-facing layers and command/runtime naming.
- Normalized terminal distribution version metadata to `1.0.0`.
- Added release runbook to operations references:
  - `docs/operations/health-check.md`
  - `docs/platforms/linux/reference/foundation.md`
  - `wiki/Operations-Runbook.md`
- Added launcher/build consolidation around `rldyourterm` entrypoints and wrappers.
- Preserved foundation runtime code as the upstream terminal core under `rldyourterm/` for source continuity.
- Added release infrastructure for tag-driven publishing:
  - `.github/workflows/release.yml`
  - `scripts/release-check.sh`
  - `docs/operations/release.md`
