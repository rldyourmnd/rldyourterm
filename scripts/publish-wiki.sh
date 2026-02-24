#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WIKI_SRC_DIR="$ROOT_DIR/wiki"

if [[ ! -d "$WIKI_SRC_DIR" ]]; then
  echo "[ERROR] Wiki source directory not found: $WIKI_SRC_DIR"
  exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "[ERROR] gh CLI is required. Install and authenticate first."
  exit 1
fi

if ! gh auth status >/dev/null 2>&1; then
  echo "[ERROR] gh is not authenticated. Run: gh auth login"
  exit 1
fi

REPO="${1:-$(gh repo view --json nameWithOwner --jq .nameWithOwner)}"
WIKI_REPO="$REPO.wiki"

HAS_WIKI="$(gh repo view "$REPO" --json hasWikiEnabled --jq .hasWikiEnabled)"
if [[ "$HAS_WIKI" != "true" ]]; then
  echo "[INFO] Enabling wiki for $REPO"
  gh repo edit "$REPO" --enable-wiki
fi

TMP_DIR="$(mktemp -d -t publish-wiki-XXXXXX)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

set +e
gh repo clone "$WIKI_REPO" "$TMP_DIR/wiki" >/tmp/publish-wiki-clone.log 2>&1
CLONE_EXIT=$?
set -e

if [[ $CLONE_EXIT -ne 0 ]]; then
  echo "[ERROR] Could not clone $WIKI_REPO."
  echo "[HINT] GitHub may require creating the first wiki page in UI before .wiki.git exists."
  echo "[HINT] Open: https://github.com/$REPO/wiki"
  echo "[HINT] Create a Home page once, then rerun this script."
  echo "[DETAIL] Clone output:"
  cat /tmp/publish-wiki-clone.log
  exit 2
fi

# Keep wiki git metadata intact while mirroring source content.
rsync -a --delete --exclude '.git/' "$WIKI_SRC_DIR/" "$TMP_DIR/wiki/"

cd "$TMP_DIR/wiki"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "[ERROR] Cloned wiki directory is not a git repository: $TMP_DIR/wiki"
  exit 3
fi

if [[ -z "$(git status --porcelain)" ]]; then
  echo "[INFO] Wiki is already up to date."
  echo "[INFO] URL: https://github.com/$REPO/wiki"
  exit 0
fi

git add .
git commit -m "docs(wiki): sync wiki from repository source"
git push

echo "[SUCCESS] Wiki published: https://github.com/$REPO/wiki"
