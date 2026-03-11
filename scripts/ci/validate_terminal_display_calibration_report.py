#!/usr/bin/env python3
import argparse
import json
import pathlib
import subprocess
import sys

EXPECTED_TOOL = "terminal-display-calibration"
EXPECTED_STATUS = "pass"


def fail(message: str) -> None:
    raise SystemExit(f"terminal display calibration validation failed: {message}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("--benchmark-report", type=pathlib.Path, required=True)
    parser.add_argument("--baseline", type=pathlib.Path, required=True)
    parser.add_argument("--comparison-mode", choices=["advisory", "enforced"], required=True)
    parser.add_argument("--runner-readiness-report", type=pathlib.Path)
    args = parser.parse_args()

    with args.report.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)

    if payload.get("system_tool") != EXPECTED_TOOL:
        fail(f"system_tool must be {EXPECTED_TOOL!r}")
    if payload.get("status") != EXPECTED_STATUS:
        fail(f"status must be {EXPECTED_STATUS!r}")
    generated_at_utc = payload.get("generated_at_utc")
    if not isinstance(generated_at_utc, str) or not generated_at_utc:
        fail("generated_at_utc must be a non-empty string")
    if payload.get("benchmark_report") != str(args.benchmark_report):
        fail(
            f"benchmark_report must be {str(args.benchmark_report)!r}, got {payload.get('benchmark_report')!r}"
        )
    if payload.get("baseline") != str(args.baseline):
        fail(f"baseline must be {str(args.baseline)!r}, got {payload.get('baseline')!r}")
    if payload.get("comparison_mode") != args.comparison_mode:
        fail(
            f"comparison_mode must be {args.comparison_mode!r}, got {payload.get('comparison_mode')!r}"
        )
    expected_runner_readiness_report = (
        None if args.runner_readiness_report is None else str(args.runner_readiness_report)
    )
    if payload.get("runner_readiness_report") != expected_runner_readiness_report:
        fail(
            f"runner_readiness_report must be {expected_runner_readiness_report!r}, got {payload.get('runner_readiness_report')!r}"
        )

    if not args.benchmark_report.is_file():
        fail(f"benchmark report does not exist: {args.benchmark_report}")
    if not args.baseline.is_file():
        fail(f"baseline does not exist: {args.baseline}")
    if args.runner_readiness_report is not None and not args.runner_readiness_report.is_file():
        fail(f"runner readiness report does not exist: {args.runner_readiness_report}")

    subprocess.run(
        [
            sys.executable,
            "scripts/ci/validate_terminal_display_benchmark_report.py",
            str(args.benchmark_report),
            "--require-full-suite",
        ],
        check=True,
    )

    threshold_args = [
        sys.executable,
        "scripts/ci/validate_terminal_benchmark_thresholds.py",
        str(args.benchmark_report),
        str(args.baseline),
    ]
    if args.comparison_mode == "advisory":
        threshold_args.append("--allow-advisory")
    subprocess.run(threshold_args, check=True)
    if args.runner_readiness_report is not None:
        subprocess.run(
            [
                sys.executable,
                "scripts/ci/validate_terminal_display_runner_readiness_report.py",
                str(args.runner_readiness_report),
                "--require-pass",
            ],
            check=True,
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
