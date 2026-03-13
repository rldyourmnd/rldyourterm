#!/usr/bin/env python3
from __future__ import annotations

from typing import Any

def require_suite_manifest(payload: dict[str, Any]) -> dict[str, dict[str, Any]]:
    manifest = payload.get("suite_manifest")
    if not isinstance(manifest, dict):
        raise ValueError("suite_manifest must be an object")

    schema_version = manifest.get("schema_version")
    if not isinstance(schema_version, int) or schema_version <= 0:
        raise ValueError("suite_manifest.schema_version must be a positive integer")

    scenarios = manifest.get("scenarios")
    if not isinstance(scenarios, list) or not scenarios:
        raise ValueError("suite_manifest.scenarios must be a non-empty list")

    scenario_map: dict[str, dict[str, Any]] = {}
    for entry in scenarios:
        if not isinstance(entry, dict):
            raise ValueError("suite_manifest.scenarios entries must be objects")
        scenario = entry.get("scenario")
        if not isinstance(scenario, str) or not scenario:
            raise ValueError("suite_manifest scenario name must be a non-empty string")
        if scenario in scenario_map:
            raise ValueError(f"suite_manifest contains duplicate scenario {scenario!r}")
        for key in ("layer", "benchmark_kind", "description", "primary_unit_label"):
            value = entry.get(key)
            if not isinstance(value, str) or not value:
                raise ValueError(f"suite_manifest scenario {scenario!r} field {key} must be a non-empty string")
        backend = entry.get("backend")
        if backend is not None and (not isinstance(backend, str) or not backend):
            raise ValueError(f"suite_manifest scenario {scenario!r} backend must be null or a non-empty string")
        controlled_monitor_cadence = entry.get("controlled_monitor_cadence")
        if not isinstance(controlled_monitor_cadence, bool):
            raise ValueError(
                f"suite_manifest scenario {scenario!r} controlled_monitor_cadence must be a bool"
            )
        scenario_map[scenario] = entry

    return scenario_map


def _require_coverage_layers(
    coverage: dict[str, Any],
    label: str,
    manifest_names: set[str],
) -> dict[str, dict[str, Any]]:
    entries = coverage.get(label)
    if not isinstance(entries, list):
        raise ValueError(f"suite_manifest.coverage.{label} must be a list")

    layers: dict[str, dict[str, Any]] = {}
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError(f"suite_manifest.coverage.{label} entries must be objects")
        layer = entry.get("layer")
        if not isinstance(layer, str) or not layer:
            raise ValueError(f"suite_manifest.coverage.{label}.layer must be a non-empty string")
        if layer in layers:
            raise ValueError(f"suite_manifest.coverage.{label} contains duplicate layer {layer!r}")

        benchmark_scenarios = entry.get("benchmark_scenarios")
        validation_commands = entry.get("validation_commands")
        notes = entry.get("notes")
        if not isinstance(benchmark_scenarios, list):
            raise ValueError(
                f"suite_manifest.coverage.{label} entry {layer!r} benchmark_scenarios must be a list"
            )
        if len(benchmark_scenarios) != len(set(benchmark_scenarios)):
            raise ValueError(
                f"suite_manifest.coverage.{label} entry {layer!r} benchmark_scenarios must be unique"
            )
        for scenario in benchmark_scenarios:
            if not isinstance(scenario, str) or not scenario:
                raise ValueError(
                    f"suite_manifest.coverage.{label} entry {layer!r} benchmark_scenarios must contain non-empty strings"
                )
            if scenario not in manifest_names:
                raise ValueError(
                    f"suite_manifest.coverage.{label} entry {layer!r} references unknown scenario {scenario!r}"
                )
        if not isinstance(validation_commands, list) or not validation_commands:
            raise ValueError(
                f"suite_manifest.coverage.{label} entry {layer!r} validation_commands must be a non-empty list"
            )
        if any(not isinstance(command, str) or not command for command in validation_commands):
            raise ValueError(
                f"suite_manifest.coverage.{label} entry {layer!r} validation_commands must contain non-empty strings"
            )
        if len(validation_commands) != len(set(validation_commands)):
            raise ValueError(
                f"suite_manifest.coverage.{label} entry {layer!r} validation_commands must be unique"
            )
        if not isinstance(notes, str) or not notes:
            raise ValueError(f"suite_manifest.coverage.{label} entry {layer!r} notes must be a non-empty string")

        layers[layer] = {
            "benchmark_scenarios": set(benchmark_scenarios),
            "validation_commands": validation_commands,
            "notes": notes,
        }

    return layers


def require_headless_manifest_coverage(payload: dict[str, Any]) -> dict[str, dict[str, dict[str, Any]]]:
    if payload.get("suite") != "canonical-headless":
        raise ValueError("headless coverage helpers require canonical-headless suite")

    manifest = payload.get("suite_manifest")
    if not isinstance(manifest, dict):
        raise ValueError("suite_manifest must be an object")

    coverage = manifest.get("coverage")
    if not isinstance(coverage, dict):
        raise ValueError("suite_manifest.coverage must be an object for canonical-headless suite")

    manifest_names = set(require_suite_manifest(payload))
    return {
        "benchmarked_layers": _require_coverage_layers(
            coverage,
            "benchmarked_layers",
            manifest_names,
        ),
        "verified_only_layers": _require_coverage_layers(
            coverage,
            "verified_only_layers",
            manifest_names,
        ),
    }


def require_suite_manifest_names(payload: dict[str, Any]) -> set[str]:
    return set(require_suite_manifest(payload))

def controlled_display_cpu_scenarios(payload: dict[str, Any]) -> set[str]:
    scenarios = require_suite_manifest(payload)
    controlled = {
        name
        for name, entry in scenarios.items()
        if entry.get("backend") == "cpu" and entry.get("controlled_monitor_cadence") is True
    }
    if payload.get("suite") == "live-display" and not controlled:
        raise ValueError("live-display suite_manifest must mark at least one controlled CPU scenario")
    return controlled
