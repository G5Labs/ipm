from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from record_playwright_shard_results import PREVIEW_SELECTOR, record_shard_results  # noqa: E402
from value_loop_receipt import ReceiptError, load_catalog  # noqa: E402


class ShardReceiptTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.catalog_path = ROOT / "tests/e2e/value_loop_matrix.tsv"
        catalog = load_catalog(self.catalog_path)
        self.selectors = {
            key.selector: catalog.source_by_selector[key]
            for key in catalog.loops_by_selector
            if key.lane == "playwright"
            and key.selector not in {
                "e2e/flows/chat-restart-continuity.spec.ts",
                "e2e/flows/native-requests.spec.ts",
            }
        }
        self.assertEqual(len(self.selectors), 32)
        self.assertIn("e2e/flows/personal-workspace.spec.ts", self.selectors)
        self.assertTrue({f"e2e/flows/source-control-{name}.spec.ts" for name in ["selection", "comparisons", "history"]} <= set(self.selectors))
        ci_selectors = dict(
            line.split("|") for line in (ROOT / "tests/e2e/playwright_shard_value_loops.tsv").read_text().splitlines()
        )
        self.assertEqual(ci_selectors, self.selectors)
        self.selectors_path = self.root / "selectors.tsv"
        self.selectors_path.write_text(
            "".join(f"{selector}|{source}\n" for selector, source in self.selectors.items()),
            encoding="utf-8",
        )
        self.inventory_path = self.root / "inventory.json"
        self.shard_paths = [self.root / "shard1.json", self.root / "shard2.json"]
        self.provider_path = self.root / "provider.json"
        self.results_path = self.root / "results.jsonl"
        self.artifact_root = self.root / "artifacts"
        self.candidate_sha = "a" * 40

        all_tests = [(selector, f"{Path(selector).stem}-test", "ordinary") for selector in self.selectors]
        all_tests = [row for row in all_tests if row[0] != PREVIEW_SELECTOR]
        all_tests.extend(
            (PREVIEW_SELECTOR, f"preview-{index}", title)
            for index, title in enumerate(
                ["launches Preview", "M12 Stop fences Preview", "M9/M12 transfer fences live chat"]
            )
        )
        self.inventory = self._report(all_tests, executed=False)
        self.shard1 = self._report(all_tests[:5], executed=True)
        self.shard2 = self._report(all_tests[5:-1], executed=True)
        self.provider = self._report(all_tests[-1:], executed=True)
        self._write_reports()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    @staticmethod
    def _report(rows: list[tuple[str, str, str]], *, executed: bool) -> dict:
        suites = []
        for selector, spec_id, title in rows:
            test = {
                "projectName": "chromium",
                "expectedStatus": "passed",
                "status": "expected" if executed else "skipped",
                "results": [
                    {
                        "status": "passed",
                        "retry": 0,
                        "errors": [],
                        "startTime": "2026-09-26T12:00:00.000Z",
                        "duration": 1000,
                    }
                ] if executed else [],
            }
            suites.append({
                "file": Path(selector).name,
                "specs": [{
                    "file": Path(selector).name,
                    "id": spec_id,
                    "title": title,
                    "ok": True,
                    "tests": [test],
                }],
            })
        return {"suites": suites, "errors": []}

    def _write_reports(self) -> None:
        for path, value in [
            (self.inventory_path, self.inventory),
            (self.shard_paths[0], self.shard1),
            (self.shard_paths[1], self.shard2),
            (self.provider_path, self.provider),
        ]:
            path.write_text(json.dumps(value), encoding="utf-8")

    def _record(self) -> None:
        self._write_reports()
        record_shard_results(
            catalog_path=self.catalog_path,
            selectors_path=self.selectors_path,
            inventory_path=self.inventory_path,
            shard_paths=self.shard_paths,
            provider_path=self.provider_path,
            candidate_sha=self.candidate_sha,
            results_path=self.results_path,
            artifact_root=self.artifact_root,
        )

    def test_omitting_a_review_selector_cannot_qualify_the_catalog(self) -> None:
        rows = self.selectors_path.read_text(encoding="utf-8").splitlines()
        self.selectors_path.write_text(
            "\n".join(row for row in rows if "workspace-review-content.spec.ts" not in row) + "\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(ReceiptError, "missing=.*workspace-review-content"):
            self._record()
        self.assertFalse(self.results_path.exists())

    def test_exact_inventory_passes_and_records_all_shared_selectors(self) -> None:
        self._record()
        rows = [json.loads(line) for line in self.results_path.read_text().splitlines()]
        self.assertEqual(len(rows), 32)
        self.assertEqual({row["status"] for row in rows}, {"passed"})
        for row in rows:
            self.assertTrue((self.artifact_root / row["evidenceArtifact"]).is_file())
        preview = next(row for row in rows if row["selector"] == PREVIEW_SELECTOR)
        self.assertEqual(preview["profile"], "playwright-workspace-vm")
        self.assertEqual(
            len(json.loads((self.artifact_root / preview["evidenceArtifact"]).read_text())["tests"]),
            3,
        )

    def test_three_shard_reports_qualify_the_same_selectors(self) -> None:
        third = self.root / "shard3.json"
        moved = self.shard2["suites"][-3:]
        del self.shard2["suites"][-3:]
        third.write_text(json.dumps({"suites": moved, "errors": []}), encoding="utf-8")
        self.shard_paths.append(third)
        self._record()
        rows = [json.loads(line) for line in self.results_path.read_text().splitlines()]
        self.assertEqual(len(rows), 32)
        artifacts = {
            test["reportArtifact"]
            for row in rows
            for test in json.loads((self.artifact_root / row["evidenceArtifact"]).read_text())["tests"]
        }
        self.assertIn("playwright-test-results-shard-3.tar.gz", artifacts)

    def test_missing_broad_test_fails_without_passing_rows(self) -> None:
        self.shard1["suites"].pop()
        with self.assertRaisesRegex(ReceiptError, "inventory/report mismatch"):
            self._record()
        self.assertFalse(self.results_path.exists())

    def test_grep_exclusion_of_a_new_test_fails(self) -> None:
        selector = next(iter(self.selectors))
        self.inventory["suites"].extend(self._report([(selector, "new-test", "new browser outcome")], executed=False)["suites"])
        with self.assertRaisesRegex(ReceiptError, "inventory/report mismatch"):
            self._record()

    def test_missing_one_save_transport_selection_combination_fails(self) -> None:
        selector = "e2e/flows/source-control-selection.spec.ts"
        combinations = [
            (selector, f"save-{transport}-{choice}", f"{transport} Save preserves {choice}")
            for transport in ["agent CLI", "chat"]
            for choice in ["full", "custom"]
        ]
        self.inventory["suites"].extend(self._report(combinations, executed=False)["suites"])
        self.shard2["suites"].extend(self._report(combinations[:-1], executed=True)["suites"])
        with self.assertRaisesRegex(ReceiptError, "inventory/report mismatch"):
            self._record()
        self.assertFalse(self.results_path.exists())
        self.shard2["suites"].extend(self._report(combinations[-1:], executed=True)["suites"])
        self._record()
        evidence = next(
            row for row in map(json.loads, self.results_path.read_text().splitlines())
            if row["selector"] == selector
        )
        self.assertEqual(
            len(json.loads((self.artifact_root / evidence["evidenceArtifact"]).read_text())["tests"]),
            5,
        )

    def test_home_alone_does_not_qualify_personal_changes(self) -> None:
        selector = "e2e/flows/personal-workspace.spec.ts"
        changes = [(selector, "personal-unpushed", "personal unpushed Changes excludes reference edits")]
        self.inventory["suites"].extend(self._report(changes, executed=False)["suites"])
        with self.assertRaisesRegex(ReceiptError, "inventory/report mismatch"):
            self._record()
        self.assertFalse(self.results_path.exists())
        self.shard2["suites"].extend(self._report(changes, executed=True)["suites"])
        self._record()
        evidence = next(
            row for row in map(json.loads, self.results_path.read_text().splitlines())
            if row["selector"] == selector
        )
        self.assertEqual(
            len(json.loads((self.artifact_root / evidence["evidenceArtifact"]).read_text())["tests"]),
            2,
        )

    def test_comparison_alone_does_not_qualify_file_type_filters(self) -> None:
        selector = "e2e/flows/source-control-comparisons.spec.ts"
        filters = [(selector, "file-types", "file-type filters keep matching filenames and diffs current")]
        self.inventory["suites"].extend(self._report(filters, executed=False)["suites"])
        with self.assertRaisesRegex(ReceiptError, "inventory/report mismatch"):
            self._record()
        self.assertFalse(self.results_path.exists())
        self.shard2["suites"].extend(self._report(filters, executed=True)["suites"])
        self._record()

    def test_skipped_or_retry_passed_test_fails(self) -> None:
        test = self.shard1["suites"][0]["specs"][0]["tests"][0]
        test["status"] = "skipped"
        with self.assertRaisesRegex(ReceiptError, "did not pass once"):
            self._record()
        test["status"] = "flaky"
        test["results"].append({"status": "passed", "retry": 1, "errors": []})
        with self.assertRaisesRegex(ReceiptError, "did not pass once"):
            self._record()

    def test_preview_requires_provider_transfer_and_broad_tests(self) -> None:
        self.provider["suites"].clear()
        with self.assertRaisesRegex(ReceiptError, "exactly the transfer test"):
            self._record()
        self.provider = self._report([(PREVIEW_SELECTOR, "preview-2", "M9/M12 transfer fences live chat")], executed=True)
        self.shard2["suites"] = [
            suite for suite in self.shard2["suites"]
            if suite["specs"][0]["id"] != "preview-1"
        ]
        with self.assertRaisesRegex(ReceiptError, "inventory/report mismatch"):
            self._record()

    def test_duplicate_shard_result_and_catalog_mismatch_fail(self) -> None:
        self.shard2["suites"].append(self.shard1["suites"][0])
        with self.assertRaisesRegex(ReceiptError, "duplicate shard test"):
            self._record()
        self.shard2["suites"].pop()
        self.selectors_path.write_text(
            self.selectors_path.read_text().replace("frontend/e2e/flows/", "wrong/", 1)
        )
        with self.assertRaisesRegex(ReceiptError, "disagrees with catalog"):
            self._record()


if __name__ == "__main__":
    unittest.main()
