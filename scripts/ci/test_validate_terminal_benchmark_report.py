#!/usr/bin/env python3
from __future__ import annotations

import json
import pathlib
import tempfile
import unittest
from unittest import mock

try:
    from scripts.ci import validate_terminal_benchmark_report
except ModuleNotFoundError:
    import validate_terminal_benchmark_report


class ValidateTerminalBenchmarkReportTests(unittest.TestCase):
    def test_valid_full_suite_report_passes(self) -> None:
        self.assertEqual(self._run_validator(make_headless_report()), 0)

    def test_missing_benchmarked_layer_fails(self) -> None:
        payload = make_headless_report()
        payload["coverage"]["benchmarked_layers"] = payload["coverage"]["benchmarked_layers"][:-1]

        with self.assertRaises(SystemExit) as exc:
            self._run_validator(payload)

        self.assertIn("coverage.benchmarked_layers mismatch", str(exc.exception))

    def test_empty_coverage_lists_fail(self) -> None:
        payload = make_headless_report()
        payload["coverage"]["benchmarked_layers"] = []
        payload["coverage"]["verified_only_layers"] = []

        with self.assertRaises(SystemExit) as exc:
            self._run_validator(payload)

        self.assertIn("coverage.benchmarked_layers mismatch", str(exc.exception))

    def test_benchmarked_layer_scenarios_must_match_manifest(self) -> None:
        payload = make_headless_report()
        payload["coverage"]["benchmarked_layers"][0]["benchmark_scenarios"] = []

        with self.assertRaises(SystemExit) as exc:
            self._run_validator(payload)

        self.assertIn("benchmark_scenarios mismatch", str(exc.exception))

    def _run_validator(self, payload: dict) -> int:
        with tempfile.TemporaryDirectory() as temp_dir:
            report_path = pathlib.Path(temp_dir) / "report.json"
            report_path.write_text(json.dumps(payload), encoding="utf-8")
            argv = [
                "validate_terminal_benchmark_report.py",
                str(report_path),
                "--require-full-suite",
            ]
            with mock.patch("sys.argv", argv):
                return validate_terminal_benchmark_report.main()


def make_headless_report() -> dict:
    scenarios = [
        {
            "scenario": "core-ingest-burst",
            "layer": "core",
            "benchmark_kind": "throughput",
            "description": "core ingest burst",
            "primary_unit_label": "bytes",
        },
        {
            "scenario": "ui-command-cycle",
            "layer": "ui",
            "benchmark_kind": "control-plane",
            "description": "ui command cycle",
            "primary_unit_label": "commands",
        },
        {
            "scenario": "cpu-render-full",
            "layer": "features/render-cpu",
            "benchmark_kind": "raster",
            "description": "cpu render full",
            "primary_unit_label": "frames",
        },
    ]
    scenario_names = [entry["scenario"] for entry in scenarios]
    return {
        "benchmark_tool": "terminal-benchmark",
        "suite": "canonical-headless",
        "suite_manifest": {
            "schema_version": 1,
            "scenarios": [
                {
                    **entry,
                    "backend": None,
                    "controlled_monitor_cadence": False,
                }
                for entry in scenarios
            ],
        },
        "selected_scenarios": scenario_names,
        "results": [
            {
                **entry,
                "stats": {},
                "notes": [],
            }
            for entry in scenarios
        ],
        "coverage": {
            "benchmarked_layers": [
                coverage_layer(
                    "core",
                    ["core-ingest-burst"],
                    ["cargo test -p rldyourterm-core --locked"],
                    "core coverage",
                ),
                coverage_layer(
                    "ui",
                    ["ui-command-cycle"],
                    ["cargo test -p rldyourterm-ui --locked"],
                    "ui coverage",
                ),
                coverage_layer(
                    "features/render-cpu",
                    ["cpu-render-full"],
                    ["cargo test -p rldyourterm-render-cpu --locked"],
                    "cpu render coverage",
                ),
            ],
            "verified_only_layers": [
                coverage_layer("app", [], ["cargo test -p rldyourterm-app --locked"], "app coverage"),
                coverage_layer(
                    "foundation",
                    [],
                    ["cargo test -p rldyourterm-foundation --locked"],
                    "foundation coverage",
                ),
                coverage_layer(
                    "foundation-platform",
                    [],
                    ["cargo test -p rldyourterm-foundation-platform --locked"],
                    "foundation platform coverage",
                ),
                coverage_layer(
                    "features/diagnostics",
                    [],
                    ["cargo test -p rldyourterm-diagnostics --locked"],
                    "diagnostics coverage",
                ),
            ],
        },
        "workload": {
            "ai_burst_bytes": 1,
            "scrollback_flood_bytes": 1,
            "render_seed_bytes": 1,
            "delta_batches": 1,
            "session_cycles": 1,
            "ui_batch_repetitions": 1,
            "settings_rounds": 1,
            "shell_rounds": 1,
            "font_passes": 1,
            "surface_rounds": 1,
        },
    }


def coverage_layer(
    layer: str,
    benchmark_scenarios: list[str],
    validation_commands: list[str],
    notes: str,
) -> dict:
    return {
        "layer": layer,
        "benchmark_scenarios": benchmark_scenarios,
        "validation_commands": validation_commands,
        "notes": notes,
    }


if __name__ == "__main__":
    unittest.main()
