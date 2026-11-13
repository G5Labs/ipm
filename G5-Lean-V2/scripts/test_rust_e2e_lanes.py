"""Unit checks for lane scheduling; no API, worker, or Docker replacements."""

import csv
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest


GENERATION = Path(__file__).resolve().parents[1]
RECEIPT_OWNED = {
    "shared_vm_chats": "test_streamed_chat_survives_api_restart_and_continues_same_task",
    "formal_agent_approvals": "explicit_formal_reviews_keep_verdict_and_evidence_after_isolated_resources_disappear",
}


def receipt_owned_tests(runner_source: str) -> list[tuple[str, str]]:
    """Parse run-rust-e2e-lanes.sh's receipt_owned_tests array."""
    block = re.search(r"^receipt_owned_tests=\(\n(.*?)^\)", runner_source, re.MULTILINE | re.DOTALL)
    if block is None:
        raise AssertionError("run-rust-e2e-lanes.sh must declare receipt_owned_tests")
    entries = re.findall(r'^\s+"(\S+) (\S+)"\s*$', block.group(1), re.MULTILINE)
    if len(entries) != len([line for line in block.group(1).splitlines() if line.strip()]):
        raise AssertionError("every receipt_owned_tests entry must be \"<scenario> <test name>\"")
    return entries


def unowned_skips(entries: list[tuple[str, str]], catalog: str) -> list[str]:
    """Skips the value-loop receipt catalog does not route to a Rust receipt selector
    in that scenario's module (ontology g5.quality.delivery.ci.single-execution-owner)."""
    rows = list(csv.DictReader(catalog.splitlines(), delimiter="\t"))
    owned = {
        (row["selector"], row["source"])
        for row in rows
        if row["lane"] == "e2e" and row["profile"] == "rust-e2e-broad"
    }
    return [
        f"{scenario} {test}"
        for scenario, test in entries
        if (test, f"tests/e2e/src/scenarios/{scenario}.rs") not in owned
    ]


class RustE2eLaneSelectionTest(unittest.TestCase):
    def test_only_retained_workspace_vm_scenarios_run(self):
        runner = Path(__file__).with_name("run-rust-e2e-lanes.sh")
        with tempfile.TemporaryDirectory(prefix="g5-e2e-lanes-") as directory:
            scratch = Path(directory)
            record = scratch / "cargo-invocations.jsonl"
            cargo = scratch / "cargo"
            cargo.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['CARGO_RECORD_FILE'], 'a') as output:\n"
                "    output.write(json.dumps(sys.argv[1:]) + '\\n')\n"
            )
            cargo.chmod(0o755)
            result = subprocess.run(
                ["bash", str(runner)],
                env={
                    "PATH": f"{scratch}{os.pathsep}{os.defpath}",
                    "CARGO_RECORD_FILE": str(record),
                    "G5_E2E_NO_COMPILE": "true",
                    "G5_E2E_STACK_THREADS": "16",
                },
                text=True,
                capture_output=True,
                timeout=15,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            calls = [json.loads(line) for line in record.read_text().splitlines()]

        # Mirrors retained_scenarios in run-rust-e2e-lanes.sh.
        retained = [
            "browser_interaction",
            "ci_e2e_lane_isolation",
            "ci_stale_build_cancellation",
            "cli_ops",
            "codex_control_frames",
            "comment_notifications",
            "host_boot_timings",
            "ontology_lifecycle",
            "r2_guest_lifecycle",
            "shared_vm_chats",
            "formal_agent_approvals",
            "value_loop_matrix",
        ]
        self.assertEqual(
            calls,
            [
                [
                    "test",
                    "-p",
                    "g5-e2e",
                    scenario,
                    "--",
                    f"--test-threads={1 if scenario in ('r2_guest_lifecycle', 'shared_vm_chats') else 16}",
                    *(["--skip", RECEIPT_OWNED[scenario]] if scenario in RECEIPT_OWNED else []),
                ]
                for scenario in retained
            ],
        )

    def test_every_broad_skip_has_one_receipt_owner(self):
        runner = (GENERATION / "scripts/run-rust-e2e-lanes.sh").read_text()
        catalog = (GENERATION / "tests/e2e/value_loop_matrix.tsv").read_text()
        entries = receipt_owned_tests(runner)
        self.assertEqual(dict(entries), RECEIPT_OWNED)
        self.assertEqual(unowned_skips(entries, catalog), [])
        # A skip must name one test in a retained module, never the module itself.
        retained = re.search(r"^retained_scenarios=\((.*?)^\)", runner, re.MULTILINE | re.DOTALL).group(1).split()
        # libtest --skip and the receipt's nextest test() filter both match by
        # substring, so each owned name must be contained in no other g5-e2e test
        # name; otherwise a new test would leave the broad lane or join the receipt.
        all_fns = [
            name
            for path in (GENERATION / "tests/e2e/src").rglob("*.rs")
            for name in re.findall(r"\bfn (\w+)\s*[<(]", path.read_text())
        ]
        for scenario, test in entries:
            self.assertIn(scenario, retained)
            source = (GENERATION / f"tests/e2e/src/scenarios/{scenario}.rs").read_text()
            self.assertEqual(len(re.findall(rf"\bfn {re.escape(test)}\(", source)), 1, test)
            self.assertNotEqual(test, scenario)
            self.assertEqual([name for name in all_fns if test in name], [test], test)
        # The only owner is the receipt lane, which fails the build when a selector never runs.
        cloudbuild = (GENERATION / "infra/ci/cloudbuild-ci-e2e.yaml").read_text()
        receipt = cloudbuild.split("  - id: value-loop-rust-api-profiles\n", 1)[1].split("\n  - id:", 1)[0]
        self.assertIn('-E "test($$selector)"', receipt)
        self.assertIn("'rust-e2e'", receipt.rsplit("waitFor:", 1)[1])
        finalize = cloudbuild.split("  - id: finalize-value-loop-receipt\n", 1)[1].split("\n  - id:", 1)[0]
        self.assertIn("--fill-missing", finalize)

    def test_unowned_or_misattributed_skips_are_rejected(self):
        catalog = (GENERATION / "tests/e2e/value_loop_matrix.tsv").read_text()
        self.assertEqual(
            unowned_skips(
                [
                    ("formal_agent_approvals", "explicit_formal_reviews"),  # not an exact selector
                    ("shared_vm_chats", RECEIPT_OWNED["formal_agent_approvals"]),  # wrong module
                    ("cli_ops", "test_not_in_any_receipt"),
                    ("shared_vm_chats", RECEIPT_OWNED["shared_vm_chats"]),
                ],
                catalog,
            ),
            [
                "formal_agent_approvals explicit_formal_reviews",
                f"shared_vm_chats {RECEIPT_OWNED['formal_agent_approvals']}",
                "cli_ops test_not_in_any_receipt",
            ],
        )
        # Playwright and API-integration selectors are not Rust receipt owners.
        playwright_only = "\n".join(
            line for line in catalog.splitlines() if "\trust-e2e-broad\t" not in line
        )
        self.assertEqual(
            unowned_skips([("shared_vm_chats", RECEIPT_OWNED["shared_vm_chats"])], playwright_only),
            [f"shared_vm_chats {RECEIPT_OWNED['shared_vm_chats']}"],
        )
        with self.assertRaises(AssertionError):
            receipt_owned_tests('receipt_owned_tests=(\n  "shared_vm_chats"\n)\n')

    def test_failed_lane_keeps_run_red_and_later_lanes_execute(self):
        runner = Path(__file__).with_name("run-rust-e2e-lanes.sh")
        with tempfile.TemporaryDirectory(prefix="g5-e2e-failure-") as directory:
            scratch = Path(directory)
            record = scratch / "calls.jsonl"
            cargo = scratch / "cargo"
            cargo.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['CARGO_RECORD_FILE'], 'a') as output:\n"
                "    output.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "args = sys.argv[1:]\n"
                "selectors = args[:args.index('--')] if '--' in args else args\n"
                "sys.exit(19 if os.environ['FAIL_SELECTOR'] in selectors else 0)\n"
            )
            cargo.chmod(0o755)
            base_env = {
                "PATH": f"{scratch}{os.pathsep}{os.defpath}",
                "CARGO_RECORD_FILE": str(record),
                "G5_E2E_STACK_THREADS": "2",
            }
            failed = subprocess.run(
                ["bash", str(runner)],
                env={**base_env, "G5_E2E_NO_COMPILE": "true", "FAIL_SELECTOR": "comment_notifications"},
                capture_output=True, text=True, timeout=15, check=False,
            )
            calls = [json.loads(line) for line in record.read_text().splitlines()]
            self.assertNotEqual(failed.returncode, 0)
            self.assertIn("1 retained Rust E2E scenario group(s) failed", failed.stderr)
            self.assertTrue(any("ci_stale_build_cancellation" in call[:call.index("--")] for call in calls))
            self.assertTrue(any("--test-threads=2" in call for call in calls))
            self.assertNotIn("lane runner complete", failed.stdout)

            record.unlink()
            compile_failed = subprocess.run(
                ["bash", str(runner)],
                env={**base_env, "FAIL_SELECTOR": "--no-run"},
                capture_output=True, text=True, timeout=15, check=False,
            )
            calls = [json.loads(line) for line in record.read_text().splitlines()]
            self.assertNotEqual(compile_failed.returncode, 0)
            self.assertEqual(len(calls), 2, "compile failure must stop before test lanes")

    def test_native_formal_clients_use_the_staged_runtime_and_exact_ignored_selector(self):
        generation = Path(__file__).resolve().parents[1]
        cloudbuild = (generation / "infra/ci/cloudbuild-ci-e2e.yaml").read_text()
        step = cloudbuild.split("  - id: formal-review-native-clients\n", 1)[1].split("  - id:", 1)[0]
        self.assertIn("G5_NATIVE_PROVIDER_TEST_BIN_DIR=/workspace/g5d-host/runtime/bin", step)
        self.assertIn("formal_review::formal_review_real_native_clients_enforce_tools_and_report_transport", step)
        self.assertIn("formal_review::formal_review_real_codex_cancellation_and_missing_report_settle_no_verdict", step)
        self.assertIn("-- --ignored --exact", step)
        self.assertIn("waitFor: ['start-workers', 'e2e-rust-prime']", step)
        self.assertNotIn("allowFailure: true", step)

    def test_ci_invokes_retained_runner_without_legacy_attachment_gate(self):
        generation = Path(__file__).resolve().parents[1]
        cloudbuild = (generation / "infra/ci/cloudbuild-ci-e2e.yaml").read_text()
        self.assertIn("./scripts/run-rust-e2e-lanes.sh", cloudbuild)
        self.assertNotIn("G5_E2E_REQUIRE_CHAT_FILE_ATTACHMENTS=true", cloudbuild)


if __name__ == "__main__":
    unittest.main()
