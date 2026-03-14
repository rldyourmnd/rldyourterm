#!/usr/bin/env python3
from __future__ import annotations

import hmac
import importlib.util
import io
import json
import os
from hashlib import sha256
from http import HTTPStatus
from pathlib import Path
import unittest
from unittest import mock
import urllib.error


MODULE_PATH = Path(__file__).with_name("router.py")
CONFIG_PATH = Path(__file__).with_name("repositories.json")
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


class BrokenPipeHeadersHandler(DummyHandler):
    def end_headers(self) -> None:
        raise BrokenPipeError


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

    def test_repository_config_keeps_ready_for_review_trigger(self) -> None:
        config = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
        repo_config = config["repositories"]["rldyourmnd/rldyourterm"]
        actions = repo_config["events"]["pull_request"]["actions"]

        self.assertEqual(actions["ready_for_review"], "Rldyourterm/PR-Validation")
        self.assertEqual(repo_config["events"]["issue_comment"]["trigger"], "@jenkins")

    def test_transient_jenkins_error_detects_retryable_failures(self) -> None:
        bad_gateway = urllib.error.HTTPError(
            "https://jenkins.example/job/test",
            HTTPStatus.BAD_GATEWAY,
            "bad gateway",
            {},
            io.BytesIO(),
        )
        unauthorized = urllib.error.HTTPError(
            "https://jenkins.example/job/test",
            HTTPStatus.UNAUTHORIZED,
            "unauthorized",
            {},
            io.BytesIO(),
        )

        self.assertTrue(router.is_transient_jenkins_error(urllib.error.URLError("controller restarting")))
        self.assertTrue(router.is_transient_jenkins_error(bad_gateway))
        self.assertFalse(router.is_transient_jenkins_error(unauthorized))
        invalid_crumb = urllib.error.HTTPError(
            "https://jenkins.example/job/test",
            HTTPStatus.FORBIDDEN,
            "forbidden",
            {},
            io.BytesIO(b'{"message":"No valid crumb was included in the request"}'),
        )
        forbidden = urllib.error.HTTPError(
            "https://jenkins.example/job/test",
            HTTPStatus.FORBIDDEN,
            "forbidden",
            {},
            io.BytesIO(b"no permission"),
        )

        self.assertTrue(router.is_transient_jenkins_error(invalid_crumb))
        self.assertFalse(router.is_transient_jenkins_error(forbidden))
        bad_gateway.close()
        unauthorized.close()
        invalid_crumb.close()
        forbidden.close()

    def test_json_response_ignores_broken_pipe(self) -> None:
        handler = DummyHandler(BrokenPipeStream())

        router.GithubWebhookRouter._json_response(
            handler,
            HTTPStatus.ACCEPTED,
            {"status": "triggered"},
        )

        self.assertTrue(handler.close_connection)
        self.assertIn(("status", HTTPStatus.ACCEPTED), handler.responses)

    def test_status_only_response_ignores_broken_pipe(self) -> None:
        handler = BrokenPipeHeadersHandler(BrokenPipeStream())

        router.GithubWebhookRouter._status_only_response(
            handler,
            HTTPStatus.OK,
        )

        self.assertTrue(handler.close_connection)
        self.assertIn(("status", HTTPStatus.OK), handler.responses)

    def test_trigger_jenkins_build_retries_transient_errors(self) -> None:
        class DummyResponse:
            def __init__(self, status: HTTPStatus) -> None:
                self.status = status

            def __enter__(self) -> "DummyResponse":
                return self

            def __exit__(self, exc_type, exc, tb) -> None:
                return None

        opener = mock.Mock()
        opener.open.return_value = DummyResponse(HTTPStatus.CREATED)

        with (
            mock.patch.object(router, "build_basic_auth_header", return_value="Basic test"),
            mock.patch.object(router, "build_jenkins_opener", return_value=opener),
            mock.patch.object(
                router,
                "load_crumb",
                side_effect=[
                    urllib.error.URLError("controller restarting"),
                    ("Jenkins-Crumb", "crumb-value"),
                ],
            ) as load_crumb,
            mock.patch.object(router, "stop_matching_running_builds") as stop_builds,
            mock.patch.object(router.time, "sleep") as sleep_mock,
        ):
            router.trigger_jenkins_build(
                base_url="https://jenkins.example",
                job_name="Rldyourterm/PR-Validation",
                username="jenkins",
                password="secret",
                params={"PR_NUMBER": "32", "REPO_FULL_NAME": "rldyourmnd/rldyourterm"},
            )

        self.assertEqual(load_crumb.call_count, 2)
        stop_builds.assert_called_once()
        sleep_mock.assert_called_once_with(2.0)

    def test_trigger_jenkins_build_retries_invalid_crumb_errors(self) -> None:
        class DummyResponse:
            def __init__(self, status: HTTPStatus) -> None:
                self.status = status

            def __enter__(self) -> "DummyResponse":
                return self

            def __exit__(self, exc_type, exc, tb) -> None:
                return None

        invalid_crumb = urllib.error.HTTPError(
            "https://jenkins.example/job/test",
            HTTPStatus.FORBIDDEN,
            "forbidden",
            {},
            io.BytesIO(b'{"message":"No valid crumb was included in the request"}'),
        )
        opener = mock.Mock()
        opener.open.return_value = DummyResponse(HTTPStatus.CREATED)
        with (
            mock.patch.object(router, "build_basic_auth_header", return_value="Basic test"),
            mock.patch.object(router, "build_jenkins_opener", return_value=opener),
            mock.patch.object(router, "load_crumb", return_value=("Jenkins-Crumb", "crumb-value")),
            mock.patch.object(
                router,
                "stop_matching_running_builds",
                side_effect=[invalid_crumb, None],
            ) as stop_builds,
            mock.patch.object(router.time, "sleep") as sleep_mock,
        ):
            router.trigger_jenkins_build(
                base_url="https://jenkins.example",
                job_name="Rldyourterm/PR-Validation",
                username="jenkins",
                password="secret",
                params={"PR_NUMBER": "32", "REPO_FULL_NAME": "rldyourmnd/rldyourterm"},
            )

        self.assertEqual(load_crumb.call_count, 2)
        self.assertEqual(stop_builds.call_count, 2)
        sleep_mock.assert_called_once_with(2.0)

    def test_stop_matching_running_builds_times_out_when_builds_do_not_stop(self) -> None:
        build_payload = [
            {
                "building": True,
                "number": 99,
                "actions": [
                    {
                        "parameters": [
                            {"name": "PR_NUMBER", "value": "99"},
                        ],
                    },
                ],
            },
        ]

        with (
            mock.patch.dict(
                os.environ,
                {
                    "JENKINS_BUILD_CANCEL_MAX_ATTEMPTS": "2",
                    "JENKINS_BUILD_CANCEL_RETRY_SECONDS": "0.1",
                },
            ),
            mock.patch.object(router, "list_job_builds", return_value=build_payload),
            mock.patch.object(router, "stop_build") as stop_builds,
            mock.patch.object(router.time, "sleep") as sleep_mock,
        ):
            with self.assertRaises(RuntimeError) as ctx:
                router.stop_matching_running_builds(
                    base_url="https://jenkins.example",
                    job_name="Rldyourterm/PR-Validation",
                    pr_number="99",
                    opener=mock.Mock(),
                    crumb_header="Jenkins-Crumb",
                    crumb_value="token",
                )

        self.assertIn("timed out waiting for PR 99 builds to stop", str(ctx.exception))
        self.assertEqual(stop_builds.call_count, 2)
        self.assertEqual(sleep_mock.call_count, 1)


if __name__ == "__main__":
    unittest.main()
