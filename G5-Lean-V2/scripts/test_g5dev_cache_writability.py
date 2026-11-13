#!/usr/bin/env python3
"""Keep the g5dev image's uid-1000 cache writability in step with what it ships.

The final stage of ``infra/docker/Dockerfile.g5d-g5dev`` copies the warm cargo
registry, cargo git checkouts and sccache objects from the discarded prime
stage, then makes those trees writable for the uid-1000 agent and preview
identity in one ``chmod -R a+rwX`` layer, and the image gate probes a uid-1000
write in them. A cache tree that is copied but not covered by that chmod (or a
chmod that runs before the copy and is overwritten by it) ships root-only and
every guest build that needs a crate the image did not bake fails with
``Permission denied`` (QA, 2026-10-14; ontology concepts/deployment
``g5.scope.deployment.images.g5dev-toolchain`` and software_architecture/runtime
``g5.runtime.boot.image.rootfs``). These checks are structural: they read the
real Dockerfile rather than a copy of its lines.
"""

from pathlib import Path
import re
import shlex
import unittest


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "infra/docker/Dockerfile.g5d-g5dev"

# Reference files the final stage copies from the build stages for the gate's
# own use; they are read-only inputs, not caches the guest writes.
REFERENCE_PREFIX = "/usr/local/share/g5/"


def logical_lines(text):
    """Dockerfile instructions with backslash continuations joined."""
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


class CacheWritabilityTests(unittest.TestCase):
    def setUp(self):
        text = SOURCE.read_text()
        froms = [match.start() for match in re.finditer(r"(?m)^FROM ", text)]
        self.assertTrue(froms, "the Dockerfile declares build stages")
        self.final_stage = logical_lines(text[froms[-1] :])

    def shipped_cache_trees(self):
        trees = []
        for index, line in enumerate(self.final_stage):
            if not line.startswith("COPY ") or "--from=" not in line:
                continue
            words = shlex.split(line)
            destination = words[-1]
            if destination.startswith(REFERENCE_PREFIX):
                continue
            trees.append((index, destination))
        return trees

    def chmod_layer(self):
        for index, line in enumerate(self.final_stage):
            if line.startswith("RUN chmod -R a+rwX "):
                return index, line
        raise AssertionError("the final stage has no `RUN chmod -R a+rwX` layer")

    def test_every_shipped_cache_tree_is_made_uid_1000_writable(self):
        trees = self.shipped_cache_trees()
        self.assertTrue(trees, "the final stage copies cache trees from the prime stage")
        chmod_index, chmod_line = self.chmod_layer()
        first_command = chmod_line[len("RUN ") :].split("&&")[0]
        covered = shlex.split(first_command)[3:]
        for copy_index, tree in trees:
            self.assertTrue(
                any(tree == root or tree.startswith(root.rstrip("/") + "/") for root in covered),
                f"{tree} is copied from the prime stage but not under the chmod -R a+rwX roots {covered}",
            )
            self.assertLess(
                copy_index,
                chmod_index,
                f"{tree} is copied after the chmod layer, which the copy's own modes would undo",
            )

    def test_gate_probes_uid_1000_writes_in_the_cargo_trees(self):
        probes = [
            line
            for line in self.final_stage
            if "runuser -u ubuntu" in line and ".g5-write-probe" in line
        ]
        self.assertTrue(probes, "the gate must probe a uid-1000 write in the baked cargo trees")
        probe = " ".join(probes)
        for tree in ("/usr/local/cargo/registry/", "/usr/local/cargo/git/"):
            self.assertIn(tree, probe, f"the uid-1000 write probe does not touch {tree}")
        locks = [line for line in self.final_stage if "chmod 0666 /usr/local/cargo/.package-cache" in line]
        self.assertTrue(locks, "cargo's package-cache lock must be world-writable")
        sccache = [
            line
            for line in self.final_stage
            if "runuser -u ubuntu" in line and "G5_EXPECT_SCCACHE_DIR=" in line
        ]
        self.assertTrue(sccache, "the gate must compile as uid 1000 against the shipped sccache directory")


if __name__ == "__main__":
    unittest.main()
