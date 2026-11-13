"""TLC adapter qualification. Set TLC_JAVA and TLC_JAR for the real-tool tests.

No downloaded tools or synthetic success logs: protocol-negative tests mutate
actual TLC output. The fixture's trace correspondence is checked separately.
"""
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parent
EXAMPLE = SCRIPTS.parent / "examples" / "tlc-refinement"
ADAPTER = SCRIPTS / "tlc_adapter.py"
HELPER = SCRIPTS / "model_evidence.py"
LABEL = "line 22, col 35 to line 22, col 66 of module Implementation"


def load_module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


adapter = load_module(ADAPTER, "tlc_adapter")
fixture = load_module(EXAMPLE / "store_fixture.py", "store_fixture")


class ConfigTests(unittest.TestCase):
    def test_property_selection_must_match(self):
        with self.assertRaises(ValueError):
            adapter.selected_property(EXAMPLE / "good.cfg", "Unrelated")

    def test_ambiguous_or_pruned_configuration_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "config.cfg"
            for extra in ["INVARIANT Other", "CONSTRAINT Prune", "SYMMETRY Symmetry"]:
                with self.subTest(extra=extra):
                    path.write_text("SPECIFICATION Spec\nINVARIANT Safe\n" + extra)
                    with self.assertRaises(ValueError):
                        adapter.selected_property(path, "Safe")


@unittest.skipUnless(os.environ.get("TLC_JAR") and os.environ.get("TLC_JAVA"),
                     "set TLC_JAR and TLC_JAVA to qualify against the actual supported TLC")
class RealTLCTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.work = Path(self.tmp.name)
        self.count = 0
        self.java = Path(os.environ["TLC_JAVA"]).resolve()
        self.jar = Path(os.environ["TLC_JAR"]).resolve()

    def command(self, config="good.cfg", prop="RefinesIntent", expected="satisfied", *extra):
        return [sys.executable, str(ADAPTER), "--java", str(self.java), "--jar", str(self.jar),
                "--root", str(EXAMPLE), "--module", "Implementation", "--config", config,
                "--property", prop, "--expect", expected, "--action-label", LABEL, *extra]

    def invoke(self, config="good.cfg", prop="RefinesIntent", expected="satisfied", *extra):
        self.count += 1
        result_path = self.work / f"result-{self.count}.json"
        process = subprocess.run(self.command(config, prop, expected, *extra),
                                 env={**os.environ, "G5_MODEL_RESULT": str(result_path)},
                                 capture_output=True, text=True, timeout=20)
        return process, json.loads(result_path.read_text())

    def test_actual_completed_refinement(self):
        process, result = self.invoke()
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertEqual(result["outcome"], "satisfied")
        self.assertEqual(result["distinct_states"], 3)
        self.assertEqual(result["tlc_version"], adapter.VERSION)

    def test_broken_mapping_is_targeted_action_violation(self):
        process, result = self.invoke("bad-map.cfg", expected="violated")
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertEqual(result["tlc_returncode"], 13)
        self.assertEqual(len(result["trace"]), 2)
        wrong, rejected = self.invoke("bad-map.cfg", "RefinesIntent", "violated",
                                      "--action-label", "UnrelatedProperty")
        self.assertNotEqual(wrong.returncode, 0)
        self.assertFalse(rejected["complete"])

    def test_path_witness_matches_fixture_operations(self):
        process, result = self.invoke("stage-witness.cfg", "NoStageThenCommit", "witness")
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertEqual(result["outcome"], "witness")
        store = fixture.Store()
        snapshots = [(store.phase, store.stored)]
        store.stage()
        snapshots.append((store.phase, store.stored))
        store.publish()
        snapshots.append((store.phase, store.stored))
        trace = [(re.search(r'phase = "([^\"]+)"', state)[1],
                  int(re.search(r"stored = (\d+)", state)[1])) for state in result["trace"]]
        self.assertEqual(trace, snapshots)
        self.assertEqual(store.read(), (True, 1))
        with self.assertRaises(ValueError):
            adapter.parse_result(process.stdout, "", 12, "INVARIANT", "DifferentInvariant")

    def test_behavior_bug_fails_strong_property_but_passes_weak_one(self):
        process, result = self.invoke("bad-write.cfg", expected="violated")
        self.assertEqual(process.returncode, 0, process.stderr)
        store = fixture.Store()
        store.stage()
        store.publish(bad_write=True)
        self.assertEqual(store.read(), (True, 2))
        self.assertIn("stored = 2", result["trace"][-1])
        weak, weak_result = self.invoke("weak.cfg", "WeakTypeOnly")
        self.assertEqual(weak.returncode, 0, weak.stderr)
        self.assertEqual(weak_result["outcome"], "satisfied")

    def test_stuck_model_passes_safety_but_has_no_required_witness(self):
        process, result = self.invoke("stuck.cfg")
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertEqual(result["distinct_states"], 2)
        witness, result = self.invoke("stuck-witness.cfg", "NoStageThenCommit", "witness")
        self.assertNotEqual(witness.returncode, 0)
        self.assertEqual(result["outcome"], "satisfied")

    def test_direct_commit_preserves_intent_with_a_different_path(self):
        process, result = self.invoke("direct.cfg")
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertEqual(result["distinct_states"], 2)
        process, result = self.invoke("direct-witness.cfg", "NoDirectCommit", "witness")
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertEqual(len(result["trace"]), 2)
        store = fixture.Store()
        store.commit_directly()
        self.assertEqual(store.read(), (True, 1))

    def test_real_parse_and_config_errors_never_count_as_violations(self):
        for cfg, prop, extra in [("parse-error.cfg", "Safe", ["--module", "ParseError"]),
                                 ("config-error.cfg", "MissingInvariant", [])]:
            with self.subTest(config=cfg):
                process, result = self.invoke(cfg, prop, "violated", *extra)
                self.assertNotEqual(process.returncode, 0)
                self.assertFalse(result["complete"])

    def test_actual_timeout_and_jvm_resource_failure_never_pass(self):
        for extra in [["--timeout", "0.001"], ["--heap-mb", "1"]]:
            with self.subTest(extra=extra):
                process, result = self.invoke("stage-witness.cfg", "NoStageThenCommit", "witness", *extra)
                self.assertNotEqual(process.returncode, 0)
                self.assertFalse(result["complete"])

    def test_truncated_and_incomplete_actual_output_is_rejected(self):
        process, _ = self.invoke()
        marker = process.stdout.index("@!@!@STARTMSG 2186")
        for text in [process.stdout[:marker], process.stdout[:-8],
                     process.stdout.replace(adapter.VERSION, "TLC2 Version unknown")]:
            with self.subTest(length=len(text)):
                with self.assertRaises(ValueError):
                    adapter.parse_result(text, "", 0, "PROPERTY", "RefinesIntent", LABEL)
        witness, _ = self.invoke("stage-witness.cfg", "NoStageThenCommit", "witness")
        no_trace = re.sub(r"@!@!@STARTMSG 2217:4 @!@!@.*?@!@!@ENDMSG 2217 @!@!@\n",
                          "", witness.stdout, flags=re.DOTALL)
        with self.assertRaises(ValueError):
            adapter.parse_result(no_trace, "", 12, "INVARIANT", "NoStageThenCommit")
        with self.assertRaises(ValueError):
            adapter.parse_result(witness.stdout, "", 1, "INVARIANT", "NoStageThenCommit")

    def test_helper_and_adapter_end_to_end_receipt_and_stale_binding(self):
        binding = self.work / "binding.md"
        binding.write_text("fixture stage/publish correspond to Stage/Publish")
        manifest = self.work / "manifest.json"
        manifest.write_text(json.dumps({
            "command": self.command("stage-witness.cfg", "NoStageThenCommit", "witness"),
            "timeout_seconds": 15,
            "inputs": {"formal": [str(EXAMPLE / name) for name in
                                  ["Intent.tla", "Implementation.tla", "stage-witness.cfg"]],
                       "tool": [str(ADAPTER), str(self.java), str(self.jar)],
                       "implementation": [str(EXAMPLE / "store_fixture.py")],
                       "binding": [str(binding)]},
            "expected": {"outcome": "witness", "properties": ["NoStageThenCommit"]}}))
        output = self.work / "run"

        def helper(action):
            return subprocess.run([sys.executable, str(HELPER), action, str(manifest), str(output)],
                                  capture_output=True, text=True, timeout=20)

        execution = helper("run")
        self.assertEqual(execution.returncode, 0, execution.stderr)
        self.assertEqual(helper("check").returncode, 0)
        self.assertTrue(json.loads((output / "receipt.json").read_text())["matched"])
        self.assertIn("@!@!@STARTMSG 2262", (output / "stdout.log").read_text())
        binding.write_text("mapping changed after the run")
        self.assertEqual(helper("check").returncode, 1)


if __name__ == "__main__":
    unittest.main()
