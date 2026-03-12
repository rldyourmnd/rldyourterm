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
