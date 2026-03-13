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
    governance_mode: str,
    benchmark_baseline: pathlib.Path | None,
    live_display_mode: str | None,
    live_display_report: pathlib.Path | None,
    live_display_baseline: pathlib.Path | None,
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
        "system-suite",
        "validate",
        "--report",
        str(report),
        "--benchmark-report",
        str(benchmark_report),
        "--governance-mode",
        governance_mode,
    ]
    if benchmark_baseline is not None:
        command.extend(["--benchmark-baseline", str(benchmark_baseline)])
    if live_display_mode is not None:
        command.extend(["--live-display-mode", live_display_mode])
    if live_display_report is not None:
        command.extend(["--live-display-report", str(live_display_report)])
    if live_display_baseline is not None:
        command.extend(["--live-display-baseline", str(live_display_baseline)])
    return command


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("--benchmark-report", type=pathlib.Path, required=True)
    parser.add_argument("--governance-mode", choices=["ci", "release"], required=True)
    parser.add_argument("--benchmark-baseline", type=pathlib.Path)
    parser.add_argument(
        "--live-display-mode", choices=["smoke", "full", "controlled"]
    )
    parser.add_argument("--live-display-report", type=pathlib.Path)
    parser.add_argument("--live-display-baseline", type=pathlib.Path)
    args = parser.parse_args()

    subprocess.run(
        build_command(
            args.report,
            args.benchmark_report,
            args.governance_mode,
            args.benchmark_baseline,
            args.live_display_mode,
            args.live_display_report,
            args.live_display_baseline,
        ),
        check=True,
        cwd=REPO_ROOT,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
