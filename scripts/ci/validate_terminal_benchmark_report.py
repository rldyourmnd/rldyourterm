#!/usr/bin/env python3
from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]


def build_command(
    report: pathlib.Path,
    required_scenarios: list[str],
    require_full_suite: bool,
) -> list[str]:
    command = [
        "cargo",
        "run",
        "-q",
        "--locked",
        "-p",
        "rldyourterm-terminal-benchmark",
        "--",
        "validate",
        "--suite",
        "canonical-headless",
        "--report",
        str(report),
    ]
    for scenario in required_scenarios:
        command.extend(["--require-scenario", scenario])
    if require_full_suite:
        command.append("--require-full-suite")
    return command


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("--require-scenario", action="append", default=[])
    parser.add_argument("--require-full-suite", action="store_true")
    args = parser.parse_args()

    subprocess.run(
        build_command(args.report, args.require_scenario, args.require_full_suite),
        check=True,
        cwd=REPO_ROOT,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
