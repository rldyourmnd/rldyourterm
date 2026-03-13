#!/usr/bin/env python3
from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]


def build_command(
    report: pathlib.Path,
    benchmark_report: pathlib.Path,
    baseline: pathlib.Path,
    comparison_mode: str,
    required_session_type: str | None,
    required_display_server_hint: str | None,
    runner_readiness_report: pathlib.Path | None,
) -> list[str]:
    command = [
        "cargo",
        "run",
        "-q",
        "--locked",
        "-p",
        "rldyourterm-terminal-benchmark",
        "--",
        "governance",
        "calibration",
        "validate",
        "--report",
        str(report),
        "--benchmark-report",
        str(benchmark_report),
        "--baseline",
        str(baseline),
        "--comparison-mode",
        comparison_mode,
    ]
    if required_session_type is not None:
        command.extend(["--required-session-type", required_session_type])
    if required_display_server_hint is not None:
        command.extend(
            ["--required-display-server-hint", required_display_server_hint]
        )
    if runner_readiness_report is not None:
        command.extend(["--runner-readiness-report", str(runner_readiness_report)])
    return command


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("--benchmark-report", type=pathlib.Path, required=True)
    parser.add_argument("--baseline", type=pathlib.Path, required=True)
    parser.add_argument(
        "--comparison-mode",
        choices=["advisory", "enforced"],
        required=True,
    )
    parser.add_argument("--required-session-type")
    parser.add_argument("--required-display-server-hint")
    parser.add_argument("--runner-readiness-report", type=pathlib.Path)
    args = parser.parse_args()

    subprocess.run(
        build_command(
            args.report,
            args.benchmark_report,
            args.baseline,
            args.comparison_mode,
            args.required_session_type,
            args.required_display_server_hint,
            args.runner_readiness_report,
        ),
        check=True,
        cwd=REPO_ROOT,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
