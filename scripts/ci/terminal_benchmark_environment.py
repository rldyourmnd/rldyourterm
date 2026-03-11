#!/usr/bin/env python3
from __future__ import annotations

from typing import Any


PORTABLE_HEADLESS_SCOPE = "portable-headless"
LOCAL_DISPLAY_SCOPE = "local-display-session"
CONTROLLED_DISPLAY_SCOPE = "controlled-display-session"


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

    cpu_results = [entry for entry in results if isinstance(entry, dict) and entry.get("backend") == "cpu"]
    if not cpu_results:
        return LOCAL_DISPLAY_SCOPE

    for entry in cpu_results:
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

