#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

print_usage() {
  cat <<'USAGE'
Usage: ./scripts/release-check.sh [--tag <tag>]

Checks that local release metadata is consistent with the release tag.

Options:
  --tag <tag>   Override tag to validate (default: $GITHUB_REF_NAME if set).
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  print_usage
  exit 0
fi

if [[ -f VERSION ]]; then
  VERSION="$(tr -d '[:space:]' < VERSION)"
else
  echo "ERROR: VERSION file not found."
  exit 1
fi

if [[ -z "$VERSION" || ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "ERROR: VERSION must be a valid semver value in VERSION, got: ${VERSION:-<empty>}"
  exit 1
fi

if [[ "$#" -eq 0 ]]; then
  TAG="${GITHUB_REF_NAME:-}"
elif [[ "$1" == "--tag" ]]; then
  if [[ "$#" -lt 2 || -z "${2:-}" ]]; then
    echo "ERROR: --tag requires a value."
    exit 1
  fi
  TAG="$2"
else
  echo "ERROR: unsupported argument '$1'."
  print_usage
  exit 1
fi

if [[ -z "${TAG:-}" ]]; then
  echo "ERROR: No tag provided. Set GITHUB_REF_NAME or pass --tag <tag>."
  exit 1
fi

if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "ERROR: Tag must follow vMAJOR.MINOR.PATCH format."
  exit 1
fi

RELEASE_VERSION="${TAG#v}"
if [[ "$RELEASE_VERSION" != "$VERSION" ]]; then
  echo "ERROR: Tag version (${RELEASE_VERSION}) does not match VERSION (${VERSION})."
  exit 1
fi

if [[ ! -f "docs/operations/release-${VERSION}.md" ]]; then
  echo "ERROR: Missing docs/operations/release-${VERSION}.md."
  exit 1
fi

if ! git tag --list "$TAG" | grep -qx "$TAG"; then
  echo "ERROR: Git tag '${TAG}' is not available in this checkout."
  exit 1
fi

TAG_COMMIT="$(git rev-parse "${TAG}^{commit}")"
HEAD_COMMIT="$(git rev-parse HEAD)"
if [[ "$TAG_COMMIT" != "$HEAD_COMMIT" ]]; then
  echo "ERROR: Tag '${TAG}' does not point to the checked-out commit."
  exit 1
fi

if ! grep -Eq "^## \\[${VERSION}\\]" CHANGELOG.md; then
  echo "ERROR: CHANGELOG.md missing release section for ${VERSION}."
  exit 1
fi

if ! grep -Eq "^## \\[Unreleased\\]" CHANGELOG.md; then
  echo "ERROR: CHANGELOG.md missing Unreleased section."
  exit 1
fi

if ! grep -Eq "\\[Unreleased\\]: https://github.com/.*/compare/v${VERSION}\\.\\.\\.HEAD" CHANGELOG.md; then
  echo "ERROR: CHANGELOG.md Unreleased compare link must point from v${VERSION}."
  exit 1
fi

if ! grep -Eq "\\[${VERSION}\\]: https://github.com/.*/releases/tag/v${VERSION}" CHANGELOG.md; then
  echo "ERROR: CHANGELOG.md missing release link for v${VERSION}."
  exit 1
fi

if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  echo "ERROR: Working tree contains uncommitted changes."
  exit 1
fi

echo "Release check passed for ${TAG} (VERSION=${VERSION})."
