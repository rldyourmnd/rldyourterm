#!/usr/bin/env python3
import argparse
import json
import pathlib
import sys

try:
    from scripts.ci.terminal_benchmark_manifest import controlled_display_cpu_scenarios
except ModuleNotFoundError:
    from terminal_benchmark_manifest import controlled_display_cpu_scenarios


def fail(message: str) -> None:
    raise SystemExit(f"live display environment validation failed: {message}")


def load_json(path: pathlib.Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("--require-session-type")
    parser.add_argument("--require-display-server-hint")
    parser.add_argument("--require-monitor-cadence", action="store_true")
    parser.add_argument("--require-monitor-scale-factor", action="store_true")
    args = parser.parse_args()

    payload = load_json(args.report)

    if payload.get("benchmark_tool") != "terminal-benchmark":
        fail("benchmark_tool must be 'terminal-benchmark'")
    if payload.get("suite") != "live-display":
        fail("suite must be 'live-display'")

    environment = payload.get("environment")
    if not isinstance(environment, dict):
        fail("environment must be an object")

    session_type = environment.get("session_type")
    display_server_hint = environment.get("display_server_hint")

    if args.require_session_type is not None and session_type != args.require_session_type:
        fail(
            f"environment.session_type must be {args.require_session_type!r}, got {session_type!r}"
        )
    if (
        args.require_display_server_hint is not None
        and display_server_hint != args.require_display_server_hint
    ):
        fail(
            "environment.display_server_hint must be "
            f"{args.require_display_server_hint!r}, got {display_server_hint!r}"
        )

    results = payload.get("results")
    if not isinstance(results, list) or not results:
        fail("results must be a non-empty list")
    try:
        controlled_cpu_scenarios = controlled_display_cpu_scenarios(payload)
    except ValueError as exc:
        fail(str(exc))

    scenario_map = {}
    for entry in results:
        if not isinstance(entry, dict):
            fail("results entries must be objects")
        scenario = entry.get("scenario")
        if not isinstance(scenario, str) or not scenario:
            fail("scenario must be a non-empty string")
        scenario_map[scenario] = entry

    if args.require_monitor_cadence:
        available = sorted(controlled_cpu_scenarios & set(scenario_map))
        if not available:
            fail("report does not contain any CPU monitor-cadence scenarios")
        for scenario in available:
            entry = scenario_map[scenario]
            if entry.get("pacing_mode") != "monitor-cadence":
                fail(
                    f"scenario {scenario!r} must use monitor-cadence, got {entry.get('pacing_mode')!r}"
                )
            refresh = entry.get("monitor_refresh_rate_millihz")
            if not isinstance(refresh, int) or refresh <= 0:
                fail(
                    f"scenario {scenario!r} must expose a positive monitor_refresh_rate_millihz"
                )

    if args.require_monitor_scale_factor:
        available = sorted(controlled_cpu_scenarios & set(scenario_map))
        if not available:
            fail("report does not contain any CPU monitor scenarios")
        for scenario in available:
            entry = scenario_map[scenario]
            scale_factor = entry.get("monitor_scale_factor")
            if not isinstance(scale_factor, (int, float)) or scale_factor <= 0:
                fail(
                    f"scenario {scenario!r} must expose a positive monitor_scale_factor"
                )

    print(f"live display environment validation ok: {args.report}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
