"""Regressions at Git, Docker inspection, recipe execution and receipt boundaries."""

import contextlib
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import local_validation as validation
import compose_health
import check_local_validation_contracts as contracts

ROOT = Path(__file__).resolve().parents[1]


class LocalValidationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Test")
        self.git("config", "commit.gpgsign", "false")
        (self.root / ".gitignore").write_text(".dev/\n.env*\nfrontend/e2e/.fixtures/\nbin/\ndocker-state.json\n")
        (self.root / "source.rs").write_text("original\n")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        self.addCleanup(patch.stopall)
        patch.object(validation, "ROOT", self.root).start()
        patch.object(validation, "STATE", self.root / ".dev/validation").start()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        patch.dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}",
                   G5_CI_INPUT_CONFIG_DIGEST="fixture-config").start()
        self.project = "fixture-validation"

    def git(self, *arguments):
        return subprocess.check_output(["git", *arguments], cwd=self.root, text=True).strip()

    def docker(self):
        self.state = {"images": {"g5-ci-runner": "sha256:builder",
                                 f"{self.project}-playwright:ci": "sha256:browser"},
                      "schema": "1:abc:true", "containers": [],
                      "workspace": {"workspace": "fixture-id", "org": "fixture-org", "branches": [
                          {"branch_name": "main", "repo_id": "repo-a", "role": "code", "root_path": "a"},
                          {"branch_name": "main", "repo_id": "repo-b", "role": "docs", "root_path": "docs"}]}}
        for service in ("postgres", "oauth-stub", "g5-api", "client", "g5d-worker"):
            self.state["containers"].append({"Id": f"id-{service}", "Image": f"sha256:{service}",
                "RestartCount": 0, "Config": {"Labels": {"com.docker.compose.service": service}},
                "State": {"Running": True, "Paused": False, "Status": "running",
                          "StartedAt": "initial", "Health": {"Status": "healthy"}}})
        (self.root / ".env.test").write_text("G5_E2E_TEST_WORKSPACE_ID=00000000-0000-0000-0000-000000000001\n")
        self.write_state()
        executable = self.bin / "docker"
        executable.write_text("#!/usr/bin/env python3\n" + '''import json, sys
from pathlib import Path
state = json.loads((Path(__file__).parent.parent / "docker-state.json").read_text())
args = sys.argv[1:]
if args[:2] == ["ps", "-aq"]:
    assert args[2:] == ["--filter", "label=com.docker.compose.project=fixture-validation"], args
    print("\\n".join(c["Id"] for c in state["containers"]))
elif args[0] == "inspect":
    print(json.dumps(state["containers"]))
elif args[:2] == ["image", "inspect"]:
    if args[2] not in state["images"]: sys.exit(1)
    print(state["images"][args[2]])
elif args[0] == "exec":
    print(state["schema"] if "_sqlx_migrations" in args[-1] else json.dumps(state["workspace"]))
else:
    raise AssertionError(args)
''')
        executable.chmod(0o755)
        snapshot = validation.stack_snapshot(self.project)
        snapshot["ready_phases"] = ["5", "6", "7"]
        validation.atomic_json(validation.stack_receipt(self.project), snapshot)

    def write_state(self):
        (self.root / "docker-state.json").write_text(json.dumps(self.state))

    def test_dirty_source_untracked_deletion_modes_and_env_invalidate_same_head(self):
        before = validation.candidate()
        for mutate in [lambda: (self.root / "source.rs").write_text("edited\n"),
                       lambda: (self.root / "source.rs").unlink(),
                       lambda: (self.root / "untracked.rs").write_text("new\n"),
                       lambda: (self.root / ".env").write_text("SECRET=changed\n")]:
            mutate()
            after = validation.candidate()
            self.assertEqual(after["head"], before["head"])
            self.assertNotEqual(after["source"], before["source"])
            before = after
        (self.root / "untracked.rs").chmod(0o755)
        self.assertNotEqual(validation.candidate()["source"], before["source"])

    def test_workspace_row_order_is_stable_but_semantic_changes_reject_reuse(self):
        self.docker()
        self.state["workspace"]["branches"].reverse()
        self.write_state()
        validation.check_stack(self.project, "6")
        original = copy.deepcopy(self.state)
        for change in [lambda: self.state["workspace"]["branches"][0].update(branch_name="different"),
                       lambda: self.state["workspace"]["branches"][0].update(repo_id="replaced"),
                       lambda: self.state["workspace"]["branches"].pop(),
                       lambda: self.state.update(workspace=None)]:
            self.state = copy.deepcopy(original)
            change()
            self.write_state()
            with self.assertRaisesRegex(ValueError, "workspace"):
                validation.check_stack(self.project, "6")

    def test_ignored_receipts_do_not_dirty_candidate(self):
        before = validation.candidate()
        validation.atomic_json(validation.STATE / "run.json", {"status": "passed"})
        self.assertEqual(before, validation.candidate())

    def test_builder_key_covers_copied_build_scripts_sqlx_and_tool_manifests(self):
        dockerfile = self.root / "infra/docker/Dockerfile.ci-builder"
        dockerfile.parent.mkdir(parents=True)
        dockerfile.write_text("ARG BASE_IMAGE=fixture-parent\nFROM ${BASE_IMAGE}\n"
                              "COPY .cargo/ .cargo/\nCOPY .sqlx/ .sqlx/\n"
                              "COPY backend/example/build.rs backend/example/build.rs\n"
                              "COPY tools/example/Cargo.toml tools/example/Cargo.toml\n")
        files = [".cargo/config.toml", ".sqlx/query.json", "backend/example/build.rs", "tools/example/Cargo.toml"]
        for name in files:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("original")
        with patch.object(validation, "capture", return_value="sha256:parent"):
            before = validation.builder_key("0", "0")
            for name in files + [".dockerignore"]:
                (self.root / name).write_text("changed")
                after = validation.builder_key("0", "0")
                self.assertNotEqual(before, after, name)
                before = after
            self.assertNotEqual(before, validation.builder_key("1", "0"))
        with patch.object(validation, "capture", return_value="sha256:rebuilt-parent"):
            self.assertNotEqual(before, validation.builder_key("0", "0"))

    def test_stack_reuse_checks_actual_docker_identities_and_partial_recovery(self):
        self.docker()
        validation.check_stack(self.project, "6")
        original = copy.deepcopy(self.state)
        mutations = [
            lambda: self.state["images"].update({"g5-ci-runner": "sha256:other"}),
            lambda: self.state["containers"][0].update(Id="replacement"),
            lambda: self.state["containers"][0]["State"].update(StartedAt="restarted"),
            lambda: self.state["containers"][0]["State"].update(Paused=True),
            lambda: self.state["containers"].pop(0),
            lambda: self.state.update(schema="1:abc:false"),
            lambda: self.state.update(schema="2:def:true"),
        ]
        for mutate in mutations:
            self.state = copy.deepcopy(original)
            mutate()
            self.write_state()
            with self.assertRaises(ValueError):
                validation.check_stack(self.project, "6")
        self.state = original
        self.write_state()
        validation.check_stack(self.project, "6")
        (self.root / ".env.test").write_text("G5_E2E_TEST_WORKSPACE_ID=reseeded\n")
        with self.assertRaisesRegex(ValueError, "fixtures"):
            validation.check_stack(self.project, "6")

    def test_missing_or_partial_receipt_cannot_authorize_reuse(self):
        self.docker()
        receipt = validation.stack_receipt(self.project)
        receipt.write_text("{}")
        with self.assertRaises(ValueError):
            validation.check_stack(self.project, "6")
        receipt.unlink()
        with self.assertRaises(OSError):
            validation.check_stack(self.project, "6")

    def test_existing_runner_image_does_not_certify_an_unprepared_lane(self):
        snapshot = {"candidate": {"source": "new"}, "configuration": "same",
                    "images": {"g5-ci-runner": "sha256:builder"}}
        old = dict(snapshot, candidate={"source": "old"}, ready_phases=["5", "6", "7"])
        self.assertEqual(validation.prepared_phases(snapshot, "6"), ["5", "6"])
        self.assertEqual(validation.prepared_phases(snapshot, "7", old), ["7"])
        self.assertEqual(validation.prepared_phases(snapshot, "6", prepare_all=True), ["5", "6", "7"])
        old["candidate"] = snapshot["candidate"]
        self.assertEqual(validation.prepared_phases(snapshot, "7", old), ["5", "6", "7"])

    def test_project_identity_differs_across_worktrees(self):
        with patch.dict(os.environ, {}, clear=True):
            self.assertNotEqual(validation.project_name(self.root), validation.project_name(self.root / "sibling"))
        with patch.dict(os.environ, G5_CI_COMPOSE_PROJECT="dev; unsafe"):
            with self.assertRaises(ValueError):
                validation.project_name()

    def test_lock_rejects_overlapping_runs_and_releases_after_failure(self):
        # Preflight itself holds the real host lock. Exercise this boundary in
        # an isolated lock directory, including the competing child process.
        locks = self.root / ".dev"
        locks.mkdir()
        with patch.object(validation.tempfile, "gettempdir", return_value=str(locks)), patch.dict(os.environ, TMPDIR=str(locks)):
            with validation.validation_lock(self.project):
                command = f"import sys; sys.path.insert(0, {str(ROOT / 'scripts')!r}); import local_validation as v; v.validation_lock('other').__enter__()"
                result = subprocess.run([sys.executable, "-c", command], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Another local validation run", result.stderr)
            with validation.validation_lock(self.project):
                pass

    def test_failed_preflight_stops_before_next_command_and_records_rerun(self):
        directory = self.root / ".dev/preflight"
        directory.mkdir(parents=True)
        commands = [("bad", [sys.executable, "-c", "raise SystemExit(23)"]),
                    ("must-not-run", [sys.executable, "-c", "raise AssertionError('too late')"])]
        with patch.object(validation, "preflight_commands", return_value=commands), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(validation.run_preflight(directory), 23)
        receipt = json.loads((directory / "preflight.json").read_text())
        self.assertEqual(len(receipt), 1)
        self.assertEqual(receipt[0]["status"], 23)
        self.assertEqual(shlex.split(receipt[0]["rerun"]), commands[0][1])

    def test_failed_recipe_records_status_and_prints_rerun_before_service_logs(self):
        (self.root / "Justfile").write_text('''[positional-arguments]
_ci-docker +args:
    #!/usr/bin/env bash
    echo '=== Phase 6: Rust E2E tests ==='
    echo 'test scenarios::case ... FAILED'
    echo '=== E2E failures detected — dumping logs ==='
    echo 'SERVICE HEARTBEATS'
    exit 23
''')
        directory = self.root / ".dev/run"
        directory.mkdir(parents=True)
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = validation.execute(["6", "false", "scenarios::case", "", "auto"], directory, self.project)
        self.assertEqual(status, 23)
        text = output.getvalue()
        self.assertLess(text.index("Rerun:"), text.index("SERVICE HEARTBEATS"))
        receipt = json.loads((directory / "receipt.json").read_text())
        self.assertEqual(receipt["status"], 23)
        self.assertEqual(receipt["phases"][-1]["status"], "failed")
        self.assertIn("scenarios::case", receipt["rerun"])

    def test_selectors_preserve_quotes_and_invalid_modes_fail_early(self):
        selector = '--grep "a phrase; $(must-not-run)" e2e/example.spec.ts'
        self.assertEqual(validation.normalize([selector]), ["7", "false", "", selector, "auto"])
        self.assertEqual(validation.normalize(["5", "test(foo)"]), ["5", "false", "test(foo)", "", "auto"])
        with self.assertRaises(ValueError):
            validation.normalize(["7", "true"])

    def test_integration_narrowing_requires_exact_conjunctive_scope(self):
        source = (ROOT / "Justfile").read_text()
        def select(expression):
            metadata = {"packages": [{"name": "g5d", "targets": [{"name": "crate_architecture", "kind": ["test"]}]}]}
            with patch.object(validation, "cargo_metadata", return_value=metadata):
                return validation.integration_targets(expression, source)
        self.assertEqual(select("package(=g5d)"), ["-p", "g5d", "--test", "*"])
        self.assertEqual(select("package(=g5d) and binary(=crate_architecture)"),
                         ["-p", "g5d", "--test", "crate_architecture"])
        self.assertEqual(select("package(=g5d) and test(=a::b)"), ["-p", "g5d", "--test", "*"])
        for expression in ["package(=g5d) or package(=g5-api)",
                           "package(=g5d) and binary(=one) or binary(=two)",
                           "not package(=g5d)", "package(/g5/)", "test(=one)",
                           "package(=unlisted)", "package(=g5d) and binary(/wild/)"]:
            self.assertEqual(select(expression), [], expression)

    def test_health_rejects_missing_services_and_failed_oneshot(self):
        self.docker()
        config = {"services": {name: {} for name in ("postgres", "g5-api")}}
        self.assertEqual(compose_health.problems(config, self.state["containers"]), [])
        self.assertTrue(compose_health.problems(config, self.state["containers"][1:]))
        config["services"]["g5-api"]["depends_on"] = {"db-migrate": {"condition": "service_completed_successfully"}}
        migration = {"Config": {"Labels": {"com.docker.compose.service": "db-migrate"}},
                     "State": {"Status": "exited", "ExitCode": 1}}
        self.assertTrue(compose_health.problems(config, self.state["containers"] + [migration]))
        migration["State"]["ExitCode"] = 0
        self.assertEqual(compose_health.problems(config, self.state["containers"] + [migration]), [])

    def test_migration_duplicates_and_fixed_fixture_names_fail_preflight(self):
        migrations = self.root / "backend/g5-api/src/db/migrations"
        migrations.mkdir(parents=True)
        (migrations / "001_first.sql").write_text("select 1;")
        (migrations / "1_second.sql").write_text("select 2;")
        directory = self.root / "frontend/e2e"
        directory.mkdir(parents=True)
        (directory / "bad.ts").write_text('const api = "g5-test-api";')
        errors = contracts.check(self.root)
        self.assertTrue(any("duplicate migration" in error for error in errors))
        self.assertTrue(any("Compose identity" in error for error in errors))

    def test_actual_cleanup_retains_failed_stack_and_honors_explicit_teardown(self):
        source = (ROOT / "Justfile").read_text()
        start = source.index("    cleanup() {", source.index("_ci-docker start_phase="))
        body = source[start:source.index("    cleanup_named_test_containers() {", start)]
        prelude = '''
COMPOSE_PROJECT=fixture-validation
COMPOSE_FILE=fixture.yml
ci_section() { :; }
ci_info() { echo "$*"; }
dump_ci_docker_diagnostics() { echo DIAGNOSTICS; }
docker() { echo TEARDOWN; }
python3() { echo INVALIDATE_RECEIPT; }
cleanup_named_test_containers() { :; }
_force_remove_project_containers() { :; }
cleanup_repos_volume() { :; }
'''
        for mode, status, retained in [("on-failure", 23, True), ("on-failure", 0, False),
                                       ("always", 0, True), ("never", 23, False)]:
            result = subprocess.run(["bash", "-c", prelude + body + f"\nG5_CI_KEEP_STACK={mode}\ncleanup {status}"],
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode, status, result.stderr)
            self.assertEqual("TEARDOWN" not in result.stdout, retained)
            self.assertEqual("INVALIDATE_RECEIPT" not in result.stdout, retained)
            self.assertEqual("DIAGNOSTICS" in result.stdout, status != 0)

    def test_real_compose_interpolation_isolates_projects(self):
        # Rendering Compose config performs no daemon mutation.
        names = []
        for project in ("validation-one", "validation-two"):
            env = dict(os.environ, G5_TEST_API_IMAGE=f"{project}/api:test",
                       G5_CI_COMPOSE_PROJECT=project, G5_LOCAL_VALIDATION_WORKTREE=str(ROOT))
            config = json.loads(subprocess.check_output(
                ["docker", "compose", "--profile", "*", "-p", project, "-f", str(ROOT / validation.COMPOSE), "config", "--format", "json"],
                cwd=ROOT, env=env, text=True))
            names.append({service["container_name"] for service in config["services"].values() if "container_name" in service})
            self.assertEqual(config["services"]["g5-api"]["image"], f"{project}/api:test")
            self.assertEqual(config["services"]["db-migrate"]["image"], f"{project}/api:test")
            builds = {name: service["build"] for name, service in config["services"].items() if service.get("build")}
            self.assertTrue({"g5d-host", "test-runner"} <= builds.keys())
            for build in builds.values():
                self.assertEqual(build["labels"]["io.g5.validation.worktree"], str(ROOT))
                self.assertEqual(build["labels"]["io.g5.validation.project"], project)
        self.assertFalse(names[0] & names[1])

    def test_failure_selectors_produce_focused_reruns(self):
        self.assertEqual(validation.failed_selector("FAIL [ 0.02s] g5d::crate_architecture inventory", "5"), "test(=inventory)")
        self.assertEqual(validation.failed_selector("test scenarios::foo ... FAILED", "6"), "scenarios::foo")
        self.assertEqual(validation.failed_selector("1) [chromium] › e2e/flows/foo.spec.ts:12:3 › failure", "7"), "e2e/flows/foo.spec.ts:12")

    def test_test_counts_distinguish_execution_from_skipping(self):
        self.assertEqual(validation.observed_counts("Summary [ 0.1s] 1 test run: 1 passed, 100 skipped", "5"),
                         {"passed": 1, "failed": 0, "ignored": 100})
        self.assertEqual(validation.observed_counts("test result: ok. 0 passed; 0 failed; 0 ignored; 10 filtered out", "6")["passed"], 0)
        self.assertEqual(validation.observed_counts("  3 skipped", "7"), {"passed": 0, "failed": 0, "ignored": 3})
        self.assertEqual(validation.observed_counts("  1 flaky", "7")["flaky"], 1)
        self.assertEqual(validation.observed_counts("Summary [ 0.1s] 2 tests run: 1 passed, 1 failed", "1"),
                         {"passed": 1, "failed": 1, "ignored": 0})
        self.assertIsNone(validation.observed_counts("    test result: FAILED. 0 passed; 1 failed; 0 ignored", "1"))
        self.assertIsNone(validation.observed_counts("test result: ok. 1 passed; 0 failed; 0 ignored", "7"))

    def test_browser_auth_success_cannot_hide_a_fully_skipped_selector(self):
        (self.root / "Justfile").write_text('''[positional-arguments]
_ci-docker +args:
    #!/usr/bin/env bash
    echo '=== Phase 7: Playwright E2E ==='
    echo '  1 passed (1s)'
    echo '=== Chromium tests ==='
    echo '  3 skipped'
''')
        directory = self.root / ".dev/run"
        directory.mkdir(parents=True)
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(validation.execute(["7", "false", "", "e2e/skipped.spec.ts", "auto"], directory, self.project), 1)
        receipt = json.loads((directory / "receipt.json").read_text())
        self.assertEqual(receipt["test_counts"]["7"]["passed"], 0)


    def test_nextest_progress_and_failure_stdout_produce_valid_receipts(self):
        (self.root / 'Justfile').write_text('''[positional-arguments]
_ci-docker +args:
    #!/usr/bin/env bash
    echo '=== Phase 5: Integration tests ==='
    echo '        FAIL [   0.543s] (   1/2) g5-api::api_git my::failing_test'
    echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 17 filtered out;'
    echo '     Summary [   1.000s] 2 tests run: 1 passed, 1 failed, 10 skipped'
    exit 23
''')
        directory = self.root / '.dev/nextest'
        directory.mkdir(parents=True)
        with contextlib.redirect_stdout(io.StringIO()):
            status = validation.execute(['5', 'false', 'test(my)', '', 'auto'], directory, self.project)
        self.assertEqual(status, 23)
        receipt = json.loads((directory / 'receipt.json').read_text())
        self.assertEqual(receipt['failed_selectors'], ['test(=my::failing_test)'])
        self.assertEqual(receipt['test_counts']['5'], {'passed': 1, 'failed': 1, 'ignored': 10})
        self.assertIn('test(=my::failing_test)', shlex.split(receipt['rerun']))


    def test_url_literals_are_checked_without_flagging_comments(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            ts=root/'frontend/e2e/url.ts'; ts.parent.mkdir(parents=True)
            ts.write_text('''// "http://g5-test-api" is explanatory
const api = "http://g5-test-api:7500/path";
const pg = 'postgres://g5-test-postgres/db'; // tail comment
/* block comment
"http://g5-test-client"
*/
const url = `http://g5-test-client:80`;
const quoted = "\\\"http://g5-test-api\\\"";
''')
            rust=root/'tests/e2e/src/url.rs';rust.parent.mkdir(parents=True)
            rust.write_text('''fn borrowed(value: &'static str) {}
let api = r###"inner "quoted" http://g5-test-api/path//rest"###;
// "http://g5-test-client"
let ok = "http://localhost"; // "http://g5-test-api"
let cloud_build_edge = "--name=g5-test-client-pa"; // a suffixed Cloud Build name is not a Compose identity
''')
            findings=[error for error in contracts.check(root) if 'Compose identity' in error]
            self.assertEqual(len(findings),5,findings)
            for line in (2,3,7,8):
                self.assertTrue(any(f'frontend/e2e/url.ts:{line}:' in error for error in findings),findings)
            self.assertTrue(any('tests/e2e/src/url.rs:2:' in error for error in findings),findings)


if __name__ == "__main__":
    unittest.main()
