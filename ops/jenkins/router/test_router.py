#!/usr/bin/env python3
from __future__ import annotations

import hmac
import importlib.util
from hashlib import sha256
from http import HTTPStatus
from pathlib import Path
import unittest


MODULE_PATH = Path(__file__).with_name("router.py")
MODULE_SPEC = importlib.util.spec_from_file_location("jenkins_router", MODULE_PATH)
assert MODULE_SPEC is not None
assert MODULE_SPEC.loader is not None
router = importlib.util.module_from_spec(MODULE_SPEC)
MODULE_SPEC.loader.exec_module(router)


class BrokenPipeStream:
    def write(self, _: bytes) -> int:
        raise BrokenPipeError


class DummyHandler:
    def __init__(self, stream: object) -> None:
        self.wfile = stream
        self.close_connection = False
        self.responses: list[object] = []

    def send_response(self, status: HTTPStatus) -> None:
        self.responses.append(("status", status))

    def send_header(self, name: str, value: str) -> None:
        self.responses.append(("header", name, value))

    def end_headers(self) -> None:
        self.responses.append(("end_headers",))


class RouterHelpersTest(unittest.TestCase):
    def test_build_job_path_supports_foldered_jobs(self) -> None:
        self.assertEqual(
            router.build_job_path("Rldyourterm/PR-Validation"),
            "job/Rldyourterm/job/PR-Validation",
        )

    def test_extract_pr_number_requires_positive_integer(self) -> None:
        self.assertEqual(router.extract_pr_number(32), "32")
        self.assertEqual(router.extract_pr_number("32"), "32")
        self.assertIsNone(router.extract_pr_number(0))
        self.assertIsNone(router.extract_pr_number("0"))
        self.assertIsNone(router.extract_pr_number("32a"))

    def test_verify_signature_matches_expected_sha256(self) -> None:
        secret = "top-secret"
        body = b'{"event":"pull_request"}'
        signature = "sha256=" + hmac.new(secret.encode("utf-8"), body, sha256).hexdigest()

        self.assertTrue(router.verify_signature(secret, body, signature))
        self.assertFalse(router.verify_signature(secret, body, "sha256=deadbeef"))

    def test_json_response_ignores_broken_pipe(self) -> None:
        handler = DummyHandler(BrokenPipeStream())

        router.GithubWebhookRouter._json_response(
            handler,
            HTTPStatus.ACCEPTED,
            {"status": "triggered"},
        )

        self.assertTrue(handler.close_connection)
        self.assertIn(("status", HTTPStatus.ACCEPTED), handler.responses)


if __name__ == "__main__":
    unittest.main()
