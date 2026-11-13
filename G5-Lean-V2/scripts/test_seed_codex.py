#!/usr/bin/env python3
"""Execute the seed script's printed explainer command with a recording curl."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class SeedExplainerCommandTests(unittest.TestCase):
    def test_printed_command_submits_the_explainer_request(self):
        self.check_printed_command(instruction=True)

    def test_missing_instruction_uses_general_architecture_prompt(self):
        self.check_printed_command(instruction=False)

    def check_printed_command(self, instruction):
        source = (ROOT / "scripts/seed-codex.sh").read_text()
        # Exercise only the manual next-step output, without provisioning a
        # workspace, accessing credentials, or contacting an external service.
        step = source[source.index("# ─── Step 10:"):source.index("# ─── Summary")]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            if instruction:
                (root / "docs").mkdir()
                (root / "docs/tool_system_refactor.md").write_text("Refactor the tools.\n")
            curl = root / "curl"
            curl.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['SEED_TEST_CALL'], 'w') as output:\n"
                "    json.dump(sys.argv[1:], output)\n"
            )
            curl.chmod(0o755)
            call = root / "call.json"
            env = {
                "PATH": f"{root}:/usr/bin:/bin",
                "API_URL": "http://seed.example.test:3000",
                "WS_ID": "10000000-0000-4000-8000-000000000001",
                "TOKEN": "local-test-token",
                "WORKSPACE_DIR": str(root),
                "SEED_TEST_CALL": str(call),
            }
            printed = subprocess.run(
                ["bash", "-eu", "-c", step], env=env,
                text=True, capture_output=True, check=True, timeout=10,
            ).stdout
            command = "curl -X POST" + printed.split("curl -X POST", 1)[1].split("\n\n", 1)[0]
            subprocess.run(
                ["bash", "-eu", "-c", command], env=env,
                text=True, capture_output=True, check=True, timeout=10,
            )
            args = json.loads(call.read_text())
            self.assertEqual(args[:3], [
                "-X", "POST", f"{env['API_URL']}/api/v1/workspaces/{env['WS_ID']}/sessions/start",
            ])
            headers = [args[i + 1] for i, arg in enumerate(args) if arg == "-H"]
            self.assertIn("Content-Type: application/json", headers)
            self.assertIn(f"Authorization: Bearer {env['TOKEN']}", headers)
            payload = json.loads(args[args.index("-d") + 1])
            self.assertEqual(set(payload), {"title", "content"})
            self.assertTrue(payload["title"].strip())
            if instruction:
                self.assertIn("docs/tool_system_refactor.md", payload["content"])
            else:
                self.assertNotIn("tool_system_refactor.md", payload["content"])
                self.assertIn("tool dispatch", payload["content"])
            self.assertIn("self-contained HTML page", payload["content"])
            self.assertIn(f"docs/{env['WS_ID']}/explainers/", payload["content"])
            self.assertIn("Explainer creation", printed)
            self.assertNotIn("arch_view", printed)


if __name__ == "__main__":
    unittest.main()
