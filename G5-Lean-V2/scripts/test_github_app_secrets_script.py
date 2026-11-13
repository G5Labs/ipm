"""Behavior tests for scripts/github-app-secrets.sh.

The script is run end to end against stub `gcloud`, `kubectl` and `curl`
executables that record every call and payload and never contact Google or a
cluster. Each case runs under every bash on the machine (macOS ships 3.2).

Run with: python3 -m unittest scripts.test_github_app_secrets_script
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import textwrap
import unittest
from base64 import b64encode
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "github-app-secrets.sh"
CONTAINERS = [
    "g5-github-app-id",
    "g5-github-app-slug",
    "g5-github-app-private-key",
    "g5-github-app-oauth-client-id",
    "g5-github-app-oauth-client-secret",
    "g5-github-app-oauth-callback-url",
    "g5-github-app-connection-state-secret",
]
SETTINGS = [
    "G5_GITHUB_APP_ID",
    "G5_GITHUB_APP_SLUG",
    "G5_GITHUB_APP_PRIVATE_KEY",
    "G5_GITHUB_APP_OAUTH_CLIENT_ID",
    "G5_GITHUB_APP_OAUTH_CLIENT_SECRET",
    "G5_GITHUB_APP_OAUTH_CALLBACK_URL",
    "G5_GITHUB_APP_CONNECTION_STATE_SECRET",
]
CALLBACK = "https://qa.g5labs.ai/api/v1/github-app/oauth/callback"
CLIENT_SECRET = "s3cret-client-value"

# One stub program dispatching on the name it is invoked as. State lives in
# $STUB_DIR: `missing` / `fail_add` list containers, `added/` holds payloads,
# `es.json` the ExternalSecret status, `secret.json` the synced Secret.
STUB = textwrap.dedent(
    r'''
    #!/usr/bin/env python3
    import base64, json, os, sys
    from pathlib import Path
    state = Path(os.environ["STUB_DIR"])
    tool, args = Path(sys.argv[0]).name, sys.argv[1:]
    with open(state / "calls.log", "a") as log:
        log.write(json.dumps([tool] + args) + "\n")
    def listed(name, secret_id):
        path = state / name
        return path.exists() and secret_id in path.read_text().split()
    if tool == "gcloud":
        if args[:2] == ["secrets", "describe"]:
            sys.exit(1 if listed("missing", args[2]) else 0)
        if args[:3] == ["secrets", "versions", "add"]:
            if listed("fail_add", args[3]):
                sys.exit(1)
            added = state / "added"
            added.mkdir(exist_ok=True)
            (added / args[3]).write_bytes(sys.stdin.buffer.read())
            print(f"projects/p/secrets/{args[3]}/versions/2")
            sys.exit(0)
        if args[:3] == ["container", "clusters", "get-credentials"]:
            sys.exit(0)
    if tool == "kubectl":
        es = json.loads((state / "es.json").read_text())
        if "annotate" in args:
            # External Secrets copies the latest versions into the Secret.
            added = state / "added"
            if added.exists():
                secret = json.loads((state / "secret.json").read_text())
                for item in added.iterdir():
                    setting = "G5_" + item.name[3:].upper().replace("-", "_")
                    secret["data"][setting] = base64.b64encode(item.read_bytes()).decode()
                (state / "secret.json").write_text(json.dumps(secret))
            es["refreshTime"] = es["nextRefreshTime"]
            es["ready"] = es["nextReady"]
            (state / "es.json").write_text(json.dumps(es))
            sys.exit(0)
        if "externalsecret" in args:
            if not es.get("exists", True):
                sys.exit(1)
            path = [a for a in args if a.startswith("-o")][0]
            path = args[args.index("-o") + 1] if path == "-o" else path
            if "refreshTime" in path and "Ready" in path:
                print(f'{es["refreshTime"]} {es["ready"]}', end="")
            elif "refreshTime" in path:
                print(es["refreshTime"], end="")
            elif "message" in path:
                print("stub not ready", end="")
            elif "Ready" in path:
                print(es["ready"], end="")
            else:
                print("g5-github-app-secrets", end="")
            sys.exit(0)
        if "secret" in args:
            print((state / "secret.json").read_text())
            sys.exit(0)
        if "rollout" in args:
            sys.exit(0)
    if tool == "curl":
        print((state / "instance.json").read_text())
        sys.exit(0)
    sys.exit(f"stub: unhandled {tool} {args}")
    '''
).lstrip()


def bashes() -> list[str]:
    found = []
    for candidate in ("/bin/bash", shutil.which("bash")):
        if candidate and os.path.exists(candidate) and candidate not in found:
            found.append(candidate)
    return found


@unittest.skipUnless(shutil.which("jq") and shutil.which("openssl"), "needs jq and openssl")
class GithubAppSecretsScriptTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        bin_dir = self.tmp / "bin"
        bin_dir.mkdir()
        stub = bin_dir / "stub"
        stub.write_text(STUB)
        stub.chmod(0o755)
        for tool in ("gcloud", "kubectl", "curl"):
            (bin_dir / tool).symlink_to(stub)
        self.state = self.tmp / "state"
        self.state.mkdir()
        self.pem = self.tmp / "app.pem"
        subprocess.run(
            ["openssl", "genrsa", "-traditional", "-out", str(self.pem), "2048"],
            check=True,
            capture_output=True,
        )
        self.env = {
            **os.environ,
            "PATH": f"{bin_dir}:{os.environ['PATH']}",
            "STUB_DIR": str(self.state),
            "SYNC_WAIT_SECONDS": "4",
        }
        self.set_es(refresh="t1", ready="True", next_refresh="t2", next_ready="True")
        self.set_secret({name: "value" for name in SETTINGS})
        (self.state / "instance.json").write_text(
            json.dumps({"data": {"features": ["chat", "github_app", "external_issues"]}})
        )

    def set_es(self, refresh, ready, next_refresh, next_ready, exists=True):
        (self.state / "es.json").write_text(
            json.dumps(
                {
                    "refreshTime": refresh,
                    "ready": ready,
                    "nextRefreshTime": next_refresh,
                    "nextReady": next_ready,
                    "exists": exists,
                }
            )
        )

    def set_secret(self, values):
        data = {k: b64encode(v.encode()).decode() for k, v in values.items()}
        (self.state / "secret.json").write_text(json.dumps({"data": data}))

    def run_script(self, bash, *args, stdin="", reset=True):
        if reset:
            self.reset_state()
        return subprocess.run(
            [bash, str(SCRIPT), *args],
            input=stdin,
            capture_output=True,
            text=True,
            env=self.env,
            timeout=60,
        )

    def reset_state(self):
        shutil.rmtree(self.state / "added", ignore_errors=True)
        (self.state / "calls.log").unlink(missing_ok=True)

    def added(self):
        added = self.state / "added"
        return {p.name: p.read_bytes() for p in added.iterdir()} if added.exists() else {}

    def calls(self):
        log = self.state / "calls.log"
        return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []

    def seed_input(self, app_id="123456", slug="lean-app", client_id="Iv1.abc", confirm="yes"):
        return f"{app_id}\n{slug}\n{client_id}\n{CLIENT_SECRET}\n{confirm}\n"

    def test_seed_writes_exact_values_on_stdin_only(self):
        for bash in bashes():
            with self.subTest(bash=bash):
                result = self.run_script(bash, "seed", str(self.pem), stdin=self.seed_input())
                self.assertEqual(result.returncode, 0, result.stderr)
                added = self.added()
                self.assertEqual(sorted(added), sorted(CONTAINERS))
                self.assertEqual(added["g5-github-app-id"], b"123456")
                self.assertEqual(added["g5-github-app-slug"], b"lean-app")
                self.assertEqual(added["g5-github-app-private-key"], self.pem.read_bytes())
                self.assertEqual(added["g5-github-app-oauth-client-id"], b"Iv1.abc")
                self.assertEqual(added["g5-github-app-oauth-client-secret"], CLIENT_SECRET.encode())
                self.assertEqual(added["g5-github-app-oauth-callback-url"], CALLBACK.encode())
                state_secret = added["g5-github-app-connection-state-secret"]
                self.assertRegex(state_secret.decode(), r"^[0-9a-f]{64}$")
                # Only the project's containers are written, never a value on argv.
                for call in self.calls():
                    self.assertNotIn(CLIENT_SECRET, " ".join(call))
                    self.assertNotIn(state_secret.decode(), " ".join(call))
                    if call[:4] == ["gcloud", "secrets", "versions", "add"]:
                        self.assertIn("--project=g5-qa-1ccc", call)
                        self.assertIn("--data-file=-", call)
                self.assertNotIn(CLIENT_SECRET, result.stdout + result.stderr)

    def test_release_seed_and_restart_keep_project_cluster_and_callback_together(self):
        for bash in bashes():
            for environment, origin in (("staging", "https://lean.g5labs.ai"),
                                        ("prod", "https://app.g5labs.ai")):
                project = f"g5lean-{environment}-1ccc"
                target = ("--project", project, "--external-url", origin)
                with self.subTest(bash=bash, environment=environment):
                    result = self.run_script(bash, *target, "seed", str(self.pem), stdin=self.seed_input())
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(self.added()["g5-github-app-oauth-callback-url"],
                                     f"{origin}/api/v1/github-app/oauth/callback".encode())
                    self.assertTrue(all(f"--project={project}" in call for call in self.calls()))
                    self.assertIn(f"--project {project}", result.stderr)
                    self.assertIn(f"--external-url {origin}", result.stderr)
                    result = self.run_script(bash, *target, "restart", stdin="yes\n")
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn(["gcloud", "container", "clusters", "get-credentials", f"g5-lean-{environment}",
                                   "--location=us-central1", f"--project={project}"], self.calls())
                    self.assertIn(["curl", "-fsS", f"{origin}/api/v1/instance/info"], self.calls())

    def test_explicit_replacement_hostname_is_supported_without_trace_leaks(self):
        for bash in bashes():
            with self.subTest(bash=bash):
                result = subprocess.run([bash, "-x", str(SCRIPT), "--project", "g5lean-prod-1ccc",
                    "--external-url", "https://replacement.example.com", "seed", str(self.pem)],
                    input=self.seed_input(), capture_output=True, text=True, env=self.env, timeout=60)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertNotIn(CLIENT_SECRET, result.stdout + result.stderr)
                self.assertNotIn("BEGIN RSA PRIVATE KEY", result.stdout + result.stderr)
                self.assertEqual(self.added()["g5-github-app-oauth-callback-url"],
                                 b"https://replacement.example.com/api/v1/github-app/oauth/callback")

    def test_bad_release_targets_fail_before_any_external_calls(self):
        valid = ("--project", "g5lean-staging-1ccc", "--external-url", "https://lean.g5labs.ai")
        cases = [
            ("--project", "g5lean-staging-1ccc"),
            ("--project", "g5lean-prod-1ccc", "--external-url", "https://qa.g5labs.ai"),
            ("--project", "g5lean-prod-1ccc", "--external-url", "https://lean.g5labs.ai"),
            (*valid, "--cluster", "g5-lean-prod"), (*valid, "--region", "us-east1"),
            ("--project", "not a project", "--external-url", "https://safe.example.com"),
            ("--project", "g5lean-admin-1ccc", "--external-url", "https://safe.example.com"),
        ]
        for url in ("https://g5labs.ai", "http://lean.g5labs.ai", "https://app.g5labs.ai",
                    "https://user:password@example.com", "https://example.com/path", "https://example.com?x=y",
                    "https://example.com#frag", "https://example.com:443", "https://example.com\n"):
            cases.append(("--project", "g5lean-staging-1ccc", "--external-url", url))
        for bash in bashes():
            for flags in cases:
                with self.subTest(bash=bash, flags=flags):
                    result = self.run_script(bash, *flags, "rollback", "--dry-run")
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(self.calls(), [])
            result = self.run_script(bash, "sync", "--dry-run")
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(self.calls(), [])

    def test_seed_dry_run_and_declined_confirmation_write_nothing(self):
        for bash in bashes():
            with self.subTest(bash=bash, case="dry run"):
                result = self.run_script(
                    bash, "seed", "--dry-run", str(self.pem), stdin=self.seed_input()
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.added(), {})
            with self.subTest(bash=bash, case="declined"):
                result = self.run_script(
                    bash, "seed", str(self.pem), stdin=self.seed_input(confirm="y")
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.added(), {})

    def test_seed_accepts_every_app_id_g5_api_parses(self):
        for bash in bashes():
            for app_id in ("1", "000123", "18446744073709551615"):
                with self.subTest(bash=bash, app_id=app_id):
                    result = self.run_script(
                        bash, "seed", "--dry-run", str(self.pem),
                        stdin=self.seed_input(app_id=app_id),
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)

    def test_seed_refuses_values_g5_api_would_reject_before_writing(self):
        cases = {
            "non-numeric id": self.seed_input(app_id="lean-app"),
            "zero id": self.seed_input(app_id="0"),
            "zeros id": self.seed_input(app_id="000"),
            "id above u64": self.seed_input(app_id="18446744073709551616"),
            "id far above u64": self.seed_input(app_id="99999999999999999999"),
            "21-digit id": self.seed_input(app_id="100000000000000000000"),
            "bad slug": self.seed_input(slug="lean app"),
            "empty client id": self.seed_input(client_id=" "),
        }
        for bash in bashes():
            for case, stdin in cases.items():
                with self.subTest(bash=bash, case=case):
                    result = self.run_script(bash, "seed", str(self.pem), stdin=stdin)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(self.added(), {})
            with self.subTest(bash=bash, case="not a key"):
                bad = self.tmp / "bad.pem"
                bad.write_text("not a key\n")
                result = self.run_script(bash, "seed", str(bad), stdin=self.seed_input())
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.added(), {})

    def test_seed_refuses_when_a_container_is_missing(self):
        (self.state / "missing").write_text("g5-github-app-slug\n")
        for bash in bashes():
            with self.subTest(bash=bash):
                result = self.run_script(bash, "seed", str(self.pem), stdin=self.seed_input())
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("g5-github-app-slug does not exist", result.stderr)
                self.assertEqual(self.added(), {})

    def test_a_failed_write_stops_and_names_what_was_added(self):
        (self.state / "fail_add").write_text("g5-github-app-private-key\n")
        for bash in bashes():
            with self.subTest(bash=bash):
                result = self.run_script(bash, "seed", str(self.pem), stdin=self.seed_input())
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(sorted(self.added()), ["g5-github-app-id", "g5-github-app-slug"])
                self.assertIn("Added so far: g5-github-app-id g5-github-app-slug", result.stderr)

    def test_rollback_blanks_all_seven(self):
        for bash in bashes():
            with self.subTest(bash=bash):
                result = self.run_script(bash, "rollback", stdin="yes\n")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.added(), {name: b" " for name in CONTAINERS})
                result = self.run_script(bash, "rollback", "--dry-run")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.added(), {})

    def test_sync_waits_for_a_new_successful_refresh_and_checks_every_setting(self):
        for bash in bashes():
            with self.subTest(bash=bash, case="synced"):
                self.set_es("t1", "True", "t2", "True")
                result = self.run_script(bash, "sync")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("All seven set", result.stderr)
                self.assertIn("G5_GITHUB_APP_SLUG 5", result.stdout)
                self.assertNotIn("value", result.stdout)
            with self.subTest(bash=bash, case="refresh failed"):
                self.set_es("t1", "True", "t2", "False")
                result = self.run_script(bash, "sync")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("no successful refresh", result.stderr)
            with self.subTest(bash=bash, case="no new refresh"):
                self.set_es("t1", "True", "t1", "True")
                result = self.run_script(bash, "sync")
                self.assertNotEqual(result.returncode, 0)
            with self.subTest(bash=bash, case="missing ExternalSecret"):
                self.set_es("t1", "True", "t2", "True", exists=False)
                result = self.run_script(bash, "sync")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("not found", result.stderr)

    def test_rollback_then_sync_then_restart_turns_the_app_off(self):
        for bash in bashes():
            with self.subTest(bash=bash):
                self.reset_state()
                self.set_secret({name: "value" for name in SETTINGS})
                self.set_es("t1", "True", "t2", "True")
                result = self.run_script(bash, "rollback", stdin="yes\n", reset=False)
                self.assertEqual(result.returncode, 0, result.stderr)
                result = self.run_script(bash, "sync", reset=False)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("All seven blank", result.stderr)
                (self.state / "instance.json").write_text(
                    json.dumps({"data": {"features": ["chat", "github_app"]}})
                )
                result = self.run_script(bash, "restart", stdin="yes\n", reset=False)
                self.assertNotEqual(result.returncode, 0, "the App must be off after restart")
                self.assertIn("still offered", result.stderr)
                (self.state / "instance.json").write_text(
                    json.dumps({"data": {"features": ["chat"]}})
                )
                result = self.run_script(bash, "restart", stdin="yes\n", reset=False)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("The G5 GitHub App is off", result.stderr)
                self.assertTrue(any("rollout" in call for call in self.calls()))

    def test_restart_fails_without_a_feature_list(self):
        blank = {name: " " for name in SETTINGS}
        full = {name: "value" for name in SETTINGS}
        for bash in bashes():
            for state, values in {"unconfigured": blank, "configured": full}.items():
                for body in ({"data": {}}, {"data": {"features": None}}, {"data": {"features": {}}}):
                    with self.subTest(bash=bash, state=state, body=body):
                        self.set_secret(values)
                        (self.state / "instance.json").write_text(json.dumps(body))
                        result = self.run_script(bash, "restart", stdin="yes\n")
                        self.assertNotEqual(result.returncode, 0)
                        self.assertIn("no feature list", result.stderr)
                        self.assertNotIn("The G5 GitHub App is", result.stderr)

    def test_a_missing_or_blank_setting_blocks_the_restart(self):
        blank = {name: "value" for name in SETTINGS} | {"G5_GITHUB_APP_SLUG": " \n"}
        missing = {name: "value" for name in SETTINGS[1:]}
        for bash in bashes():
            for case, values in {"blank": blank, "missing": missing}.items():
                with self.subTest(bash=bash, case=case):
                    self.set_secret(values)
                    result = self.run_script(bash, "restart", stdin="yes\n")
                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(any("rollout" in call for call in self.calls()))

    def test_restart_rolls_both_workloads_and_requires_the_app_feature(self):
        for bash in bashes():
            with self.subTest(bash=bash, case="configured"):
                self.set_secret({name: "value" for name in SETTINGS})
                result = self.run_script(bash, "restart", stdin="yes\n")
                self.assertEqual(result.returncode, 0, result.stderr)
                rollouts = [c for c in self.calls() if c[0] == "kubectl" and "rollout" in c]
                self.assertIn(
                    ["kubectl", "-n", "g5", "rollout", "restart",
                     "deployment/g5-api", "deployment/g5-daemon"],
                    rollouts,
                )
                self.assertEqual(sum("status" in c for c in rollouts), 2)
            with self.subTest(bash=bash, case="declined"):
                result = self.run_script(bash, "restart", stdin="no\n")
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any("rollout" in call for call in self.calls()))
            with self.subTest(bash=bash, case="feature absent"):
                (self.state / "instance.json").write_text(
                    json.dumps({"data": {"features": ["chat"]}})
                )
                result = self.run_script(bash, "restart", stdin="yes\n")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("github_app is not offered", result.stderr)
                (self.state / "instance.json").write_text(
                    json.dumps({"data": {"features": ["github_app"]}})
                )


if __name__ == "__main__":
    unittest.main()
