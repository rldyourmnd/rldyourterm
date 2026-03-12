#!/usr/bin/env python3
import argparse
import json
import pathlib
import sys

try:
    from scripts.ci.terminal_benchmark_manifest import require_suite_manifest
except ModuleNotFoundError:
    from terminal_benchmark_manifest import require_suite_manifest

EXPECTED_TOOL = "terminal-benchmark"
EXPECTED_SUITE = "canonical-headless"


def fail(message: str) -> None:
    raise SystemExit(f"benchmark report validation failed: {message}")


def require_list(payload: dict, key: str) -> list:
    value = payload.get(key)
    if not isinstance(value, list):
        fail(f"{key} must be a list")
    return value


def validate_coverage_layers(entries: list, manifest_names: set[str], label: str) -> None:
    actual_layers = set()
    for entry in entries:
        if not isinstance(entry, dict):
            fail(f"coverage.{label} entries must be objects")
        layer = entry.get("layer")
        if not isinstance(layer, str) or not layer:
            fail(f"coverage.{label}.layer must be a non-empty string")
        actual_layers.add(layer)
        benchmark_scenarios = entry.get("benchmark_scenarios")
        validation_commands = entry.get("validation_commands")
        notes = entry.get("notes")
        if not isinstance(benchmark_scenarios, list):
            fail(f"coverage entry {layer} benchmark_scenarios must be a list")
        for scenario in benchmark_scenarios:
            if not isinstance(scenario, str) or not scenario:
                fail(f"coverage entry {layer} benchmark_scenarios must contain non-empty strings")
            if scenario not in manifest_names:
                fail(f"coverage entry {layer} references unknown scenario {scenario!r}")
        if not isinstance(validation_commands, list) or not validation_commands:
            fail(f"coverage entry {layer} validation_commands must be a non-empty list")
        if not isinstance(notes, str) or not notes:
            fail(f"coverage entry {layer} notes must be a non-empty string")
    if len(actual_layers) != len(entries):
        fail(f"coverage.{label} must not contain duplicate layers")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("--require-scenario", action="append", default=[])
    parser.add_argument("--require-full-suite", action="store_true")
    args = parser.parse_args()

    with args.report.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)

    if payload.get("benchmark_tool") != EXPECTED_TOOL:
        fail(f"benchmark_tool must be {EXPECTED_TOOL!r}")
    if payload.get("suite") != EXPECTED_SUITE:
        fail(f"suite must be {EXPECTED_SUITE!r}")
    try:
        manifest_map = require_suite_manifest(payload)
    except ValueError as exc:
        fail(str(exc))

    selected_scenarios = require_list(payload, "selected_scenarios")
    if not selected_scenarios:
        fail("selected_scenarios must not be empty")
    if any(not isinstance(name, str) or not name for name in selected_scenarios):
        fail("selected_scenarios entries must be non-empty strings")
    if len(selected_scenarios) != len(set(selected_scenarios)):
        fail("selected_scenarios must be unique")

    results = require_list(payload, "results")
    if not results:
        fail("results must not be empty")

    result_names: list[str] = []
    for entry in results:
        if not isinstance(entry, dict):
            fail("results entries must be objects")
        scenario = entry.get("scenario")
        if not isinstance(scenario, str) or not scenario:
            fail("result scenario must be a non-empty string")
        expected = manifest_map.get(scenario)
        if expected is None:
            fail(f"unexpected scenario {scenario!r}")
        if entry.get("layer") != expected["layer"]:
            fail(f"scenario {scenario!r} has unexpected layer {entry.get('layer')!r}")
        if entry.get("benchmark_kind") != expected["benchmark_kind"]:
            fail(
                f"scenario {scenario!r} has unexpected benchmark_kind {entry.get('benchmark_kind')!r}"
            )
        if not isinstance(entry.get("description"), str) or not entry["description"]:
            fail(f"scenario {scenario!r} must include a non-empty description")
        if not isinstance(entry.get("primary_unit_label"), str) or not entry["primary_unit_label"]:
            fail(f"scenario {scenario!r} must include primary_unit_label")
        if not isinstance(entry.get("stats"), dict):
            fail(f"scenario {scenario!r} must include stats object")
        if not isinstance(entry.get("notes"), list):
            fail(f"scenario {scenario!r} must include notes list")
        result_names.append(scenario)

    if len(result_names) != len(set(result_names)):
        fail("results must not contain duplicate scenario names")
    if set(result_names) != set(selected_scenarios):
        fail(
            f"results must match selected_scenarios exactly: results={sorted(result_names)} selected={sorted(selected_scenarios)}"
        )

    for scenario in args.require_scenario:
        if scenario not in result_names:
            fail(f"missing required scenario {scenario!r}")

    if args.require_full_suite:
        canonical_names = set(manifest_map)
        if set(selected_scenarios) != canonical_names:
            fail(
                f"full suite mismatch: expected {sorted(canonical_names)}, got {sorted(selected_scenarios)}"
            )

    coverage = payload.get("coverage")
    if not isinstance(coverage, dict):
        fail("coverage must be an object")
    manifest_names = set(manifest_map)
    validate_coverage_layers(
        require_list(coverage, "benchmarked_layers"),
        manifest_names,
        "benchmarked_layers",
    )
    validate_coverage_layers(
        require_list(coverage, "verified_only_layers"),
        manifest_names,
        "verified_only_layers",
    )

    workload = payload.get("workload")
    if not isinstance(workload, dict):
        fail("workload must be an object")
    required_workload_keys = [
        "ai_burst_bytes",
        "scrollback_flood_bytes",
        "render_seed_bytes",
        "delta_batches",
        "session_cycles",
        "ui_batch_repetitions",
        "settings_rounds",
        "shell_rounds",
        "font_passes",
        "surface_rounds",
    ]
    for key in required_workload_keys:
        if key not in workload:
            fail(f"workload missing key {key!r}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
