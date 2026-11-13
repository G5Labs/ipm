#!/usr/bin/env python3
"""Daemon-free contract for infra/docker/Dockerfile.g5d-pairsoft-preview.

The image only builds in build-mode deploys, so these checks keep the
Dockerfile, its vendored npm seeds and the g5dev files it shares honest
before a deploy proves the build. infra/docker/test-pairsoft-preview-image.sh
is the Docker-backed twin for operators.
"""

import json
import re
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from test_g5dev_cache_writability import logical_lines  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
DOCKERFILE = ROOT / "infra/docker/Dockerfile.g5d-pairsoft-preview"
G5DEV_DOCKERFILE = ROOT / "infra/docker/Dockerfile.g5d-g5dev"
APP_SEED = ROOT / "infra/docker/pairsoft-npm/package.json"
E2E_SEED = ROOT / "infra/docker/pairsoft-e2e-npm/package.json"
PROBE_SCRIPT = ROOT / "infra/docker/test-pairsoft-preview-image.sh"
SHA256 = re.compile(r"^[0-9a-f]{64}$")


def dockerfile_args(text):
    return dict(re.findall(r"^ARG ([A-Z0-9_]+)=(\S+)$", text, re.M))


def run_blocks(text):
    """RUN instructions, continuations joined, in order (shared g5dev parser)."""
    return [line[4:] for line in logical_lines(text) if line.startswith("RUN ")]


class PairsoftPreviewImageContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = DOCKERFILE.read_text()
        cls.args = dockerfile_args(cls.text)
        cls.runs = run_blocks(cls.text)
        cls.gate = cls.runs[-1]

    def test_release_tarballs_are_pinned_and_checksummed(self):
        for tool in ("POWERSHELL", "SQLCMD", "NODE"):
            self.assertIn(f"{tool}_VERSION", self.args, tool)
            self.assertRegex(self.args[f"{tool}_SHA256"], SHA256, tool)
            self.assertIn(f"${{{tool}_SHA256}}", self.text)
        self.assertEqual(self.args["POWERSHELL_VERSION"].split(".")[:2], ["7", "4"], "7.4 LTS line")

    def test_playwright_pin_matches_the_e2e_seed(self):
        e2e = json.loads(E2E_SEED.read_text())
        self.assertEqual(
            e2e["devDependencies"]["@playwright/test"], self.args["PLAYWRIGHT_VERSION"],
            "bump the ARG and the seed together",
        )
        self.assertIn('install chromium', self.text)
        self.assertIn("PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright", self.text)
        for home in ("/home/ubuntu", "/home/agent"):
            self.assertIn(f"ln -s /opt/ms-playwright {home}/.cache/ms-playwright", self.text)

    def test_seeds_are_vendored_pairsoft_package_files(self):
        app = json.loads(APP_SEED.read_text())
        self.assertIn("@angular/core", app["dependencies"])
        self.assertIn("COPY infra/docker/pairsoft-npm/package.json /opt/pairsoft/npm-seed/package.json", self.text)
        self.assertIn("COPY infra/docker/pairsoft-e2e-npm/package.json /opt/pairsoft/npm-seed-e2e/package.json", self.text)
        warm = next(r for r in self.runs if "--offline" in r)
        for seed in ("/opt/pairsoft/npm-seed", "/opt/pairsoft/npm-seed-e2e"):
            self.assertIn(seed, warm)
        # [I30]: no app node_modules, lockfile or dist may ship.
        self.assertNotIn("package-lock.json", self.text)
        self.assertIn("rm -rf /tmp/npm-warm", warm)
        self.assertIn("chown -R 1000:1000 /opt/pairsoft/npm-cache", warm, "chown inside the creating layer")
        # A failed iteration must fail the layer: `for` swallows a failed
        # `&&` chain unless the chain itself exits.
        self.assertEqual(len(re.findall(r"\|\| exit 1;\s+done", warm)), 2, "both seed loops exit on failure")

    def test_in_guest_docker_reuses_the_g5dev_files(self):
        g5dev = G5DEV_DOCKERFILE.read_text()
        self.assertIn("COPY infra/docker/g5-dev-dockerd-start.sh /usr/local/bin/g5-dev-dockerd-start", self.text)
        self.assertIn("COPY infra/docker/g5-dev-dockerd-start.sh /usr/local/bin/g5-dev-dockerd-start", g5dev)
        for key in ("com.g5labs.runtime.docker.engine", "com.g5labs.runtime.kernel.features"):
            label = re.search(rf'{re.escape(key)}="([^"]+)"', self.text)
            self.assertIsNotNone(label, key)
            self.assertIn(f'{key}="{label.group(1)}"', g5dev, f"{key} must match g5dev")
        for asset in (
            "COPY infra/docker/g5-docker-engine-install.sh /usr/local/share/g5/docker-engine-install.sh",
            "COPY infra/docker/g5-dockerd-daemon.json /etc/docker/daemon.json",
            "COPY infra/docker/g5-dev-dockerd.sudoers /etc/sudoers.d/g5-dev-dockerd",
            "COPY infra/docker/g5-browser-ldd-gate.sh /usr/local/bin/g5-browser-ldd-gate",
        ):
            self.assertIn(asset, self.text, asset)
            self.assertIn(asset, g5dev, f"{asset} must be shared with g5dev")
        self.assertEqual(json.loads((ROOT / "infra/docker/g5-dockerd-daemon.json").read_text())["group"], "ubuntu")

    def test_sudo_is_scoped_to_the_dockerd_starter(self):
        self.assertNotIn("NOPASSWD", self.text, "the only rule is the shared sudoers file")
        self.assertEqual(
            (ROOT / "infra/docker/g5-dev-dockerd.sudoers").read_text().split(),
            "ubuntu ALL=(root) NOPASSWD: /usr/local/bin/g5-dev-dockerd-start".split(),
        )
        self.assertIn("visudo -cf /etc/sudoers.d/g5-dev-dockerd", self.gate)
        self.assertRegex(self.text, re.compile(r"^\s+sudo \\$", re.M), "sudo package")

    def test_boot_hook_starts_dockerd_and_raises_inotify(self):
        hook = next(r for r in self.runs if "/usr/local/bin/g5-guest-boot" in r and "printf" in r)
        self.assertIn("fs.inotify.max_user_instances", hook)
        self.assertIn("'/usr/local/bin/g5-dev-dockerd-start'", hook)
        self.assertIn("test -x /usr/local/bin/g5-guest-boot", self.gate)

    def test_build_gate_covers_every_tool(self):
        for needle in (
            "pwsh -NoProfile", "powershell -NoProfile", "sqlcmd --version", "docker --version",
            "docker compose version", "docker buildx version", "func --version", "azurite --version",
            "serve --version", "chromium --version", "chromedriver --version", "node --version",
            "dotnet --list-sdks", "/usr/local/bin/g5-browser-ldd-gate /opt/ms-playwright", "perl-modules-",
        ):
            self.assertIn(needle, self.gate, needle)
        self.assertTrue(self.text.rstrip().endswith("USER 1000:1000"))

    def test_probe_script_parses(self):
        subprocess.run(["bash", "-n", str(PROBE_SCRIPT)], check=True)
        self.assertIn("--privileged", PROBE_SCRIPT.read_text())


if __name__ == "__main__":
    unittest.main()
