"""Exercise the CI entry point against real migration files and mutations."""

import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


CHECKER = Path(__file__).with_name("check-migration-checksums.py")


class MigrationChecksumsTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)
        self.baseline = self.directory / "001_baseline.sql"
        self.baseline.write_bytes(b"CREATE TABLE example (id int);\n")
        self.manifest = self.directory / "checksums.sha384"
        self.manifest.write_text(self.entry(self.baseline))

    def entry(self, path):
        return f"{hashlib.sha384(path.read_bytes()).hexdigest()}  {path.name}\n"

    def run_check(self, success):
        result = subprocess.run(
            [sys.executable, str(CHECKER), "--directory", str(self.directory)],
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0 if success else 1, result.stdout + result.stderr)
        return result.stderr

    def test_unchanged_catalog_passes(self):
        self.run_check(True)

    def test_comment_only_edit_fails(self):
        self.baseline.write_bytes(self.baseline.read_bytes() + b"-- clarification\n")
        self.assertIn("bytes changed", self.run_check(False))

    def test_deleted_migration_fails(self):
        self.baseline.unlink()
        self.assertIn("missing", self.run_check(False))

    def test_renamed_migration_fails(self):
        self.baseline.rename(self.directory / "001_renamed.sql")
        self.assertIn("missing", self.run_check(False))

    def test_new_migration_requires_catalog_entry(self):
        new = self.directory / "002_feature.sql"
        new.write_bytes(b"ALTER TABLE example ADD COLUMN name text;\n")
        self.assertIn("new migration", self.run_check(False))
        with self.manifest.open("a") as handle:
            handle.write(self.entry(new))
        self.run_check(True)

    def test_missing_or_empty_catalog_fails(self):
        self.manifest.unlink()
        self.run_check(False)
        self.manifest.write_text("")
        self.assertIn("empty", self.run_check(False))

    def test_distinct_names_cannot_share_a_migration_version(self):
        duplicate = self.directory / "001_other_change.sql"
        duplicate.write_text("SELECT 1;\n")
        with self.manifest.open("a") as handle:
            handle.write(self.entry(duplicate))
        self.assertIn("duplicate migration version 1", self.run_check(False))

    def test_invalid_or_duplicate_entries_fail(self):
        for contents in ("invalid\n", self.entry(self.baseline) * 2,
                         f"{'0' * 96}  ../001_baseline.sql\n"):
            with self.subTest(contents=contents):
                self.manifest.write_text(contents)
                self.run_check(False)

    def test_symlink_migration_fails(self):
        data = self.directory / "outside"
        self.baseline.rename(data)
        self.baseline.symlink_to(data)
        self.assertIn("regular file", self.run_check(False))


if __name__ == "__main__":
    unittest.main()
