"""Exercise real command dispatch with external operations replaced by stubs."""

from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
JUST = shutil.which("just")
DOCKER = shutil.which("docker")


class JustCommandsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="g5-just-command-test-")
        self.addCleanup(self.temporary.cleanup)
        self.scratch = Path(self.temporary.name)
        self.bin = self.scratch / "bin"
        self.bin.mkdir()
        self.log = self.scratch / "commands.jsonl"
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}",
                        G5_COMMAND_TEST_LOG=str(self.log))
        # A developer's environment must not redirect fixture operations.
        for name in ("G5_BASE_DOCKERFILE_DIR", "G5_BASE_BUILD_CONTEXT", "G5_BASE_IMAGE_TAG",
                     "G5_BASE_LOCK_DIR", "G5_CLOUD_BUILD_REGION"):
            self.env.pop(name, None)
        self.env["G5_BASE_LOCK_DIR"] = str(self.scratch / "build.lock")
        self.env["G5_BASE_LOCK_HEARTBEAT_INTERVAL"] = "0.1"

    def stub(self, name, tail=""):
        script = self.bin / name
        script.write_text(
            "#!/usr/bin/env python3\nimport json, os, sys\n"
            "from pathlib import Path\n"
            "with open(os.environ['G5_COMMAND_TEST_LOG'], 'a') as log:\n"
            "    log.write(json.dumps([Path(sys.argv[0]).name, *sys.argv[1:]]) + '\\n')\n"
            + tail
        )
        script.chmod(0o755)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def run_just(self, *arguments):
        return subprocess.run(
            [JUST, *arguments], cwd=ROOT, env=self.env,
            check=True, capture_output=True, text=True,
        )

    def test_all_recipe_syntax_and_entry_points(self):
        subprocess.run(["python3", "scripts/check-just-commands.py"], cwd=ROOT,
                       capture_output=True, text=True, check=True)

    def test_cloud_dispatch_and_artifact_registry_substitution(self):
        self.stub("gcloud", "if sys.argv[1:4] == ['config', 'get-value', 'project']:\n    print('fixture-project')\n")
        self.run_just("ar-images")
        self.run_just("cb-ci", "fixture-branch")
        calls = self.calls()
        image_call = next(call for call in calls if call[1:4] == ["artifacts", "docker", "images"])
        self.assertIn("us-central1-docker.pkg.dev/fixture-project/g5", image_call)
        triggers = [call for call in calls if call[1:4] == ["builds", "triggers", "run"]]
        self.assertEqual([call[4] for call in triggers], ["g5-ci-rust", "g5-ci-e2e"])
        for call in triggers:
            self.assertIn("--branch=fixture-branch", call)
            self.assertIn("--region=us-central1", call)

    def test_branch_deploy_only_dispatches_existing_qa_trigger(self):
        self.stub("gcloud")
        self.run_just("cb-deploy", "qa", "fixture-branch")
        self.assertEqual(self.calls(), [["gcloud", "builds", "triggers", "run",
                         "g5-deploy-qa", "--project=g5-qa-1ccc",
                         "--branch=fixture-branch", "--region=us-central1"]])
        for stage in ("dev", "staging", "prod"):
            result = subprocess.run([JUST, "cb-deploy", stage], cwd=ROOT,
                                    env=self.env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn("deploys only to qa", result.stderr)
        self.assertEqual(len(self.calls()), 1)

    def test_test_container_receives_arguments_for_cargo_entrypoint(self):
        self.stub("just")
        self.stub("docker")
        self.run_just("test-all-docker")
        self.run_just("test-isolation-docker")
        runs = [call for call in self.calls() if call[0] == "docker" and "run" in call]
        arguments = [call[call.index("--rm") + 1:] for call in runs]
        arguments = [args[1:] if args[0] == "--build" else args for args in arguments]
        self.assertEqual(arguments, [
            ["test-runner", "--workspace", "--", "--test-threads=2"],
            ["test-runner", "-p", "g5d-isolation", "--lib", "--all-features",
             "--", "isolation", "--test-threads=2"],
            ["test-runner", "-p", "g5d", "--test", "isolation_backends",
             "--all-features", "--", "--test-threads=1"],
        ])

    def test_demo_missing_source_fails_before_teardown(self):
        self.stub("just")
        self.stub("docker")
        # Run in an isolated checkout so a real .env cannot replace the
        # missing-source fixture and turn this preflight test into a seed.
        fixture_root = self.scratch / "repository"
        fixture_root.mkdir()
        shutil.copy(ROOT / "Justfile", fixture_root / "Justfile")
        (fixture_root / "scripts").mkdir()
        shutil.copy(ROOT / "scripts/seed-demo-multi.sh",
                    fixture_root / "scripts/seed-demo-multi.sh")
        # seed-demo-multi.sh sources this helper at load time, before the
        # source-repo check; the fixture must provide it or the script dies
        # with exit 1 on the missing `source` instead of the exit-2 guard.
        shutil.copy(ROOT / "scripts/seed-workspace-model.sh",
                    fixture_root / "scripts/seed-workspace-model.sh")
        self.env["G5_CODEX_DEMO_SOURCE_REPO"] = str(self.scratch / "absent-codex")
        result = subprocess.run([JUST, "demo-codex"], cwd=fixture_root, env=self.env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("Missing Codex demo source repository", result.stderr)
        self.assertFalse(self.log.exists(), "No stack operation should precede source validation")

    def test_e2e_up_uses_authoritative_workspace_host_harness(self):
        self.stub("just")
        fixture_root = self.scratch / "repository"
        fixture_root.mkdir()
        shutil.copy(ROOT / "Justfile", fixture_root / "Justfile")
        subprocess.run([JUST, "e2e-up"], cwd=fixture_root, env=self.env,
                       capture_output=True, text=True, check=True)
        self.assertEqual(self.calls(), [["just", "ci-docker-prepare"]])

    def test_dev_down_stops_worker_before_compose_teardown(self):
        self.stub("just")
        self.stub("docker")
        self.run_just("dev-down")
        self.assertEqual(self.calls(), [
            ["just", "worker-host-stop"],
            ["just", "dev-stack-down"],
        ])

    def test_worker_setup_refuses_macos_microvm_without_building(self):
        fixture = self.scratch / "setup-repository"
        (fixture / "scripts").mkdir(parents=True)
        shutil.copy(ROOT / "Justfile", fixture / "Justfile")
        (fixture / "scripts/worker-host-env.sh").write_text("G5D_ISOLATION_MODE=microvm\n")
        self.stub("uname", "print({'-s': 'Darwin', '-m': 'arm64'}[sys.argv[1]])\n")
        result = subprocess.run([JUST, "worker-host-setup"], cwd=fixture, env=self.env,
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Cloud Hypervisor requires Linux", result.stderr)
        self.assertFalse(any(call[0] in {"cargo", "docker"} for call in self.calls()))

    def test_worker_host_starts_without_llm_credential_or_auth_setup(self):
        # Workspace startup must not depend on agent-turn credentials. Use an
        # isolated recipe fixture so auth setup cannot affect the real checkout.
        fixture_root = self.scratch / "repository"
        fixture_scripts = fixture_root / "scripts"
        fixture_scripts.mkdir(parents=True)
        (fixture_scripts / "lib").mkdir()
        shutil.copy(ROOT / "scripts/lib/stack-identity.sh", fixture_scripts / "lib/stack-identity.sh")
        shutil.copy(ROOT / "Justfile", fixture_root / "Justfile")
        (fixture_scripts / "worker-host-env.sh").write_text("""
GEN_ROOT="$(pwd)"
G5_WORKER_HOST_G5D="$GEN_ROOT/scripts/fake-g5d"
G5D_API_URL="http://localhost:7500"
g5_worker_host_build() { :; }
g5_worker_host_prepare_runtime() { :; }
g5_worker_host_ensure_runtime() { :; }
g5_worker_host_check_artifacts() { :; }
g5_worker_host_seed_token() { :; }
g5_worker_host_check_llm_env() { echo "unexpected credential gate" >&2; return 1; }
g5_worker_host_exec_as_root() { exec "$@"; }
""")
        fake_g5d = fixture_scripts / "fake-g5d"
        fake_g5d.write_text("#!/usr/bin/env bash\nprintf 'host started: %s\\n' \"$*\"\n")
        fake_g5d.chmod(0o755)
        self.stub("claude")
        self.stub("script")
        env = self.env.copy()
        for name in ("ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN",
                     "CLAUDE_CODE_OAUTH_TOKEN", "G5_CLAUDE_CODE_FAKE",
                     "ANTHROPIC_BASE_URL"):
            env.pop(name, None)
        result = subprocess.run(
            [JUST, "worker-host"], cwd=fixture_root, env=env,
            capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("host started: host", result.stdout)
        self.assertFalse(self.log.exists(), "worker startup must launch no auth commands")
        self.assertFalse((fixture_root / ".env").exists(), "worker startup must not write .env")

    def test_background_worker_host_starts_without_llm_credential(self):
        fixture_root = self.scratch / "repository"
        fixture_scripts = fixture_root / "scripts"
        fixture_scripts.mkdir(parents=True)
        (fixture_scripts / "lib").mkdir()
        shutil.copy(ROOT / "scripts/lib/stack-identity.sh", fixture_scripts / "lib/stack-identity.sh")
        shutil.copy(ROOT / "Justfile", fixture_root / "Justfile")
        (fixture_scripts / "worker-host-env.sh").write_text("""
GEN_ROOT="$(pwd)"
G5_DEV_HOST_DIR="$GEN_ROOT/.dev/host"
mkdir -p "$G5_DEV_HOST_DIR"
G5_WORKER_HOST_G5D="$GEN_ROOT/scripts/fake-g5d"
G5_WORKER_HOST_PID_FILE="$G5_DEV_HOST_DIR/host.pid"
G5_WORKER_HOST_MODE_FILE="$G5_DEV_HOST_DIR/host.mode"
G5_WORKER_HOST_LAUNCH_FINGERPRINT_FILE="$G5_DEV_HOST_DIR/host.fingerprint"
G5_WORKER_HOST_LOG_FILE="$G5_DEV_HOST_DIR/host.log"
G5D_ISOLATION_MODE="microvm"
G5D_HOST_MAX_MICROVMS=2
G5D_API_URL="http://localhost:7500"
g5_worker_host_reap_strays() { :; }
g5_worker_host_build() { :; }
g5_worker_host_prepare_runtime() { :; }
g5_worker_host_check_host_vm_exec() { :; }
g5_worker_host_ensure_runtime() { :; }
g5_worker_host_check_artifacts() { :; }
g5_worker_host_launch_fingerprint() { echo fixture; }
g5_worker_host_seed_token() { :; }
g5_worker_host_write_launch_fingerprint() { :; }
g5_worker_host_check_llm_env() { echo "unexpected credential gate" >&2; return 1; }
# The root boundary helpers of the real script, without sudo: the fixture
# host is the caller's own process.
g5_worker_host_exec_as_root() { exec "$@"; }
g5_worker_host_as_root() { "$@"; }
g5_worker_host_kill() { kill "$1" -- "$2"; }
g5_worker_host_resolve_launched_pid() { printf '%s' "$1"; }
g5_worker_host_alive() { kill -0 "$1" 2>/dev/null; }
""")
        fake_g5d = fixture_scripts / "fake-g5d"
        fake_g5d.write_text(
            "#!/usr/bin/env bash\necho 'workspace VM host started'\nexec sleep 30\n"
        )
        fake_g5d.chmod(0o755)
        self.stub("claude")
        self.stub("script")
        env = self.env.copy()
        for name in ("ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN",
                     "CLAUDE_CODE_OAUTH_TOKEN", "G5_CLAUDE_CODE_FAKE"):
            env.pop(name, None)
        pid_file = fixture_root / ".dev" / "host" / "host.pid"
        try:
            result = subprocess.run(
                [JUST, "worker-host-start"], cwd=fixture_root, env=env,
                capture_output=True, text=True, timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("native microVM worker host running", result.stdout)
            self.assertFalse(self.log.exists(), "worker startup must launch no auth commands")
            self.assertFalse((fixture_root / ".env").exists(), "worker startup must not write .env")
        finally:
            if pid_file.exists():
                try:
                    os.kill(int(pid_file.read_text().strip()), signal.SIGTERM)
                except ProcessLookupError:
                    pass

    def test_dev_workspace_image_env_resolves_default_without_env_file(self):
        image_ref = "mirror.gcr.io/library/buildpack-deps@sha256:" + "a" * 64
        for name in ("G5_DEV_WORKSPACE_BASE_IMAGE", "G5D_HOST_BASE_IMAGE",
                     "G5_NEW_ORG_DEFAULT_IMAGE_TRACK", "G5_PROMOTED_BASE_IMAGES"):
            self.env.pop(name, None)
        self.stub("docker", f"""
if sys.argv[1:3] == ['image', 'inspect']:
    if '--format' in sys.argv:
        print('{image_ref}')
else:
    sys.exit(f'unexpected docker call: {{sys.argv[1:]}}')
""")
        result = subprocess.run(
            ["bash", "-c", "source scripts/dev-workspace-image-env.sh; "
             "printf '%s\\n' \"$G5_NEW_ORG_DEFAULT_IMAGE_TRACK\" "
             "\"$G5_PROMOTED_BASE_IMAGES\""],
            cwd=ROOT, env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines(),
                         ["local-dev", "local-dev=" + image_ref])
        self.assertEqual(self.calls(), [
            ["docker", "image", "inspect", "mirror.gcr.io/library/buildpack-deps:resolute"],
            ["docker", "image", "inspect", "--format",
             "{{range .RepoDigests}}{{println .}}{{end}}",
             "mirror.gcr.io/library/buildpack-deps:resolute"],
        ])

    def test_dev_workspace_image_env_rejects_unqualified_digest(self):
        self.stub("docker", "sys.exit('unexpected Docker lookup')\n")
        env = self.env.copy()
        for name in ("G5D_HOST_BASE_IMAGE", "G5_NEW_ORG_DEFAULT_IMAGE_TRACK",
                     "G5_PROMOTED_BASE_IMAGES"):
            env.pop(name, None)
        env["G5_DEV_WORKSPACE_BASE_IMAGE"] = "buildpack-deps@sha256:" + "a" * 64
        result = subprocess.run(
            ["bash", "-c", "source scripts/dev-workspace-image-env.sh"],
            cwd=ROOT, env=env, capture_output=True, text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Could not resolve an immutable local workspace image", result.stderr)
        self.assertFalse(self.log.exists(), "digest-pinned refs need no Docker lookup")

    @unittest.skipUnless(DOCKER, "Docker Compose CLI is needed to resolve volume names")
    def test_dev_storage_is_isolated_and_has_no_worker_workspace_volume(self):
        # Compose config is read-only and requires no running Docker daemon.
        config = json.loads(subprocess.check_output(
            [DOCKER, "compose", "-p", "g5-dev", "-f",
             "infra/docker/docker-compose.dev.yml", "--profile", "demo",
             "config", "--no-env-resolution", "--format", "json"], cwd=ROOT,
        ))
        names = {key: value["name"] for key, value in config["volumes"].items()}
        original = {"g5-dev_pgdata_dev", "g5-dev_api_repos", "g5-dev_worker_workspace"}
        self.assertTrue(original.isdisjoint(names.values()), "Lean must not reset original G5 data")
        self.assertNotIn("worker_workspace", names)

    @unittest.skipUnless(DOCKER, "Docker Compose CLI is needed to resolve service environment")
    def test_dev_stack_disables_cloud_provisioning_by_default(self):
        # Exercise the effective Compose config used by dev-up. An explicit
        # env file keeps a developer's local .env out of this contract.
        env = self.env.copy()
        env.pop("ENABLE_CLOUD_FEATURES", None)
        env.pop("G5_GIT_EXTERNAL_URL", None)
        env.pop("G5_PORT_API", None)
        env.pop("G5_NEW_ORG_DEFAULT_IMAGE_TRACK", None)
        env.pop("G5_PROMOTED_BASE_IMAGES", None)
        command = [DOCKER, "compose", "--env-file", "/dev/null", "-p", "g5-dev",
                   "-f", "infra/docker/docker-compose.dev.yml", "config", "--format", "json"]

        def cloud_flags():
            config = json.loads(subprocess.check_output(command, cwd=ROOT, env=env))
            return {name: config["services"][name]["environment"]["ENABLE_CLOUD_FEATURES"]
                    for name in ("g5-api", "g5-daemon")}

        self.assertEqual(cloud_flags(), {"g5-api": "false", "g5-daemon": "false"})
        config = json.loads(subprocess.check_output(command, cwd=ROOT, env=env))
        for name in ("g5-api", "g5-daemon"):
            self.assertEqual(config["services"][name]["environment"]
                             ["G5_NEW_ORG_DEFAULT_IMAGE_TRACK"], "")
            self.assertEqual(config["services"][name]["environment"]
                             ["G5_PROMOTED_BASE_IMAGES"], "")
            self.assertEqual(config["services"][name]["environment"]["G5_GIT_EXTERNAL_URL"],
                             "http://localhost:7500")
        env["G5_PORT_API"] = "7600"
        config = json.loads(subprocess.check_output(command, cwd=ROOT, env=env))
        for name in ("g5-api", "g5-daemon"):
            self.assertEqual(config["services"][name]["environment"]["G5_GIT_EXTERNAL_URL"],
                             "http://localhost:7600")
        env["ENABLE_CLOUD_FEATURES"] = "true"
        self.assertEqual(cloud_flags(), {"g5-api": "true", "g5-daemon": "true"})

        image_ref = "mirror.gcr.io/library/buildpack-deps@sha256:" + "a" * 64
        env["G5_NEW_ORG_DEFAULT_IMAGE_TRACK"] = "local-dev"
        env["G5_PROMOTED_BASE_IMAGES"] = "local-dev=" + image_ref
        config = json.loads(subprocess.check_output(command, cwd=ROOT, env=env))
        for name in ("g5-api", "g5-daemon"):
            with self.subTest(service=name):
                self.assertEqual(config["services"][name]["environment"]
                                 ["G5_NEW_ORG_DEFAULT_IMAGE_TRACK"], "local-dev")
                self.assertEqual(config["services"][name]["environment"]
                                 ["G5_PROMOTED_BASE_IMAGES"], "local-dev=" + image_ref)

    @unittest.skipUnless(DOCKER, "Docker Compose CLI is needed to resolve service environment")
    def test_dev_github_app_credentials_reach_api_and_integration_worker(self):
        # API requests and background Git transport resolve the same App
        # binding. Exercise Compose interpolation, including multiline PEMs,
        # without loading the developer's actual credentials.
        env = {key: value for key, value in self.env.items()
               if not key.startswith("G5_GITHUB_APP_")}
        configured = {
            "G5_GITHUB_APP_ID": "12345",
            "G5_GITHUB_APP_PRIVATE_KEY": "-----BEGIN PRIVATE KEY-----\nfixture\n-----END PRIVATE KEY-----\n",
            "G5_GITHUB_APP_SLUG": "fixture-app",
            "G5_GITHUB_APP_OAUTH_CLIENT_ID": "fixture-client",
            "G5_GITHUB_APP_OAUTH_CLIENT_SECRET": "fixture-secret",
            "G5_GITHUB_APP_OAUTH_CALLBACK_URL": "http://localhost:7500/github/callback",
            "G5_GITHUB_APP_CONNECTION_STATE_SECRET": "fixture-state-secret",
            "G5_GITHUB_APP_API_BASE_URL": "https://api.github.example",
            "G5_GITHUB_APP_WEB_BASE_URL": "https://github.example",
        }
        command = [DOCKER, "compose", "--env-file", "/dev/null", "-p", "g5-dev",
                   "-f", "infra/docker/docker-compose.dev.yml", "config", "--format", "json"]
        for enabled in (False, True):
            effective_env = dict(env, **configured) if enabled else env
            config = json.loads(subprocess.check_output(command, cwd=ROOT, env=effective_env))
            for service in ("g5-api", "g5-daemon"):
                with self.subTest(enabled=enabled, service=service):
                    actual = config["services"][service]["environment"]
                    for key, value in configured.items():
                        default = {
                            "G5_GITHUB_APP_API_BASE_URL": "https://api.github.com",
                            "G5_GITHUB_APP_WEB_BASE_URL": "https://github.com",
                        }.get(key, "")
                        self.assertEqual(actual.get(key), value if enabled else default, key)

    def test_warm_base_fingerprint_covers_relocated_inputs(self):
        self.stub("docker", "if sys.argv[1:3] == ['image', 'inspect']:\n    sys.exit(1)\n")
        files = {
            "infra/docker/Dockerfile.base": "FROM fixture\n",
            "infra/docker/Dockerfile.base-rust": "FROM fixture\n",
            "infra/docker/Dockerfile.base-rust-warm": "FROM fixture\n",
            "scripts/build-cloud-hypervisor-runtime.sh": "#!/bin/sh\n",
            "Cargo.toml": '[workspace]\nmembers=["backend/example", "tests/e2e"]\n',
            "Cargo.lock": "# fixture\n",
            ".cargo/config.toml": "# fixture\n",
            "backend/example/Cargo.toml": "# member manifest\n",
            "backend/example/build.rs": "fn main() {}\n",
            "backend/example/src/lib.rs": "pub fn original() {}\n",
            "tests/e2e/Cargo.toml": "# test manifest\n",
            "tests/e2e/src/main.rs": "fn main() {}\n",
        }
        for name, content in files.items():
            path = self.scratch / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        self.env["G5_BASE_LOCK_DIR"] = str(self.scratch / "build.lock")

        def fingerprint():
            subprocess.run(["bash", str(ROOT / "scripts/ensure-base-images.sh"), "base-rust-warm"],
                           cwd=self.scratch, env=self.env, capture_output=True, text=True, check=True)
            build = [call for call in self.calls()
                     if call[1] == "build" and "g5/base-rust-warm:26.04" in call][-1]
            return next(arg for arg in build if arg.startswith("g5.tier.dockerfile_sha="))

        previous = fingerprint()
        for name in ["backend/example/Cargo.toml", "tests/e2e/Cargo.toml", "backend/example/build.rs"]:
            with (self.scratch / name).open("a") as file:
                file.write("\n# changed input\n")
            current = fingerprint()
            self.assertNotEqual(previous, current, name)
            previous = current
        target = self.scratch / "backend/example/tests/new_target.rs"
        target.parent.mkdir()
        target.write_text("// new test target\n")
        current = fingerprint()
        self.assertNotEqual(previous, current)
        target.write_text("// changed test content, same dependency topology\n")
        self.assertEqual(current, fingerprint())


if __name__ == "__main__":
    unittest.main()
