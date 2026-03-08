#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: validate_release_evidence_freshness.sh [--manifest <path>] [--mode <policy|strict>]

Modes:
  policy  - validate manifest schema + policy/doc anchors only.
  strict  - policy checks + HEAD SHA match + artifact existence/content checks.
USAGE
}

manifest_path="planning/operations/v1.0.0-evidence-manifest.json"
mode="strict"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --manifest" >&2
        usage
        exit 2
      fi
      manifest_path="$2"
      shift 2
      ;;
    --mode)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --mode" >&2
        usage
        exit 2
      fi
      mode="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

case "$mode" in
  policy|strict) ;;
  *)
    echo "invalid mode: $mode (expected policy|strict)" >&2
    exit 2
    ;;
esac

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for JSON validation" >&2
  exit 1
fi

head_sha="$(git rev-parse HEAD)"

python3 - "$manifest_path" "$mode" "$head_sha" <<'PY'
import json
import sys
from pathlib import Path

manifest_rel = sys.argv[1]
mode = sys.argv[2]
head_sha = sys.argv[3]
root = Path.cwd()
manifest_file = root / manifest_rel

errors: list[str] = []
info: list[str] = []

if not manifest_file.exists():
    errors.append(f"manifest missing: {manifest_rel}")
else:
    try:
        data = json.loads(manifest_file.read_text(encoding="utf-8"))
    except Exception as exc:  # pragma: no cover - CLI guard
        errors.append(f"manifest is not valid JSON: {exc}")
        data = {}

    def require(condition: bool, message: str) -> None:
        if not condition:
            errors.append(message)

    require(data.get("schema_version") == "1.0.0", "schema_version must be 1.0.0")
    require(data.get("manifest_kind") == "release-evidence", "manifest_kind must be release-evidence")

    repo = data.get("repo")
    require(isinstance(repo, dict), "repo section must exist")
    if isinstance(repo, dict):
        require(isinstance(repo.get("head_sha"), str), "repo.head_sha must be a string")
        require(isinstance(repo.get("head_short"), str), "repo.head_short must be a string")
        if mode == "strict":
            require(repo.get("head_sha") == head_sha, f"repo.head_sha ({repo.get('head_sha')}) != git HEAD ({head_sha})")

    policy = data.get("policy")
    require(isinstance(policy, dict), "policy section must exist")
    anchor_literal = ""
    docs_to_anchor: list[str] = []
    if isinstance(policy, dict):
        require(policy.get("head_bound_evidence") is True, "policy.head_bound_evidence must be true")
        require(
            policy.get("forbid_unanchored_current_run_claims") is True,
            "policy.forbid_unanchored_current_run_claims must be true",
        )
        anchor_literal = policy.get("required_manifest_anchor_literal", "")
        require(isinstance(anchor_literal, str) and anchor_literal != "", "policy.required_manifest_anchor_literal must be a non-empty string")
        docs_to_anchor = policy.get("docs_require_manifest_anchor", [])
        require(isinstance(docs_to_anchor, list) and len(docs_to_anchor) > 0, "policy.docs_require_manifest_anchor must be a non-empty list")

    if anchor_literal and docs_to_anchor:
        for doc_rel in docs_to_anchor:
            if not isinstance(doc_rel, str) or not doc_rel:
                errors.append(f"invalid docs_require_manifest_anchor entry: {doc_rel!r}")
                continue
            doc_path = root / doc_rel
            if not doc_path.exists():
                errors.append(f"anchored doc is missing: {doc_rel}")
                continue
            text = doc_path.read_text(encoding="utf-8")
            if anchor_literal not in text:
                errors.append(f"manifest anchor literal '{anchor_literal}' is missing in {doc_rel}")
            if "CURRENT RUN" in text:
                errors.append(f"forbidden bare CURRENT RUN claim in {doc_rel}")

    if mode == "strict":
        artifacts = data.get("artifacts")
        require(isinstance(artifacts, list) and len(artifacts) > 0, "artifacts must be a non-empty list in strict mode")
        if isinstance(artifacts, list):
            for entry in artifacts:
                if not isinstance(entry, dict):
                    errors.append(f"artifact entry is not an object: {entry!r}")
                    continue
                name = entry.get("name")
                path_rel = entry.get("path")
                required_patterns = entry.get("required_patterns")
                if not isinstance(name, str) or not name:
                    errors.append(f"artifact.name must be non-empty string: {entry!r}")
                    continue
                if not isinstance(path_rel, str) or not path_rel:
                    errors.append(f"artifact.path must be non-empty string ({name})")
                    continue
                artifact_path = root / path_rel
                if not artifact_path.exists():
                    errors.append(f"artifact file is missing ({name}): {path_rel}")
                    continue
                if not isinstance(required_patterns, list) or len(required_patterns) == 0:
                    errors.append(f"artifact.required_patterns must be non-empty list ({name})")
                    continue
                content = artifact_path.read_text(encoding="utf-8", errors="replace")
                for pattern in required_patterns:
                    if not isinstance(pattern, str) or not pattern:
                        errors.append(f"invalid pattern in {name}: {pattern!r}")
                        continue
                    if pattern not in content:
                        errors.append(f"missing pattern in {path_rel}: {pattern}")
                info.append(f"checked artifact: {name} ({path_rel})")

if errors:
    for message in errors:
        print(f"[FAIL] {message}")
    sys.exit(1)

print(f"[PASS] release evidence manifest validated (mode={mode})")
for message in info:
    print(f"[INFO] {message}")
PY
