"""Execute extracted CI phase bodies to verify failures cannot become success."""
from pathlib import Path
import json
import os
import re
import shlex
import subprocess
import unittest
import importlib.util
import tempfile
import io
from contextlib import redirect_stdout, redirect_stderr
from unittest.mock import patch

import workspace_metadata

ROOT = Path(__file__).resolve().parents[1]
SOURCE = (ROOT / "Justfile").read_text()


def phase(start, end):
    return SOURCE[SOURCE.index(start):SOURCE.index(end, SOURCE.index(start))]


class DockerCiContractTests(unittest.TestCase):
    def assert_phase_fails(self, body, prelude=""):
        result = subprocess.run(
            ["bash", "-euo", "pipefail", "-c", """
should_run() { return 0; }
failing_runner() {
  case "$*" in
    *nextest*|*"llvm-cov report"*) echo INJECTED_FAILURE; return 23 ;;
    *) return 0 ;;
  esac
}
docker() { echo INJECTED_FAILURE; return 23; }
CI_RUN=failing_runner
CI_RUN_LOW=failing_runner
CI_DOCKER_COVERAGE_ENABLED=1
G5D_CI_FEATURES=test-helpers
HOST_SRC=/unused
COMPOSE_NETWORK=fixture
CI_E2E_API_URL=http://fixture
CI_CONTAINER_WORKSPACE=/unused
""" + prelude + "\n" + body + '\nprintf "UNEXPECTED_SUCCESS\\n"'],
            capture_output=True, text=True,
        )
        self.assertIn("INJECTED_FAILURE", result.stdout, result.stderr)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertNotIn("UNEXPECTED_SUCCESS", result.stdout)

    def test_requested_alias_preserves_existing_recipe(self):
        metadata = json.loads(subprocess.check_output(["just", "--dump", "--dump-format", "json"], cwd=ROOT))
        self.assertEqual(metadata["aliases"]["docker-ci"]["target"], "ci-docker")

    def test_value_loop_runner_supports_every_catalog_profile(self):
        catalog_profiles = {
            line.split("\t")[2]
            for line in (ROOT / "tests/e2e/value_loop_matrix.tsv").read_text().splitlines()[1:]
            if line
        }
        runner = (ROOT / "scripts/run-value-loop-matrix.sh").read_text()
        profile_case = runner[
            runner.index('  case "$profile" in') : runner.index("  esac", runner.index('  case "$profile" in'))
        ]
        supported_profiles = set(re.findall(r"^    ([a-z0-9-]+)\)", profile_case, re.MULTILINE))
        self.assertLessEqual(catalog_profiles, supported_profiles)

    def test_cloud_e2e_uses_workspace_host_identity_in_all_four_lanes(self):
        source = (ROOT / "infra/ci/cloudbuild-ci-e2e.yaml").read_text()

        def step(step_id):
            start = source.index(f"  - id: {step_id}\n")
            end = source.find("\n  - id:", start + 1)
            return source[start:] if end == -1 else source[start:end]

        for start_id, wait_id in [
            ("start-workers", "wait-workers"),
            ("start-value-loop-worker", "wait-value-loop-workers"),
            ("start-rust-e2e-worker", "wait-rust-e2e-workers"),
            ("start-pa-worker", "wait-pa-workers"),
        ]:
            with self.subTest(start=start_id, wait=wait_id):
                start_step = step(start_id)
                wait_step = step(wait_id)
                self.assertIn('-e G5_HOST_JWT="$$HOST_JWT"', start_step)
                self.assertIn('-e G5D_HOST_ID="$$HOST_ID"', start_step)
                self.assertIn(
                    '-e G5D_HOST_RUNTIME_BIN_DIR=/opt/g5-runtime/bin', start_step
                )
                self.assertIn('-e G5_CLAUDE_CODE_FAKE=1', start_step)
                self.assertIn('-e G5_CODEX_FAKE=1', start_step)
                self.assertIn('--cgroupns=host', start_step)
                self.assertIn('-v /sys/fs/cgroup:/sys/fs/cgroup:ro', start_step)
                # The broad host admits fourteen one-vCPU 2.5 GiB guests; g5d caps
                # mem*slots at RAM - max(1.5 GiB, 10 %) and cpus*slots at the host
                # vCPUs (22). Experiment: fake-provider guests need one vCPU.
                cpus = 1 if start_id == 'start-workers' else 2
                self.assertIn(f'-e G5D_HOST_CPUS_PER_VM={cpus}', start_step)
                mem_mib = 2560 if start_id == 'start-workers' else 4096
                self.assertIn(f'-e G5D_HOST_MEM_MIB_PER_VM={mem_mib}', start_step)
                self.assertIn('-e G5D_SCRATCH_DISK_MIB="$$HOST_DISK_MIB"', start_step)
                self.assertIn('-e G5D_HOST_XFS_SIZE_GB="$$HOST_DISK_GIB"', start_step)
                self.assertNotIn('-e G5_RUNTIME_BIN_DIR=', start_step)
                self.assertRegex(start_step, r"ci-host-xfs-entrypoint\.sh \\\n\s+host")
                self.assertNotIn("G5D_REGISTRATION_TOKEN", start_step)
                self.assertNotIn("FROM workers", wait_step)
                self.assertNotIn("worker_session_generation", wait_step)
                self.assertIn("FROM worker_hosts", wait_step)
                self.assertIn("host_id = '$$HOST_ID'", wait_step)

    def test_host_launchers_use_measured_sizes_except_explicit_fixtures(self):
        compose_test = (ROOT / "infra/docker/docker-compose.test.yml").read_text()
        local_env = (ROOT / "scripts/worker-host-env.sh").read_text()
        self_hosted = (ROOT / "infra/self-hosted/scripts/g5-workerhost.sh").read_text()
        azure = (ROOT / ".github/workflows/self-hosted-azure-install.yml").read_text()

        self.assertIn("G5D_HOST_CPUS_PER_VM: 2", compose_test)
        self.assertIn("G5D_HOST_MEM_MIB_PER_VM: 4096", compose_test)
        self.assertNotIn("export G5D_HOST_CPUS_PER_VM=2", local_env)
        self.assertNotIn("export G5D_HOST_MEM_MIB_PER_VM=4096", local_env)
        self.assertNotIn("-e G5D_HOST_CPUS_PER_VM=2", self_hosted)
        self.assertNotIn("-e G5D_HOST_MEM_MIB_PER_VM=4096", self_hosted)
        for source in (compose_test, local_env, self_hosted):
            self.assertIn("G5D_SCRATCH_DISK_MIB", source)
        self.assertIn("G5D_HOST_CPUS_PER_VM=2", azure)
        self.assertIn("G5D_HOST_MEM_MIB_PER_VM=4096", azure)
        self.assertIn("G5D_SCRATCH_DISK_MIB=10240", azure)

    def test_local_e2e_api_keeps_provider_bringup_disabled(self):
        compose = (ROOT / "infra/docker/docker-compose.test.yml").read_text()
        self.assertIn(
            "ENABLE_CLOUD_FEATURES: ${ENABLE_CLOUD_FEATURES:-false}", compose
        )

    def test_coverage_unit_failure_fails_ci(self):
        self.assert_phase_fails(phase("    # Phase 1: Unit + bin", "    # Phase 1b: Isolation"))

    def test_isolation_failure_fails_ci(self):
        self.assert_phase_fails(phase("    # Phase 1b: Isolation", "    # Phase 2: Client"))

    def test_api_smoke_failure_fails_ci(self):
        self.assert_phase_fails(phase("    # Phase 6b: API smoke", "    # Phase 7: Playwright"))

    def test_coverage_report_failure_fails_ci(self):
        self.assert_phase_fails(phase("    # Phase 5c: Generate", "    # Phase 6: Rust E2E"))

    def test_default_e2e_runner_has_docker_and_bound_workspace_host(self):
        setup = phase('    PHASE6_DOCKER_SOCKET_FLAG=', '    if [[ -n "$E2E_FILTER" ]]; then')
        self.assertIn('/var/run/docker.sock:/var/run/docker.sock', setup)
        self.assertIn('PHASE6_HOST_CONTAINERS="$(list_g5d_host_containers)"', setup)
        runner = phase('    run_phase6_container() {', '    if true; then')
        self.assertIn('$PHASE6_DOCKER_SOCKET_FLAG', runner)
        self.assertIn('-e G5_E2E_HOST_CONTAINERS="$PHASE6_HOST_CONTAINERS"', runner)
        discovery = phase('    list_g5d_host_containers() {', '    refresh_ci_e2e_urls() {')
        self.assertIn('label=com.docker.compose.project=${COMPOSE_PROJECT}', discovery)
        self.assertIn('label=com.docker.compose.service=g5d-host', discovery)
        self.assertNotIn('docker pause', SOURCE)
        self.assertNotIn('G5_E2E_WORKER_CONTAINERS', SOURCE)

    def test_reuse_loads_complete_authoritative_seed_env_without_rewriting_it(self):
        body = phase('    load_seeded_fixture_env() {', '    running_stack_is_ready() {')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture = ("G5_E2E_TEST_WORKSPACE_ID=seed-workspace\n"
                       "G5_E2E_TEST_REPO_ID=seed-primary-repo\n"
                       "G5_E2E_TEST_ORG_ID=seed-org\n"
                       "G5_E2E_TEST_DOCS_REPO_ID=seed-docs-repo\n"
                       "G5_E2E_OVERLAY_SUPPORT_REPO_ID=seed-support-repo\n")
            (root / ".env.test").write_text(fixture)
            result = subprocess.run(["bash", "-euo", "pipefail", "-c", body + """
load_seeded_fixture_env
[[ "$G5_E2E_TEST_REPO_ID" == seed-primary-repo ]]
[[ "$G5_E2E_TEST_DOCS_REPO_ID" == seed-docs-repo ]]
[[ "$G5_E2E_OVERLAY_SUPPORT_REPO_ID" == seed-support-repo ]]
[[ "$G5D_ORG_ID" == seed-org ]]
"""], cwd=root, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / ".env.test").read_text(), fixture)

    def script_runner(self):
        spec = importlib.util.spec_from_file_location("ci_script_runner", ROOT / "scripts/run-ci-script-tests.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        return runner

    def test_browser_container_receives_project_identity_and_artifact_mount(self):
        body = phase("    # Phase 7: Playwright", "    fi  # Phase 7") + "\nfi\n"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            prelude = '''
should_run() { return 0; }
prepare_playwright_image() { PLAYWRIGHT_RUNNER_IMAGE=fixture-runner; }
resolve_host_path() { printf '%s' "$1"; }
python3() { :; }
docker() {
  case "$1" in
    create) echo fixture-container ;;
    cp) echo fixture-binary > "$3" ;;
    run) printf '%s\\n' "$@" > "$HOST_SRC/docker-arguments" ;;
  esac
  return 0
}
HOST_SRC=$PWD
CI_DOCKER_LOG_DIR=$PWD/logs
COMPOSE_PROJECT=isolated-browser
COMPOSE_NETWORK=isolated-network
CI_E2E_BROWSER_URL=https://app.g5-e2e.test
CI_E2E_PREVIEW_HOST_RESOLVER='MAP *.preview.g5-e2e.test client'
CI_E2E_API_URL=http://g5-api:7500
CI_E2E_OAUTH_URL=http://oauth-stub:9876
PLAYWRIGHT_FILTER=e2e/flows/login.spec.ts
G5_CI_PLAYWRIGHT_ARGS='["e2e/flows/login.spec.ts"]'
'''
            result = subprocess.run(["bash", "-euo", "pipefail", "-c", prelude + body],
                                    cwd=root, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            arguments = (root / "docker-arguments").read_text().splitlines()
            self.assertIn("E2E_API_CONTAINER=isolated-browser-api", arguments)
            self.assertIn("E2E_BASE_URL=https://app.g5-e2e.test", arguments)
            self.assertIn("E2E_PREVIEW_HOST_RESOLVER=MAP *.preview.g5-e2e.test client", arguments)
            self.assertIn(f"{root.resolve()}/logs/playwright:/artifacts", arguments)
            self.assertIn('G5_CI_PLAYWRIGHT_ARGS=["e2e/flows/login.spec.ts"]', arguments)

    def test_integration_selector_is_one_literal_argument_and_glob_is_preserved(self):
        body = phase("    # Phase 5: Integration", "    # Phase 5b:")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "glob-decoy").touch()
            selector = 'test(/quote"|$(touch unexpectedly-executed)/)'
            prelude = '''
should_run() { return 0; }
cargo() { printf '%s\\n' "$@" >> "$G5_TEST_ARGUMENT_LOG"; }
export -f cargo
docker() {
  while (( $# )); do
    case "$1" in
      -e) export "$2"; shift 2 ;;
      bash) "$@"; return ;;
      *) shift ;;
    esac
  done
}
E2E_FILTER=$G5_TEST_FILTER
CI_DOCKER_COVERAGE_ENABLED=0
KVM_FLAG=""
CI_VOLS=""
SCCACHE_ENV=""
CI_CONTAINER_WORKSPACE=/workspace
CI_CONTAINER_CARGO_HOME=/cargo
CI_CONTAINER_TARGET=/target
COMPOSE_PROJECT=fixture
COMPOSE_NETWORK=fixture
CI_IMAGE=fixture
'''
            log = root / "arguments"
            result = subprocess.run(["bash", "-euo", "pipefail", "-c", prelude + body], cwd=root,
                text=True, capture_output=True,
                env=dict(os.environ, G5_TEST_FILTER=selector, G5_TEST_ARGUMENT_LOG=str(log)))
            self.assertEqual(result.returncode, 0, result.stderr)
            arguments = log.read_text().splitlines()
            self.assertEqual(arguments.count(selector), 2)
            self.assertEqual(arguments.count("*"), 2)
            self.assertFalse((root / "unexpectedly-executed").exists())
            log.unlink()
            targets = ["-p", "g5d", "--test", "crate_architecture"]
            result = subprocess.run(["bash", "-euo", "pipefail", "-c", prelude + body], cwd=root,
                text=True, capture_output=True, env=dict(os.environ,
                    G5_TEST_FILTER="package(=g5d) and binary(=crate_architecture)",
                    G5_CI_INTEGRATION_ARGS=json.dumps(targets), G5_TEST_ARGUMENT_LOG=str(log)))
            self.assertEqual(result.returncode, 0, result.stderr)
            arguments = log.read_text().splitlines()
            self.assertEqual(arguments.count("nextest"), 1)
            self.assertEqual(arguments[2:6], targets)
            self.assertNotIn("g5-api", arguments)
            self.assertNotIn("*", arguments)

    def test_rust_e2e_selectors_are_parsed_as_data_in_every_slice(self):
        source = phase('    # Phase 6: Rust E2E', '    # Phase 6b: API smoke')
        commands = re.findall(r"run_phase6_container '([^']+)'", source)
        filtered = [command.replace("\\\n", "\n") for command in commands if "FILTER_ARGS=()" in command]
        self.assertEqual(len(filtered), 1)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            selector = 'name with spaces$(touch unexpectedly-executed)'
            for command in filtered:
                log = root / "arguments"
                result = subprocess.run(["bash", "-c", 'cargo() { printf "%s\\n" "$@" > "$G5_TEST_ARGUMENT_LOG"; }; ' + command],
                    cwd=root, text=True, capture_output=True,
                    env=dict(os.environ, E2E_FILTER=selector, G5_CI_RUST_ARGS=json.dumps([selector]),
                             G5_TEST_ARGUMENT_LOG=str(log)))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(selector, log.read_text().splitlines())
                self.assertFalse((root / "unexpectedly-executed").exists())

    def test_script_discovery_propagates_nested_failure(self):
        runner = self.script_runner()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "scripts/nested").mkdir(parents=True)
            (root / "scripts/nested/test-failure.sh").write_text("exit 23\n")
            (root / "scripts/test-success.py").write_text("pass\n")
            with patch.object(runner, "ROOT", root), redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                self.assertEqual(runner.main(), 1)

    def test_release_smoke_requires_complete_inputs_and_runs_when_configured(self):
        runner = self.script_runner()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "scripts").mkdir()
            marker = root / "ran"
            (root / "scripts/test-self-hosted-smoke.sh").write_text('touch ran\n')
            with patch.object(runner, "ROOT", root), patch.dict(runner.os.environ, {}, clear=True), redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                self.assertEqual(runner.main(), 0)
                self.assertFalse(marker.exists())
                runner.os.environ["G5_SELFHOST_SMOKE_API_IMAGE"] = "fixture-api"
                self.assertEqual(runner.main(), 1)
                self.assertFalse(marker.exists())
                runner.os.environ["G5_SELFHOST_SMOKE_EDGE_IMAGE"] = "fixture-edge"
                self.assertEqual(runner.main(), 0)
                self.assertTrue(marker.exists())

    def test_all_backend_test_targets_have_a_lane(self):
        metadata = workspace_metadata.cargo_metadata()
        units = phase("    # Phase 1: Unit + bin", "    # Phase 1b: Isolation")
        integration = phase("    # Phase 5: Integration", "    # Phase 5b:")
        for package in metadata["packages"]:
            if package["name"] == "g5-e2e":
                continue
            if any("lib" in target["kind"] for target in package["targets"]):
                self.assertRegex(units, rf'-p {re.escape(package["name"])}(?=\s)')
            if any("test" in target["kind"] for target in package["targets"]):
                self.assertRegex(integration, rf'-p {re.escape(package["name"])}(?=\s)')

    def test_baked_artifact_cleanup_covers_every_workspace_member(self):
        metadata = workspace_metadata.cargo_metadata()
        expected = [package["name"] for package in metadata["packages"]]

        def assert_packages(arguments):
            self.assertEqual(arguments[0], "clean")
            self.assertCountEqual(arguments[2::2], expected)
            self.assertEqual(arguments[1::2], ["-p"] * len(expected))

        # Both the dev/test warmup and the optional release warmup must drop
        # every local crate. Otherwise a fresh target cache can reuse a stub
        # whose timestamp is newer than the real mounted checkout.
        dockerfile = "\n".join(
            line for line in (ROOT / "infra/docker/Dockerfile.ci-builder").read_text().splitlines()
            if not line.lstrip().startswith("#")
        ).replace("\\\n", " ")
        clean_commands = re.findall(r"\bcargo clean\s+.*?(?=&&|;)", dockerfile, re.DOTALL)
        self.assertGreaterEqual(len(clean_commands), 2)
        for index, command in enumerate(clean_commands):
            with self.subTest(image_cleanup=index):
                assert_packages(shlex.split(command)[1:])

        # Exercise the runtime script's actual argv for both normal and
        # instrumented targets, without cleaning the developer's build cache.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "target"
            (target / "llvm-cov-target").mkdir(parents=True)
            binary = root / "cargo"
            log = root / "calls"
            binary.write_text(
                '#!/bin/sh\n'
                'printf "%s\\t" "$CARGO_TARGET_DIR" "$@" >> "$G5_CI_CLEAN_TEST_LOG"\n'
                'printf "\\n" >> "$G5_CI_CLEAN_TEST_LOG"\n'
            )
            binary.chmod(0o755)
            subprocess.run(
                ["bash", str(ROOT / "scripts/clean-baked-rust-workspace-artifacts.sh")],
                cwd=ROOT, check=True, capture_output=True, text=True,
                env=dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                         CARGO_TARGET_DIR=str(target), G5_CI_CLEAN_TEST_LOG=str(log),
                         G5_CI_SKIP_BAKED_ARTIFACT_CLEAN="false"),
            )
            calls = [line.rstrip("\t").split("\t") for line in log.read_text().splitlines()]
            self.assertEqual([call[0] for call in calls], [str(target), str(target / "llvm-cov-target")])
            for path, *arguments in calls:
                with self.subTest(runtime_cleanup=path):
                    assert_packages(arguments)


if __name__ == "__main__":
    unittest.main()
