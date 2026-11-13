#!/usr/bin/env python3
"""Regression tests for Lean's optional ontology corpus CI boundary."""

from pathlib import Path
import os
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci/run-ontology-corpus-validation.sh"


class OntologyCorpusValidationTests(unittest.TestCase):
    def run_script(self, workspace: Path, *, fail_validation: bool = False):
        bin_dir = workspace / "bin"
        bin_dir.mkdir()
        calls = workspace / "cargo-calls"
        cargo = bin_dir / "cargo"
        cargo.write_text(textwrap.dedent("""\
            #!/usr/bin/env bash
            set -euo pipefail
            printf '%s\\n' "$*" >> "${CARGO_CALLS}"
            if [[ "${FAIL_VALIDATION:-0}" == "1" && "$*" != *"--ontology-lint advisory"* ]]; then
              printf '%s\\n' '{"status":"error","message":"fixture failure"}'
              exit 2
            fi
            if [[ "$*" == *"--ontology-lint advisory"* ]]; then
              printf '%s\\n' '{"ontologyLint":{"valid":true,"diagnostics":[]}}'
            else
              printf '%s\\n' '{"valid":true,"checked":1,"issues":[]}'
            fi
        """))
        cargo.chmod(0o755)
        artifacts = workspace / "artifacts"
        env = {
            **os.environ,
            "PATH": f"{bin_dir}:{os.environ['PATH']}",
            "CARGO_CALLS": str(calls),
            "G5_CI_ARTIFACT_ROOT": str(artifacts),
            "FAIL_VALIDATION": "1" if fail_validation else "0",
        }
        result = subprocess.run(
            ["bash", str(SCRIPT)],
            cwd=workspace,
            env=env,
            text=True,
            capture_output=True,
        )
        return result, calls, artifacts

    def test_missing_directory_skips_without_invoking_cargo(self):
        with tempfile.TemporaryDirectory() as temporary:
            result, calls, artifacts = self.run_script(Path(temporary))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("SKIP: ontology corpus validation", result.stdout)
            self.assertFalse(calls.exists())
            self.assertFalse(artifacts.exists())

    def test_existing_directory_runs_both_validations_against_repo_root(self):
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary)
            (workspace / ".semlift/specs").mkdir(parents=True)
            result, calls, artifacts = self.run_script(workspace)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(
                calls.read_text().splitlines(),
                [
                    "run -p g5d -- --workspace . --json spec validate --all",
                    "run -p g5d -- --workspace . --json spec validate --all --ontology-lint advisory",
                ],
            )
            self.assertTrue((artifacts / "ontology-v4-validation.json").is_file())
            self.assertTrue((artifacts / "ontology-v4-lint-advisory.json").is_file())
            self.assertIn("valid=True checked=1 issues=0", result.stdout)

    def test_structural_validation_failure_propagates_and_prints_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary)
            (workspace / ".semlift/specs").mkdir(parents=True)
            result, calls, _ = self.run_script(workspace, fail_validation=True)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertEqual(len(calls.read_text().splitlines()), 1)
            self.assertIn("ontology-v4 corpus validation failed (exit 2)", result.stdout)
            self.assertIn('"message":"fixture failure"', result.stdout)


if __name__ == "__main__":
    unittest.main()
