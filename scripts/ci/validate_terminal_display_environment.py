#!/usr/bin/env python3
from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]


def build_command(
    report: pathlib.Path,
    require_session_type: str | None,
    require_display_server_hint: str | None,
    require_monitor_cadence: bool,
    require_monitor_scale_factor: bool,
) -> list[str]:
    command = [
        "cargo",
        "run",
        "-q",
        "--locked",
        "-p",
        "rldyourterm-terminal-benchmark",
        "--",
        "environment",
        "validate",
        "--report",
        str(report),
    ]
    if require_session_type is not None:
        command.extend(["--require-session-type", require_session_type])
    if require_display_server_hint is not None:
        command.extend(["--require-display-server-hint", require_display_server_hint])
    if require_monitor_cadence:
        command.append("--require-monitor-cadence")
    if require_monitor_scale_factor:
        command.append("--require-monitor-scale-factor")
    return command


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("--require-session-type")
    parser.add_argument("--require-display-server-hint")
    parser.add_argument("--require-monitor-cadence", action="store_true")
    parser.add_argument("--require-monitor-scale-factor", action="store_true")
    args = parser.parse_args()

    subprocess.run(
        build_command(
            args.report,
            args.require_session_type,
            args.require_display_server_hint,
            args.require_monitor_cadence,
            args.require_monitor_scale_factor,
        ),
        check=True,
        cwd=REPO_ROOT,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
