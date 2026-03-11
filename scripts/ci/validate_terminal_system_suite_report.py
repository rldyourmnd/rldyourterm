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
    parser.add_argument("--governance-mode", choices=["ci", "release"], required=True)
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
        f"bash scripts/ci/run_terminal_benchmark_full.sh {args.benchmark_report}",
        f"bash scripts/ci/run_e2e_governance.sh --mode {args.governance_mode}",
    ]
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
    return 0


if __name__ == "__main__":
    sys.exit(main())
