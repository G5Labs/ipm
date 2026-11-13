from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "value_loop_receipt.py"
SELECTOR_RUNNER = Path(__file__).resolve().parents[1] / "run-value-loop-selector.sh"
SPEC = importlib.util.spec_from_file_location("value_loop_receipt", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
receipt = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = receipt
SPEC.loader.exec_module(receipt)


class ValueLoopReceiptTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        (self.root / "e2e").mkdir()
        self.catalog_path = self.root / "e2e" / "value_loop_matrix.tsv"
        self.api_source = self.root / "api.rs"
        self.e2e_source = self.root / "e2e.rs"
        self.playwright_source = self.root / "flow.spec.ts"

        profiles = [
            ("api", "api-integration", "api-selector"),
            ("e2e", "rust-e2e-broad", "broad-selector-extra"),
            ("playwright", "playwright-workspace-vm", "flow.spec.ts"),
        ]
        profiles.extend(
            ("e2e", "rust-e2e-broad", f"broad-selector-{index:02}")
            for index in range(len(receipt.EXPECTED_LOOPS) - len(profiles))
        )
        self.rows = []
        for index, (lane, profile, selector) in enumerate(profiles):
            source = {
                "api": "api.rs",
                "e2e": "e2e.rs",
                "playwright": "flow.spec.ts",
            }[lane]
            self.rows.append(
                [sorted(receipt.EXPECTED_LOOPS)[index], lane, profile, selector, source, f"proof {index}"]
            )
        self._write_catalog(self.rows)
        self.api_source.write_text("api-selector\n", encoding="utf-8")
        self.e2e_source.write_text(
            "\n".join(row[3] for row in self.rows if row[1] == "e2e") + "\n",
            encoding="utf-8",
        )
        self.playwright_source.write_text("browser proof\n", encoding="utf-8")
        self.candidate_sha = "a" * 40

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _write_catalog(self, rows: list[list[str]], path: Path | None = None) -> None:
        lines = ["\t".join(receipt.CATALOG_FIELDS)]
        lines.extend("\t".join(row) for row in rows)
        (path or self.catalog_path).write_text("\n".join(lines) + "\n", encoding="utf-8")

    def test_default_source_root_matches_relocated_tests_e2e_catalog(self) -> None:
        relocated_catalog = self.root / "tests" / "e2e" / "value_loop_matrix.tsv"
        relocated_catalog.parent.mkdir(parents=True)
        self._write_catalog(self.rows, relocated_catalog)

        catalog = receipt.load_catalog(relocated_catalog)

        self.assertEqual(len(catalog.loops_by_selector), len(receipt.EXPECTED_LOOPS))
        self.assertEqual(
            {loop for loops in catalog.loops_by_selector.values() for loop in loops},
            receipt.EXPECTED_LOOPS,
        )

    def test_shipped_catalog_satisfies_the_receipt_invariants(self) -> None:
        shipped_catalog = SCRIPT.parents[1] / "tests" / "e2e" / "value_loop_matrix.tsv"

        catalog = receipt.load_catalog(shipped_catalog)

        self.assertEqual(
            {loop for loops in catalog.loops_by_selector.values() for loop in loops},
            receipt.EXPECTED_LOOPS,
        )
        self.assertEqual(
            {key.profile for key in catalog.loops_by_selector},
            set(receipt.PROFILE_LANES),
        )

    # ------------------------------------------------------------------
    # Producer-versus-catalog parity: a producer that names a selector or
    # profile the catalog does not carry fails here, in seconds, instead of at
    # receipt finalization after a full Cloud Build lane has run.
    # ------------------------------------------------------------------
    def _pipeline(self, body: str = "", arms: list[str] | None = None) -> Path:
        arms = sorted(receipt.PROFILE_LANES) if arms is None else arms
        router = "\n".join(f"            {profile})\n              continue\n              ;;" for profile in arms)
        pipeline = self.root / "pipeline.yaml"
        pipeline.write_text(
            "        while IFS=$'\\t' read -r profile lane selector loop_ids source; do\n"
            '          case "$$profile" in\n'
            f"{router}\n"
            "            *)\n"
            '              echo "unknown value-loop profile $$profile" >&2\n'
            "              exit 1\n"
            "              ;;\n"
            "          esac\n"
            "        done\n"
            f"{body}"
            "        done <<'PLAYWRIGHT_VALUE_LOOPS'\n"
            "        flow.spec.ts|flow.spec.ts\n"
            "        PLAYWRIGHT_VALUE_LOOPS\n",
            encoding="utf-8",
        )
        return pipeline

    def _record_command(self, profile: str, selector: str) -> str:
        return (
            "        python3 scripts/value_loop_receipt.py record \\\n"
            '          --results "$$RESULTS" \\\n'
            "          --lane e2e \\\n"
            f"          --profile {profile} \\\n"
            f"          --selector {selector} \\\n"
            "          --status passed\n"
        )

    def test_producers_accept_the_shipped_pipeline(self) -> None:
        root = SCRIPT.parents[1]
        catalog = receipt.load_catalog(root / "tests" / "e2e" / "value_loop_matrix.tsv")

        receipt.verify_producers(catalog, root / "infra" / "ci" / "cloudbuild-ci-e2e.yaml")

    def test_producers_accept_a_literal_row_the_catalog_carries(self) -> None:
        catalog = receipt.load_catalog(self.catalog_path, self.root)

        receipt.verify_producers(
            catalog,
            self._pipeline(self._record_command("rust-e2e-broad", "broad-selector-00")),
        )

    def test_producers_reject_a_recorded_profile_absent_from_the_catalog(self) -> None:
        catalog = receipt.load_catalog(self.catalog_path, self.root)
        pipeline = self._pipeline(
            self._record_command("rust-e2e-persistent-ignored", '"$$VALUE_LOOP_SELECTOR"')
        )

        with self.assertRaises(receipt.ReceiptError) as raised:
            receipt.verify_producers(catalog, pipeline)

        self.assertIn("profiles the catalog does not carry", str(raised.exception))
        self.assertIn("rust-e2e-persistent-ignored", str(raised.exception))

    def test_producers_reject_a_recorded_selector_absent_from_the_catalog(self) -> None:
        catalog = receipt.load_catalog(self.catalog_path, self.root)
        pipeline = self._pipeline(self._record_command("rust-e2e-broad", "ghost_selector"))

        with self.assertRaises(receipt.ReceiptError) as raised:
            receipt.verify_producers(catalog, pipeline)

        self.assertIn("selectors the catalog does not carry", str(raised.exception))
        self.assertIn("ghost_selector", str(raised.exception))

    def test_producers_reject_a_heredoc_selector_list_absent_from_the_catalog(self) -> None:
        # The exact P1.4 failure: a loop over selectors deleted with their
        # scenario file, recorded through a shell variable.
        catalog = receipt.load_catalog(self.catalog_path, self.root)
        pipeline = self._pipeline(
            self._record_command("rust-e2e-broad", '"$$VALUE_LOOP_SELECTOR"')
            + "        done <<'PERSISTENT_VALUE_LOOPS'\n"
            "        retired_selector_no_longer_catalogued|budget exhaustion\n"
            "        worker_loss_replacement_value_loop_publishes_preserved_work|worker loss\n"
            "        PERSISTENT_VALUE_LOOPS\n"
        )

        with self.assertRaises(receipt.ReceiptError) as raised:
            receipt.verify_producers(catalog, pipeline)

        self.assertIn("selectors the catalog does not carry", str(raised.exception))
        self.assertIn("retired_selector_no_longer_catalogued", str(raised.exception))

    def test_shipped_pipeline_names_every_catalogued_playwright_selector(self) -> None:
        root = SCRIPT.parents[1]
        catalog = receipt.load_catalog(root / "tests" / "e2e" / "value_loop_matrix.tsv")
        source = (root / "infra" / "ci" / "cloudbuild-ci-e2e.yaml").read_text()
        _, selectors, _ = receipt._pipeline_producers(source)
        expected = {
            key.selector for key in catalog.loops_by_selector if key.lane == "playwright"
        }

        self.assertLessEqual(expected, selectors)

    def test_external_shard_manifest_is_read_and_missing_or_unknown_selectors_fail(self) -> None:
        catalog = receipt.load_catalog(self.catalog_path, self.root)
        pipeline = self._pipeline()
        # Replace the inline list, so only the actual manifest can qualify it.
        source = pipeline.read_text().split("        done <<'PLAYWRIGHT_VALUE_LOOPS'")[0]
        source += (
            "        python3 scripts/record_playwright_shard_results.py \\\n"
            "          --selectors /workspace/selectors.tsv\n"
        )
        manifest = self.root / "selectors.tsv"
        pipeline.write_text(source)
        manifest.write_text("flow.spec.ts|flow.spec.ts\n")
        receipt.verify_producers(catalog, pipeline, self.root)
        manifest.unlink()
        with self.assertRaisesRegex(receipt.ReceiptError, "cannot read shard selector manifest"):
            receipt.verify_producers(catalog, pipeline, self.root)
        manifest.write_text("flow.spec.ts|flow.spec.ts\nunknown.spec.ts|unknown.spec.ts\n")
        with self.assertRaisesRegex(receipt.ReceiptError, "selectors the catalog does not carry"):
            receipt.verify_producers(catalog, pipeline, self.root)

    def test_external_shard_manifest_omission_and_duplicate_are_rejected(self) -> None:
        root = SCRIPT.parents[1]
        catalog = receipt.load_catalog(root / "tests/e2e/value_loop_matrix.tsv")
        source = (root / "infra/ci/cloudbuild-ci-e2e.yaml").read_text()
        manifest = self.root / "tests/e2e/playwright_shard_value_loops.tsv"
        manifest.parent.mkdir(parents=True)
        rows = (root / "tests/e2e/playwright_shard_value_loops.tsv").read_text().splitlines()
        pipeline = self.root / "pipeline.yaml"
        pipeline.write_text(source)
        manifest.write_text("\n".join(row for row in rows if "source-control-selection" not in row) + "\n")
        with self.assertRaisesRegex(receipt.ReceiptError, "does not produce every catalogued Playwright selector.*source-control-selection"):
            receipt.verify_producers(catalog, pipeline, self.root)
        manifest.write_text("\n".join(rows + [rows[0]]) + "\n")
        with self.assertRaisesRegex(receipt.ReceiptError, "invalid shard selector manifest row"):
            receipt.verify_producers(catalog, pipeline, self.root)

    def test_producers_reject_a_router_that_drops_a_profile(self) -> None:
        catalog = receipt.load_catalog(self.catalog_path, self.root)
        arms = [profile for profile in sorted(receipt.PROFILE_LANES) if profile != "rust-e2e-broad"]

        with self.assertRaises(receipt.ReceiptError) as raised:
            receipt.verify_producers(catalog, self._pipeline(arms=arms))

        self.assertIn("disagree on the execution profiles", str(raised.exception))

    def test_producers_require_a_catalog_driven_router(self) -> None:
        catalog = receipt.load_catalog(self.catalog_path, self.root)
        pipeline = self.root / "no-router.yaml"
        pipeline.write_text("steps: []\n", encoding="utf-8")

        with self.assertRaises(receipt.ReceiptError) as raised:
            receipt.verify_producers(catalog, pipeline)

        self.assertIn("no catalog-driven value-loop router", str(raised.exception))

    def _results(self, catalog: object) -> list[dict[str, object]]:
        return [
            {
                "candidateSha": self.candidate_sha,
                "catalogDigest": catalog.digest,
                "lane": key.lane,
                "profile": key.profile,
                "selector": key.selector,
                "startedAt": "2026-08-04T12:00:00Z",
                "endedAt": "2026-08-04T12:00:01Z",
                "status": "passed",
                "evidenceArtifact": f"value-loop/logs/{index:02}.log",
            }
            for index, key in enumerate(catalog.ordered_selectors())
        ]

    def test_exact_current_selector_and_loop_receipt_passes(self) -> None:
        catalog = receipt.load_catalog(self.catalog_path, self.root)
        expected_selector_count = len(catalog.loops_by_selector)
        built = receipt.build_receipt(catalog, self._results(catalog), self.candidate_sha)
        receipt.validate_receipt(catalog, built, self.candidate_sha)
        self.assertEqual(built["summary"]["selectorStatuses"], {"passed": expected_selector_count})
        self.assertEqual(built["summary"]["loopStatuses"], {"passed": len(receipt.EXPECTED_LOOPS)})

        with self.assertRaisesRegex(receipt.ReceiptError, "evidence artifact does not exist"):
            receipt.validate_receipt(catalog, built, self.candidate_sha, self.root)
        for row in built["selectors"]:
            evidence = self.root / row["evidenceArtifact"]
            evidence.parent.mkdir(parents=True, exist_ok=True)
            evidence.write_text("passed\n", encoding="utf-8")
        receipt.validate_receipt(catalog, built, self.candidate_sha, self.root)

        output = self.root / "receipt.json"
        receipt.write_receipt(output, built)
        receipt.validate_receipt(
            catalog,
            json.loads(output.read_text(encoding="utf-8")),
            self.candidate_sha,
        )

    def test_catalog_rejects_unknown_profile(self) -> None:
        unknown = copy.deepcopy(self.rows)
        unknown[0][2] = "unknown-profile"
        self._write_catalog(unknown)
        with self.assertRaisesRegex(receipt.ReceiptError, "unknown profile"):
            receipt.load_catalog(self.catalog_path, self.root)

    def test_receipt_rejects_missing_extra_duplicate_and_nonpassing_results(self) -> None:
        self._write_catalog(self.rows)
        catalog = receipt.load_catalog(self.catalog_path, self.root)
        results = self._results(catalog)

        with self.assertRaisesRegex(receipt.ReceiptError, "missing selector results"):
            receipt.build_receipt(catalog, results[:-1], self.candidate_sha)

        duplicate = copy.deepcopy(results)
        duplicate[-1] = copy.deepcopy(duplicate[0])
        with self.assertRaisesRegex(receipt.ReceiptError, "duplicate result"):
            receipt.build_receipt(catalog, duplicate, self.candidate_sha)

        extra = copy.deepcopy(results)
        extra[0]["selector"] = "extra-selector"
        with self.assertRaisesRegex(receipt.ReceiptError, "extra selector"):
            receipt.build_receipt(catalog, extra, self.candidate_sha)

        failed = copy.deepcopy(results)
        failed[0]["status"] = "failed"
        built = receipt.build_receipt(catalog, failed, self.candidate_sha)
        with self.assertRaisesRegex(receipt.ReceiptError, "not all passed"):
            receipt.validate_receipt(catalog, built, self.candidate_sha)

    def test_fill_missing_materializes_a_nonpassing_terminal_row_and_evidence(self) -> None:
        catalog = receipt.load_catalog(self.catalog_path, self.root)
        results_path = self.root / "selector-results.jsonl"
        results = self._results(catalog)
        for result in results[:-1]:
            receipt.record_result(
                results_path,
                candidate_sha=str(result["candidateSha"]),
                catalog_digest_value=str(result["catalogDigest"]),
                key=receipt.SelectorKey(
                    str(result["lane"]),
                    str(result["profile"]),
                    str(result["selector"]),
                ),
                started_at=str(result["startedAt"]),
                ended_at=str(result["endedAt"]),
                status=str(result["status"]),
                evidence_artifact=str(result["evidenceArtifact"]),
            )
        receipt.fill_missing_results(catalog, results_path, self.candidate_sha, self.root)
        built = receipt.build_receipt(
            catalog,
            receipt._read_results(results_path),
            self.candidate_sha,
        )
        self.assertEqual(
            built["summary"]["selectorStatuses"],
            {"not_run": 1, "passed": len(catalog.loops_by_selector) - 1},
        )
        missing = next(row for row in built["selectors"] if row["status"] == "not_run")
        self.assertTrue((self.root / missing["evidenceArtifact"]).is_file())
        with self.assertRaisesRegex(receipt.ReceiptError, "not all passed"):
            receipt.validate_receipt(catalog, built, self.candidate_sha)

    def test_receipt_rejects_candidate_and_catalog_mismatch(self) -> None:
        catalog = receipt.load_catalog(self.catalog_path, self.root)
        results = self._results(catalog)
        results[0]["candidateSha"] = "b" * 40
        with self.assertRaisesRegex(receipt.ReceiptError, "candidate SHA"):
            receipt.build_receipt(catalog, results, self.candidate_sha)

        results = self._results(catalog)
        results[0]["catalogDigest"] = "sha256:" + "0" * 64
        with self.assertRaisesRegex(receipt.ReceiptError, "catalog digest"):
            receipt.build_receipt(catalog, results, self.candidate_sha)

        built = receipt.build_receipt(catalog, self._results(catalog), self.candidate_sha)
        built["catalogDigest"] = "sha256:" + "0" * 64
        with self.assertRaisesRegex(receipt.ReceiptError, "catalog digest mismatch"):
            receipt.validate_receipt(catalog, built, self.candidate_sha)

    def test_result_record_rejects_unbounded_evidence_or_time(self) -> None:
        with self.assertRaisesRegex(receipt.ReceiptError, "normalized relative"):
            receipt.record_result(
                self.root / "results.jsonl",
                candidate_sha=self.candidate_sha,
                catalog_digest_value="sha256:" + "0" * 64,
                key=receipt.SelectorKey("api", "api-integration", "selector"),
                started_at="2026-08-04T12:00:00Z",
                ended_at="2026-08-04T12:00:01Z",
                status="passed",
                evidence_artifact="../secret.log",
            )
        with self.assertRaisesRegex(receipt.ReceiptError, "must not precede"):
            receipt.record_result(
                self.root / "results.jsonl",
                candidate_sha=self.candidate_sha,
                catalog_digest_value="sha256:" + "0" * 64,
                key=receipt.SelectorKey("api", "api-integration", "selector"),
                started_at="2026-08-04T12:00:02Z",
                ended_at="2026-08-04T12:00:01Z",
                status="passed",
                evidence_artifact="value-loop/logs/result.log",
            )

    def test_selector_runner_records_failure_then_continues_to_passing_selector(self) -> None:
        results = self.root / "selector-results.jsonl"
        digest = "sha256:" + "0" * 64
        logs = self.root / "logs"
        logs.mkdir()

        failed = subprocess.run(
            [
                str(SELECTOR_RUNNER),
                str(results),
                str(SCRIPT),
                self.candidate_sha,
                digest,
                "e2e",
                "rust-e2e-broad",
                "first-fails",
                str(logs / "first.log"),
                "logs/first.log",
                "--",
                "bash",
                "-c",
                "exit 7",
            ],
            check=False,
        )
        passed = subprocess.run(
            [
                str(SELECTOR_RUNNER),
                str(results),
                str(SCRIPT),
                self.candidate_sha,
                digest,
                "e2e",
                "rust-e2e-broad",
                "second-passes",
                str(logs / "second.log"),
                "logs/second.log",
                "--",
                "bash",
                "-c",
                "exit 0",
            ],
            check=False,
        )

        self.assertEqual(failed.returncode, 7)
        self.assertEqual(passed.returncode, 0)
        rows = [json.loads(line) for line in results.read_text(encoding="utf-8").splitlines()]
        self.assertEqual(
            [(row["selector"], row["status"]) for row in rows],
            [("first-fails", "failed"), ("second-passes", "passed")],
        )


if __name__ == "__main__":
    unittest.main()
