#!/usr/bin/env python3
"""Run the images' shared browser gate (infra/docker/g5-browser-ldd-gate.sh,
installed as /usr/local/bin/g5-browser-ldd-gate by Dockerfile.g5d-g5dev and
Dockerfile.g5d-pairsoft-preview) against Linux ELF fixtures."""

from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
GATE = ROOT / "infra/docker/g5-browser-ldd-gate.sh"


@unittest.skipUnless(sys.platform.startswith("linux"), "requires Linux ldd")
class BrowserGateTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.browsers = self.root / "browsers"
        self.browsers.mkdir()

    def binary(self, relative, broken=False):
        path = self.browsers / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        payload = Path("/bin/true").read_bytes()
        if broken:
            self.assertIn(b"libc.so.6\0", payload)
            payload = payload.replace(b"libc.so.6\0", b"gone.so.6\0", 1)
        path.write_bytes(payload)
        path.chmod(0o755)
        return path

    def gate(self):
        return subprocess.run(
            ["bash", str(GATE), str(self.browsers)],
            capture_output=True, text=True,
        )

    def test_one_revision_has_both_required_browser_types(self):
        for headless in ["chrome-headless-shell", "headless_shell"]:
            with self.subTest(headless=headless):
                shutil.rmtree(self.browsers)
                self.browsers.mkdir()
                self.binary("chromium-1208/chrome-linux64/chrome")
                self.binary(f"chromium_headless_shell-1208/chrome-headless-shell-linux64/{headless}")
                result = self.gate()
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_multiple_revisions_remain_supported(self):
        for revision in [1208, 1234]:
            self.binary(f"chromium-{revision}/chrome-linux/chrome")
            self.binary(f"chromium_headless_shell-{revision}/chrome-linux/headless_shell")
        result = self.gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_duplicate_chrome_binaries_do_not_replace_headless_shell(self):
        for revision in [1208, 1234, 1235]:
            self.binary(f"chromium-{revision}/chrome-linux/chrome")
        self.assertNotEqual(self.gate().returncode, 0)

    def test_duplicate_headless_binaries_do_not_replace_chrome(self):
        for revision in [1208, 1234, 1235]:
            self.binary(f"chromium_headless_shell-{revision}/chrome-linux/headless_shell")
        self.assertNotEqual(self.gate().returncode, 0)

    def test_empty_browser_installation_fails(self):
        self.assertNotEqual(self.gate().returncode, 0)

    def test_every_baked_browser_must_have_resolvable_libraries(self):
        self.binary("chromium-1208/chrome-linux64/chrome")
        self.binary("chromium_headless_shell-1208/chrome-headless-shell-linux64/chrome-headless-shell")
        broken = self.binary("chromium-1234/chrome-linux64/chrome", broken=True)
        result = self.gate()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(str(broken), result.stdout + result.stderr)
        self.assertIn("FATAL:", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
