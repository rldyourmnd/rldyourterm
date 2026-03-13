#!/usr/bin/env python3
from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]


def build_command(report: pathlib.Path, require_pass: bool) -> list[str]:
    command = [
        "cargo",
        "run",
        "-q",
        "--locked",
        "-p",
        "rldyourterm-terminal-benchmark",
        "--",
        "governance",
        "runner-readiness",
        "validate",
        "--report",
        str(report),
    ]
    if require_pass:
        command.append("--require-pass")
    return command


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("--require-pass", action="store_true")
    args = parser.parse_args()

    subprocess.run(
        build_command(args.report, args.require_pass),
        check=True,
        cwd=REPO_ROOT,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
