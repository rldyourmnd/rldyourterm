#!/usr/bin/env python3
from __future__ import annotations

import pathlib
import tempfile
import unittest
from unittest import mock

try:
    from scripts.ci import validate_terminal_benchmark_thresholds
except ModuleNotFoundError:
    import validate_terminal_benchmark_thresholds


class ValidateTerminalBenchmarkThresholdsWrapperTests(unittest.TestCase):
    def test_wrapper_forwards_enforced_threshold_validation(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            report_path = pathlib.Path(temp_dir) / "report.json"
            baseline_path = pathlib.Path(temp_dir) / "baseline.json"
            report_path.write_text("{}", encoding="utf-8")
            baseline_path.write_text("{}", encoding="utf-8")

            argv = [
                "validate_terminal_benchmark_thresholds.py",
                str(report_path),
                str(baseline_path),
            ]
            with mock.patch("subprocess.run") as run:
                with mock.patch("sys.argv", argv):
                    self.assertEqual(validate_terminal_benchmark_thresholds.main(), 0)

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
                    "threshold",
                    "validate",
                    "--report",
                    str(report_path),
                    "--baseline",
                    str(baseline_path),
                ],
                check=True,
                cwd=validate_terminal_benchmark_thresholds.REPO_ROOT,
            )

    def test_wrapper_forwards_allow_advisory_flag(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            report_path = pathlib.Path(temp_dir) / "report.json"
            baseline_path = pathlib.Path(temp_dir) / "baseline.json"
            report_path.write_text("{}", encoding="utf-8")
            baseline_path.write_text("{}", encoding="utf-8")

            argv = [
                "validate_terminal_benchmark_thresholds.py",
                str(report_path),
                str(baseline_path),
                "--allow-advisory",
            ]
            with mock.patch("subprocess.run") as run:
                with mock.patch("sys.argv", argv):
                    self.assertEqual(validate_terminal_benchmark_thresholds.main(), 0)

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
                    "threshold",
                    "validate",
                    "--report",
                    str(report_path),
                    "--baseline",
                    str(baseline_path),
                    "--allow-advisory",
                ],
                check=True,
                cwd=validate_terminal_benchmark_thresholds.REPO_ROOT,
            )


if __name__ == "__main__":
    unittest.main()
