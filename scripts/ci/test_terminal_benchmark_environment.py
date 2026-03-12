#!/usr/bin/env python3
import unittest

try:
    from scripts.ci.terminal_benchmark_environment import (
        CONTROLLED_DISPLAY_SCOPE,
        LOCAL_DISPLAY_SCOPE,
        extract_environment_requirements_for_baseline,
        infer_report_environment_scope,
        validate_report_against_environment_requirements,
    )
except ModuleNotFoundError:
    from terminal_benchmark_environment import (
        CONTROLLED_DISPLAY_SCOPE,
        LOCAL_DISPLAY_SCOPE,
        extract_environment_requirements_for_baseline,
        infer_report_environment_scope,
        validate_report_against_environment_requirements,
    )


def make_live_display_report() -> dict:
    return {
        "suite": "live-display",
        "environment": {
            "display_server_hint": "wayland",
            "session_type": "wayland",
        },
        "results": [
            {
                "scenario": "startup-first-frame-cpu",
                "backend": "cpu",
                "pacing_mode": "event-driven",
                "monitor_refresh_rate_millihz": None,
                "monitor_scale_factor": None,
            },
            {
                "scenario": "steady-redraw-cpu",
                "backend": "cpu",
                "pacing_mode": "monitor-cadence",
                "monitor_refresh_rate_millihz": 143998,
                "monitor_scale_factor": 1.0,
            },
            {
                "scenario": "resize-cycle-cpu",
                "backend": "cpu",
                "pacing_mode": "monitor-cadence",
                "monitor_refresh_rate_millihz": 59982,
                "monitor_scale_factor": 2.0,
            },
        ],
    }


class TerminalBenchmarkEnvironmentTests(unittest.TestCase):
    def test_infer_controlled_scope_ignores_first_frame_cpu_scenario(self) -> None:
        report = make_live_display_report()
        self.assertEqual(infer_report_environment_scope(report), CONTROLLED_DISPLAY_SCOPE)

    def test_infer_local_scope_when_controlled_cpu_scenarios_are_absent(self) -> None:
        report = make_live_display_report()
        report["results"] = [report["results"][0]]

        self.assertEqual(infer_report_environment_scope(report), LOCAL_DISPLAY_SCOPE)

    def test_extract_requirements_tracks_only_controlled_cpu_scenarios(self) -> None:
        report = make_live_display_report()
        requirements = extract_environment_requirements_for_baseline(report)

        self.assertIsNotNone(requirements)
        self.assertEqual(requirements["display_server_hint"], "wayland")
        self.assertEqual(requirements["session_type"], "wayland")
        self.assertEqual(
            sorted(requirements["cpu_scenarios"].keys()),
            ["resize-cycle-cpu", "steady-redraw-cpu"],
        )

    def test_validate_report_against_extracted_requirements(self) -> None:
        report = make_live_display_report()
        requirements = extract_environment_requirements_for_baseline(report)

        assert requirements is not None
        validate_report_against_environment_requirements(report, requirements)


if __name__ == "__main__":
    unittest.main()
