"""Execute the CI status boundary with failed, absent and malformed receipts."""
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / 'scripts/ci-stage-status.sh'
MANIFEST = ROOT / 'infra/ci/e2e-stage-results.tsv'
STAGES = [line.split() for line in MANIFEST.read_text().splitlines()]
PIPELINE = (ROOT / 'infra/ci/cloudbuild-ci-e2e.yaml').read_text()


class StageStatus(unittest.TestCase):
    def invoke(self, *args):
        return subprocess.run(['sh', str(SCRIPT), *map(str, args)], text=True,
                              capture_output=True)

    def receipts(self, directory):
        for _, key, filename in STAGES:
            (directory / filename).write_text(f'{key}=passed\n')

    def test_gate_requires_every_stage_to_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            self.assertNotEqual(self.invoke('assert', directory, MANIFEST).returncode, 0)
            self.receipts(directory)
            self.assertEqual(self.invoke('assert', directory, MANIFEST).returncode, 0)
            for _, key, filename in STAGES:
                for invalid in (None, '', f'{key}=failed\n', 'wrong=passed\n',
                                f'{key}=skipped\n', f'{key}=passed\n{key}=failed\n'):
                    with self.subTest(stage=key, receipt=invalid):
                        path = directory / filename
                        if invalid is None:
                            path.unlink()
                        else:
                            path.write_text(invalid)
                        result = self.invoke('assert', directory, MANIFEST)
                        self.assertNotEqual(result.returncode, 0, result.stdout)
                        self.assertIn(key, result.stderr)
                        path.write_text(f'{key}=passed\n')

    def test_actual_exit_traps_fail_after_test_failure_and_hard_abort(self):
        # Execute the actual pipeline trap in both shells used by its steps.
        # Markers are still written for downstream waiters after a hard abort.
        for step, key, filename in STAGES:
            block = PIPELINE.split(f'  - id: {step}\n', 1)[1].split('\n  - id:', 1)[0]
            trap = next(line.strip() for line in block.splitlines()
                        if line.strip().startswith("trap 'rc="))
            shell = re.search(r'    entrypoint: (\w+)', block)[1]
            for code, receipt, expected in ((0, 'passed', 0), (0, 'failed', 1),
                                             (9, 'passed', 1), (9, None, 1),
                                             (0, None, 1), (0, 'skipped', 1)):
                with self.subTest(step=step, code=code, receipt=receipt):
                    with tempfile.TemporaryDirectory() as tmp:
                        directory = Path(tmp)
                        path = directory / filename
                        if receipt is not None:
                            path.write_text(f'{key}={receipt}\n')
                        actual_trap = trap.replace('$$', '$').replace(
                            '/workspace/scripts/ci-stage-status.sh', str(SCRIPT)).replace(
                            f'/workspace/{filename}', str(path))
                        result = subprocess.run([shell, '-c',
                                                 f'set -eu\n{actual_trap}\nexit {code}'],
                                                text=True, capture_output=True)
                        self.assertEqual(result.returncode, expected, result.stderr)
                        status = 'passed' if expected == 0 else 'failed'
                        self.assertEqual(path.read_text(), f'{key}={status}\n')

    def test_deferred_steps_cannot_make_the_final_build_pass(self):
        expected = {key for _, key, _ in STAGES}
        producers = set(re.findall(r'echo "([A-Z0-9_]+)=[^\n]+> /workspace/step-results-', PIPELINE))
        self.assertEqual(expected, producers)
        self.assertEqual(len(expected), len(STAGES))
        for step, _, _ in STAGES:
            block = PIPELINE.split(f'  - id: {step}\n', 1)[1].split('\n  - id:', 1)[0]
            self.assertIn('    allowFailure: true\n', block)
        gate = PIPELINE.split('  - id: assert-ci-status\n', 1)[1]
        self.assertNotIn('allowFailure:', gate)
        self.assertNotIn('allowExitCodes:', gate)
        self.assertIn('sh scripts/ci-stage-status.sh assert /workspace infra/ci/e2e-stage-results.tsv', gate)
        self.assertIn("'export-e2e-artifacts'", gate)


if __name__ == '__main__':
    unittest.main()
