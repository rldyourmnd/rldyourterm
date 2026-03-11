#!/usr/bin/env python3
import argparse
import json
import pathlib
import sys
from datetime import datetime, timezone

DEFAULTS_BY_SUITE = {
    "canonical-headless": {
        "comparison_mode": "enforced",
        "defaults": {
            "max_mean_nanos_ratio": 2.5,
            "max_p95_nanos_ratio": 3.0,
            "min_primary_units_per_second_ratio": 0.40,
            "min_bytes_per_second_ratio": 0.40,
        },
        "environment_scope": "portable-headless",
    },
    "live-display": {
        "comparison_mode": "advisory",
        "defaults": {
            "max_mean_nanos_ratio": 3.0,
            "max_p95_nanos_ratio": 3.5,
            "min_primary_units_per_second_ratio": 0.35,
        },
        "environment_scope": "local-display-session",
    },
}


def fail(message: str) -> None:
    raise SystemExit(f"benchmark baseline refresh failed: {message}")


def load_report(path: pathlib.Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--comparison-mode", choices=["enforced", "advisory"])
    parser.add_argument("--environment-scope")
    parser.add_argument("--notes")
    args = parser.parse_args()

    report = load_report(args.report)
    suite = report.get("suite")
    if suite not in DEFAULTS_BY_SUITE:
        fail(f"unsupported suite {suite!r}")
    defaults = DEFAULTS_BY_SUITE[suite]

    results = report.get("results")
    if not isinstance(results, list) or not results:
        fail("report.results must be a non-empty list")

    scenarios = {}
    for entry in results:
        if not isinstance(entry, dict):
            fail("report.results entries must be objects")
        scenario = entry.get("scenario")
        if not isinstance(scenario, str) or not scenario:
            fail("report result scenario must be a non-empty string")
        stats = entry.get("stats")
        if not isinstance(stats, dict):
            fail(f"report result {scenario!r} must contain stats")
        baseline_metrics = {
            "mean_nanos": stats.get("mean_nanos"),
            "p95_nanos": stats.get("p95_nanos"),
            "primary_units_per_second": entry.get("primary_units_per_second"),
        }
        bytes_per_second = entry.get("bytes_per_second")
        if isinstance(bytes_per_second, (int, float)) and float(bytes_per_second) > 0:
            baseline_metrics["bytes_per_second"] = bytes_per_second
        scenarios[scenario] = {
            "baseline_metrics": baseline_metrics,
            "thresholds": {},
        }

    payload = {
        "baseline_tool": "terminal-benchmark-thresholds",
        "benchmark_tool": report.get("benchmark_tool"),
        "suite": suite,
        "scale": report.get("scale"),
        "comparison_mode": args.comparison_mode or defaults["comparison_mode"],
        "environment_scope": args.environment_scope or defaults["environment_scope"],
        "generated_at_utc": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
        "scenario_selection": report.get("scenario_selection"),
        "defaults": defaults["defaults"],
        "notes": args.notes
        or (
            "Generated from canonical full benchmark report. Update only after intentional performance-baseline review."
            if suite == "canonical-headless"
            else "Generated from local live-display benchmark report. Advisory only unless calibrated for a controlled display environment."
        ),
        "scenarios": scenarios,
    }

    if args.output.parent:
        args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"benchmark baseline refreshed: {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
