#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import hmac
import json
import os
import base64
import http.cookiejar
import time
import urllib.error
import urllib.parse
import urllib.request
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def load_config() -> dict[str, object]:
    config_path = Path(os.environ["ROUTER_CONFIG"])
    with config_path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def verify_signature(secret: str, body: bytes, signature_header: str | None) -> bool:
    if not signature_header or not signature_header.startswith("sha256="):
        return False
    expected = "sha256=" + hmac.new(
        secret.encode("utf-8"),
        body,
        hashlib.sha256,
    ).hexdigest()
    return hmac.compare_digest(expected, signature_header)


def build_basic_auth_header(username: str, password: str) -> str:
    token = base64.b64encode(f"{username}:{password}".encode("utf-8")).decode("ascii")
    return f"Basic {token}"


def build_jenkins_opener(auth_header: str) -> urllib.request.OpenerDirector:
    cookie_jar = http.cookiejar.CookieJar()
    opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cookie_jar))
    opener.addheaders = [("Authorization", auth_header)]
    return opener


def load_crumb(base_url: str, opener: urllib.request.OpenerDirector) -> tuple[str, str]:
    crumb_request = urllib.request.Request(
        url=f"{base_url.rstrip('/')}/crumbIssuer/api/json",
        method="GET",
    )
    with opener.open(crumb_request, timeout=15) as response:
        payload = json.load(response)
    return payload["crumbRequestField"], payload["crumb"]


def build_job_path(job_name: str) -> str:
    segments = [segment for segment in job_name.split("/") if segment]
    if not segments:
        raise ValueError("job name must not be empty")
    encoded_segments = [urllib.parse.quote(segment, safe="") for segment in segments]
    return "/".join(f"job/{segment}" for segment in encoded_segments)


def extract_pr_number(value: object) -> str | None:
    if isinstance(value, int):
        return str(value) if value > 0 else None
    if isinstance(value, str) and value.isdigit() and int(value) > 0:
        return value
    return None


def _is_invalid_crumb_error(exc: urllib.error.HTTPError) -> bool:
    if exc.code != HTTPStatus.FORBIDDEN:
        return False

    body = _safe_read_http_error_body(exc)
    if not body:
        return False

    body_lower = body.lower()
    return "invalid crumb" in body_lower or "no valid crumb" in body_lower


def _safe_read_http_error_body(exc: urllib.error.HTTPError) -> str:
    try:
        raw = exc.fp.read() if exc.fp is not None else b""
        if raw is None:
            return ""
        if isinstance(raw, str):
            return raw
        return raw.decode("utf-8", errors="replace")
    except Exception:
        return ""


def is_transient_jenkins_error(exc: Exception) -> bool:
    if isinstance(exc, urllib.error.HTTPError):
        if exc.code == HTTPStatus.FORBIDDEN:
            return _is_invalid_crumb_error(exc)
        return exc.code in (
            HTTPStatus.BAD_GATEWAY,
            HTTPStatus.SERVICE_UNAVAILABLE,
            HTTPStatus.GATEWAY_TIMEOUT,
        )
    return isinstance(exc, urllib.error.URLError)


def trigger_jenkins_build(base_url: str, job_name: str, username: str, password: str, params: dict[str, str]) -> None:
    retry_attempts = int(os.environ.get("JENKINS_TRIGGER_RETRY_ATTEMPTS", "15"))
    retry_delay_seconds = float(os.environ.get("JENKINS_TRIGGER_RETRY_DELAY_SECONDS", "2"))
    last_error: Exception | None = None

    for attempt in range(retry_attempts):
        try:
            auth_header = build_basic_auth_header(username, password)
            opener = build_jenkins_opener(auth_header)
            crumb_header, crumb_value = load_crumb(base_url, opener)
            stop_matching_running_builds(
                base_url=base_url,
                job_name=job_name,
                pr_number=params.get("PR_NUMBER", ""),
                opener=opener,
                crumb_header=crumb_header,
                crumb_value=crumb_value,
            )
            query = urllib.parse.urlencode(params)
            job_path = build_job_path(job_name)
            url = f"{base_url.rstrip('/')}/{job_path}/buildWithParameters?{query}"
            request = urllib.request.Request(
                url=url,
                method="POST",
                headers={crumb_header: crumb_value},
            )
            with opener.open(request, timeout=15) as response:
                if response.status not in (HTTPStatus.CREATED, HTTPStatus.FOUND, HTTPStatus.OK, HTTPStatus.ACCEPTED):
                    raise RuntimeError(f"unexpected Jenkins response status: {response.status}")
            return
        except Exception as exc:
            last_error = exc
            if attempt == retry_attempts - 1 or not is_transient_jenkins_error(exc):
                raise
            time.sleep(retry_delay_seconds)

    assert last_error is not None
    raise last_error


def list_job_builds(base_url: str, job_name: str, opener: urllib.request.OpenerDirector) -> list[dict[str, object]]:
    job_path = build_job_path(job_name)
    tree = urllib.parse.quote("builds[number,building,actions[parameters[name,value]]]", safe="")
    request = urllib.request.Request(
        url=f"{base_url.rstrip('/')}/{job_path}/api/json?tree={tree}",
        method="GET",
    )
    with opener.open(request, timeout=15) as response:
        payload = json.load(response)
    builds = payload.get("builds", [])
    return builds if isinstance(builds, list) else []


def build_parameter(build: dict[str, object], name: str) -> str | None:
    actions = build.get("actions", [])
    if not isinstance(actions, list):
        return None
    for action in actions:
        if not isinstance(action, dict):
            continue
        parameters = action.get("parameters", [])
        if not isinstance(parameters, list):
            continue
        for parameter in parameters:
            if not isinstance(parameter, dict):
                continue
            if parameter.get("name") != name:
                continue
            value = parameter.get("value")
            return str(value) if value is not None else None
    return None


def stop_build(
    base_url: str,
    job_name: str,
    build_number: int,
    opener: urllib.request.OpenerDirector,
    crumb_header: str,
    crumb_value: str,
) -> None:
    job_path = build_job_path(job_name)
    request = urllib.request.Request(
        url=f"{base_url.rstrip('/')}/{job_path}/{build_number}/stop",
        method="POST",
        headers={crumb_header: crumb_value},
    )
    try:
        with opener.open(request, timeout=15) as response:
            if response.status not in (HTTPStatus.CREATED, HTTPStatus.FOUND, HTTPStatus.OK, HTTPStatus.ACCEPTED):
                raise RuntimeError(
                    f"unexpected Jenkins response status while stopping build {build_number}: {response.status}"
                )
    except urllib.error.HTTPError as exc:
        if exc.code in (HTTPStatus.NOT_FOUND, HTTPStatus.CONFLICT):
            return
        raise


def stop_matching_running_builds(
    base_url: str,
    job_name: str,
    pr_number: str,
    opener: urllib.request.OpenerDirector,
    crumb_header: str,
    crumb_value: str,
) -> None:
    if not pr_number:
        return

    max_attempts = int(os.environ.get("JENKINS_BUILD_CANCEL_MAX_ATTEMPTS", "8"))
    retry_delay_seconds = float(os.environ.get("JENKINS_BUILD_CANCEL_RETRY_SECONDS", "2"))

    for attempt in range(max_attempts):
        matching_builds: list[int] = []
        for build in list_job_builds(base_url, job_name, opener):
            if not isinstance(build, dict):
                continue
            if not build.get("building"):
                continue
            if build_parameter(build, "PR_NUMBER") != pr_number:
                continue
            build_number = build.get("number")
            if isinstance(build_number, int):
                matching_builds.append(build_number)

        if not matching_builds:
            return

        for build_number in matching_builds:
            stop_build(
                base_url=base_url,
                job_name=job_name,
                build_number=build_number,
                opener=opener,
                crumb_header=crumb_header,
                crumb_value=crumb_value,
            )

        if attempt + 1 >= max_attempts:
            break

        time.sleep(retry_delay_seconds)

    raise RuntimeError(
        f"timed out waiting for PR {pr_number} builds to stop for job '{job_name}' after {max_attempts} attempts"
    )


class GithubWebhookRouter(BaseHTTPRequestHandler):
    server_version = "NDDevGithubWebhookRouter/1.0"

    def _json_response(self, status: HTTPStatus, payload: dict[str, object]) -> None:
        response_body = json.dumps(payload, sort_keys=True).encode("utf-8")
        try:
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(response_body)))
            self.end_headers()
            self.wfile.write(response_body)
        except (BrokenPipeError, ConnectionResetError):
            self.close_connection = True

    def _status_only_response(self, status: HTTPStatus) -> None:
        try:
            self.send_response(status)
            self.send_header("Content-Length", "0")
            self.end_headers()
        except (BrokenPipeError, ConnectionResetError):
            self.close_connection = True

    def do_GET(self) -> None:  # noqa: N802
        if self.path == "/healthz":
            self._json_response(HTTPStatus.OK, {"status": "ok"})
            return
        self._json_response(HTTPStatus.NOT_FOUND, {"error": "not found"})

    def do_HEAD(self) -> None:  # noqa: N802
        if self.path == "/healthz":
            self._status_only_response(HTTPStatus.OK)
            return
        self._status_only_response(HTTPStatus.NOT_FOUND)

    def do_POST(self) -> None:  # noqa: N802
        if self.path != "/github/webhook":
            self._json_response(HTTPStatus.NOT_FOUND, {"error": "not found"})
            return

        content_length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(content_length)
        signature = self.headers.get("X-Hub-Signature-256")
        event = self.headers.get("X-GitHub-Event", "")

        secret = os.environ["GITHUB_WEBHOOK_SECRET"]
        if not verify_signature(secret, body, signature):
            self._json_response(HTTPStatus.UNAUTHORIZED, {"error": "invalid signature"})
            return

        try:
            payload = json.loads(body)
        except json.JSONDecodeError:
            self._json_response(HTTPStatus.BAD_REQUEST, {"error": "invalid json"})
            return

        config = load_config()
        repositories = config["repositories"]
        repository = payload.get("repository", {})
        repo_full_name = repository.get("full_name")
        if not isinstance(repo_full_name, str) or repo_full_name not in repositories:
            self._json_response(HTTPStatus.ACCEPTED, {"status": "ignored", "reason": "repository not configured"})
            return

        repo_cfg = repositories[repo_full_name]
        allowed_login = repo_cfg["allowed_login"]
        events_cfg = repo_cfg["events"]
        sender_login = payload.get("sender", {}).get("login", "")
        if sender_login != allowed_login:
            self._json_response(HTTPStatus.ACCEPTED, {"status": "ignored", "reason": "sender not allowed"})
            return

        params: dict[str, str] | None = None
        job_name: str | None = None
        action = payload.get("action", "")

        if event == "pull_request":
            pull_request = payload.get("pull_request", {})
            pr_author = pull_request.get("user", {}).get("login", "")
            pr_number = extract_pr_number(pull_request.get("number"))
            if pr_author != allowed_login:
                self._json_response(HTTPStatus.ACCEPTED, {"status": "ignored", "reason": "pr author not allowed"})
                return
            if pr_number is None:
                self._json_response(HTTPStatus.BAD_REQUEST, {"error": "pull_request number is missing or invalid"})
                return
            pr_cfg = events_cfg["pull_request"]
            actions = pr_cfg["actions"]
            if action not in actions:
                self._json_response(HTTPStatus.ACCEPTED, {"status": "ignored", "reason": "pull_request action not allowed"})
                return
            job_name = actions[action]
            params = {
                "REPO_FULL_NAME": repo_full_name,
                "PR_NUMBER": pr_number,
                "TRIGGER_EVENT": event,
                "TRIGGER_ACTION": action,
                "TRIGGER_ACTOR": sender_login,
                "TRIGGER_COMMENT": "",
            }
        elif event == "issue_comment":
            issue = payload.get("issue", {})
            pr_number = extract_pr_number(issue.get("number"))
            if "pull_request" not in issue:
                self._json_response(HTTPStatus.ACCEPTED, {"status": "ignored", "reason": "not a pull request comment"})
                return
            pr_author = issue.get("user", {}).get("login", "")
            if pr_author != allowed_login:
                self._json_response(HTTPStatus.ACCEPTED, {"status": "ignored", "reason": "pr author not allowed"})
                return
            if pr_number is None:
                self._json_response(HTTPStatus.BAD_REQUEST, {"error": "issue number is missing or invalid"})
                return
            if action != "created":
                self._json_response(HTTPStatus.ACCEPTED, {"status": "ignored", "reason": "comment action not allowed"})
                return
            comment_cfg = events_cfg["issue_comment"]
            comment_body = payload.get("comment", {}).get("body", "")
            if comment_cfg["trigger"] not in comment_body:
                self._json_response(HTTPStatus.ACCEPTED, {"status": "ignored", "reason": "comment trigger missing"})
                return
            job_name = comment_cfg["job"]
            params = {
                "REPO_FULL_NAME": repo_full_name,
                "PR_NUMBER": pr_number,
                "TRIGGER_EVENT": event,
                "TRIGGER_ACTION": action,
                "TRIGGER_ACTOR": sender_login,
                "TRIGGER_COMMENT": comment_body,
            }
        else:
            self._json_response(HTTPStatus.ACCEPTED, {"status": "ignored", "reason": "event not handled"})
            return

        assert params is not None
        assert job_name is not None

        try:
            trigger_jenkins_build(
                base_url=os.environ["JENKINS_BASE_URL"],
                job_name=job_name,
                username=os.environ["JENKINS_ADMIN_USER"],
                password=os.environ["JENKINS_ADMIN_PASSWORD"],
                params=params,
            )
        except Exception as exc:  # pragma: no cover - boundary path
            self._json_response(HTTPStatus.BAD_GATEWAY, {"error": "jenkins trigger failed", "detail": str(exc)})
            return
        self._json_response(HTTPStatus.ACCEPTED, {"status": "triggered", "job": job_name, "parameters": params})


def main() -> None:
    host = "0.0.0.0"
    port = int(os.environ.get("ROUTER_PORT", "8080"))
    server = ThreadingHTTPServer((host, port), GithubWebhookRouter)
    server.serve_forever()


if __name__ == "__main__":
    main()
