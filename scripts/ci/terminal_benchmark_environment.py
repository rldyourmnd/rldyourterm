#!/usr/bin/env python3
from __future__ import annotations

import math
from typing import Any


PORTABLE_HEADLESS_SCOPE = "portable-headless"
LOCAL_DISPLAY_SCOPE = "local-display-session"
CONTROLLED_DISPLAY_SCOPE = "controlled-display-session"
CONTROLLED_DISPLAY_CPU_SCENARIOS = frozenset({"steady-redraw-cpu", "resize-cycle-cpu"})


def _is_positive_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and float(value) > 0


def infer_report_environment_scope(report: dict[str, Any]) -> str:
    suite = report.get("suite")
    if suite == "canonical-headless":
        return PORTABLE_HEADLESS_SCOPE
    if suite != "live-display":
        raise ValueError(f"unsupported benchmark suite {suite!r}")

    results = report.get("results")
    if not isinstance(results, list) or not results:
        raise ValueError("report.results must be a non-empty list")

    cpu_results = {
        entry.get("scenario"): entry
        for entry in results
        if isinstance(entry, dict)
        and entry.get("backend") == "cpu"
        and entry.get("scenario") in CONTROLLED_DISPLAY_CPU_SCENARIOS
    }
    if not cpu_results:
        return LOCAL_DISPLAY_SCOPE

    for entry in cpu_results.values():
        pacing_mode = entry.get("pacing_mode")
        refresh_rate_millihz = entry.get("monitor_refresh_rate_millihz")
        monitor_scale_factor = entry.get("monitor_scale_factor")
        if pacing_mode != "monitor-cadence":
            return LOCAL_DISPLAY_SCOPE
        if not _is_positive_number(refresh_rate_millihz):
            return LOCAL_DISPLAY_SCOPE
        if not _is_positive_number(monitor_scale_factor):
            return LOCAL_DISPLAY_SCOPE

    return CONTROLLED_DISPLAY_SCOPE


def extract_environment_requirements_for_baseline(report: dict[str, Any]) -> dict[str, Any] | None:
    if infer_report_environment_scope(report) != CONTROLLED_DISPLAY_SCOPE:
        return None

    environment = report.get("environment")
    if not isinstance(environment, dict):
        raise ValueError("report.environment must be an object")

    display_server_hint = environment.get("display_server_hint")
    if not isinstance(display_server_hint, str) or not display_server_hint:
        raise ValueError("report.environment.display_server_hint must be a non-empty string")

    requirements: dict[str, Any] = {
        "display_server_hint": display_server_hint,
        "cpu_scenarios": {},
    }

    session_type = environment.get("session_type")
    if isinstance(session_type, str) and session_type:
        requirements["session_type"] = session_type

    results = report.get("results")
    if not isinstance(results, list) or not results:
        raise ValueError("report.results must be a non-empty list")

    cpu_scenarios: dict[str, Any] = requirements["cpu_scenarios"]
    for entry in results:
        if not isinstance(entry, dict) or entry.get("backend") != "cpu":
            continue
        scenario = entry.get("scenario")
        if not isinstance(scenario, str) or not scenario:
            raise ValueError("cpu benchmark result scenario must be a non-empty string")
        if scenario not in CONTROLLED_DISPLAY_CPU_SCENARIOS:
            continue

        pacing_mode = entry.get("pacing_mode")
        refresh_rate_millihz = entry.get("monitor_refresh_rate_millihz")
        monitor_scale_factor = entry.get("monitor_scale_factor")
        if pacing_mode != "monitor-cadence":
            raise ValueError(f"cpu scenario {scenario!r} must use monitor-cadence for controlled calibration")
        if not _is_positive_number(refresh_rate_millihz):
            raise ValueError(
                f"cpu scenario {scenario!r} must expose a positive monitor_refresh_rate_millihz for controlled calibration"
            )
        if not _is_positive_number(monitor_scale_factor):
            raise ValueError(
                f"cpu scenario {scenario!r} must expose a positive monitor_scale_factor for controlled calibration"
            )

        cpu_scenarios[scenario] = {
            "pacing_mode": pacing_mode,
            "monitor_refresh_rate_millihz": int(refresh_rate_millihz),
            "monitor_scale_factor": float(monitor_scale_factor),
        }

    if not cpu_scenarios:
        raise ValueError("controlled calibration requires at least one cpu live-display scenario")

    return requirements


def validate_report_against_environment_requirements(
    report: dict[str, Any], requirements: dict[str, Any]
) -> None:
    environment = report.get("environment")
    if not isinstance(environment, dict):
        raise ValueError("report.environment must be an object")
    display_server_hint = requirements.get("display_server_hint")
    if not isinstance(display_server_hint, str) or not display_server_hint:
        raise ValueError("baseline environment_requirements.display_server_hint must be a non-empty string")
    report_display_server_hint = environment.get("display_server_hint")
    if report_display_server_hint != display_server_hint:
        raise ValueError(
            "display_server_hint mismatch between report and baseline requirements: "
            f"report={report_display_server_hint!r} baseline={display_server_hint!r}"
        )

    required_session_type = requirements.get("session_type")
    if required_session_type is not None:
        if not isinstance(required_session_type, str) or not required_session_type:
            raise ValueError("baseline environment_requirements.session_type must be a non-empty string when present")
        report_session_type = environment.get("session_type")
        if report_session_type != required_session_type:
            raise ValueError(
                "session_type mismatch between report and baseline requirements: "
                f"report={report_session_type!r} baseline={required_session_type!r}"
            )

    cpu_scenarios = requirements.get("cpu_scenarios")
    if cpu_scenarios is None:
        return
    if not isinstance(cpu_scenarios, dict) or not cpu_scenarios:
        raise ValueError("baseline environment_requirements.cpu_scenarios must be a non-empty object when present")

    results = report.get("results")
    if not isinstance(results, list) or not results:
        raise ValueError("report.results must be a non-empty list")
    result_map = {}
    for entry in results:
        if not isinstance(entry, dict):
            continue
        scenario = entry.get("scenario")
        if isinstance(scenario, str) and scenario:
            result_map[scenario] = entry

    for scenario, scenario_requirements in cpu_scenarios.items():
        if not isinstance(scenario_requirements, dict):
            raise ValueError(f"baseline environment_requirements cpu scenario {scenario!r} must be an object")
        entry = result_map.get(scenario)
        if not isinstance(entry, dict):
            raise ValueError(f"report is missing cpu scenario required by baseline: {scenario!r}")

        required_pacing_mode = scenario_requirements.get("pacing_mode")
        report_pacing_mode = entry.get("pacing_mode")
        if report_pacing_mode != required_pacing_mode:
            raise ValueError(
                f"scenario {scenario!r} pacing_mode mismatch: report={report_pacing_mode!r} baseline={required_pacing_mode!r}"
            )

        required_refresh = scenario_requirements.get("monitor_refresh_rate_millihz")
        if not _is_positive_number(required_refresh):
            raise ValueError(
                f"baseline environment_requirements cpu scenario {scenario!r} monitor_refresh_rate_millihz must be positive"
            )
        report_refresh = entry.get("monitor_refresh_rate_millihz")
        if int(report_refresh) != int(required_refresh):
            raise ValueError(
                f"scenario {scenario!r} monitor_refresh_rate_millihz mismatch: "
                f"report={report_refresh!r} baseline={required_refresh!r}"
            )

        required_scale = scenario_requirements.get("monitor_scale_factor")
        if not _is_positive_number(required_scale):
            raise ValueError(
                f"baseline environment_requirements cpu scenario {scenario!r} monitor_scale_factor must be positive"
            )
        report_scale = entry.get("monitor_scale_factor")
        if not _is_positive_number(report_scale) or not math.isclose(
            float(report_scale), float(required_scale), rel_tol=0.0, abs_tol=1e-9
        ):
            raise ValueError(
                f"scenario {scenario!r} monitor_scale_factor mismatch: report={report_scale!r} baseline={required_scale!r}"
            )
