#!/usr/bin/env python3
from __future__ import annotations

import pathlib
import tempfile
import unittest
from unittest import mock

try:
    from scripts.ci import validate_terminal_system_suite_report
except ModuleNotFoundError:
    import validate_terminal_system_suite_report


class ValidateTerminalSystemSuiteWrapperTests(unittest.TestCase):
    def test_wrapper_forwards_system_suite_contract_arguments(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            report_path = pathlib.Path(temp_dir) / "system-suite.json"
            benchmark_report_path = pathlib.Path(temp_dir) / "benchmark.json"
            benchmark_baseline_path = pathlib.Path(temp_dir) / "benchmark-baseline.json"
            live_display_report_path = pathlib.Path(temp_dir) / "display.json"
            live_display_baseline_path = pathlib.Path(temp_dir) / "display-baseline.json"
            for path in (
                report_path,
                benchmark_report_path,
                benchmark_baseline_path,
                live_display_report_path,
                live_display_baseline_path,
            ):
                path.write_text("{}", encoding="utf-8")

            argv = [
                "validate_terminal_system_suite_report.py",
                str(report_path),
                "--benchmark-report",
                str(benchmark_report_path),
                "--governance-mode",
                "ci",
                "--benchmark-baseline",
                str(benchmark_baseline_path),
                "--live-display-mode",
                "controlled",
                "--live-display-report",
                str(live_display_report_path),
                "--live-display-baseline",
                str(live_display_baseline_path),
            ]
            with mock.patch("subprocess.run") as run:
                with mock.patch("sys.argv", argv):
                    self.assertEqual(validate_terminal_system_suite_report.main(), 0)

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
                    "system-suite",
                    "validate",
                    "--report",
                    str(report_path),
                    "--benchmark-report",
                    str(benchmark_report_path),
                    "--governance-mode",
                    "ci",
                    "--benchmark-baseline",
                    str(benchmark_baseline_path),
                    "--live-display-mode",
                    "controlled",
                    "--live-display-report",
                    str(live_display_report_path),
                    "--live-display-baseline",
                    str(live_display_baseline_path),
                ],
                check=True,
                cwd=validate_terminal_system_suite_report.REPO_ROOT,
            )


if __name__ == "__main__":
    unittest.main()
