#!/usr/bin/env python3
import argparse
import json
import pathlib
import sys

EXPECTED_TOOL = "terminal-display-runner-readiness"


def fail(message: str) -> None:
    raise SystemExit(f"terminal display runner readiness validation failed: {message}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=pathlib.Path)
    parser.add_argument("--require-pass", action="store_true")
    args = parser.parse_args()

    with args.report.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)

    if payload.get("system_tool") != EXPECTED_TOOL:
        fail(f"system_tool must be {EXPECTED_TOOL!r}")
    status = payload.get("status")
    if status not in {"pass", "fail"}:
        fail("status must be 'pass' or 'fail'")
    if args.require_pass and status != "pass":
        fail(f"status must be 'pass', got {status!r}")

    generated_at_utc = payload.get("generated_at_utc")
    if not isinstance(generated_at_utc, str) or not generated_at_utc:
        fail("generated_at_utc must be a non-empty string")

    os_name = payload.get("os")
    if not isinstance(os_name, str) or not os_name:
        fail("os must be a non-empty string")

    session_type = payload.get("session_type")
    if session_type is not None and (not isinstance(session_type, str) or not session_type):
        fail("session_type must be null or a non-empty string")

    display_server_hint = payload.get("display_server_hint")
    if not isinstance(display_server_hint, str) or not display_server_hint:
        fail("display_server_hint must be a non-empty string")

    display_env_present = payload.get("display_env_present")
    if not isinstance(display_env_present, bool):
        fail("display_env_present must be a boolean")

    required_session_type = payload.get("required_session_type")
    if required_session_type is not None and (
        not isinstance(required_session_type, str) or not required_session_type
    ):
        fail("required_session_type must be null or a non-empty string")

    required_display_server_hint = payload.get("required_display_server_hint")
    if required_display_server_hint is not None and (
        not isinstance(required_display_server_hint, str) or not required_display_server_hint
    ):
        fail("required_display_server_hint must be null or a non-empty string")

    errors = payload.get("errors")
    if not isinstance(errors, list):
        fail("errors must be a list")
    if any(not isinstance(entry, str) or not entry for entry in errors):
        fail("errors entries must be non-empty strings")
    if status == "pass" and errors:
        fail("errors must be empty when status is 'pass'")
    if status == "fail" and not errors:
        fail("errors must be non-empty when status is 'fail'")
    return 0


if __name__ == "__main__":
    sys.exit(main())
