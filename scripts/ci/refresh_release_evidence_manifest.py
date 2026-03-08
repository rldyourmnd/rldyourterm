#!/usr/bin/env python3
"""Refresh the release evidence manifest for the current HEAD and latest MVP logs."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


ARTIFACT_PREFIXES = {
    "compatibility_matrix_transcript": "matrix-",
    "profile_claude_log": "claude-",
    "profile_codex_log": "codex-",
    "profile_gemini_log": "gemini-",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Refresh planning/operations/v1.0.0-evidence-manifest.json for the current HEAD "
            "and latest scripts/mvp/output/*.log artifacts."
        )
    )
    parser.add_argument(
        "--manifest",
        default="planning/operations/v1.0.0-evidence-manifest.json",
        help="source manifest path relative to repo root (default: %(default)s)",
    )
    parser.add_argument(
        "--output",
        help=(
            "write refreshed manifest to this path relative to repo root or absolute path; "
            "defaults to overwriting --manifest"
        ),
    )
    parser.add_argument(
        "--artifacts-dir",
        default="scripts/mvp/output",
        help="directory containing MVP artifact logs (default: %(default)s)",
    )
    parser.add_argument(
        "--head-sha",
        help="override git HEAD SHA for detached/manual release preparation",
    )
    parser.add_argument(
        "--generated-at-utc",
        help="override manifest generated_at_utc (RFC3339 / ISO-8601 UTC)",
    )
    return parser.parse_args()


def repo_root() -> Path:
    return Path.cwd()


def resolve_path(root: Path, path: str) -> Path:
    candidate = Path(path)
    if candidate.is_absolute():
        return candidate
    return root / candidate


def git_head_sha(root: Path) -> str:
    return (
        subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True)
        .strip()
    )


def latest_matching_log(artifacts_dir: Path, prefix: str) -> Path:
    matches = sorted(artifacts_dir.glob(f"{prefix}*.log"))
    if not matches:
        raise FileNotFoundError(
            f"no artifact logs matching {prefix}*.log in {artifacts_dir}"
        )
    return matches[-1]


def main() -> int:
    args = parse_args()
    root = repo_root()
    manifest_path = resolve_path(root, args.manifest)
    output_path = resolve_path(root, args.output or args.manifest)
    artifacts_dir = resolve_path(root, args.artifacts_dir)

    if not manifest_path.exists():
        print(f"manifest not found: {manifest_path}", file=sys.stderr)
        return 1
    if not artifacts_dir.exists():
        print(f"artifacts directory not found: {artifacts_dir}", file=sys.stderr)
        return 1

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    head_sha = args.head_sha or git_head_sha(root)
    generated_at = args.generated_at_utc or datetime.now(timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ"
    )

    repo = manifest.setdefault("repo", {})
    repo["head_sha"] = head_sha
    repo["head_short"] = head_sha[:7]
    manifest["generated_at_utc"] = generated_at

    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, list):
        print("manifest.artifacts must be a list", file=sys.stderr)
        return 1

    artifact_map = {}
    for entry in artifacts:
        name = entry.get("name")
        if isinstance(name, str) and name:
            artifact_map[name] = entry

    missing_entries = [name for name in ARTIFACT_PREFIXES if name not in artifact_map]
    if missing_entries:
        print(
            "manifest is missing required artifact entries: "
            + ", ".join(sorted(missing_entries)),
            file=sys.stderr,
        )
        return 1

    for name, prefix in ARTIFACT_PREFIXES.items():
        latest = latest_matching_log(artifacts_dir, prefix)
        try:
            artifact_map[name]["path"] = latest.relative_to(root).as_posix()
        except ValueError:
            artifact_map[name]["path"] = latest.as_posix()

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )

    print(f"[PASS] refreshed release evidence manifest: {output_path}")
    print(f"[INFO] repo.head_sha={head_sha}")
    for name in ARTIFACT_PREFIXES:
        print(f"[INFO] {name} -> {artifact_map[name]['path']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
