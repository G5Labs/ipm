from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from plan_playwright_shards import (  # noqa: E402
    PlanError,
    load_durations,
    make_plan,
    measure_durations,
    predict_ms,
    verify_plan,
)


def report(rows: list[tuple[str, str]], durations: dict[str, list[int]] | None = None) -> dict:
    def test(test_id: str) -> dict:
        entry = {"projectName": "chromium"}
        if durations is not None:
            entry["results"] = [{"duration": ms} for ms in durations[test_id]]
        return entry

    return {
        "errors": [],
        "suites": [
            {
                "file": file,
                "specs": [{"file": file, "id": test_id, "tests": [test(test_id)]}],
            }
            for file, test_id in rows
        ],
    }


MINUTE = 60_000


class ShardPlanTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.original = [self.root / "original-1.json", self.root / "original-2.json"]
        self.planned = [self.root / "planned-1.json", self.root / "planned-2.json"]
        self.durations = self.root / "durations.json"
        # Discovery order is a < b < … ; the heavy review files sort last, as in CI.
        self.rows = [
            [("a.spec.ts", "a1"), ("b.spec.ts", "b1"), ("c.spec.ts", "c1"), ("d.spec.ts", "d1")],
            [("e.spec.ts", "e1"), ("w-content.spec.ts", "wc1"), ("w-journey.spec.ts", "wj1"),
             ("w-recovery.spec.ts", "wr1"), ("w-usability.spec.ts", "wu1")],
        ]
        self.write_originals(self.rows)
        self.write_durations({
            "a.spec.ts": 2 * MINUTE, "b.spec.ts": 2 * MINUTE, "c.spec.ts": 2 * MINUTE,
            "d.spec.ts": 2 * MINUTE, "e.spec.ts": 6 * MINUTE,
            "w-content.spec.ts": 15 * MINUTE, "w-journey.spec.ts": 19 * MINUTE,
            "w-recovery.spec.ts": 9 * MINUTE, "w-usability.spec.ts": 8 * MINUTE,
        })

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write_originals(self, rows: list[list[tuple[str, str]]]) -> None:
        for path, shard_rows in zip(self.original, rows):
            path.write_text(json.dumps(report(shard_rows)), encoding="utf-8")

    def write_durations(self, files: dict[str, int]) -> None:
        self.durations.write_text(json.dumps({"files": files}), encoding="utf-8")

    def plan(self) -> dict:
        make_plan(self.original, self.root, self.durations)
        return json.loads((self.root / "plan.json").read_text(encoding="utf-8"))

    def write_planned_reports(self, plan: dict) -> None:
        rows = [row for shard in self.rows for row in shard]
        for path, files in zip(self.planned, plan["plannedFiles"]):
            path.write_text(json.dumps(report([row for row in rows if row[0] in files])), encoding="utf-8")

    def test_prediction_replays_discovery_order_onto_the_earliest_free_worker(self) -> None:
        order = {"a.spec.ts": 0, "b.spec.ts": 1, "z.spec.ts": 2}
        weights = {"a.spec.ts": 4, "b.spec.ts": 4, "z.spec.ts": 10}
        # z starts only after a worker frees at 4, so the shard ends at 14, not 10.
        self.assertEqual(predict_ms(list(order), order, weights, 2), 14)
        self.assertEqual(predict_ms(["z.spec.ts", "a.spec.ts"], order, weights, 2), 10)
        self.assertEqual(predict_ms(list(order), order, weights, 1), 18)

    def test_places_whole_heavy_files_apart_and_preserves_every_test(self) -> None:
        plan = self.plan()
        # Optimal here: no shard outlasts the heaviest file, which must start first on its worker.
        self.assertEqual(max(plan["predictedShardMs"]), 19 * MINUTE)
        order = {file: index for index, (file, _id) in enumerate(row for shard in self.rows for row in shard)}
        weights = plan["fileWeightsMs"]
        original = max(predict_ms([f for f, _ in shard], order, weights, 2) for shard in self.rows)
        self.assertEqual(original, 32 * MINUTE)
        review = {"w-content.spec.ts", "w-journey.spec.ts", "w-recovery.spec.ts", "w-usability.spec.ts"}
        self.assertTrue(all(review - set(files) for files in plan["plannedFiles"]))
        all_ids = sorted(test_id for shard in self.rows for _file, test_id in shard)
        self.assertEqual(sorted(plan["plannedIds"][0] + plan["plannedIds"][1]), all_ids)
        self.assertFalse(set(plan["plannedFiles"][0]) & set(plan["plannedFiles"][1]))
        for index, files in enumerate(plan["plannedFiles"], start=1):
            listed = (self.root / f"shard-{index}.test-list").read_text(encoding="utf-8").split()
            self.assertEqual(listed, files)
        self.write_planned_reports(plan)
        verify_plan(self.root / "plan.json", self.planned)

    def test_plan_is_deterministic(self) -> None:
        first = self.plan()
        self.assertEqual(self.plan(), first)

    def test_unmeasured_files_weigh_the_measured_mean_and_are_listed(self) -> None:
        self.rows[0].append(("new-suite.spec.ts", "n1"))
        self.write_originals(self.rows)
        plan = self.plan()
        self.assertEqual(plan["unmeasuredFiles"], ["new-suite.spec.ts"])
        measured = [w for f, w in plan["fileWeightsMs"].items() if f != "new-suite.spec.ts"]
        self.assertEqual(plan["fileWeightsMs"]["new-suite.spec.ts"], round(sum(measured) / len(measured)))
        self.assertIn("n1", plan["plannedIds"][0] + plan["plannedIds"][1])

    def test_invalid_duration_tables_are_refused(self) -> None:
        for files in ({}, {"a.spec.ts": -1}, {"a.spec.ts": "60"}, {"a.txt": 1}, {"a.spec.ts": True}):
            self.write_durations(files)  # type: ignore[arg-type]
            with self.assertRaisesRegex(PlanError, "invalid file durations"):
                self.plan()
        self.write_durations({"elsewhere.spec.ts": 1})
        with self.assertRaisesRegex(PlanError, "no file in the broad inventory"):
            self.plan()

    def test_missing_or_duplicated_test_fails_before_shards_run(self) -> None:
        plan = self.plan()
        self.write_planned_reports(plan)
        moved = json.loads(self.planned[0].read_text(encoding="utf-8"))
        extra = json.loads(self.planned[1].read_text(encoding="utf-8"))
        moved["suites"].append(extra["suites"].pop())
        self.planned[0].write_text(json.dumps(moved), encoding="utf-8")
        self.planned[1].write_text(json.dumps(extra), encoding="utf-8")
        with self.assertRaisesRegex(PlanError, "file inventory changed"):
            verify_plan(self.root / "plan.json", self.planned)

        self.write_planned_reports(plan)
        dropped = json.loads(self.planned[0].read_text(encoding="utf-8"))
        dropped["suites"][0]["specs"][0]["id"] = "renamed"
        self.planned[0].write_text(json.dumps(dropped), encoding="utf-8")
        with self.assertRaisesRegex(PlanError, "test-ID inventory changed"):
            verify_plan(self.root / "plan.json", self.planned)

        self.write_originals([self.rows[0] + [("e.spec.ts", "e1")], self.rows[1]])
        with self.assertRaisesRegex(PlanError, "more than one original shard"):
            make_plan(self.original, self.root, self.durations)

    def test_three_shards_balance_the_measured_tail_and_partition_every_test(self) -> None:
        # The 2026-10-27 CI tail (minutes, heaviest first) on three one-worker shards:
        # no exhaustive 8-file placement lets the slowest shard outlast total/3 by much.
        heavy = {
            "w-journey.spec.ts": 19, "w-content.spec.ts": 15, "w-recovery.spec.ts": 10,
            "sc-selection.spec.ts": 9, "w-usability.spec.ts": 8, "project-doc.spec.ts": 5,
            "sc-comparisons.spec.ts": 4, "alignment-shell.spec.ts": 3, "explicit-save.spec.ts": 3,
            "fork-outcomes.spec.ts": 2, "question-store.spec.ts": 2,
        }
        light = {f"f{index:02d}.spec.ts": 1 for index in range(23)}
        files = sorted({**heavy, **light})  # discovery order; CI lists 61 files
        rows = [[(file, file.removesuffix(".spec.ts")) for file in files[start::3]] for start in range(3)]
        rows = [[(file, test_id) for file, test_id in sorted(row)] for row in rows]
        self.original = [self.root / f"original-{index}.json" for index in (1, 2, 3)]
        self.planned = [self.root / f"planned-{index}.json" for index in (1, 2, 3)]
        self.write_originals(rows)
        self.write_durations({name: minutes * MINUTE for name, minutes in {**heavy, **light}.items()})
        make_plan(self.original, self.root, self.durations, workers=1, shards=3)
        plan = json.loads((self.root / "plan.json").read_text(encoding="utf-8"))
        self.assertEqual(len(plan["plannedFiles"]), 3)
        self.assertEqual(len(plan["plannedIds"]), 3)
        total = sum({**heavy, **light}.values())
        self.assertEqual(sum(plan["predictedShardMs"]), total * MINUTE)
        self.assertLessEqual(max(plan["predictedShardMs"]), (total // 3 + 1) * MINUTE)
        self.assertEqual(sorted(sum(plan["plannedFiles"], [])), files)
        self.assertEqual(sorted(sum(plan["plannedIds"], [])), sorted(file.removesuffix(".spec.ts") for file in files))
        for index in (1, 2, 3):
            listed = (self.root / f"shard-{index}.test-list").read_text(encoding="utf-8").split()
            self.assertEqual(listed, plan["plannedFiles"][index - 1])
        all_rows = [row for shard in rows for row in shard]
        for path, planned in zip(self.planned, plan["plannedFiles"]):
            path.write_text(json.dumps(report([row for row in all_rows if row[0] in planned])), encoding="utf-8")
        verify_plan(self.root / "plan.json", self.planned)
        with self.assertRaisesRegex(PlanError, "exactly 3 planned shard reports"):
            verify_plan(self.root / "plan.json", self.planned[:2])
        with self.assertRaisesRegex(PlanError, "exactly 3 original shard inventories"):
            make_plan(self.original[:2], self.root, self.durations, workers=1, shards=3)

    def test_plan_with_unexpected_keys_is_refused(self) -> None:
        plan = self.plan()
        self.write_planned_reports(plan)
        plan["extra"] = True
        (self.root / "plan.json").write_text(json.dumps(plan), encoding="utf-8")
        with self.assertRaisesRegex(PlanError, "invalid Playwright shard plan"):
            verify_plan(self.root / "plan.json", self.planned)


class DurationRefreshTests(unittest.TestCase):
    def test_median_of_first_attempts_from_reports_and_shard_summaries(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            rows = [("a.spec.ts", "a1"), ("a.spec.ts", "a2"), ("b.spec.ts", "b1")]
            # A retried test counts its first attempt only.
            (root / "r1.json").write_text(json.dumps(report(rows, {"a1": [100], "a2": [50, 999], "b1": [10]})))
            (root / "r2.json").write_text(json.dumps(report(rows, {"a1": [300], "a2": [50], "b1": [30]})))
            (root / "s3.json").write_text(json.dumps({
                "kind": "g5-shard-durations",
                "files": {"e2e/flows/a.spec.ts": {"firstAttemptMs": 120, "retryMs": 0}},
            }))
            output = root / "durations.json"
            measure_durations([root / "r1.json", root / "r2.json", root / "s3.json"], output, ["build-x"])
            table = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(table["files"], {"a.spec.ts": 150, "b.spec.ts": 20})
            self.assertEqual(table["sources"], ["build-x"])

    def test_committed_table_covers_the_current_broad_inventory_shape(self) -> None:
        table = json.loads((ROOT / "infra" / "ci" / "playwright-file-durations.json").read_text(encoding="utf-8"))
        self.assertTrue(table["sources"])
        flows = {path.name for path in (ROOT / "frontend" / "e2e" / "flows").glob("*.spec.ts")}
        for name, ms in table["files"].items():
            self.assertTrue(name.endswith(".spec.ts"), name)
            self.assertIsInstance(ms, int)
        # The four review suites dominate the broad lane; losing them would silently flatten the plan.
        # (PR-tier matrices trimmed them to roughly 8-10 minutes; the bound guards the measurement, not the size.)
        for heavy in ("workspace-review-journey.spec.ts", "workspace-review-content.spec.ts"):
            self.assertIn(heavy, flows)
            self.assertGreater(table["files"][heavy], 5 * MINUTE)


@unittest.skipUnless(shutil.which("node"), "node is required for report-shard-durations.js")
class ShardSummaryTests(unittest.TestCase):
    """scripts/report-shard-durations.js: the shard's timing evidence and its refresh input."""

    def run_summary(self, root: Path, *args: str) -> str:
        completed = subprocess.run(
            ["node", str(ROOT / "scripts" / "report-shard-durations.js"), "shard 1/2", *args],
            cwd=root, check=True, capture_output=True, text=True,
        )
        return completed.stdout

    def test_separates_first_attempts_retries_predictions_and_recorded_phases(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            timings = {"setup/actors": {"ms": 700, "count": 1}, "body/shell.open-terminal": {"ms": 90, "count": 3}}
            report_data = {
                "errors": [],
                "suites": [{
                    "file": "w-journey.spec.ts",
                    "specs": [
                        {"file": "w-journey.spec.ts", "id": "j1", "tests": [{"projectName": "chromium", "results": [
                            {"duration": 1000, "annotations": [{"type": "g5-timings", "description": json.dumps(timings)}]},
                            {"duration": 400, "annotations": [{"type": "g5-timings", "description": json.dumps(timings)}]},
                        ]}]},
                        {"file": "w-journey.spec.ts", "id": "j2", "tests": [{"projectName": "chromium", "results": [
                            {"duration": 500, "annotations": [{"type": "g5-timings", "description": "not json"}]},
                        ]}]},
                        {"file": "w-journey.spec.ts", "id": "j3", "tests": [{"projectName": "chromium", "results": [
                            # A beforeAll annotation copied ahead of the test's own: only the last counts.
                            {"duration": 0, "annotations": [
                                {"type": "g5-timings", "description": json.dumps({"stale/hook": {"ms": 5, "count": 1}})},
                                {"type": "g5-timings", "description": json.dumps({"setup/actors": {"ms": 300, "count": 1}})},
                            ]},
                        ]}]},
                    ],
                }, {
                    # Each shard runs the setup project as a dependency; it is never a placed file.
                    "file": "../auth.setup.ts",
                    "specs": [{"file": "auth.setup.ts", "id": "s1", "tests": [{"projectName": "setup", "results": [
                        {"duration": 4000},
                    ]}]}],
                }],
            }
            (root / "report.json").write_text(json.dumps(report_data))
            (root / "plan.json").write_text(json.dumps({
                "plannedFiles": [["w-journey.spec.ts"], []],
                "fileWeightsMs": {"w-journey.spec.ts": 1600},
                "predictedShardMs": [1600, 0],
            }))
            stdout = self.run_summary(root, "report.json", "--plan", "plan.json", "--shard", "1",
                                      "--summary", "summary.json")
            summary = json.loads((root / "summary.json").read_text())
            journey = summary["files"]["w-journey.spec.ts"]
            self.assertEqual(set(summary["files"]), {"w-journey.spec.ts"})
            self.assertEqual((journey["firstAttemptMs"], journey["retryMs"], journey["attempts"], journey["tests"]),
                             (1500, 400, 4, 3))
            self.assertEqual(journey["predictedMs"], 1600)
            # Phases count first attempts only; a malformed annotation is ignored, never fatal.
            self.assertEqual(journey["phases"], {
                "setup/actors": {"ms": 1000, "count": 2},
                "body/shell.open-terminal": {"ms": 90, "count": 3},
            })
            self.assertEqual(summary["predictedShardMs"], 1600)
            self.assertIn("first attempts 1500 ms, retries 400 ms, planner predicted 1600 ms elapsed", stdout)
            self.assertIn("w-journey.spec.ts: setup/actors=1000/2 body/shell.open-terminal=90/3", stdout)

            # The logged machine-readable line refreshes the committed table on its own.
            line = next(row for row in stdout.splitlines() if row.startswith("G5_SHARD_DURATIONS "))
            (root / "logged.txt").write_text(line + "\n")
            measure_durations([root / "logged.txt", root / "summary.json"], root / "table.json", [])
            self.assertEqual(json.loads((root / "table.json").read_text())["files"], {"w-journey.spec.ts": 1500})
            # A summary naming a non-spec file still refreshes a table the planner accepts.
            (root / "with-setup.json").write_text(json.dumps({"kind": "g5-shard-durations", "files": {
                "auth.setup.ts": {"firstAttemptMs": 4000, "retryMs": 0},
                "w-journey.spec.ts": {"firstAttemptMs": 1500, "retryMs": 0},
            }}))
            measure_durations([root / "with-setup.json"], root / "table2.json", [])
            self.assertEqual(load_durations(root / "table2.json"), {"w-journey.spec.ts": 1500})

    def test_missing_or_unreadable_report_never_fails_the_shard(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.assertIn("no report at", self.run_summary(root))
            (root / "broken.json").write_text("{")
            self.assertIn("cannot read broken.json", self.run_summary(root, "broken.json"))


if __name__ == "__main__":
    unittest.main()
