#!/usr/bin/env python3
from __future__ import annotations

import pathlib
import tempfile
import unittest
from unittest import mock

try:
    from scripts.ci import (
        validate_terminal_display_calibration_report,
        validate_terminal_display_runner_readiness_report,
    )
except ModuleNotFoundError:
    import validate_terminal_display_calibration_report
    import validate_terminal_display_runner_readiness_report


class ValidateTerminalDisplayRunnerReadinessWrapperTests(unittest.TestCase):
    def test_wrapper_forwards_require_pass(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            report_path = pathlib.Path(temp_dir) / "runner-readiness.json"
            report_path.write_text("{}", encoding="utf-8")
            argv = [
                "validate_terminal_display_runner_readiness_report.py",
                str(report_path),
                "--require-pass",
            ]
            with mock.patch("subprocess.run") as run:
                with mock.patch("sys.argv", argv):
                    self.assertEqual(
                        validate_terminal_display_runner_readiness_report.main(), 0
                    )

            run.assert_called_once_with(
                [
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
                    str(report_path),
                    "--require-pass",
                ],
                check=True,
                cwd=validate_terminal_display_runner_readiness_report.REPO_ROOT,
            )


class ValidateTerminalDisplayCalibrationWrapperTests(unittest.TestCase):
    def test_wrapper_forwards_calibration_contract_arguments(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            report_path = pathlib.Path(temp_dir) / "calibration.json"
            benchmark_report_path = pathlib.Path(temp_dir) / "benchmark.json"
            baseline_path = pathlib.Path(temp_dir) / "baseline.json"
            readiness_path = pathlib.Path(temp_dir) / "readiness.json"
            for path in (
                report_path,
                benchmark_report_path,
                baseline_path,
                readiness_path,
            ):
                path.write_text("{}", encoding="utf-8")

            argv = [
                "validate_terminal_display_calibration_report.py",
                str(report_path),
                "--benchmark-report",
                str(benchmark_report_path),
                "--baseline",
                str(baseline_path),
                "--comparison-mode",
                "advisory",
                "--required-session-type",
                "wayland",
                "--required-display-server-hint",
                "wayland",
                "--runner-readiness-report",
                str(readiness_path),
            ]
            with mock.patch("subprocess.run") as run:
                with mock.patch("sys.argv", argv):
                    self.assertEqual(
                        validate_terminal_display_calibration_report.main(), 0
                    )

            run.assert_called_once_with(
                [
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
                    str(report_path),
                    "--benchmark-report",
                    str(benchmark_report_path),
                    "--baseline",
                    str(baseline_path),
                    "--comparison-mode",
                    "advisory",
                    "--required-session-type",
                    "wayland",
                    "--required-display-server-hint",
                    "wayland",
                    "--runner-readiness-report",
                    str(readiness_path),
                ],
                check=True,
                cwd=validate_terminal_display_calibration_report.REPO_ROOT,
            )


if __name__ == "__main__":
    unittest.main()
