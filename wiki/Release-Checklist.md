# Release Checklist

## 1. Prepare

- Ensure `CHANGELOG.md` has complete `[Unreleased]` notes.
- Confirm docs and wiki reflect actual commands/flags.
- Confirm version bump/tag strategy.
- Confirm `VERSION` matches release tag.

## 2. Validate

- Linux:
  - `./scripts/install.sh --dry-run`
  - `./scripts/health-check.sh --strict --summary`
  - validate renderer/display fallbacks for terminal stability:
    - `rldyourterm-stable --mode stable`
    - `rldyourterm-stable --mode stable`
    - `rldyourterm-stable --mode minimal`
    - `rldyourterm-stable --mode wayland`
    - `rldyourterm-stable --mode software`
- macOS:
  - `./scripts/macos/install.sh --dry-run`
  - `./scripts/health-check-macos.sh --strict`
- Windows:
  - `.\scripts\install-windows.ps1 -DryRun`
  - `.\scripts\health-check-windows.ps1 -Strict`

## 3. CI Gate

- Required workflows must be green.
- No failing lint/link checks.

## 4. Publish

- Merge to `main`.
- Create tag and release notes.
- Validate with:
  - `./scripts/release-check.sh --tag <tag>`
- Publish wiki updates if changed:

```bash
./scripts/publish-wiki.sh
```

## 5. Post-Release

- Update comparison links in changelog if needed.
- Confirm release page assets/notes are correct.
