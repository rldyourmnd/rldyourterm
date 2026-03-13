#!/usr/bin/env python3
import argparse
import json
import pathlib
import subprocess
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]

DEFAULT_METRICS = {
    "max_mean_nanos_ratio": "mean_nanos",
    "max_p95_nanos_ratio": "p95_nanos",
    "min_primary_units_per_second_ratio": "primary_units_per_second",
}
OPTIONAL_METRICS = {
    "min_bytes_per_second_ratio": "bytes_per_second",
}


def fail(message: str) -> None:
    raise SystemExit(f"benchmark threshold validation failed: {message}")


def load_json(path: pathlib.Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def require_object(payload: dict, key: str) -> dict:
    value = payload.get(key)
    if not isinstance(value, dict):
        fail(f"{key} must be an object")
    return value


def require_metric(source: dict, key: str, *, label: str) -> float:
    value = source.get(key)
    if not isinstance(value, (int, float)):
        fail(f"{label}.{key} must be numeric")
    return float(value)


def compare_max_ratio(scenario: str, metric_name: str, current: float, baseline: float, ratio: float) -> None:
    if baseline <= 0:
        fail(f"scenario {scenario!r} baseline {metric_name} must be > 0")
    limit = baseline * ratio
    if current > limit:
        raise ValueError(
            f"scenario {scenario!r} {metric_name} regression: current={current:.6f} exceeds baseline={baseline:.6f} * ratio={ratio:.3f} (limit={limit:.6f})"
        )


def compare_min_ratio(scenario: str, metric_name: str, current: float, baseline: float, ratio: float) -> None:
    if baseline <= 0:
        fail(f"scenario {scenario!r} baseline {metric_name} must be > 0")
    floor = baseline * ratio
    if current < floor:
        raise ValueError(
            f"scenario {scenario!r} {metric_name} regression: current={current:.6f} is below baseline={baseline:.6f} * ratio={ratio:.3f} (floor={floor:.6f})"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("baseline", type=pathlib.Path)
    parser.add_argument("--allow-advisory", action="store_true")
    args = parser.parse_args()

    report = load_json(args.report)
    baseline = load_json(args.baseline)

    if baseline.get("baseline_tool") != "terminal-benchmark-thresholds":
        fail("baseline_tool must be 'terminal-benchmark-thresholds'")
    if report.get("benchmark_tool") != baseline.get("benchmark_tool"):
        fail("benchmark_tool mismatch between report and baseline")
    if report.get("suite") != baseline.get("suite"):
        fail("suite mismatch between report and baseline")
    if report.get("scale") != baseline.get("scale"):
        fail("scale mismatch between report and baseline")

    try:
        subprocess.run(
            [
                "cargo",
                "run",
                "-q",
                "--locked",
                "-p",
                "rldyourterm-terminal-benchmark",
                "--",
                "environment",
                "validate-baseline",
                "--report",
                str(args.report),
                "--baseline",
                str(args.baseline),
            ],
            check=True,
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as exc:
        detail = (exc.stderr or exc.stdout or "").strip()
        if detail:
            fail(detail)
        fail(f"environment contract validation failed with exit code {exc.returncode}")

    comparison_mode = baseline.get("comparison_mode")
    if comparison_mode not in {"enforced", "advisory"}:
        fail("comparison_mode must be 'enforced' or 'advisory'")
    if comparison_mode == "advisory" and not args.allow_advisory:
        fail("baseline comparison_mode is advisory; rerun with --allow-advisory to acknowledge environment-specific thresholds")

    advisory_violations = []

    defaults = require_object(baseline, "defaults")
    scenarios = require_object(baseline, "scenarios")

    report_results = report.get("results")
    if not isinstance(report_results, list) or not report_results:
        fail("report.results must be a non-empty list")
    result_map = {}
    for entry in report_results:
        if not isinstance(entry, dict):
            fail("report.results entries must be objects")
        scenario = entry.get("scenario")
        if not isinstance(scenario, str) or not scenario:
            fail("report result scenario must be a non-empty string")
        result_map[scenario] = entry

    baseline_names = set(scenarios)
    report_names = set(result_map)
    if baseline_names != report_names:
        fail(
            f"scenario set mismatch between report and baseline: baseline={sorted(baseline_names)} report={sorted(report_names)}"
        )

    for scenario, scenario_policy in scenarios.items():
        if not isinstance(scenario_policy, dict):
            fail(f"baseline scenario {scenario!r} policy must be an object")
        baseline_metrics = require_object(scenario_policy, "baseline_metrics")
        threshold_overrides = scenario_policy.get("thresholds")
        if threshold_overrides is None:
            threshold_overrides = {}
        if not isinstance(threshold_overrides, dict):
            fail(f"baseline scenario {scenario!r} thresholds must be an object")

        current_entry = result_map[scenario]
        current_stats = require_object(current_entry, "stats")

        for ratio_key, metric_name in DEFAULT_METRICS.items():
            ratio = threshold_overrides.get(ratio_key, defaults.get(ratio_key))
            if not isinstance(ratio, (int, float)) or ratio <= 0:
                fail(f"scenario {scenario!r} threshold {ratio_key} must be a positive number")
            baseline_value = require_metric(baseline_metrics, metric_name, label=f"baseline[{scenario}]")
            source = current_stats if metric_name.endswith("_nanos") else current_entry
            current_value = require_metric(source, metric_name, label=f"report[{scenario}]")
            try:
                if ratio_key.startswith("max_"):
                    compare_max_ratio(scenario, metric_name, current_value, baseline_value, float(ratio))
                else:
                    compare_min_ratio(scenario, metric_name, current_value, baseline_value, float(ratio))
            except ValueError as exc:
                if comparison_mode == "advisory":
                    advisory_violations.append(str(exc))
                else:
                    fail(str(exc))

        for ratio_key, metric_name in OPTIONAL_METRICS.items():
            ratio = threshold_overrides.get(ratio_key, defaults.get(ratio_key))
            if ratio is None:
                continue
            if not isinstance(ratio, (int, float)) or ratio <= 0:
                fail(f"scenario {scenario!r} threshold {ratio_key} must be a positive number")
            baseline_value = baseline_metrics.get(metric_name)
            current_value = current_entry.get(metric_name)
            if baseline_value is None or current_value is None:
                continue
            if not isinstance(baseline_value, (int, float)) or not isinstance(current_value, (int, float)):
                fail(f"scenario {scenario!r} optional metric {metric_name} must be numeric when present")
            if float(baseline_value) <= 0:
                continue
            try:
                compare_min_ratio(scenario, metric_name, float(current_value), float(baseline_value), float(ratio))
            except ValueError as exc:
                if comparison_mode == "advisory":
                    advisory_violations.append(str(exc))
                else:
                    fail(str(exc))

    mode_note = "advisory" if comparison_mode == "advisory" else "enforced"
    if advisory_violations:
        print(
            f"benchmark threshold validation advisory regressions ({len(advisory_violations)}): {args.report} vs {args.baseline}",
            file=sys.stderr,
        )
        for violation in advisory_violations:
            print(f"- {violation}", file=sys.stderr)
    print(f"benchmark threshold validation ok ({mode_note}): {args.report} vs {args.baseline}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
