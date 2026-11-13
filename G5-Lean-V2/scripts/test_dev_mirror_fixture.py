"""Unit checks for CI fixture ownership/lifecycle, not substitutes for S-32 E2E."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
FIXTURE_ID = "a" * 64
OWNER = "unit-build"


class DevMirrorFixtureTest(unittest.TestCase):
    def run_fixture(self, *, mode="", test_status=0, exists=False,
                    owner=OWNER, cleanup=False):
        with tempfile.TemporaryDirectory(prefix="dev-mirror-unit-") as directory:
            scratch = Path(directory)
            state = scratch / "state.json"
            record = scratch / "commands.jsonl"
            state.write_text(json.dumps({"exists": exists, "owner": owner}))
            docker = scratch / "docker"
            docker.write_text(
                f"#!{sys.executable}\n"
                "import json, os, pathlib, sys\n"
                "args = sys.argv[1:]\n"
                "with open(os.environ['RECORD'], 'a') as output:\n"
                "    output.write(json.dumps(args) + '\\n')\n"
                "state = pathlib.Path(os.environ['STATE'])\n"
                "data = json.loads(state.read_text())\n"
                "mode = os.environ['MODE']\n"
                "if args[0] == 'create':\n"
                "    if data['exists']: sys.exit(125)\n"
                "    data['exists'] = True\n"
                "    state.write_text(json.dumps(data))\n"
                "    print('a' * 64)\n"
                "elif args[0] == 'start':\n"
                "    if mode == 'start-failure': sys.exit(31)\n"
                "elif args[0] == 'exec':\n"
                "    if mode == 'readiness-failure': sys.exit(32)\n"
                "elif args[0] == 'inspect':\n"
                "    print('someone-else' if mode == 'owner-mismatch' else data['owner'])\n"
                "elif args[0] == 'ps':\n"
                "    if data['exists'] and ('label=g5.e2e.dev-mirror-owner=' + data['owner']) in args:\n"
                "        print('a' * 64)\n"
                "elif args[0] == 'rm':\n"
                "    if mode == 'cleanup-failure': sys.exit(33)\n"
                "    data['exists'] = False\n"
                "    state.write_text(json.dumps(data))\n"
                "else: sys.exit('unexpected Docker command: ' + repr(args))\n"
            )
            docker.chmod(0o755)
            command = scratch / "scenario"
            command.write_text(
                f"#!{sys.executable}\n"
                "import json, os, signal, sys\n"
                "with open(os.environ['RECORD'], 'a') as output:\n"
                "    output.write(json.dumps(['scenario']) + '\\n')\n"
                "if os.environ['MODE'] == 'interrupt':\n"
                "    os.kill(os.getppid(), signal.SIGTERM)\n"
                f"sys.exit({test_status})\n"
            )
            command.chmod(0o755)
            args = ["--cleanup", OWNER] if cleanup else [
                "candidate-api:test", OWNER, "--", str(command),
            ]
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/with-dev-mirror-fixture.sh"), *args],
                env={"PATH": f"{scratch}{os.pathsep}{os.defpath}",
                     "STATE": str(state), "RECORD": str(record), "MODE": mode},
                text=True, capture_output=True, timeout=15, check=False,
            )
            calls = [json.loads(line) for line in record.read_text().splitlines()]
            return result, calls, json.loads(state.read_text())

    def test_success_runs_after_readiness_and_cleans_exact_owned_id(self):
        result, calls, state = self.run_fixture()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call[0] for call in calls],
                         ["create", "start", "exec", "scenario", "inspect", "rm"])
        create = calls[0]
        for required in ["--network=none", "--no-healthcheck", "--user=0:0",
                         "--name=g5-dev-api", "--entrypoint=/bin/sh",
                         f"--label=g5.e2e.dev-mirror-owner={OWNER}", "candidate-api:test"]:
            self.assertIn(required, create)
        self.assertFalse(any(arg.startswith(("--volume", "--mount", "--env", "--publish"))
                             for arg in create))
        self.assertIn("git --version", calls[2][-1])
        self.assertIn("test -w /data/repos", calls[2][-1])
        self.assertEqual(calls[-1], ["rm", "-f", FIXTURE_ID])
        self.assertFalse(state["exists"])

    def test_scenario_failure_keeps_status_and_cleans(self):
        result, calls, state = self.run_fixture(test_status=17)
        self.assertEqual(result.returncode, 17)
        self.assertEqual(calls[-1], ["rm", "-f", FIXTURE_ID])
        self.assertFalse(state["exists"])

    def test_existing_container_is_never_replaced_or_removed(self):
        result, calls, state = self.run_fixture(exists=True)
        self.assertEqual(result.returncode, 125)
        self.assertEqual([call[0] for call in calls], ["create"])
        self.assertTrue(state["exists"])

    def test_start_and_readiness_failures_do_not_run_scenario(self):
        for mode, status in [("start-failure", 31), ("readiness-failure", 32)]:
            with self.subTest(mode=mode):
                result, calls, state = self.run_fixture(mode=mode)
                self.assertEqual(result.returncode, status)
                self.assertNotIn(["scenario"], calls)
                self.assertFalse(state["exists"])

    def test_cleanup_failure_fails_green_but_preserves_original_failure(self):
        for status in [0, 17]:
            result, _, _ = self.run_fixture(mode="cleanup-failure", test_status=status)
            self.assertEqual(result.returncode, status or 1)

    def test_owner_mismatch_refuses_removal_and_fails(self):
        result, calls, state = self.run_fixture(mode="owner-mismatch")
        self.assertEqual(result.returncode, 1)
        self.assertNotIn("rm", [call[0] for call in calls])
        self.assertTrue(state["exists"])

    def test_interruption_cleans_and_preserves_signal_status(self):
        result, calls, state = self.run_fixture(mode="interrupt")
        self.assertEqual(result.returncode, 143)
        self.assertEqual(calls[-1], ["rm", "-f", FIXTURE_ID])
        self.assertFalse(state["exists"])

    def test_teardown_backstop_removes_only_matching_build(self):
        for exists, owner in [(False, OWNER), (True, "other-build"), (True, OWNER)]:
            with self.subTest(exists=exists, owner=owner):
                result, calls, state = self.run_fixture(
                    exists=exists, owner=owner, cleanup=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                removed = exists and owner == OWNER
                self.assertEqual("rm" in [call[0] for call in calls], removed)
                self.assertEqual(state["exists"], exists and not removed)

    def test_ci_wraps_runner_and_owns_cleanup_before_final_gate(self):
        config = (ROOT / "infra/ci/cloudbuild-ci-e2e.yaml").read_text()
        rust = config.split("  - id: rust-e2e\n", 1)[1].split("\n  - id:", 1)[0]
        self.assertIn("with-dev-mirror-fixture.sh", rust)
        self.assertIn('${_CI_REPOSITORY}/api:$SHORT_SHA "$BUILD_ID" --', rust)
        self.assertEqual(rust.count("./scripts/run-rust-e2e-lanes.sh"), 1)
        self.assertLess(rust.index("with-dev-mirror-fixture.sh"),
                        rust.index("./scripts/run-rust-e2e-lanes.sh"))
        teardown = config.split("  - id: stop-e2e-stack\n", 1)[1].split("\n  - id:", 1)[0]
        self.assertIn('with-dev-mirror-fixture.sh --cleanup "$BUILD_ID"', teardown)
        self.assertIn("scripts.test_dev_mirror_fixture", config)


if __name__ == "__main__":
    unittest.main()
