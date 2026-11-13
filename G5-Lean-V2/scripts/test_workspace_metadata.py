#!/usr/bin/env python3
"""Regression tests for the host/container cargo-metadata gateway (G5-Lean#355)."""
from __future__ import annotations

import json
import subprocess
import unittest
from unittest.mock import patch

import workspace_metadata as wm


def completed(returncode=0, stdout="", stderr=""):
    return subprocess.CompletedProcess(args=[], returncode=returncode, stdout=stdout, stderr=stderr)


CONTAINER_JSON = json.dumps({
    "packages": [{
        "name": "g5-contracts",
        "manifest_path": "/src/backend/g5-contracts/Cargo.toml",
        "targets": [{"name": "g5-contracts", "kind": ["lib"],
                     "src_path": "/src/backend/g5-contracts/src/lib.rs"}],
    }],
    "workspace_root": "/src",
})

HOST_JSON = CONTAINER_JSON.replace("/src", str(wm.ROOT))


class CargoMetadataGatewayTests(unittest.TestCase):
    def test_host_fast_path_skips_docker(self):
        calls = []

        def fake_run(command, **kwargs):
            calls.append(command)
            if command[0] == "cargo":
                return completed(stdout=HOST_JSON)
            raise AssertionError(f"docker must not run on the host fast path: {command}")

        with patch.object(wm.subprocess, "run", side_effect=fake_run):
            metadata = wm.cargo_metadata()
        self.assertEqual(metadata["packages"][0]["name"], "g5-contracts")
        self.assertTrue(all(call[0] == "cargo" for call in calls))

    def test_host_probe_disables_rustup_auto_install(self):
        captured = {}

        def fake_run(command, **kwargs):
            if command[0] == "cargo":
                captured["env"] = kwargs.get("env")
                return completed(stdout=HOST_JSON)
            raise AssertionError("docker must not run on the host fast path")

        with patch.object(wm.subprocess, "run", side_effect=fake_run):
            wm.cargo_metadata()
        # Must not let a rustup proxy auto-install a missing toolchain on the host.
        self.assertIsNotNone(captured["env"])
        self.assertEqual(captured["env"].get("RUSTUP_AUTO_INSTALL"), "0")

    def test_broken_host_cargo_falls_back_to_container_and_rebases_paths(self):
        def fake_run(command, **kwargs):
            if command[0] == "cargo":
                return completed(returncode=1, stderr="rustup could not choose a version")
            if command[:3] == ["docker", "image", "inspect"]:
                return completed(returncode=0)  # image already present
            if command[:2] == ["docker", "run"]:
                return completed(stdout=CONTAINER_JSON)
            raise AssertionError(f"unexpected command {command}")

        with patch.object(wm.subprocess, "run", side_effect=fake_run):
            metadata = wm.cargo_metadata()
        target = metadata["packages"][0]["targets"][0]
        # Container /src paths are rebased to the host checkout, including the
        # interior `src` directory, which must survive untouched.
        self.assertEqual(target["src_path"], f"{wm.ROOT}/backend/g5-contracts/src/lib.rs")
        self.assertEqual(metadata["workspace_root"], str(wm.ROOT))

    def test_missing_cargo_binary_falls_back(self):
        def fake_run(command, **kwargs):
            if command[0] == "cargo":
                raise FileNotFoundError("cargo")
            if command[:3] == ["docker", "image", "inspect"]:
                return completed(returncode=0)
            if command[:2] == ["docker", "run"]:
                return completed(stdout=CONTAINER_JSON)
            raise AssertionError(f"unexpected command {command}")

        with patch.object(wm.subprocess, "run", side_effect=fake_run):
            metadata = wm.cargo_metadata()
        self.assertEqual(metadata["packages"][0]["name"], "g5-contracts")

    def test_builds_image_when_absent(self):
        built = []

        def fake_run(command, **kwargs):
            if command[0] == "cargo":
                return completed(returncode=1)
            if command[:3] == ["docker", "image", "inspect"]:
                return completed(returncode=1)  # absent → must build
            if command[:2] == ["docker", "build"]:
                built.append(command)
                return completed(returncode=0)
            if command[:2] == ["docker", "run"]:
                return completed(stdout=CONTAINER_JSON)
            raise AssertionError(f"unexpected command {command}")

        with patch.object(wm.subprocess, "run", side_effect=fake_run):
            wm.cargo_metadata()
        self.assertEqual(len(built), 1)

    def test_raises_when_both_backends_fail(self):
        def fake_run(command, **kwargs):
            if command[0] == "cargo":
                return completed(returncode=1)
            if command[:3] == ["docker", "image", "inspect"]:
                return completed(returncode=0)
            if command[:2] == ["docker", "run"]:
                return completed(returncode=1, stderr="metadata boom")
            raise AssertionError(f"unexpected command {command}")

        with patch.object(wm.subprocess, "run", side_effect=fake_run):
            with self.assertRaises(wm.MetadataError):
                wm.cargo_metadata()

    def test_image_build_failure_raises(self):
        def fake_run(command, **kwargs):
            if command[0] == "cargo":
                return completed(returncode=1)
            if command[:3] == ["docker", "image", "inspect"]:
                return completed(returncode=1)
            if command[:2] == ["docker", "build"]:
                return completed(returncode=1, stderr="build boom")
            raise AssertionError(f"unexpected command {command}")

        with patch.object(wm.subprocess, "run", side_effect=fake_run):
            with self.assertRaises(wm.MetadataError):
                wm.cargo_metadata()

    def test_rebase_paths_leaves_interior_components_and_non_paths(self):
        rebased = wm._rebase_paths(
            {"a": "/src", "b": "/src/x/src/lib.rs", "c": "/srcother", "n": 7},
            "/src", "/work/code")
        self.assertEqual(rebased["a"], "/work/code")
        self.assertEqual(rebased["b"], "/work/code/x/src/lib.rs")
        self.assertEqual(rebased["c"], "/srcother")  # not a /src path boundary
        self.assertEqual(rebased["n"], 7)

    def test_image_tag_tracks_dockerfile(self):
        self.assertTrue(wm.image_tag().startswith("g5-validation-metadata:"))


if __name__ == "__main__":
    unittest.main()
