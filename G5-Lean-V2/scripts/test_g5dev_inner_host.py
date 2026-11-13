#!/usr/bin/env python3
"""Keep the g5dev image able to host an inner microvm worker host.

On a nested-enabled host the full-stack preview's worker host boots real
Cloud Hypervisor guests inside the workspace VM (ontology
``g5.architecture.microvm_backend.nested``; design
``.g5/pr_ontology_diff_design/nested-virt-followup``). The in-guest
``g5d vm-exec`` needs what the host installers create: the ``g5-passt`` and
``g5-vmm`` accounts (``g5-vmm`` in the ``kvm`` group), ``nft`` for the egress
policy and ``mkfs.xfs`` for the reflink root; the platform itself travels on
the runtime disk, so the image must build no kernel and no passt. These checks
read the real Dockerfile rather than a copy of its lines.
"""

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "infra/docker/Dockerfile.g5d-g5dev"


def logical_lines(text):
    lines = []
    current = ""
    for raw in text.splitlines():
        stripped = raw.strip()
        if stripped.startswith("#") and not current:
            continue
        if stripped.endswith("\\"):
            current += stripped[:-1] + " "
            continue
        current += stripped
        if current:
            lines.append(current)
        current = ""
    return lines


class InnerHostImageTests(unittest.TestCase):
    def setUp(self):
        self.text = SOURCE.read_text()
        self.lines = logical_lines(self.text)

    def apt_install_lines(self):
        return [line for line in self.lines if "apt-get install" in line]

    def test_packages_for_the_inner_host_are_installed(self):
        installs = " ".join(self.apt_install_lines())
        for package in ("nftables", "xfsprogs"):
            self.assertRegex(installs, rf"(^|\s){package}(\s|$)", f"{package} must be installed")

    def test_identities_match_the_host_installers(self):
        account_lines = [line for line in self.lines if line.startswith("RUN") and "g5-passt" in line]
        self.assertEqual(len(account_lines), 1, "one layer creates the inner-host identities")
        line = account_lines[0]
        self.assertIn("useradd --system --no-create-home --shell /usr/sbin/nologin g5-passt", line)
        self.assertIn("useradd --system --gid g5-vmm --no-create-home --shell /usr/sbin/nologin g5-vmm", line)
        self.assertIn("usermod --append --groups kvm g5-vmm", line)
        self.assertIn('[ "$(id -u g5-passt)" != 0 ]', line)
        self.assertIn('[ "$(id -u g5-vmm)" != 0 ]', line)
        self.assertIn("test -x /usr/sbin/nft", line)
        self.assertIn("test -x /usr/sbin/mkfs.xfs", line)

    def test_image_gate_proves_the_identities_and_tools(self):
        # The final stage's tool gate is one RUN whose continuation carries
        # comment lines, so it is read as instruction text after the last FROM.
        final = self.text[self.text.rfind("\nFROM "):]
        instructions = " ".join(logical_lines(final))
        for snippet in ("id -u g5-passt >/dev/null && id -u g5-vmm >/dev/null", "id -nG g5-vmm",
                        "nft --version >/dev/null", "mkfs.xfs -V >/dev/null"):
            self.assertIn(snippet, instructions, f"the final-stage gate must run {snippet!r}")

    def test_no_platform_build_in_the_guest_image(self):
        instructions = " ".join(self.lines)
        for forbidden in ("install-cloud-hypervisor-platform.sh", "build-cloud-hypervisor-runtime.sh",
                          "58-install-passt.sh", "apparmor_parser", "passt_"):
            self.assertNotIn(forbidden, instructions, f"{forbidden} belongs to the host plane, not the guest image")
        self.assertNotRegex(" ".join(self.apt_install_lines()), r"(^|\s)passt(\s|$)",
                            "passt comes from the runtime disk, never from the archive")

    def test_profiles_reference_the_runtime_platform_not_the_image(self):
        profile = (ROOT / ".g5/preview_profiles/full-stack.yaml").read_text()
        self.assertIn("scripts/preview-microvm-host.sh", profile)
        self.assertIn("/opt/g5/platform", profile)
        prepare = (ROOT / "scripts/preview-microvm-host.sh").read_text()
        self.assertIn("/opt/g5/platform", prepare)
        for file in ("cloud-hypervisor", "vmlinux", "initramfs.cpio", "kernel.config", "passt"):
            self.assertIn(file, prepare)
        self.assertIn("getent passwd g5-passt", prepare)
        self.assertIn("full-stack-process", prepare, "the failure must point at the process-mode profile")

    def test_guest_kernel_declares_the_inner_host_options(self):
        config = (ROOT / "infra/guest/kernel.config").read_text().splitlines()
        for option in ("CONFIG_SECURITY_LANDLOCK=y", 'CONFIG_LSM="landlock"', "CONFIG_NF_TABLES_INET=y",
                       "CONFIG_NFT_CT=y", "CONFIG_NFT_FIB_IPV4=y", "CONFIG_NFT_FIB_IPV6=y",
                       "CONFIG_NFT_FIB_INET=y", "CONFIG_BLK_DEV_LOOP=y", "CONFIG_XFS_FS=y"):
            self.assertIn(option, config, f"{option} is required by the inner host's launch")


if __name__ == "__main__":
    unittest.main()
