#!/usr/bin/env python3
from __future__ import annotations

import pathlib
import tempfile
import unittest
from unittest import mock

try:
    from scripts.ci import validate_terminal_display_environment
except ModuleNotFoundError:
    import validate_terminal_display_environment


class ValidateTerminalDisplayEnvironmentWrapperTests(unittest.TestCase):
    def test_wrapper_forwards_required_environment_flags(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            report_path = pathlib.Path(temp_dir) / "report.json"
            report_path.write_text("{}", encoding="utf-8")
            argv = [
                "validate_terminal_display_environment.py",
                str(report_path),
                "--require-session-type",
                "wayland",
                "--require-display-server-hint",
                "wayland",
                "--require-monitor-cadence",
                "--require-monitor-scale-factor",
            ]
            with mock.patch("subprocess.run") as run:
                with mock.patch("sys.argv", argv):
                    self.assertEqual(validate_terminal_display_environment.main(), 0)

            run.assert_called_once_with(
                [
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
                    str(report_path),
                    "--require-session-type",
                    "wayland",
                    "--require-display-server-hint",
                    "wayland",
                    "--require-monitor-cadence",
                    "--require-monitor-scale-factor",
                ],
                check=True,
                cwd=validate_terminal_display_environment.REPO_ROOT,
            )


if __name__ == "__main__":
    unittest.main()
