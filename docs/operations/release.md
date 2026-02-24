# Release Operations

## Workflow

Releases are created by pushing a semantic version tag (`vMAJOR.MINOR.PATCH`) from `main`.

1. Finalize `CHANGELOG.md` and all release notes.
2. Ensure `VERSION` is set to the release version.
3. Commit all release-related docs/code changes.
4. Create the release tag on the release commit.

```bash
git checkout main
git pull --ff-only
git tag -a "v1.0.0" -m "Release 1.0.0"
git push origin v1.0.0
```

## Local release validation

Run before tagging:

```bash
./scripts/release-check.sh --tag v1.0.0
```

Validation checks:

- `VERSION` is a valid `MAJOR.MINOR.PATCH` value.
- The tag matches `VERSION`.
- `docs/operations/release-<VERSION>.md` exists.
- `CHANGELOG.md` includes:
  - `Unreleased` section
  - release section for the version
  - updated comparison/release links
- The tag resolves to the current commit.

## Publish

Pushing the tag triggers `.github/workflows/release.yml`, which:

1. re-runs `scripts/release-check.sh --tag <tag>`;
2. creates a GitHub release with generated notes.

## Post-Release

- Confirm the release page exists and includes expected notes.
- Keep release notes and bootstrap docs aligned.
