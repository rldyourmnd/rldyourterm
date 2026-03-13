#!/usr/bin/env python3
from __future__ import annotations

import pathlib
import tempfile
import unittest
from unittest import mock

try:
    from scripts.ci import validate_terminal_benchmark_report
except ModuleNotFoundError:
    import validate_terminal_benchmark_report


class ValidateTerminalBenchmarkReportWrapperTests(unittest.TestCase):
    def test_wrapper_forwards_full_suite_validation(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            report_path = pathlib.Path(temp_dir) / "report.json"
            report_path.write_text("{}", encoding="utf-8")
            argv = [
                "validate_terminal_benchmark_report.py",
                str(report_path),
                "--require-full-suite",
            ]
            with mock.patch("subprocess.run") as run:
                with mock.patch("sys.argv", argv):
                    self.assertEqual(validate_terminal_benchmark_report.main(), 0)

            run.assert_called_once_with(
                [
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
                    str(report_path),
                    "--require-full-suite",
                ],
                check=True,
                cwd=validate_terminal_benchmark_report.REPO_ROOT,
            )

    def test_wrapper_forwards_required_scenarios(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            report_path = pathlib.Path(temp_dir) / "report.json"
            report_path.write_text("{}", encoding="utf-8")
            argv = [
                "validate_terminal_benchmark_report.py",
                str(report_path),
                "--require-scenario",
                "core-ingest-burst",
                "--require-scenario",
                "ui-command-cycle",
            ]
            with mock.patch("subprocess.run") as run:
                with mock.patch("sys.argv", argv):
                    self.assertEqual(validate_terminal_benchmark_report.main(), 0)

            run.assert_called_once_with(
                [
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
                    str(report_path),
                    "--require-scenario",
                    "core-ingest-burst",
                    "--require-scenario",
                    "ui-command-cycle",
                ],
                check=True,
                cwd=validate_terminal_benchmark_report.REPO_ROOT,
            )


if __name__ == "__main__":
    unittest.main()
