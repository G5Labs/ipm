"""Exercise receipt integrity using a project-adapter fixture, not real TLC.

These tests qualify the wrapper only. A project's TLC adapter needs its own tests
against real completed, counterexample, parse-error and truncated TLC outputs.
"""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("model_evidence.py")
SPEC = importlib.util.spec_from_file_location("model_evidence", SCRIPT)
evidence = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(evidence)


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        for name in ["Model.tla", "Model.cfg", "source.py", "ontology.md", "binding.md"]:
            (self.root / name).write_text(name)
        self.adapter = self.root / "adapter.py"
        self.adapter.write_text(
            "import json, os\nfrom pathlib import Path\n"
            "print('terminal fixture output')\n"
            "Path(os.environ['G5_MODEL_RESULT']).write_text(json.dumps("
            "{'complete':True,'outcome':'satisfied','properties':['Invariant']}))\n")
        self.data = {"command": [sys.executable, "adapter.py"], "timeout_seconds": 5,
                     "inputs": {"formal": ["Model.tla", "Model.cfg"], "tool": ["adapter.py"],
                                "implementation": ["source.py"], "ontology": ["ontology.md"], "binding": ["binding.md"]},
                     "expected": {"outcome": "satisfied", "properties": ["Invariant"]}}
        self.manifest = self.root / "manifest.json"
        self.save_manifest()
        self.output = self.root / "run-1"

    def save_manifest(self):
        self.manifest.write_text(json.dumps(self.data))

    def run_evidence(self):
        return evidence.run(self.manifest, self.output, self.root)

    def check(self):
        return evidence.check(self.manifest, self.output, self.root)

    def test_complete_and_unchanged_is_valid(self):
        self.assertEqual(self.run_evidence(), [])
        self.assertEqual(self.check(), [])

    def test_changed_inputs_each_require_review(self):
        self.run_evidence()
        for name in ["Model.tla", "Model.cfg", "source.py", "ontology.md", "binding.md", "adapter.py"]:
            with self.subTest(name=name):
                p = self.root / name
                old = p.read_bytes()
                p.write_bytes(old + b"\nchanged")
                self.assertTrue(self.check())
                p.write_bytes(old)

    def test_changed_manifest_expectation_is_rejected(self):
        self.run_evidence()
        self.data["expected"]["properties"] = ["DifferentProperty"]
        self.save_manifest()
        with self.assertRaises(ValueError):
            self.check()

    def test_changed_command_is_rejected(self):
        self.run_evidence()
        self.data["command"].append("new-option")
        self.save_manifest()
        self.assertTrue(self.check())

    def test_expected_counterexample_is_not_a_safety_pass(self):
        self.adapter.write_text(self.adapter.read_text().replace("'satisfied'", "'violated'"))
        self.data["expected"]["outcome"] = "violated"
        self.save_manifest()
        self.assertEqual(self.run_evidence(), [])
        self.assertEqual(self.check(), [])
        self.assertEqual(json.loads((self.output / "receipt.json").read_text())["observed"]["outcome"], "violated")

    def test_wrong_property_cannot_satisfy_negative_control(self):
        self.data["expected"] = {"outcome": "violated", "properties": ["Target"]}
        self.save_manifest()
        self.adapter.write_text(self.adapter.read_text().replace("'satisfied'", "'violated'"))
        self.assertTrue(self.run_evidence())

    def test_nonzero_exit_rejects_even_a_complete_summary(self):
        self.adapter.write_text(self.adapter.read_text() + "raise SystemExit(3)\n")
        self.assertTrue(self.run_evidence())
        self.assertTrue(self.check())

    def test_incomplete_summary_is_rejected(self):
        self.adapter.write_text(self.adapter.read_text().replace("'complete':True", "'complete':False"))
        self.assertTrue(self.run_evidence())

    def test_parse_error_or_missing_result_is_rejected(self):
        self.adapter.write_text("print('TLC parse failed')\n")
        self.assertTrue(self.run_evidence())

    def test_malformed_result_is_rejected(self):
        self.adapter.write_text("import os\nfrom pathlib import Path\nPath(os.environ['G5_MODEL_RESULT']).write_text('{')\n")
        self.assertTrue(self.run_evidence())

    def test_timeout_rejects_even_an_earlier_complete_summary(self):
        self.adapter.write_text(self.adapter.read_text() + "import time\ntime.sleep(10)\n")
        self.data["timeout_seconds"] = .15
        self.save_manifest()
        self.assertTrue(self.run_evidence())
        receipt = json.loads((self.output / "receipt.json").read_text())
        self.assertTrue(receipt["timed_out"])
        self.assertFalse(receipt["matched"])

    def test_change_during_execution_is_rejected(self):
        self.adapter.write_text(self.adapter.read_text() + "Path('Model.tla').write_text('changed')\n")
        self.assertTrue(self.run_evidence())

    def test_raw_log_corruption_is_rejected(self):
        self.run_evidence()
        (self.output / "stdout.log").write_text("rewritten")
        self.assertTrue(self.check())

    def test_missing_log_is_rejected(self):
        self.run_evidence()
        (self.output / "stdout.log").unlink()
        with self.assertRaises(OSError):
            self.check()

    def test_run_directory_and_receipt_are_not_overwritten(self):
        self.run_evidence()
        before = (self.output / "receipt.json").read_bytes()
        with self.assertRaises(FileExistsError):
            self.run_evidence()
        self.assertEqual(before, (self.output / "receipt.json").read_bytes())

    def test_missing_tool_leaves_failed_receipt(self):
        self.data["command"] = ["tool-that-does-not-exist-model-evidence-001"]
        self.save_manifest()
        self.assertTrue(self.run_evidence())
        self.assertFalse(json.loads((self.output / "receipt.json").read_text())["matched"])

    def cli(self, action):
        return subprocess.run([sys.executable, str(SCRIPT), action,
                               str(self.manifest), str(self.output)], capture_output=True)

    def test_cli_success_and_stale_input_exit_codes(self):
        self.assertEqual(self.cli("run").returncode, 0)
        self.assertEqual(self.cli("check").returncode, 0)
        (self.root / "binding.md").write_text("new binding")
        self.assertEqual(self.cli("check").returncode, 1)

    def test_malformed_manifest_is_a_setup_failure(self):
        self.manifest.write_text("[]")
        self.assertEqual(self.cli("run").returncode, 2)
        self.assertFalse(self.output.exists())

    def test_corrupt_receipt_is_rejected(self):
        self.run_evidence()
        path = self.output / "receipt.json"
        original = json.loads(path.read_text())
        for value in [[], {**original, "inputs": []}, {**original, "artifacts": []},
                      {**original, "command": ["fabricated-command"]}]:
            with self.subTest(value=value):
                path.write_text(json.dumps(value))
                self.assertEqual(self.cli("check").returncode, 1)

    def test_relative_and_empty_path_entries_use_the_execution_root(self):
        caller = self.root / "caller"
        caller.mkdir()
        self.data["command"] = ["evidence-python", "adapter.py"]
        self.save_manifest()
        for index, entry in enumerate(["bin", ""]):
            with self.subTest(path_entry=entry):
                executables = []
                for base, marker in [(caller, "caller"), (self.root, "checkout")]:
                    directory = base / entry
                    directory.mkdir(exist_ok=True)
                    executable = directory / "evidence-python"
                    executable.write_text(
                        f"#!{sys.executable}\nimport os, sys\n"
                        f"print({marker!r}, flush=True)\n"
                        f"os.execv({sys.executable!r}, [{sys.executable!r}] + sys.argv[1:])\n")
                    executable.chmod(0o755)
                    executables.append(executable)
                output = self.root / f"path-run-{index}"
                env = {**os.environ, "PATH": os.pathsep.join([entry, "/usr/bin", "/bin"])}

                def invoke(action):
                    return subprocess.run(
                        [sys.executable, str(SCRIPT.resolve()), action, str(self.manifest),
                         str(output), "--root", str(self.root)],
                        cwd=caller, env=env, capture_output=True)

                result = invoke("run")
                self.assertEqual(result.returncode, 0, result.stderr)
                receipt = json.loads((output / "receipt.json").read_text())
                self.assertEqual(Path(receipt["executable"]), executables[1].resolve())
                self.assertEqual(receipt["executable_sha256"], evidence.digest(executables[1]))
                self.assertTrue((output / "stdout.log").read_text().startswith("checkout\n"))
                self.assertEqual(invoke("check").returncode, 0)
                executables[0].write_text(executables[0].read_text() + "# caller changed\n")
                self.assertEqual(invoke("check").returncode, 0)
                executables[1].write_text(executables[1].read_text() + "# actual tool changed\n")
                self.assertEqual(invoke("check").returncode, 1)


if __name__ == "__main__":
    unittest.main()
