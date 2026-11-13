#!/usr/bin/env python3
"""Real Git export tests: only the pinned history may enter delivery inputs."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
import uuid

SPEC = importlib.util.spec_from_file_location(
    "prepare_seed", Path(__file__).with_name("prepare-workload-template-input.py"))
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class DeliverySeedTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "source"
        self.repo.mkdir()
        self.git("init", "-q")
        self.git("config", "user.name", "Seed fixture")
        self.git("config", "user.email", "seed@example.test")
        (self.repo / "tracked").write_text("committed\n")
        self.git("add", ".")
        self.git("commit", "-qm", "baseline")
        self.commit = self.git("rev-parse", "HEAD")
        self.repo_id = str(uuid.uuid4())
        self.config = {
            "scope": {"org_id": str(uuid.uuid4()), "cache_generation": 1,
                      "repos": [{"repo_id": self.repo_id, "root_path": "code",
                                 "git_url": "https://internal.example/git/one"}]},
            "repositories": [{"repo_id": self.repo_id, "source": "source", "revision": "HEAD"}],
        }

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.repo), *args], check=True,
                              text=True, capture_output=True).stdout.strip()

    def test_excludes_dirty_files_credentials_and_unrelated_refs(self):
        (self.repo / "tracked").write_text("dirty\n")
        (self.repo / ".env").write_text("fixture-private-setting\n")
        self.git("config", "credential.helper", "fixture-must-not-be-exported")
        # An unrelated commit must not travel in the bundle either.
        self.git("checkout", "-qb", "unrelated")
        (self.repo / "private-branch-only").write_text("other branch\n")
        self.git("add", "private-branch-only")
        self.git("commit", "-qm", "unrelated")
        unrelated = self.git("rev-parse", "HEAD")
        self.config["repositories"][0]["revision"] = self.commit
        out = self.root / "output"
        manifest = MODULE.prepare(self.config, self.root, out)
        value = json.loads(manifest.read_text())
        self.assertEqual(value["repositories"][0]["commit"], self.commit)
        bundle = out / value["repositories"][0]["bundle"]
        clone = self.root / "clone"
        subprocess.run(["git", "clone", "-q", "--branch", "seed", str(bundle), str(clone)], check=True)
        self.assertEqual((clone / "tracked").read_text(), "committed\n")
        self.assertFalse((clone / ".env").exists())
        self.assertFalse((clone / "private-branch-only").exists())
        self.assertNotIn("credential", (clone / ".git/config").read_text())
        result = subprocess.run(["git", "-C", str(clone), "cat-file", "-e", unrelated], capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(sorted(p.suffix for p in out.iterdir()), [".bundle", ".json"])

    def test_rejects_source_escape_and_mismatched_vector(self):
        self.config["repositories"][0]["source"] = ".."
        with self.assertRaisesRegex(ValueError, "escapes"):
            MODULE.prepare(self.config, self.root, self.root / "escaped")
        self.config["repositories"][0]["repo_id"] = str(uuid.uuid4())
        with self.assertRaisesRegex(ValueError, "match"):
            MODULE.prepare(self.config, self.root, self.root / "wrong-vector")


if __name__ == "__main__":
    unittest.main()
