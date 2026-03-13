#!/usr/bin/env python3
from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]


def build_command(
    report: pathlib.Path,
    baseline: pathlib.Path,
    allow_advisory: bool,
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
        "threshold",
        "validate",
        "--report",
        str(report),
        "--baseline",
        str(baseline),
    ]
    if allow_advisory:
        command.append("--allow-advisory")
    return command


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("baseline", type=pathlib.Path)
    parser.add_argument("--allow-advisory", action="store_true")
    args = parser.parse_args()

    subprocess.run(
        build_command(args.report, args.baseline, args.allow_advisory),
        check=True,
        cwd=REPO_ROOT,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
