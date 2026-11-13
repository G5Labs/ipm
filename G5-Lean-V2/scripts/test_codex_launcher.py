#!/usr/bin/env python3
"""The checkout's Codex launcher forwards to the native CLI without touching config."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


REPO_ROOT = Path(__file__).resolve().parents[1]


class CodexLauncherTests(unittest.TestCase):
    def test_forwards_to_the_native_cli_without_config_mutation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo, home, bin_dir = root / "repo", root / "home", root / "bin"
            (repo / "scripts").mkdir(parents=True)
            (home / ".codex").mkdir(parents=True)
            bin_dir.mkdir()
            config = home / ".codex/config.toml"
            config.write_text("model = 'owner-model'\n")
            launcher = repo / "scripts/codex"
            shutil.copy2(REPO_ROOT / "scripts/codex", launcher)
            native = bin_dir / "codex"
            native.write_text("#!/bin/sh\nprintf 'cwd=%s\\n' \"$PWD\"\nprintf 'arg=%s\\n' \"$@\"\n")
            native.chmod(0o755)
            env = dict(os.environ, HOME=str(home), PATH=f"{bin_dir}:{os.environ.get('PATH', '')}")

            dry_run = subprocess.run(["bash", launcher, "--dry-run"], env=env, capture_output=True, text=True)
            self.assertEqual(dry_run.returncode, 0, dry_run.stderr)
            self.assertIn("Ordinary Codex", dry_run.stdout)

            forwarded = subprocess.run(
                ["bash", launcher, "resume", "--last"], env=env, capture_output=True, text=True
            )
            self.assertEqual(forwarded.returncode, 0, forwarded.stderr)
            self.assertEqual(forwarded.stdout, f"cwd={repo}\narg=resume\narg=--last\n")
            self.assertEqual(config.read_text(), "model = 'owner-model'\n")
            self.assertFalse((repo / ".codex").exists())


if __name__ == "__main__":
    unittest.main()
