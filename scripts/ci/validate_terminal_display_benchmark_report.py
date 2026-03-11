#!/usr/bin/env python3
import argparse
import json
import pathlib
import sys

EXPECTED_TOOL = "terminal-benchmark"
EXPECTED_SUITE = "live-display"
LIVE_DISPLAY_SCENARIOS = {
    "startup-first-frame-gpu": {
        "layer": "features/render-gpu",
        "kind": "display-startup",
        "backend": "gpu",
    },
    "startup-first-frame-cpu": {
        "layer": "features/render-cpu",
        "kind": "display-startup",
        "backend": "cpu",
    },
    "steady-redraw-gpu": {
        "layer": "features/render-gpu",
        "kind": "display-frame",
        "backend": "gpu",
    },
    "steady-redraw-cpu": {
        "layer": "features/render-cpu",
        "kind": "display-frame",
        "backend": "cpu",
    },
    "resize-cycle-gpu": {
        "layer": "features/render-gpu",
        "kind": "display-resize",
        "backend": "gpu",
    },
    "resize-cycle-cpu": {
        "layer": "features/render-cpu",
        "kind": "display-resize",
        "backend": "cpu",
    },
}
REQUIRED_STATS_KEYS = {
    "min_nanos",
    "median_nanos",
    "p95_nanos",
    "max_nanos",
    "mean_nanos",
    "total_nanos",
}
REQUIRED_CPU_PHASE_KEYS = {"buffer_acquire", "raster", "present"}
REQUIRED_CPU_BUFFER_AGE_KEYS = {"age_0", "age_1", "age_2", "age_3_plus"}


def fail(message: str) -> None:
    raise SystemExit(f"live display benchmark report validation failed: {message}")


def require_list(payload: dict, key: str) -> list:
    value = payload.get(key)
    if not isinstance(value, list):
        fail(f"{key} must be a list")
    return value


def require_int(payload: dict, key: str) -> int:
    value = payload.get(key)
    if not isinstance(value, int) or value < 0:
        fail(f"{key} must be a non-negative integer")
    return value


def validate_stats(payload: dict, scenario: str) -> None:
    if not isinstance(payload, dict):
        fail(f"scenario {scenario!r} stats must be an object")
    keys = set(payload)
    if keys != REQUIRED_STATS_KEYS:
        fail(
            f"scenario {scenario!r} stats keys mismatch: expected {sorted(REQUIRED_STATS_KEYS)}, got {sorted(keys)}"
        )
    for key, value in payload.items():
        if not isinstance(value, int) or value < 0:
            fail(f"scenario {scenario!r} stats.{key} must be a non-negative integer")


def validate_cpu_phase_stats(payload: dict, scenario: str) -> None:
    if not isinstance(payload, dict):
        fail(f"scenario {scenario!r} cpu_phase_stats must be an object")
    keys = set(payload)
    if keys != REQUIRED_CPU_PHASE_KEYS:
        fail(
            f"scenario {scenario!r} cpu_phase_stats keys mismatch: expected {sorted(REQUIRED_CPU_PHASE_KEYS)}, got {sorted(keys)}"
        )
    for phase_name, stats in payload.items():
        validate_stats(stats, f"{scenario}.{phase_name}")


def validate_cpu_buffer_age_counts(payload: dict, scenario: str) -> None:
    if not isinstance(payload, dict):
        fail(f"scenario {scenario!r} cpu_buffer_age_counts must be an object")
    keys = set(payload)
    if keys != REQUIRED_CPU_BUFFER_AGE_KEYS:
        fail(
            f"scenario {scenario!r} cpu_buffer_age_counts keys mismatch: expected {sorted(REQUIRED_CPU_BUFFER_AGE_KEYS)}, got {sorted(keys)}"
        )
    for key, value in payload.items():
        if not isinstance(value, int) or value < 0:
            fail(
                f"scenario {scenario!r} cpu_buffer_age_counts.{key} must be a non-negative integer"
            )


def validate_optional_string(payload: dict, key: str, owner: str) -> None:
    value = payload.get(key)
    if value is not None and (not isinstance(value, str) or not value):
        fail(f"{owner}.{key} must be null or a non-empty string")


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

    selected_scenarios = require_list(payload, "selected_scenarios")
    if not selected_scenarios:
        fail("selected_scenarios must not be empty")
    if any(not isinstance(name, str) or not name for name in selected_scenarios):
        fail("selected_scenarios entries must be non-empty strings")
    if len(selected_scenarios) != len(set(selected_scenarios)):
        fail("selected_scenarios must be unique")

    environment = payload.get("environment")
    if not isinstance(environment, dict):
        fail("environment must be an object")
    expected_environment = {
        "kind": "live-display",
        "window_runtime": "winit",
        "gpu_runtime": "wgpu",
        "cpu_present_runtime": "softbuffer",
    }
    for key, expected in expected_environment.items():
        if environment.get(key) != expected:
            fail(f"environment.{key} must be {expected!r}")
    if not isinstance(environment.get("platform_dependent"), bool):
        fail("environment.platform_dependent must be a bool")
    if environment.get("platform_dependent") is not True:
        fail("environment.platform_dependent must be true")
    validate_optional_string(environment, "session_type", "environment")
    display_server_hint = environment.get("display_server_hint")
    if not isinstance(display_server_hint, str) or not display_server_hint:
        fail("environment.display_server_hint must be a non-empty string")

    workload = payload.get("workload")
    if not isinstance(workload, dict):
        fail("workload must be an object")
    for key in [
        "startup_runs_per_iteration",
        "steady_frames_per_iteration",
        "resize_cycles_per_iteration",
        "requested_width",
        "requested_height",
        "resize_targets",
    ]:
        require_int(workload, key)

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
        expected = LIVE_DISPLAY_SCENARIOS.get(scenario)
        if expected is None:
            fail(f"unexpected scenario {scenario!r}")
        if entry.get("layer") != expected["layer"]:
            fail(f"scenario {scenario!r} has unexpected layer {entry.get('layer')!r}")
        if entry.get("benchmark_kind") != expected["kind"]:
            fail(
                f"scenario {scenario!r} has unexpected benchmark_kind {entry.get('benchmark_kind')!r}"
            )
        if entry.get("backend") != expected["backend"]:
            fail(f"scenario {scenario!r} has unexpected backend {entry.get('backend')!r}")
        if not isinstance(entry.get("description"), str) or not entry["description"]:
            fail(f"scenario {scenario!r} must include a non-empty description")
        if not isinstance(entry.get("primary_unit_label"), str) or not entry["primary_unit_label"]:
            fail(f"scenario {scenario!r} must include primary_unit_label")
        if not isinstance(entry.get("primary_units_per_iteration"), int) or entry["primary_units_per_iteration"] < 0:
            fail(f"scenario {scenario!r} primary_units_per_iteration must be a non-negative integer")
        if not isinstance(entry.get("primary_units_per_second"), (int, float)):
            fail(f"scenario {scenario!r} primary_units_per_second must be numeric")
        if not isinstance(entry.get("pacing_mode"), str) or not entry["pacing_mode"]:
            fail(f"scenario {scenario!r} pacing_mode must be a non-empty string")
        monitor_refresh_rate_millihz = entry.get("monitor_refresh_rate_millihz")
        if monitor_refresh_rate_millihz is not None and (
            not isinstance(monitor_refresh_rate_millihz, int) or monitor_refresh_rate_millihz < 0
        ):
            fail(
                f"scenario {scenario!r} monitor_refresh_rate_millihz must be null or a non-negative integer"
            )
        validate_optional_string(entry, "monitor_name", f"scenario {scenario!r}")
        monitor_scale_factor = entry.get("monitor_scale_factor")
        if monitor_scale_factor is not None and (
            not isinstance(monitor_scale_factor, (int, float)) or monitor_scale_factor <= 0
        ):
            fail(
                f"scenario {scenario!r} monitor_scale_factor must be null or a positive number"
            )
        display_phase_stats = entry.get("display_phase_stats")
        if not isinstance(display_phase_stats, dict):
            fail(f"scenario {scenario!r} display_phase_stats must be an object")
        validate_stats(
            display_phase_stats.get("redraw_dispatch"),
            f"scenario {scenario!r} display_phase_stats.redraw_dispatch",
        )
        frame_gap = display_phase_stats.get("frame_gap")
        if frame_gap is not None:
            validate_stats(
                frame_gap,
                f"scenario {scenario!r} display_phase_stats.frame_gap",
            )
        if not isinstance(entry.get("redraws_per_iteration"), int) or entry["redraws_per_iteration"] < 0:
            fail(f"scenario {scenario!r} redraws_per_iteration must be a non-negative integer")
        if not isinstance(entry.get("resize_cycles_per_iteration"), int) or entry["resize_cycles_per_iteration"] < 0:
            fail(f"scenario {scenario!r} resize_cycles_per_iteration must be a non-negative integer")
        notes = entry.get("notes")
        if not isinstance(notes, list) or any(not isinstance(note, str) for note in notes):
            fail(f"scenario {scenario!r} notes must be a list of strings")
        validate_stats(entry.get("stats"), scenario)
        cpu_phase_stats = entry.get("cpu_phase_stats")
        cpu_buffer_age_counts = entry.get("cpu_buffer_age_counts")
        if expected["backend"] == "cpu":
            validate_cpu_phase_stats(cpu_phase_stats, scenario)
            validate_cpu_buffer_age_counts(cpu_buffer_age_counts, scenario)
        elif cpu_phase_stats is not None:
            fail(f"scenario {scenario!r} cpu_phase_stats must be null for non-cpu backends")
        elif cpu_buffer_age_counts is not None:
            fail(
                f"scenario {scenario!r} cpu_buffer_age_counts must be null for non-cpu backends"
            )
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
        expected_names = set(LIVE_DISPLAY_SCENARIOS)
        if set(selected_scenarios) != expected_names:
            fail(
                f"full suite mismatch: expected {sorted(expected_names)}, got {sorted(selected_scenarios)}"
            )

    return 0


if __name__ == "__main__":
    sys.exit(main())
