#!/usr/bin/env python3
"""Regression tests for the specs-root handling of scripts/verify_grounding.py.

The rust PR lane (`verify-grounding-integrity` in
infra/ci/cloudbuild-ci-rust.yaml) explicitly allows the missing specs directory
because this repository tracks no `.semlift/`. The verifier stays fail-closed
without that opt-in, for an existing but empty specs directory, and whenever
specs are present.

The script resolves every path from its own location, so each case copies it
into a throwaway repository root and runs it from there.
"""
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_grounding.py"


class VerifyGroundingSpecsRoot(unittest.TestCase):
    def run_in(
        self,
        repo: Path,
        *,
        allow_missing: bool = False,
        strict: str | None = None,
    ) -> subprocess.CompletedProcess:
        (repo / "scripts").mkdir(exist_ok=True)
        shutil.copy(SCRIPT, repo / "scripts/verify_grounding.py")
        env = {
            key: value for key, value in os.environ.items()
            if key not in ("G5_VERIFY_GROUNDING_STRICT", "G5_DOCS_ROOT")
        }
        if strict:
            env["G5_VERIFY_GROUNDING_STRICT"] = strict
        command = [sys.executable, str(repo / "scripts/verify_grounding.py")]
        if allow_missing:
            command.append("--allow-missing-specs")
        return subprocess.run(
            command,
            cwd=repo, env=env, text=True, capture_output=True,
        )

    def test_missing_specs_root_fails_without_explicit_opt_in(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = self.run_in(Path(temporary))
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("FAIL: no ontology spec files found", result.stdout)

    def test_missing_specs_root_is_skipped_with_explicit_opt_in(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = self.run_in(Path(temporary), allow_missing=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("SKIP: ontology specs directory", result.stdout)
            self.assertIn("does not exist", result.stdout)
            self.assertNotIn("FAIL", result.stdout)

    def test_missing_specs_root_opt_in_applies_in_strict_mode(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = self.run_in(Path(temporary), allow_missing=True, strict="all")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("SKIP: ontology specs directory", result.stdout)

    def test_empty_specs_root_still_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            (Path(temporary) / ".semlift/specs").mkdir(parents=True)
            result = self.run_in(Path(temporary), allow_missing=True)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("FAIL: no ontology spec files found", result.stdout)
            self.assertNotIn("SKIP", result.stdout)

    def test_populated_specs_root_is_audited(self):
        with tempfile.TemporaryDirectory() as temporary:
            specs = Path(temporary) / ".semlift/specs"
            specs.mkdir(parents=True)
            (specs / "example.semlift.md").write_text(
                "# Example\n\n[I1. First intent]\n\n[D1. First decision]\n"
            )
            advisory = self.run_in(Path(temporary), allow_missing=True)
            self.assertEqual(advisory.returncode, 0, advisory.stdout + advisory.stderr)
            self.assertIn("Spec files: 1", advisory.stdout)
            self.assertIn("Missing intent record", advisory.stdout)
            strict = self.run_in(Path(temporary), allow_missing=True, strict="all")
            self.assertEqual(strict.returncode, 1, strict.stdout + strict.stderr)
            self.assertIn("intent-record drift", strict.stdout)


if __name__ == "__main__":
    unittest.main()
