#!/usr/bin/env python3
import argparse
import json
import pathlib
import subprocess
import sys

EXPECTED_TOOL = "terminal-system-suite"
EXPECTED_STATUS = "pass"


def fail(message: str) -> None:
    raise SystemExit(f"terminal system suite validation failed: {message}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("--benchmark-report", type=pathlib.Path, required=True)
    parser.add_argument("--benchmark-baseline", type=pathlib.Path)
    parser.add_argument("--governance-mode", choices=["ci", "release"], required=True)
    parser.add_argument("--live-display-mode", choices=["smoke", "full", "controlled"])
    parser.add_argument("--live-display-report", type=pathlib.Path)
    parser.add_argument("--live-display-baseline", type=pathlib.Path)
    args = parser.parse_args()

    with args.report.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)

    if payload.get("system_tool") != EXPECTED_TOOL:
        fail(f"system_tool must be {EXPECTED_TOOL!r}")
    if payload.get("status") != EXPECTED_STATUS:
        fail(f"status must be {EXPECTED_STATUS!r}")
    generated_at_utc = payload.get("generated_at_utc")
    if not isinstance(generated_at_utc, str) or not generated_at_utc:
        fail("generated_at_utc must be a non-empty string")
    if payload.get("governance_mode") != args.governance_mode:
        fail(
            f"governance_mode must be {args.governance_mode!r}, got {payload.get('governance_mode')!r}"
        )

    benchmark_report = payload.get("benchmark_report")
    if benchmark_report != str(args.benchmark_report):
        fail(
            f"benchmark_report must be {str(args.benchmark_report)!r}, got {benchmark_report!r}"
        )
    benchmark_baseline = payload.get("benchmark_baseline")
    expected_benchmark_baseline = None if args.benchmark_baseline is None else str(args.benchmark_baseline)
    if benchmark_baseline != expected_benchmark_baseline:
        fail(
            f"benchmark_baseline must be {expected_benchmark_baseline!r}, got {benchmark_baseline!r}"
        )

    live_display = payload.get("live_display")
    if args.live_display_mode is None:
        if live_display is not None:
            fail("live_display must be null when no live display mode is requested")
    else:
        if not isinstance(live_display, dict):
            fail("live_display must be an object when live display mode is requested")
        if live_display.get("mode") != args.live_display_mode:
            fail(
                f"live_display.mode must be {args.live_display_mode!r}, got {live_display.get('mode')!r}"
            )
        if args.live_display_report is None:
            fail("live display report path argument is required when live display mode is requested")
        if live_display.get("report") != str(args.live_display_report):
            fail(
                f"live_display.report must be {str(args.live_display_report)!r}, got {live_display.get('report')!r}"
            )
        expected_live_display_baseline = (
            None if args.live_display_baseline is None else str(args.live_display_baseline)
        )
        if live_display.get("baseline") != expected_live_display_baseline:
            fail(
                f"live_display.baseline must be {expected_live_display_baseline!r}, got {live_display.get('baseline')!r}"
            )

    quality_gates = payload.get("quality_gates")
    if not isinstance(quality_gates, list) or not quality_gates:
        fail("quality_gates must be a non-empty list")
    if any(not isinstance(entry, str) or not entry for entry in quality_gates):
        fail("quality_gates entries must be non-empty strings")

    expected_gates = [
        "cargo fmt --all -- --check",
        "cargo check --workspace --all-targets --locked",
        "cargo test --workspace --locked",
        "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings",
        "cargo +1.92.0 check --workspace --all-targets --locked",
        "cargo check --manifest-path fuzz/Cargo.toml --locked",
        "bash scripts/ci/run_terminal_benchmark_smoke.sh",
    ]
    if args.benchmark_baseline is not None:
        expected_gates.append(
            f"TERMINAL_BENCHMARK_BASELINE={args.benchmark_baseline} bash scripts/ci/run_terminal_benchmark_full.sh {args.benchmark_report}"
        )
    else:
        expected_gates.append(
            f"bash scripts/ci/run_terminal_benchmark_full.sh {args.benchmark_report}"
        )
    expected_gates.append(
        f"bash scripts/ci/run_e2e_governance.sh --mode {args.governance_mode}"
    )
    if args.live_display_mode is not None:
        if args.live_display_report is None:
            fail("live display report path argument is required when live display mode is requested")
        if args.live_display_baseline is not None:
            expected_gates.append(
                f"TERMINAL_DISPLAY_BENCHMARK_BASELINE={args.live_display_baseline} bash scripts/ci/run_terminal_display_benchmark_{args.live_display_mode}.sh {args.live_display_report}"
            )
        else:
            expected_gates.append(
                f"bash scripts/ci/run_terminal_display_benchmark_{args.live_display_mode}.sh {args.live_display_report}"
            )
    if quality_gates != expected_gates:
        fail(f"quality_gates mismatch: expected {expected_gates!r}, got {quality_gates!r}")

    if not args.benchmark_report.is_file():
        fail(f"benchmark report does not exist: {args.benchmark_report}")

    subprocess.run(
        [
            sys.executable,
            "scripts/ci/validate_terminal_benchmark_report.py",
            str(args.benchmark_report),
            "--require-full-suite",
        ],
        check=True,
    )
    if args.benchmark_baseline is not None:
        if not args.benchmark_baseline.is_file():
            fail(f"benchmark baseline does not exist: {args.benchmark_baseline}")
        subprocess.run(
            [
                sys.executable,
                "scripts/ci/validate_terminal_benchmark_thresholds.py",
                str(args.benchmark_report),
                str(args.benchmark_baseline),
            ],
            check=True,
        )
    if args.live_display_mode is not None:
        if not args.live_display_report.is_file():
            fail(f"live display report does not exist: {args.live_display_report}")
        subprocess.run(
            [
                sys.executable,
                "scripts/ci/validate_terminal_display_benchmark_report.py",
                str(args.live_display_report),
                "--require-full-suite",
            ],
            check=True,
        )
        if args.live_display_baseline is not None:
            if not args.live_display_baseline.is_file():
                fail(f"live display baseline does not exist: {args.live_display_baseline}")
            subprocess.run(
                [
                    sys.executable,
                    "scripts/ci/validate_terminal_benchmark_thresholds.py",
                    str(args.live_display_report),
                    str(args.live_display_baseline),
                    "--allow-advisory",
                ],
                check=True,
            )
    return 0


if __name__ == "__main__":
    sys.exit(main())
