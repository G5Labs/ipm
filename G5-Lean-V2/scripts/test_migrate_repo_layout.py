#!/usr/bin/env python3
"""Tests for the one-off per-organization repository layout move (#1079)."""

from __future__ import annotations

import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("migrate_repo_layout", Path(__file__).with_name("migrate-repo-layout.py"))
mrl = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mrl)

ORG_A = "a0000000-0000-4000-8000-000000000001"
ORG_B = "b0000000-0000-4000-8000-000000000002"
REPO_1 = "10000000-0000-4000-8000-000000000001"  # org A, with review artifacts and a lock
REPO_2 = "20000000-0000-4000-8000-000000000002"  # org A
REPO_3 = "30000000-0000-4000-8000-000000000003"  # org B
REPO_MISSING = "40000000-0000-4000-8000-000000000004"  # org B, no directory
ORPHAN = "90000000-0000-4000-8000-000000000009"
ROWS = [(REPO_1, ORG_A), (REPO_2, ORG_A), (REPO_3, ORG_B), (REPO_MISSING, ORG_B)]


def make_repo(path: Path, marker: str) -> None:
    (path / "refs" / "g5").mkdir(parents=True)
    (path / "HEAD").write_text("ref: refs/heads/main\n")
    (path / "refs" / "g5" / "snapshot").write_text(marker + "\n")


class MigrateRepoLayoutTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name) / "repos"
        self.base.mkdir()
        for repo in (REPO_1, REPO_2, REPO_3):
            make_repo(self.base / repo, repo)
        make_repo(self.base / ORPHAN, ORPHAN)
        (self.base / ".review-artifacts" / REPO_1).mkdir(parents=True)
        (self.base / ".review-artifacts" / REPO_1 / "report.json").write_text("{}\n")
        (self.base / f".{REPO_1}.lock").write_text("")
        (self.base / ".worktrees").mkdir()
        (self.base / ".attachments").mkdir()

    def tearDown(self):
        self.temp.cleanup()

    def run_migrate(self, rows=ROWS, dry_run=False):
        lines: list[str] = []
        status = mrl.migrate(self.base, rows, dry_run=dry_run, out=lines.append)
        return status, "\n".join(lines)

    def snapshot(self) -> list[str]:
        return sorted(str(p.relative_to(self.base)) for p in self.base.rglob("*"))

    def test_dry_run_reports_moves_and_changes_nothing(self):
        before = self.snapshot()
        status, output = self.run_migrate(dry_run=True)
        self.assertEqual(status, 0, output)
        self.assertEqual(self.snapshot(), before)
        self.assertIn(f"DRY-RUN MOVE: {self.base / REPO_1} -> {self.base / ORG_A / REPO_1}", output)
        self.assertIn(f"DRY-RUN MOVE: {self.base / REPO_3} -> {self.base / ORG_B / REPO_3}", output)
        self.assertIn(f"DRY-RUN MOVE-ARTIFACTS: {self.base / '.review-artifacts' / REPO_1}", output)
        self.assertIn(f"DRY-RUN DELETE-LOCK: {self.base / f'.{REPO_1}.lock'}", output)
        self.assertIn(f"DRY-RUN REMOVE-EMPTY: {self.base / '.review-artifacts'}", output)
        self.assertIn(f"DRY-RUN ORPHAN: {self.base / ORPHAN}", output)
        self.assertIn(f"DRY-RUN NO-DIRECTORY: repo {REPO_MISSING}", output)
        self.assertIn("Dry run: 4 rows; 3 to move", output)

    def test_moves_reports_and_is_idempotent(self):
        status, output = self.run_migrate()
        self.assertEqual(status, 0, output)
        for repo, org in ((REPO_1, ORG_A), (REPO_2, ORG_A), (REPO_3, ORG_B)):
            self.assertFalse((self.base / repo).exists())
            # Content, including G5-only refs, moves intact.
            self.assertEqual((self.base / org / repo / "refs" / "g5" / "snapshot").read_text(), repo + "\n")
        self.assertEqual(
            (self.base / ORG_A / ".review-artifacts" / REPO_1 / "report.json").read_text(), "{}\n")
        self.assertFalse((self.base / ".review-artifacts").exists())
        self.assertFalse((self.base / f".{REPO_1}.lock").exists())
        # Orphans and shared directories stay where they are.
        self.assertTrue((self.base / ORPHAN / "HEAD").is_file())
        self.assertTrue((self.base / ".worktrees").is_dir())
        self.assertTrue((self.base / ".attachments").is_dir())
        self.assertFalse((self.base / ORG_B / REPO_MISSING).exists())
        orphan_lines = [line for line in output.splitlines() if line.startswith("ORPHAN")]
        self.assertEqual(orphan_lines, [f"ORPHAN: {self.base / ORPHAN} (no repos row names it; left in place)"])
        self.assertIn(f"NO-DIRECTORY: repo {REPO_MISSING} (org {ORG_B})", output)
        self.assertNotIn("LEFT-BEHIND", output)

        after = self.snapshot()
        status, output = self.run_migrate()
        self.assertEqual(status, 0, output)
        self.assertEqual(self.snapshot(), after)
        self.assertNotIn("MOVE", output)
        self.assertIn("4 rows; 0 moved, 3 already in place, 1 without a directory, 0 left behind", output)
        orphan_lines = [line for line in output.splitlines() if line.startswith("ORPHAN")]
        self.assertEqual(len(orphan_lines), 1, output)

    def test_existing_target_is_refused_and_fails(self):
        (self.base / ORG_A / REPO_2).mkdir(parents=True)
        (self.base / ORG_A / REPO_2 / "keep").write_text("existing\n")
        status, output = self.run_migrate(dry_run=True)
        self.assertEqual(status, 1, output)
        self.assertIn("REFUSED", output)
        status, output = self.run_migrate()
        self.assertEqual(status, 1, output)
        self.assertIn(f"REFUSED: {self.base / REPO_2} -> {self.base / ORG_A / REPO_2}", output)
        self.assertIn(f"LEFT-BEHIND: {self.base / REPO_2} still exists", output)
        self.assertIn("FAILED", output)
        # Neither side is touched; the other rows still move.
        self.assertEqual((self.base / ORG_A / REPO_2 / "keep").read_text(), "existing\n")
        self.assertTrue((self.base / REPO_2 / "HEAD").is_file())
        self.assertTrue((self.base / ORG_A / REPO_1 / "HEAD").is_file())

    def test_old_directory_that_reappears_fails_the_check(self):
        status, _ = self.run_migrate()
        self.assertEqual(status, 0)
        # A writer that was not stopped (the old release) recreates a mirror.
        make_repo(self.base / REPO_3, "recloned")
        status, output = self.run_migrate()
        self.assertEqual(status, 1, output)
        self.assertIn(f"LEFT-BEHIND: {self.base / REPO_3} still exists", output)

    def test_rows_parse_and_database_read_requires_rls_bypass(self):
        text = f"{REPO_1} {ORG_A}\n{REPO_2}|{ORG_A}\n\n{REPO_3.upper()}\t{ORG_B}\n"
        self.assertEqual(mrl.parse_rows(text), [(REPO_1, ORG_A), (REPO_2, ORG_A), (REPO_3, ORG_B)])
        with self.assertRaises(SystemExit):
            mrl.parse_rows(f"{REPO_1} {ORG_A}\n{REPO_1} {ORG_B}\n")
        with self.assertRaises(SystemExit):
            mrl.parse_rows(f"../etc {ORG_A}\n")
        with patch.object(mrl, "psql", side_effect=["f\n"]):
            with self.assertRaises(SystemExit):
                mrl.read_rows_from_database("postgres://example.invalid/db")
        with patch.object(mrl, "psql", side_effect=["t\n", f"{REPO_1} {ORG_A}\n"]) as psql:
            self.assertEqual(mrl.read_rows_from_database("postgres://example.invalid/db"), [(REPO_1, ORG_A)])
        self.assertIn("SELECT id, org_id FROM repos", psql.call_args_list[1].args[1])

    def test_command_line_with_rows_file(self):
        rows = Path(self.temp.name) / "rows.txt"
        rows.write_text("".join(f"{repo} {org}\n" for repo, org in ROWS))
        script = Path(__file__).with_name("migrate-repo-layout.py")
        command = [sys.executable, str(script), "--base", str(self.base), "--rows-file", str(rows)]
        dry = subprocess.run([*command, "--dry-run"], capture_output=True, text=True)
        self.assertEqual(dry.returncode, 0, dry.stdout + dry.stderr)
        self.assertTrue((self.base / REPO_1).is_dir())
        real = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(real.returncode, 0, real.stdout + real.stderr)
        self.assertTrue((self.base / ORG_A / REPO_1).is_dir())
        empty = Path(self.temp.name) / "empty.txt"
        empty.write_text("")
        refused = subprocess.run([sys.executable, str(script), "--base", str(self.base), "--rows-file", str(empty)],
                                 capture_output=True, text=True)
        self.assertNotEqual(refused.returncode, 0)


if __name__ == "__main__":
    unittest.main()
