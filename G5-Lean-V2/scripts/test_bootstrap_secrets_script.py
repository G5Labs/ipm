"""Run the operator seeder with local keys and stubbed cloud calls only."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from scripts.test_github_app_secrets_script import bashes

SCRIPT = Path(__file__).with_name("bootstrap-secrets.sh")
STUB = '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
state = Path(os.environ["STUB_DIR"])
args = sys.argv[1:]
with (state / "calls").open("a") as log:
    log.write(json.dumps(args) + "\\n")
if args == ["config", "get-value", "project"]:
    print(os.environ.get("STUB_PROJECT", "g5-dev-1ccc"))
elif args[:2] == ["secrets", "describe"]:
    pass
elif args[:3] == ["secrets", "versions", "list"]:
    count = json.loads(os.environ.get("STUB_VERSIONS", "{}")).get(args[3], 0)
    for i in range(count): print(i + 1)
elif args[:3] == ["secrets", "versions", "add"]:
    (state / args[3]).write_bytes(sys.stdin.buffer.read())
else:
    sys.exit("unexpected stub call")
'''


@unittest.skipUnless(shutil.which("openssl"), "needs openssl")
class BootstrapSecretsScriptTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        stub = self.bin / "gcloud"
        stub.write_text(STUB)
        stub.chmod(0o755)
        self.env = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}",
                    "STUB_DIR": str(self.root), "STUB_PROJECT": "g5-dev-1ccc", "STUB_VERSIONS": "{}",
                    "G5_GITHUB_CLIENT_ID": "fake-client", "G5_GITHUB_CLIENT_SECRET": "fake-oauth-secret",
                    "G5_LICENSE_ADMIN_TOKEN": "fake-license-secret", "ANTHROPIC_API_KEY": "fake-anthropic-secret",
                    "OPENAI_API_KEY": "fake-openai-secret", "OPENROUTER_API_KEY": "fake-openrouter-secret"}

    def invoke(self, bash, *args, trace=False):
        for item in self.root.glob("g5-*"):
            item.unlink()
        (self.root / "calls").unlink(missing_ok=True)
        return subprocess.run([bash, *( ["-x"] if trace else []), str(SCRIPT), *args],
            env=self.env, input="", text=True, capture_output=True, timeout=20)

    def calls(self):
        path = self.root / "calls"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def writes(self):
        return [call for call in self.calls() if call[:3] == ["secrets", "versions", "add"]]

    def test_release_targets_select_secrets_callback_and_no_cluster_actions(self):
        for bash in bashes():
            for environment, url in (("staging", "https://lean.g5labs.ai"), ("prod", "https://app.g5labs.ai")):
                project = f"g5lean-{environment}-1ccc"
                with self.subTest(bash=bash, environment=environment):
                    result = self.invoke(bash, "--project", project, "--external-url", url, trace=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(len(self.writes()), 10)
                    self.assertTrue(all(f"--project={project}" in call for call in self.calls()))
                    self.assertTrue(all(call[0] == "secrets" for call in self.calls()))
                    self.assertTrue((self.root / f"g5-license-admin-token-{environment}").exists())
                    self.assertFalse((self.root / "g5-license-admin-token").exists())
                    output = result.stdout + result.stderr
                    self.assertIn(f"{url}/api/v1/auth/callback/github", output)
                    self.assertIn("does not sync or restart", output)
                    for name in ("G5_GITHUB_CLIENT_SECRET", "G5_LICENSE_ADMIN_TOKEN", "ANTHROPIC_API_KEY", "OPENAI_API_KEY", "OPENROUTER_API_KEY"):
                        self.assertNotIn(self.env[name], output)
                        self.assertNotIn(self.env[name], json.dumps(self.calls()))
                    self.assertNotIn("BEGIN PRIVATE KEY", output)
                    self.assertTrue(all("--data-file=-" in call for call in self.writes()))

    def test_legacy_dev_qa_and_replacement_origin_defaults(self):
        for project, url, flags in (
            ("g5-dev-1ccc", "https://dev.g5labs.ai", ()),
            ("g5-qa-1ccc", "https://qa.g5labs.ai", ("--project", "g5-qa-1ccc")),
            ("legacy-custom-project", "https://custom.example.com", ("--project", "legacy-custom-project", "--external-url", "https://custom.example.com")),
            ("g5lean-prod-1ccc", "https://replacement.example.com", ("--project", "g5lean-prod-1ccc", "--external-url", "https://replacement.example.com/")),
        ):
            with self.subTest(project=project):
                result = self.invoke("/bin/bash", *flags)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f"{url}/api/v1/auth/callback/github", result.stdout)
                self.assertTrue(all(f"--project={project}" in call for call in self.writes()))

    def test_invalid_explicit_target_or_url_fails_before_cloud_calls(self):
        base = ("--project", "g5lean-staging-1ccc")
        cases = [base, ("--project", "bad project"), ("--external-url", "http://bad.example.com"),
                 ("--project", "g5lean-admin-1ccc", "--external-url", "https://artifacts.example.com"),
                 (*base, "--external-url", "https://lean.g5labs.ai", "--license-admin-secret-id", "g5-license-admin-token-prod")]
        for url in ("https://app.g5labs.ai", "https://qa.g5labs.ai", "https://g5labs.ai", "https://dev.g5labs.ai",
                    "https://person:secret@example.com", "https://example.com/path", "https://example.com?query=1"):
            cases.append((*base, "--external-url", url))
        for bash in bashes():
            for args in cases:
                with self.subTest(bash=bash, args=args):
                    result = self.invoke(bash, *args)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(self.calls(), [])

    def test_release_project_cannot_be_inherited_from_gcloud_configuration(self):
        self.env["STUB_PROJECT"] = "g5lean-prod-1ccc"
        result = self.invoke("/bin/bash", "--external-url", "https://app.g5labs.ai")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [["config", "get-value", "project"]])

    def test_partial_jwt_pair_is_refused_without_any_versions_or_payload_reads(self):
        for key in ("g5-jwt-secret", "g5-jwt-public-key"):
            self.env["STUB_VERSIONS"] = json.dumps({key: 1})
            result = self.invoke("/bin/bash", "--project", "g5lean-prod-1ccc", "--external-url", "https://app.g5labs.ai")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("mismatched pair", result.stderr)
            self.assertEqual(self.writes(), [])
            self.assertTrue(all(call[:2] == ["secrets", "describe"] or call[:3] == ["secrets", "versions", "list"] for call in self.calls()))

    def test_existing_keypair_is_not_rotated_on_normal_rerun(self):
        self.env["STUB_VERSIONS"] = json.dumps({"g5-jwt-secret": 1, "g5-jwt-public-key": 1})
        result = self.invoke("/bin/bash", "--project", "g5-qa-1ccc")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(any(call[3] in {"g5-jwt-secret", "g5-jwt-public-key"} for call in self.writes()))


if __name__ == "__main__":
    unittest.main()
