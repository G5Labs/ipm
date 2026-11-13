#!/usr/bin/env python3
"""Contract tests for the g5dev image's self-configuring Rust launcher.

The guest runtime starts agents with a cleared environment, so the launcher
must supply the baked toolchain identity itself (ontology
concepts/deployment g5.scope.deployment.images.g5dev-toolchain). These tests
run the real script against a fixture toolchain: rustup proxies are shell
doubles that print the environment they received.
"""

import http.server
import os
import shutil
import socket
import stat
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path

# The profile exactly as host bring-up writes it (startup_script.rs), door
# address aside: the tier list carries spaces, which is why readers must
# treat the file as data.
PROFILE = (
    "G5_CACHE_DOOR={door}\n"
    "G5_BASE_IMAGE_REGISTRY=g5-host:7111/us-central1-docker.pkg.dev/g5-qa-1ccc/g5-tiers\n"
    "G5_BASE_IMAGE_MIRROR_TIERS=base base-rust base-rust-warm base-node\n"
)


ROOT = Path(__file__).resolve().parents[1]
LAUNCHER = ROOT / "infra/docker/g5-dev-rust-launcher.sh"
DOCKERFILE = ROOT / "infra/docker/Dockerfile.g5d-g5dev"
TOOLS = ["cargo", "rustc", "rustup", "cargo-fmt", "rustfmt", "cargo-clippy", "clippy-driver"]
PROBE = """#!/usr/bin/env bash
printf 'tool=%s\\n' "$(basename "$0")"
printf 'RUSTUP_HOME=%s\\n' "${RUSTUP_HOME-<unset>}"
printf 'CARGO_HOME=%s\\n' "${CARGO_HOME-<unset>}"
printf 'RUSTC_WRAPPER=%s\\n' "${RUSTC_WRAPPER-<unset>}"
printf 'SCCACHE_DIR=%s\\n' "${SCCACHE_DIR-<unset>}"
printf 'CARGO_INCREMENTAL=%s\\n' "${CARGO_INCREMENTAL-<unset>}"
printf 'SCCACHE_WEBDAV_ENDPOINT=%s\\n' "${SCCACHE_WEBDAV_ENDPOINT-<unset>}"
printf 'SCCACHE_IGNORE_SERVER_IO_ERROR=%s\\n' "${SCCACHE_IGNORE_SERVER_IO_ERROR-<unset>}"
printf 'args=%s\\n' "$*"
"""


class RustLauncherTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.rustup_home = root / "rustup"
        self.cargo_home = root / "cargo"
        self.sccache_dir = root / "sccache"
        self.bin = root / "bin"
        self.home = root / "home"
        for path in [self.rustup_home, self.cargo_home / "bin", self.sccache_dir, self.bin, self.home]:
            path.mkdir(parents=True)
        for tool in TOOLS:
            self.executable(self.cargo_home / "bin" / tool, PROBE)
        self.executable(self.bin / "sccache", "#!/usr/bin/env bash\nexit 0\n")
        script = LAUNCHER.read_text()
        for marker, fixture in [
            ("G5_RUSTUP_HOME=/usr/local/rustup", f"G5_RUSTUP_HOME={self.rustup_home}"),
            ("G5_CARGO_HOME=/usr/local/cargo", f"G5_CARGO_HOME={self.cargo_home}"),
            ("G5_SCCACHE_DIR=/opt/g5-build-cache/sccache", f"G5_SCCACHE_DIR={self.sccache_dir}"),
        ]:
            self.assertIn(marker, script)
            script = script.replace(marker, fixture)
        launcher = self.bin / "g5-dev-rust-launcher"
        self.executable(launcher, script)
        for tool in TOOLS:
            (self.bin / tool).symlink_to(launcher)

    @staticmethod
    def executable(path, text):
        path.write_text(text)
        path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    def run_tool(self, tool, *args, env=None):
        # The runtime's shape: a cleared environment with only PATH and HOME.
        cleared = {"PATH": f"{self.bin}:/usr/bin:/bin", "HOME": str(self.home)}
        cleared.update(env or {})
        return subprocess.run(
            ["env", "-i", *[f"{k}={v}" for k, v in cleared.items()], str(self.bin / tool), *args],
            capture_output=True, text=True,
        )

    @staticmethod
    def parse(result):
        return dict(line.split("=", 1) for line in result.stdout.splitlines())

    def test_cleared_environment_receives_the_baked_toolchain_identity(self):
        result = self.run_tool("cargo", "build", "--release")
        self.assertEqual(result.returncode, 0, result.stderr)
        seen = self.parse(result)
        self.assertEqual(seen["tool"], "cargo")
        self.assertEqual(seen["RUSTUP_HOME"], str(self.rustup_home))
        self.assertEqual(seen["CARGO_HOME"], str(self.cargo_home))
        self.assertEqual(seen["RUSTC_WRAPPER"], "sccache")
        self.assertEqual(seen["SCCACHE_DIR"], str(self.sccache_dir))
        self.assertEqual(seen["CARGO_INCREMENTAL"], "0")
        self.assertEqual(seen["args"], "build --release")

    def test_every_entry_point_dispatches_to_the_proxy_of_the_same_name(self):
        for tool in TOOLS:
            with self.subTest(tool=tool):
                result = self.run_tool(tool, "--version")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.parse(result)["tool"], tool)

    def test_explicit_caller_values_win(self):
        result = self.run_tool(
            "rustc",
            env={"RUSTUP_HOME": "/caller/rustup", "CARGO_HOME": "/caller/cargo", "CARGO_INCREMENTAL": "1"},
        )
        seen = self.parse(result)
        self.assertEqual(seen["RUSTUP_HOME"], "/caller/rustup")
        self.assertEqual(seen["CARGO_HOME"], "/caller/cargo")
        self.assertEqual(seen["RUSTC_WRAPPER"], "sccache")
        self.assertEqual(seen["CARGO_INCREMENTAL"], "1")

    def test_explicitly_empty_homes_are_the_callers_choice(self):
        # cargo and rustup fall back to $HOME on an empty home variable; the
        # launcher must not turn an explicit empty into the image default.
        seen = self.parse(self.run_tool("cargo", env={"RUSTUP_HOME": "", "CARGO_HOME": ""}))
        self.assertEqual(seen["RUSTUP_HOME"], "")
        self.assertEqual(seen["CARGO_HOME"], "")
        self.assertEqual(seen["tool"], "cargo")

    def test_explicitly_empty_sccache_dir_degrades_to_plain_compilation(self):
        seen = self.parse(self.run_tool("cargo", env={"SCCACHE_DIR": ""}))
        self.assertEqual(seen["RUSTC_WRAPPER"], "<unset>")
        self.assertEqual(seen["SCCACHE_DIR"], "")

    @staticmethod
    def answering_door():
        class Ping(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(200)
                self.end_headers()

            def log_message(self, *_):
                pass

        server = http.server.HTTPServer(("127.0.0.1", 0), Ping)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        return server

    def test_cache_door_profile_points_sccache_at_an_answering_door(self):
        server = self.answering_door()
        self.addCleanup(server.shutdown)
        door = f"http://127.0.0.1:{server.server_port}/"
        profile = Path(self.tmp.name) / "cache-profile.env"
        profile.write_text(PROFILE.format(door=door))
        result = self.run_tool("cargo", env={"G5_DEV_CACHE_PROFILE": str(profile)})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "", "the tier list must not run as a command")
        seen = self.parse(result)
        self.assertEqual(seen["RUSTC_WRAPPER"], "sccache")
        self.assertEqual(seen["SCCACHE_WEBDAV_ENDPOINT"], f"http://127.0.0.1:{server.server_port}/sccache")
        self.assertEqual(seen["SCCACHE_IGNORE_SERVER_IO_ERROR"], "1")
        # A caller's own endpoint wins, and the fallback flag is not forced over
        # an explicit value.
        seen = self.parse(self.run_tool("cargo", env={
            "G5_DEV_CACHE_PROFILE": str(profile),
            "SCCACHE_WEBDAV_ENDPOINT": "http://elsewhere/sccache",
            "SCCACHE_IGNORE_SERVER_IO_ERROR": "0",
        }))
        self.assertEqual(seen["SCCACHE_WEBDAV_ENDPOINT"], "http://elsewhere/sccache")
        self.assertEqual(seen["SCCACHE_IGNORE_SERVER_IO_ERROR"], "0")

    def test_a_door_the_sccache_server_cannot_start_on_is_dropped(self):
        # The door answers the ping but sccache's server fails to start on it
        # (the door went away in between, or its backend check failed): the
        # launcher falls back to the local cache instead of failing compiles.
        server = self.answering_door()
        self.addCleanup(server.shutdown)
        door = f"http://127.0.0.1:{server.server_port}/"
        profile = Path(self.tmp.name) / "flaky.env"
        profile.write_text(PROFILE.format(door=door))
        self.executable(
            self.bin / "sccache",
            '#!/usr/bin/env bash\nif [[ "${1-}" == --start-server && -n "${SCCACHE_WEBDAV_ENDPOINT-}" ]]; then echo "sccache: Starting the server..."; echo "Server startup failed: cache storage failed to read" >&2; exit 2; fi\nexit 0\n',
        )
        result = self.run_tool("cargo", env={"G5_DEV_CACHE_PROFILE": str(profile)})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("sccache could not start on cache door", result.stderr)
        seen = self.parse(result)
        self.assertEqual(seen["RUSTC_WRAPPER"], "sccache")
        self.assertEqual(seen["SCCACHE_WEBDAV_ENDPOINT"], "<unset>")
        # A server that is already running answers "Address in use": the
        # endpoint stays, since the running server keeps its own backend.
        self.executable(
            self.bin / "sccache",
            '#!/usr/bin/env bash\nif [[ "${1-}" == --start-server ]]; then echo "Server startup failed: Address in use" >&2; exit 2; fi\nexit 0\n',
        )
        result = self.run_tool("cargo", env={"G5_DEV_CACHE_PROFILE": str(profile)})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertEqual(self.parse(result)["SCCACHE_WEBDAV_ENDPOINT"], f"http://127.0.0.1:{server.server_port}/sccache")

    def test_a_silent_door_keeps_sccache_on_the_local_cache(self):
        # sccache 0.10 would abort server startup on an unreadable backend, so
        # a door nobody listens on must leave the endpoint unset.
        probe = socket.socket()
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
        probe.close()
        profile = Path(self.tmp.name) / "silent.env"
        profile.write_text(PROFILE.format(door=f"http://127.0.0.1:{port}"))
        result = self.run_tool("cargo", env={"G5_DEV_CACHE_PROFILE": str(profile)})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("cache door http://127.0.0.1", result.stderr)
        seen = self.parse(result)
        self.assertEqual(seen["RUSTC_WRAPPER"], "sccache")
        self.assertEqual(seen["SCCACHE_WEBDAV_ENDPOINT"], "<unset>")
        self.assertEqual(seen["SCCACHE_IGNORE_SERVER_IO_ERROR"], "<unset>")
        # rustc runs once per crate: it stays quiet about the door.
        result = self.run_tool("rustc", env={"G5_DEV_CACHE_PROFILE": str(profile)})
        self.assertEqual(result.stderr, "")

    def test_without_a_profile_sccache_stays_on_the_local_cache(self):
        seen = self.parse(self.run_tool("cargo", env={"G5_DEV_CACHE_PROFILE": str(Path(self.tmp.name) / "absent")}))
        self.assertEqual(seen["RUSTC_WRAPPER"], "sccache")
        self.assertEqual(seen["SCCACHE_WEBDAV_ENDPOINT"], "<unset>")
        self.assertEqual(seen["SCCACHE_IGNORE_SERVER_IO_ERROR"], "<unset>")
        # A profile without a door line changes nothing either.
        profile = Path(self.tmp.name) / "doorless.env"
        profile.write_text("G5_BASE_IMAGE_REGISTRY=somewhere\n")
        seen = self.parse(self.run_tool("cargo", env={"G5_DEV_CACHE_PROFILE": str(profile)}))
        self.assertEqual(seen["SCCACHE_WEBDAV_ENDPOINT"], "<unset>")
        # Nor the runtime disk's empty placeholder, which a host without a
        # door leaves in place (ontology g5.runtime.cache.profile).
        placeholder = Path(self.tmp.name) / "placeholder.env"
        placeholder.write_text("")
        seen = self.parse(self.run_tool("cargo", env={"G5_DEV_CACHE_PROFILE": str(placeholder)}))
        self.assertEqual(seen["RUSTC_WRAPPER"], "sccache")
        self.assertEqual(seen["SCCACHE_WEBDAV_ENDPOINT"], "<unset>")

    def test_empty_wrapper_keeps_sccache_off(self):
        seen = self.parse(self.run_tool("cargo", env={"RUSTC_WRAPPER": ""}))
        self.assertEqual(seen["RUSTC_WRAPPER"], "")
        self.assertEqual(seen["SCCACHE_DIR"], "<unset>")
        self.assertEqual(seen["CARGO_INCREMENTAL"], "<unset>")

    def test_caller_sccache_dir_is_kept_when_writable(self):
        other = Path(self.tmp.name) / "other-sccache"
        other.mkdir()
        seen = self.parse(self.run_tool("cargo", env={"SCCACHE_DIR": str(other)}))
        self.assertEqual(seen["RUSTC_WRAPPER"], "sccache")
        self.assertEqual(seen["SCCACHE_DIR"], str(other))

    def test_caller_sccache_dir_is_created_when_missing(self):
        fresh = Path(self.tmp.name) / "fresh" / "sccache"
        seen = self.parse(self.run_tool("cargo", env={"SCCACHE_DIR": str(fresh)}))
        self.assertEqual(seen["RUSTC_WRAPPER"], "sccache")
        self.assertEqual(seen["SCCACHE_DIR"], str(fresh))
        self.assertTrue(fresh.is_dir())

    def test_explicit_sccache_wrapper_gets_its_companions(self):
        seen = self.parse(self.run_tool("cargo", env={"RUSTC_WRAPPER": "sccache"}))
        self.assertEqual(seen["RUSTC_WRAPPER"], "sccache")
        self.assertEqual(seen["SCCACHE_DIR"], str(self.sccache_dir))
        self.assertEqual(seen["CARGO_INCREMENTAL"], "0")

    def test_other_explicit_wrapper_is_left_alone(self):
        seen = self.parse(self.run_tool("cargo", env={"RUSTC_WRAPPER": "/usr/bin/true"}))
        self.assertEqual(seen["RUSTC_WRAPPER"], "/usr/bin/true")
        self.assertEqual(seen["SCCACHE_DIR"], "<unset>")
        self.assertEqual(seen["CARGO_INCREMENTAL"], "<unset>")

    def test_cache_path_that_is_not_a_directory_degrades_to_plain_compilation(self):
        # Root-proof variant of the unwritable case: a regular file fails -d.
        blocker = Path(self.tmp.name) / "blocker"
        blocker.write_text("")
        seen = self.parse(self.run_tool("cargo", env={"SCCACHE_DIR": str(blocker)}))
        self.assertEqual(seen["RUSTUP_HOME"], str(self.rustup_home))
        self.assertEqual(seen["RUSTC_WRAPPER"], "<unset>")
        self.assertEqual(seen["CARGO_INCREMENTAL"], "<unset>")

    @unittest.skipIf(os.geteuid() == 0, "root can write anywhere")
    def test_unwritable_cache_degrades_to_plain_compilation(self):
        self.sccache_dir.chmod(0o555)
        self.addCleanup(self.sccache_dir.chmod, 0o755)
        seen = self.parse(self.run_tool("cargo"))
        self.assertEqual(seen["RUSTUP_HOME"], str(self.rustup_home))
        self.assertEqual(seen["RUSTC_WRAPPER"], "<unset>")
        self.assertEqual(seen["CARGO_INCREMENTAL"], "<unset>")

    def test_missing_sccache_binary_degrades_to_plain_compilation(self):
        (self.bin / "sccache").unlink()
        seen = self.parse(self.run_tool("cargo"))
        self.assertEqual(seen["RUSTC_WRAPPER"], "<unset>")

    def test_missing_proxy_fails_closed(self):
        (self.cargo_home / "bin" / "rustfmt").unlink()
        result = self.run_tool("rustfmt")
        self.assertEqual(result.returncode, 127)
        self.assertIn("not an executable rustup proxy", result.stderr)


class DockerfileContractTests(unittest.TestCase):
    def test_launcher_replaces_the_rustup_proxy_symlinks(self):
        source = DOCKERFILE.read_text()
        self.assertIn("COPY infra/docker/g5-dev-rust-launcher.sh /usr/local/bin/g5-dev-rust-launcher", source)
        self.assertIn("for tool in " + " ".join(TOOLS) + "; do", source)
        for tool in TOOLS:
            self.assertNotIn(f"ln -s /usr/local/cargo/bin/{tool} /usr/local/bin/{tool}", source)

    def test_launcher_defaults_match_the_image_env(self):
        source = DOCKERFILE.read_text()
        script = LAUNCHER.read_text()
        for env_line, default in [
            ("RUSTUP_HOME=/usr/local/rustup", "G5_RUSTUP_HOME=/usr/local/rustup"),
            ("CARGO_HOME=/usr/local/cargo", "G5_CARGO_HOME=/usr/local/cargo"),
            ("SCCACHE_DIR=/opt/g5-build-cache/sccache", "G5_SCCACHE_DIR=/opt/g5-build-cache/sccache"),
        ]:
            self.assertIn(env_line, source)
            self.assertIn(default, script)

    def test_gate_runs_the_toolchain_as_the_agent_under_a_cleared_environment(self):
        source = DOCKERFILE.read_text()
        start = source.index("runuser -u ubuntu -- env -i")
        gate = source[start:source.index("WORKDIR /work", start)]
        self.assertIn("HOME=/home/agent", gate)
        for check in ["cargo fmt --version", "cargo clippy --version", "cargo new", "cfg-if",
                      "cargo build --quiet --offline", "sccache --show-stats",
                      "^Cache write errors +0$", "^Cache hits +[1-9]", "Local disk:",
                      ".cache/ms-playwright"]:
            self.assertIn(check, gate)
        # The first build must already hit (prime-to-guest proof, under the
        # prime's cargo config) and the rebuild after the target is dropped
        # must hit again.
        self.assertIn("prime-cargo-config.toml", gate)
        self.assertIn("prime-workspace-manifest.toml", gate)
        # The smoke dependency is pinned to the lockfile's exact version: an
        # unpinned requirement resolves offline to the newest crate in the
        # registry cache, which the prime never cooked (deploy 14052d2e).
        self.assertIn("prime-workspace-lock.toml", gate)
        self.assertIn('cfg-if = \\"=%s\\"', gate)
        self.assertNotIn('cfg-if = \\"1\\"', gate)
        # Cargo discovers .cargo/config.toml from the working directory, not
        # the manifest path, so the gate must cd into the smoke crate and
        # build without --manifest-path.
        self.assertIn('cd "$smoke/g5-launcher-smoke"', gate)
        self.assertNotIn("--manifest-path", gate)
        build = gate.index("cargo build --quiet --offline")
        self.assertLess(gate.index("prime-cargo-config.toml"), build)
        self.assertLess(gate.index("profile\\.dev"), build)
        drop = gate.index("rm -rf target")
        self.assertLess(gate.index("^Cache hits +[1-9]"), drop)
        self.assertLess(drop, gate.rindex("^Cache hits +[1-9]"))

    def test_agent_home_carries_the_baked_browsers(self):
        source = DOCKERFILE.read_text()
        self.assertIn("ln -s /opt/ms-playwright /home/agent/.cache/ms-playwright", source)


if __name__ == "__main__":
    unittest.main()
