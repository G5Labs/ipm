#!/usr/bin/env python3
"""Exercise the harness staging in the Packer and local runtime scripts."""

import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[1]


def run(commands, env):
    return subprocess.run(["bash", "-euo", "pipefail", "-c", commands], env=env, capture_output=True, text=True)


class HarnessStagingTests(unittest.TestCase):
    def check_staging(self, commands, env_for):
        """Stage dedicated instructions and reject missing or empty files."""
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "src"
            (source / ".agents").mkdir(parents=True)
            instructions = source / ".agents/AGENTS.md"
            shutil.copy2(REPO_ROOT / ".agents/AGENTS.md", instructions)
            (source / "AGENTS.md").write_text("Repository development instructions must not ship.")
            shutil.copytree(REPO_ROOT / ".agents/skills", source / ".agents/skills")
            shutil.copytree(REPO_ROOT / ".agents/guidance", source / ".agents/guidance")
            assets = Path(tmp) / "runtime/assets"
            env = dict(os.environ, **env_for(source, assets))

            staged = run(commands(assets), env)
            self.assertEqual(staged.returncode, 0, staged.stdout + staged.stderr)
            harness = assets / "harness"
            self.assertEqual((harness / "AGENTS.md").read_bytes(), instructions.read_bytes())
            skills = sorted(path.name for path in (REPO_ROOT / ".agents/skills").iterdir() if path.is_dir())
            self.assertTrue(skills)
            self.assertEqual(sorted(path.name for path in (harness / ".agents/skills").iterdir()), skills)
            for skill in skills:
                self.assertEqual(
                    (harness / ".agents/skills" / skill / "SKILL.md").read_bytes(),
                    (REPO_ROOT / ".agents/skills" / skill / "SKILL.md").read_bytes(),
                )

            for path in (source / ".agents/guidance").rglob("*"):
                if path.is_file():
                    self.assertEqual((harness / ".agents/guidance" / path.relative_to(source / ".agents/guidance")).read_bytes(), path.read_bytes())

            # agent.turn refuses a skill directory without SKILL.md on every turn.
            (source / ".agents/skills/stray").mkdir()
            refused = run(commands(assets), env)
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("harness skill incomplete", refused.stderr)

            (source / ".agents/skills/stray").rmdir()
            instructions.write_text("Packaged workflow version two\n")
            staged = run(commands(assets), env)
            self.assertEqual(staged.returncode, 0, staged.stdout + staged.stderr)
            self.assertEqual((harness / "AGENTS.md").read_bytes(), instructions.read_bytes())

            instructions.write_text("")
            refused = run(commands(assets), env)
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("harness not staged", refused.stderr)

            instructions.unlink()
            refused = run(commands(assets), env)
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("AGENTS.md", refused.stderr)

    def test_packer_stages_the_dedicated_harness(self):
        script = (REPO_ROOT / "infra/packer/g5-worker/scripts/30-build-g5d.sh").read_text()
        start = script.index("# Stage the harness")
        stage = script[start:script.index("# Retain build scripts", start)]
        self.check_staging(
            lambda assets: stage.replace(
                "HARNESS=/var/g5d-data/runtime/assets/harness",
                f"HARNESS={shlex.quote(str(assets / 'harness'))}",
            ),
            lambda source, _: {"SRC_DIR": str(source)},
        )

    def test_local_staging_copies_the_dedicated_harness(self):
        script = (REPO_ROOT / "scripts/stage-local-worker-runtime.sh").read_text()
        start = script.index("# --- 2. Provided harness")
        stage = script[start:script.index('if [ "$ONLY_G5D" = "1" ]', start)]
        prelude = 'info() { :; }\nfail() { printf "%s\\n" "$*" >&2; exit 1; }\nIMAGE_CID=""\n'
        self.check_staging(
            lambda _: prelude + stage,
            lambda source, assets: {"GEN_ROOT": str(source), "RUNTIME_ASSETS": str(assets)},
        )


if __name__ == "__main__":
    unittest.main()
