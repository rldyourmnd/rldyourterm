#!/usr/bin/env bash
set -euo pipefail

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for cargo metadata parsing" >&2
  exit 1
fi

tmp_metadata="$(mktemp)"
cleanup() {
  rm -f "$tmp_metadata"
}
trap cleanup EXIT

cargo metadata --format-version 1 --no-deps > "$tmp_metadata"

python3 - "$tmp_metadata" <<'PY'
import json
import sys

metadata_path = sys.argv[1]
with open(metadata_path, "r", encoding="utf-8") as f:
    metadata = json.load(f)

packages = metadata.get("packages", [])
workspace_packages: dict[str, dict] = {}
for package in packages:
    name = package.get("name")
    if isinstance(name, str) and name.startswith("rldyourterm-"):
        workspace_packages[name] = package

layer_by_package = {
    "rldyourterm-terminal-benchmark": "tooling",
    "rldyourterm-app": "app",
    "rldyourterm-ui": "ui",
    "rldyourterm-services": "services",
    "rldyourterm-core": "core",
    "rldyourterm-foundation": "foundation",
    "rldyourterm-foundation-platform": "foundation",
    "rldyourterm-render-cpu": "features",
    "rldyourterm-render-gpu": "features",
    "rldyourterm-font": "features",
    "rldyourterm-settings": "features",
    "rldyourterm-shell-integration": "features",
    "rldyourterm-diagnostics": "features",
}

allowed_layer_edges = {
    ("tooling", "features"),
    ("tooling", "services"),
    ("tooling", "foundation"),
    ("app", "ui"),
    ("app", "features"),
    ("app", "services"),
    ("app", "foundation"),
    ("ui", "services"),
    ("services", "core"),
    ("services", "foundation"),
    ("features", "features"),
    ("features", "services"),
    ("features", "foundation"),
    ("foundation", "foundation"),
}

explicitly_forbidden_edges = {
    ("rldyourterm-app", "rldyourterm-core"),
    ("rldyourterm-ui", "rldyourterm-core"),
}

errors: list[str] = []
info: list[str] = []

unknown_packages = sorted(name for name in workspace_packages if name not in layer_by_package)
if unknown_packages:
    for package_name in unknown_packages:
        errors.append(f"workspace package has no layer mapping: {package_name}")

for src_name, package in sorted(workspace_packages.items()):
    src_layer = layer_by_package.get(src_name)
    if src_layer is None:
        continue
    dependencies = package.get("dependencies", [])
    for dep in dependencies:
        dep_name = dep.get("name")
        if not isinstance(dep_name, str):
            continue
        if dep_name not in workspace_packages:
            continue
        dst_layer = layer_by_package.get(dep_name)
        if dst_layer is None:
            errors.append(f"workspace dependency has no layer mapping: {src_name} -> {dep_name}")
            continue
        if (src_name, dep_name) in explicitly_forbidden_edges:
            errors.append(f"forbidden dependency edge: {src_name} -> {dep_name}")
            continue
        if (src_layer, dst_layer) not in allowed_layer_edges:
            errors.append(
                f"invalid layer edge: {src_name} ({src_layer}) -> {dep_name} ({dst_layer})"
            )
            continue
        info.append(f"ok: {src_name} ({src_layer}) -> {dep_name} ({dst_layer})")

if errors:
    for message in errors:
        print(f"[FAIL] {message}")
    sys.exit(1)

print("[PASS] VSA dependency graph policy validated against cargo metadata")
print(f"[INFO] validated internal dependency edges: {len(info)}")
PY
