"""Run the real release canary shell against a strict local HTTP command double.

No API or cloud connection is made. curl is shadowed; jq and bash are real so
removed routes, response-shape drift, exit codes and cleanup are exercised.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = "11111111-1111-4111-8111-111111111111"
CLONE = "22222222-2222-4222-8222-222222222222"
REPO = "33333333-3333-4333-8333-333333333333"
HOST = "44444444-4444-4444-8444-444444444444"
SESSION = "55555555-5555-4555-8555-555555555555"
TASK = "66666666-6666-4666-8666-666666666666"
PREVIEW_SUFFIX = "preview.lean.g5labs.ai"

CURL_DOUBLE = r'''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
from urllib.parse import urlparse

args = sys.argv[1:]
def option(name, default=None):
    return args[args.index(name) + 1] if name in args else default
method = option('-X', 'GET')
url = next(arg for arg in args if arg.startswith(('http://', 'https://')))
# Preserve exact paths: the live API does not normalize trailing slashes.
parsed = urlparse(url)
path = parsed.path
headers = [args[i + 1] for i, arg in enumerate(args) if arg == '-H']
authorization = next((h.split(':', 1)[1].strip() for h in headers
                      if h.lower().startswith('authorization:')), '')
payload = json.loads(option('-d', '{}'))
log = Path(os.environ['HTTP_LOG'])
calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
request = {'method': method, 'host': parsed.hostname, 'path': path, 'payload': payload,
           'authorization': bool(authorization)}
with log.open('a') as handle:
    handle.write(json.dumps(request) + '\n')
fixture, clone, repo, host, session, task = json.loads(os.environ['HTTP_IDS'])
mode = os.environ['HTTP_MODE']
status, data = 200, {}
workspace = '/api/v1/workspaces/'
if method == 'GET' and path == workspace + fixture:
    # Current BranchSummaryDTO has repoId/workspaceId, no legacy branch id.
    data = {'name': 'Synthetic fixture', 'branches': [
        {'workspaceId': fixture, 'repoId': repo, 'repoName': 'fixture', 'branchName': 'main'}]}
elif method == 'GET' and path.endswith('/config'):
    data = {'effectiveSecretKeys': ['ANTHROPIC_AUTH_TOKEN']}
elif method == 'GET' and path.endswith('/worker-hosts'):
    provisioned = any(c['method'] == 'POST' and c['path'].endswith('/worker-hosts') for c in calls)
    data = {'hosts': [{'hostId': host, 'status': 'ready'}] if provisioned else []}
elif method == 'POST' and path.endswith('/worker-hosts'):
    data = {'hostId': host, 'status': 'provisioning'}
elif method == 'DELETE' and path.endswith('/worker-hosts/' + host):
    status = 204
elif method == 'POST' and path == workspace + fixture + '/clone':
    data = {'workspace': {'id': clone, 'name': payload['name']}}
elif method == 'POST' and path == workspace + clone + '/resume':
    data = {'status': 'running'}
elif method == 'GET' and path == workspace + clone + '/vm':
    data = {'status': 'running', 'hostId': host}
elif method == 'POST' and path == workspace + clone + '/sessions/start':
    # Fresh synthetic orgs have no inherited claude_permission_mode. Reproduce
    # the launch resolver's 422; the failure mode also tests EXIT cleanup.
    if not payload.get('permissionMode') or mode == 'chat_start_rejected':
        status, data = 422, {'error': {'code': 'VALIDATION_ERROR', 'details': {
            'field': 'permissionMode', 'code': 'missing_agent_permission_mode'}}}
    else:
        assert payload['permissionMode'] in ('default', 'bypassPermissions')
        data = {'session': {'id': session}}
elif method == 'GET' and path == '/api/v1/sessions/' + session + '/messages':
    content = 'Apache Maven 3.9.9\n'
    if mode != 'missing_mvn':
        content += 'MVN_ON_PATH=yes\n'
    if mode != 'missing_home':
        content += 'HOME_WRITABLE=yes\n'
    data = {'messages': [{'role': 'assistant', 'status': 'complete', 'content': content}]}
elif method == 'GET' and path == '/api/v1/sessions/' + session + '/timeline':
    data = {'entries': [{'entryType': 'tool_call'}, {'entryType': 'tool_result'}]}
elif method == 'GET' and path == workspace + fixture + '/repos/' + repo + '/preview-profiles':
    data = [{'name': 'release-fixture'}]
elif method == 'POST' and path == workspace + clone + '/preview':
    assert payload['repoId'] == repo and payload['profile'] == 'release-fixture'
    data = {'taskId': task, 'ready': False, 'status': 'running'}
elif method == 'GET' and path == workspace + clone + '/preview':
    data = {'taskId': task, 'ready': True, 'status': 'running'}
elif method == 'POST' and path == workspace + clone + '/preview/cancel':
    data = {'taskId': task}
elif (method == 'GET' and path.startswith('/api/v1/preview/')
      and authorization.startswith('Bearer g5k_')):
    # #661 refuses the key before minting a handoff or a Location header.
    status, data = 403, {'error': {'code': 'FORBIDDEN', 'details': {
        'missing_scopes': ['user_session_required']}}}
elif (method == 'GET' and parsed.hostname == task + '.' + os.environ['HTTP_PREVIEW_SUFFIX']
      and path == '/nonce.txt' and authorization.startswith('Bearer ')):
    assert not any(arg in args for arg in ('-L', '--location', '-b', '-c', '--cookie', '--cookie-jar'))
    assert '--max-time' in args
    if mode == 'preview_denied':
        status, data = 403, {'error': {'code': 'FORBIDDEN', 'details': {
            'missing_scopes': ['tasks:read']}, 'message': 'RAW_RESPONSE_MUST_NOT_LEAK'}}
    elif mode == 'preview_html_error':
        status, data = 502, '<html>RAW_RESPONSE_MUST_NOT_LEAK</html>'
    elif mode == 'preview_redirect':
        status, data = 302, 'RAW_RESPONSE_MUST_NOT_LEAK'
    else:
        data = 'release-fixture-nonce'
elif method == 'POST' and path == workspace + clone + '/delete':
    assert payload == {'operationId': clone, 'force': True}
    previous = sum(c['path'] == path for c in calls)
    if mode == 'cleanup_stuck' or (mode == 'cleanup_retry' and previous == 0):
        status, data = 409, {'error': {'code': 'WORKSPACE_STOPPING'}}
    elif mode == 'cleanup_conflict':
        status, data = 409, {'error': {'code': 'WORKSPACE_HAS_CHILDREN'}}
    else:
        status = 204
else:
    # Removed DELETE workspace and /branches/:id/preview-profiles must fail.
    status, data = 405, {'error': {'code': 'UNEXPECTED_ROUTE'}}

body = data if isinstance(data, str) else json.dumps(data if status >= 400 else {'data': data})
if status == 204:
    body = ''
if status >= 400 and any(arg.startswith('-') and not arg.startswith('--') and 'f' in arg for arg in args):
    body = ''
    failed = True
else:
    failed = False
output = option('-o')
if output:
    Path(output).write_text(body)
else:
    sys.stdout.write(body)
if '-w' in args:
    sys.stdout.write(str(status))
sys.exit(22 if failed else 0)
'''


class ReleaseCanaryTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not shutil.which("jq"):
            raise RuntimeError("Release canary tests require jq, as does the release runner")

    def run_canary(self, stage="chat", mode="success", preview_suffix=PREVIEW_SUFFIX):
        with tempfile.TemporaryDirectory() as directory:
            tmp = Path(directory)
            for name, source in (("curl", CURL_DOUBLE), ("sleep", "#!/bin/sh\nexit 0\n")):
                command = tmp / name
                command.write_text(source)
                command.chmod(0o755)
            env = {key: value for key, value in os.environ.items() if not key.startswith("G5_CANARY_")}
            env.update({
                "PATH": str(tmp) + os.pathsep + env.get("PATH", ""),
                "HTTP_LOG": str(tmp / "requests.jsonl"),
                "HTTP_IDS": json.dumps([FIXTURE, CLONE, REPO, HOST, SESSION, TASK]),
                "HTTP_MODE": mode,
                "HTTP_PREVIEW_SUFFIX": PREVIEW_SUFFIX,
                "G5_CANARY_API_URL": "https://lean.g5labs.ai",
                "G5_CANARY_API_KEY": "g5k_synthetic_test",
                "G5_CANARY_ORG_ID": "77777777-7777-4777-8777-777777777777",
                "G5_CANARY_FIXTURE_WS_ID": FIXTURE,
                "G5_CANARY_STAGES": stage,
                "G5_CANARY_BASE_IMAGE_REF": "org-default" if stage == "preview" else "example/g5d-tungsten@sha256:" + "a" * 64,
                "G5_CANARY_PREVIEW_PROFILE": "release-fixture",
                "G5_CANARY_REGION": "us-central1",
                "G5_CANARY_REFRESH_TOKEN_FILE": "/dev/null",
                "G5_CANARY_LLM_SECRET_FILE": "/dev/null",
                "G5_CANARY_API_RETRY_ATTEMPTS": "2",
                "G5_CANARY_API_RETRY_SLEEP_SECS": "0",
            })
            if preview_suffix is not None:
                env["G5_CANARY_PREVIEW_HOST_SUFFIX"] = preview_suffix
            result = subprocess.run(["bash", str(ROOT / "scripts/canary-tungsten-microvm.sh")],
                                    env=env, capture_output=True, text=True, timeout=30)
            log = tmp / "requests.jsonl"
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
        return result, calls

    def test_chat_uses_current_delete_route_and_joins_stopping_operation(self):
        result, calls = self.run_canary(mode="cleanup_retry")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        cleanup = [call for call in calls if call["path"].endswith("/delete")]
        self.assertEqual(len(cleanup), 2)
        for call in cleanup:
            self.assertEqual(call["method"], "POST")
            self.assertEqual(call["path"], f"/api/v1/workspaces/{CLONE}/delete")
            self.assertEqual(call["payload"], {"force": True, "operationId": CLONE})
        self.assertFalse(any(call["path"] == f"/api/v1/workspaces/{FIXTURE}/delete" for call in calls))

    def test_chat_rejects_missing_required_evidence_and_cleans_up(self):
        for mode, marker in (("missing_mvn", "MVN_ON_PATH=yes"), ("missing_home", "HOME_WRITABLE=yes")):
            with self.subTest(mode=mode):
                result, calls = self.run_canary(mode=mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("omitted required " + marker, result.stderr)
                self.assertTrue(any(call["path"].endswith("/delete") for call in calls))

    def test_chat_selects_permission_mode_only_for_its_disposable_clone(self):
        """Launch unattended chat without changing standing runtime config."""
        result, calls = self.run_canary()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        starts = [call for call in calls if call["path"].endswith("/sessions/start")]
        self.assertEqual(len(starts), 1)
        self.assertEqual(starts[0]["path"], f"/api/v1/workspaces/{CLONE}/sessions/start")
        self.assertEqual(starts[0]["payload"]["agentProvider"], "claude_code")
        self.assertEqual(starts[0]["payload"]["permissionMode"], "bypassPermissions")
        self.assertNotIn("agentLaunch", starts[0]["payload"])
        self.assertFalse(any(call["path"].endswith("/config") and call["method"] != "GET"
                             for call in calls))
        fixture_writes = [call["path"] for call in calls
                          if call["method"] != "GET" and f"/workspaces/{FIXTURE}" in call["path"]]
        self.assertEqual(fixture_writes, [f"/api/v1/workspaces/{FIXTURE}/clone"])

    def test_chat_start_rejection_still_cleans_up_clone_and_host(self):
        """A launch-validation 422 fails once and tears down both disposable resources."""
        result, calls = self.run_canary(mode="chat_start_rejected")
        self.assertEqual(result.returncode, 22)
        self.assertEqual(sum(call["path"].endswith("/sessions/start") for call in calls), 1)
        self.assertFalse(any(call["path"].startswith(f"/api/v1/sessions/{SESSION}") for call in calls))
        cleanup = [call for call in calls if call["path"].endswith("/delete")]
        self.assertEqual(len(cleanup), 1)
        self.assertEqual(cleanup[0]["path"], f"/api/v1/workspaces/{CLONE}/delete")
        self.assertEqual(cleanup[0]["payload"], {"operationId": CLONE, "force": True})
        host_cleanup = [call for call in calls if call["method"] == "DELETE"]
        self.assertEqual(len(host_cleanup), 1)
        self.assertTrue(host_cleanup[0]["path"].endswith("/worker-hosts/" + HOST))
        self.assertFalse(any(call["path"].endswith("/config") and call["method"] != "GET"
                             for call in calls))

    def test_cleanup_retry_is_bounded_and_other_conflicts_do_not_retry(self):
        for mode, expected in (("cleanup_stuck", 4), ("cleanup_conflict", 2)):
            with self.subTest(mode=mode):
                result, calls = self.run_canary(mode=mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("could not be deleted", result.stderr)
                # Main cleanup fails, then the EXIT trap makes its bounded attempt.
                self.assertEqual(sum(call["path"].endswith("/delete") for call in calls), expected)

    def test_preview_fetches_nonce_from_preview_host_with_bearer(self):
        result, calls = self.run_canary(stage="preview")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        profiles = [call["path"] for call in calls if call["path"].endswith("/preview-profiles")]
        self.assertEqual(profiles, [f"/api/v1/workspaces/{FIXTURE}/repos/{REPO}/preview-profiles"])
        nonces = [call for call in calls if call["path"].endswith("/nonce.txt")]
        self.assertEqual(len(nonces), 5)
        for call in nonces:
            self.assertEqual(call["host"], f"{TASK}.{PREVIEW_SUFFIX}")
            self.assertEqual(call["path"], "/nonce.txt")
            self.assertTrue(call["authorization"])
        self.assertFalse(any(call["path"].startswith("/api/v1/preview/") for call in calls))
        self.assertIn("preview proxy stable nonce", result.stdout)
        self.assertTrue(any(call["path"].endswith("/delete") for call in calls))

    def test_preview_proxy_failure_is_fatal_and_still_cleans_up(self):
        result, calls = self.run_canary(stage="preview", mode="preview_denied")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("403", result.stderr)
        self.assertIn("FORBIDDEN", result.stderr)
        self.assertIn("tasks:read", result.stderr)
        self.assertNotIn("empty nonce", result.stderr)
        self.assertNotIn("RAW_RESPONSE_MUST_NOT_LEAK", result.stderr)
        self.assertTrue(any(call["path"].endswith("/preview/cancel") for call in calls))
        self.assertTrue(any(call["path"].endswith("/delete") for call in calls))

    def test_preview_non_json_errors_and_redirects_report_only_status(self):
        for mode, status in (("preview_html_error", "502"), ("preview_redirect", "302")):
            with self.subTest(mode=mode):
                result, calls = self.run_canary(stage="preview", mode=mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(status, result.stderr)
                self.assertNotIn("empty nonce", result.stderr)
                self.assertNotIn("RAW_RESPONSE_MUST_NOT_LEAK", result.stderr)
                self.assertTrue(any(call["path"].endswith("/preview/cancel") for call in calls))
                self.assertTrue(any(call["path"].endswith("/delete") for call in calls))

    def test_preview_requires_bare_hostname_before_provisioning(self):
        for suffix in (None, "", "https://preview.example", "preview.example/path",
                       "preview.example:443", "user@preview.example", "preview.example?query",
                       "preview.example#fragment", "*.preview.example", "preview..example",
                       "-preview.example", "preview-.example", "preview_example", "preview.example.",
                       " preview.example", "preview.example\n", "a" * 64 + ".example",
                       ".".join(["a" * 63] * 4)):
            with self.subTest(suffix=suffix):
                result, calls = self.run_canary(stage="preview", preview_suffix=suffix)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("G5_CANARY_PREVIEW_HOST_SUFFIX", result.stderr)
                self.assertFalse(any(call["method"] == "POST" and call["path"].endswith("/worker-hosts")
                                     for call in calls))


if __name__ == "__main__":
    unittest.main()
