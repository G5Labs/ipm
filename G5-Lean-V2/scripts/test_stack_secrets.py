#!/usr/bin/env python3
"""Exercise real key generation, publication, overrides and competing starts."""

import concurrent.futures
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import textwrap
import unittest
from unittest.mock import patch

import stack_secrets as secrets


class StackSecretsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.bundle = secrets.generate()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="g5-stack-secrets-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.path = self.root / "credentials.json"
        self.env = {key: value for key, value in os.environ.items()
                    if key not in (*secrets.KEYS, secrets.USER_SIGNER, secrets.E2E_SIGNER, "G5_CI_STACK_SECRETS_DIGEST")}
        self.env["G5_LOCAL_STACK_SECRETS_FILE"] = str(self.path)
        self.env["PATH"] = f"{self.root}:{self.env['PATH']}"
        self.env["FAKE_DOCKER_CALLS"] = str(self.root / "docker-calls")
        docker = self.root / "docker"
        docker.write_text('''#!/usr/bin/env python3
import json, os, sys
with open(os.environ["FAKE_DOCKER_CALLS"], "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\\n")
if os.environ.get("FAKE_DOCKER_FAIL"):
    sys.exit(1)
if sys.argv[1:3] == ["volume", "ls"]:
    print(os.environ.get("FAKE_DOCKER_VOLUMES", ""))
elif sys.argv[1] == "ps":
    print("retained" if os.environ.get("FAKE_DOCKER_LABELS") else "")
elif sys.argv[1] == "inspect":
    print(json.dumps([{"Config": {"Labels": json.loads(os.environ["FAKE_DOCKER_LABELS"])},
                       "NetworkSettings": {"Networks": json.loads(os.environ.get("FAKE_DOCKER_NETWORKS", "{}"))}}]))
else:
    raise AssertionError(sys.argv)
''')
        docker.chmod(0o755)

    def cli(self, **overrides):
        return subprocess.run(
            ["python3", str(secrets.ROOT / "scripts/stack_secrets.py"), "local"],
            env={**self.env, **overrides}, text=True, capture_output=True, timeout=20,
        )

    def install(self, contents):
        self.path.write_text(contents)
        self.path.chmod(0o600)

    def test_competing_initializers_publish_one_bundle_and_restart_reuses_it(self):
        self.path = self.root / "new" / "config" / "credentials.json"
        self.env["G5_LOCAL_STACK_SECRETS_FILE"] = str(self.path)
        with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
            runs = list(pool.map(lambda _: self.cli(), range(5)))
        self.assertTrue(all(run.returncode == 0 for run in runs), [run.stderr for run in runs])
        self.assertEqual(len({run.stdout for run in runs}), 1)
        original = self.path.read_bytes()
        bundle = secrets.validate(json.loads(original))
        self.assertNotEqual(bundle[secrets.SCM], secrets.FIXTURE_SCM)
        self.assertEqual(self.path.stat().st_mode & 0o777, 0o600)
        # Existing data and an unavailable daemon do not rotate retained keys.
        again = self.cli(FAKE_DOCKER_FAIL="1", FAKE_DOCKER_VOLUMES="g5-lean_pgdata_dev")
        self.assertEqual(again.returncode, 0, again.stderr)
        self.assertEqual(again.stdout, runs[0].stdout)
        self.assertEqual(self.path.read_bytes(), original)
        calls = [json.loads(line) for line in (self.root / "docker-calls").read_text().splitlines()]
        self.assertEqual(sum(call[:2] == ["volume", "ls"] for call in calls), 1)
        self.assertEqual(list(self.path.parent.glob(".g5-secrets-*")), [])

    def test_separate_jobs_have_distinct_keys(self):
        first = secrets.retained_bundle(self.root / "job1.json", local=False)
        second = secrets.retained_bundle(self.root / "job2.json", local=False)
        self.assertNotEqual(first[secrets.PRIVATE], second[secrets.PRIVATE])
        self.assertNotEqual(first[secrets.SCM], second[secrets.SCM])
        self.assertEqual(secrets.retained_bundle(self.root / "job1.json", local=False), first)

    def test_bundle_cannot_be_written_into_another_worktree(self):
        other = self.root / "another-worktree"
        other.mkdir()
        (other / ".git").write_text("gitdir: /outside/common/worktrees/another\n")
        path = other / "ignored" / "credentials.json"
        with self.assertRaisesRegex(ValueError, "outside the source checkout"):
            secrets.retained_bundle(path, local=False)
        self.assertFalse(path.parent.exists())

    def test_self_hosted_smoke_provisions_fresh_compatible_keys(self):
        source = (secrets.ROOT / "scripts/test-self-hosted-smoke.sh").read_text()
        marker = 'set +x\nsource "$ROOT/scripts/lib/jwt-keys.sh"'
        generation = marker + source.split(marker, 1)[1].split("\n\n", 1)[0]
        scm = next(line for line in source.splitlines() if line.startswith("export G5_SCM_ENCRYPTION_KEY="))
        bundles = []
        for _ in range(2):
            result = subprocess.run(
                ["bash", "-euxo", "pipefail", "-c", generation + "\n" + scm +
                 '\nprintf "%s\\0%s\\0%s" "$PRIVATE_KEY_PEM" "$PUBLIC_KEY_PEM" "$G5_SCM_ENCRYPTION_KEY"'],
                env={**self.env, "ROOT": str(secrets.ROOT)}, capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            bundle = dict(zip(secrets.KEYS, result.stdout.split("\0")))
            bundles.append(secrets.validate(bundle))
            for value in bundle.values():
                self.assertNotIn(value, result.stderr)
        self.assertNotEqual(bundles[0][secrets.PRIVATE], bundles[1][secrets.PRIVATE])
        self.assertNotEqual(bundles[0][secrets.SCM], bundles[1][secrets.SCM])

    def test_each_ci_api_launch_consumes_the_same_job_bundle(self):
        self.install(json.dumps(self.bundle))
        (self.root / "env").write_text(secrets.shell_exports(self.bundle))
        # Replace only the Docker transport, then execute the real launch script
        # through its run command. A missing source/export fails at this boundary.
        (self.root / "docker").write_text('''#!/usr/bin/env python3
import json, os, sys
bundle = json.load(open(os.environ["G5_LOCAL_STACK_SECRETS_FILE"]))
bundle["G5_JWT_SECRET"] = bundle["G5_JWT_PRIVATE_KEY"]
assert sys.argv[1:3] == ["run", "-d"]
for key, value in bundle.items():
    assert os.environ.get(key) == value, key + " missing or mismatched"
    assert any(a == "-e" and b == key for a, b in zip(sys.argv, sys.argv[1:])), key
    assert all(value not in arg for arg in sys.argv), "secret value in Docker arguments"
''')
        source = (secrets.ROOT / "infra/ci/cloudbuild-ci-e2e.yaml").read_text()
        for step in ["start-g5-api", "start-rust-e2e-api", "start-pa-api"]:
            with self.subTest(step=step):
                block = source.split(f"  - id: {step}\n", 1)[1].split("\n  - id:", 1)[0]
                self.assertIn("name: stack-secrets", block)
                self.assertIn("'generate-stack-secrets'", block)
                script = re.search(r"      - \|\n((?:        .*\n|\n)+)", block).group(1)
                script = textwrap.dedent(script).split("\nfor attempt", 1)[0]
                script = script.replace("/g5-ci-secrets/", str(self.root) + "/").replace("$$", "$")
                result = subprocess.run(
                    ["bash", "-euxo", "pipefail", "-c", script],
                    env={**self.env, "_G5_CI_RUST_E2E_ISOLATED": "true",
                         "_G5_CI_PROVIDER_AGENT_ISOLATED": "true", "_E2E_WORKER": "workspace-vm",
                         "_E2E_DB_NAME": "test", "_AR_REGION": "test", "PROJECT_ID": "test",
                         "_CI_REPOSITORY": "test", "SHORT_SHA": "test"},
                    text=True, capture_output=True, timeout=10,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                for value in self.bundle.values():
                    self.assertNotIn(value, result.stdout + result.stderr)

    def test_explicit_complete_credentials_win_without_creating_managed_state(self):
        result = self.cli(**self.bundle, FAKE_DOCKER_FAIL="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.path.exists())
        self.assertFalse(Path(str(self.path) + ".lock").exists())
        self.assertIn("export G5_JWT_SECRET=", result.stdout)

    def test_partial_overrides_never_mix_with_retained_keys(self):
        original = json.dumps(self.bundle)
        self.install(original)
        for key in (*secrets.KEYS, secrets.USER_SIGNER):
            with self.subTest(key=key):
                value = self.bundle.get(key, self.bundle[secrets.PRIVATE])
                result = self.cli(**{key: value})
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")
                self.assertEqual(self.path.read_text(), original)
                self.assertNotIn(value, result.stderr)

    def test_stale_scm_only_configuration_names_missing_fields_and_recovery(self):
        result = self.cli(**{secrets.SCM: secrets.FIXTURE_SCM})
        self.assertNotEqual(result.returncode, 0)
        for field in [secrets.PRIVATE, secrets.PUBLIC]:
            self.assertIn(field, result.stderr)
        self.assertIn("docs/local-stack-secrets.md", result.stderr)
        self.assertNotIn(secrets.FIXTURE_SCM, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertFalse(self.path.exists())

    def test_native_e2e_recipes_receive_the_stack_signer(self):
        project = self.root / "project"
        (project / "scripts/lib").mkdir(parents=True)
        for relative in ["scripts/stack_secrets.py", "scripts/lib/local-stack-secrets.sh",
                         "scripts/lib/jwt-keys.sh"]:
            shutil.copyfile(secrets.ROOT / relative, project / relative)
        # URL/network resolution and the cargo/stack launch transports are
        # outside this test; the credential loader and real recipe are intact.
        (project / "scripts/resolve-e2e-url.sh").write_text(":\n")
        self.install(json.dumps(self.bundle))
        for command in ["cargo", "just"]:
            script = self.root / command
            script.write_text('''#!/usr/bin/env python3
import json, os, sys
expected = json.load(open(os.environ["G5_LOCAL_STACK_SECRETS_FILE"]))["G5_JWT_PRIVATE_KEY"]
assert os.environ.get("G5_E2E_JWT_PRIVATE_KEY") == expected, "runner has no matching signer"
with open(os.environ["E2E_TEST_CALLS"], "a") as calls:
    calls.write(os.path.basename(sys.argv[0]) + " " + " ".join(sys.argv[1:]) + "\\n")
''')
            script.chmod(0o755)
        source = (secrets.ROOT / "Justfile").read_text()
        for recipe in ["e2e-run", "e2e-live", "test-live-e2e"]:
            with self.subTest(recipe=recipe):
                body = re.search(r"(?m)^" + recipe + r"[^\n]*:\n((?:    .*\n|\n)+)", source).group(1)
                body = textwrap.dedent(body).replace("{{ARGS}}", "host_loss").replace("{{filter}}", "host_loss")
                calls = self.root / (recipe + "-calls")
                result = subprocess.run(["bash", "-c", body], cwd=project,
                                        env={**self.env, "E2E_TEST_CALLS": str(calls)},
                                        text=True, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("cargo test -p g5-e2e", calls.read_text())
                self.assertIn("host_loss", calls.read_text())
                if recipe == "e2e-live":
                    self.assertIn("just e2e-up", calls.read_text())
                    self.assertIn("just e2e-down", calls.read_text())
                for value in self.bundle.values():
                    self.assertNotIn(value, result.stdout + result.stderr)

        # The direct runner can target an external stack with an explicit key;
        # it must not try to read even a corrupt local managed bundle.
        self.path.write_text("corrupt retained state")
        (self.root / "cargo").write_text('''#!/usr/bin/env python3
import os
assert os.environ["G5_E2E_JWT_PRIVATE_KEY"] == "explicit-external-test-signer"
''')
        for recipe in ["e2e-run", "test-live-e2e"]:
            body = re.search(r"(?m)^" + recipe + r"[^\n]*:\n((?:    .*\n|\n)+)", source).group(1)
            result = subprocess.run(["bash", "-c", textwrap.dedent(body).replace("{{ARGS}}", "host_loss")],
                                    cwd=project, env={**self.env, secrets.E2E_SIGNER: "explicit-external-test-signer"},
                                    text=True, capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_dev_e2e_wrapper_loads_signer_after_fixture_environment(self):
        project = self.root / "dev-project"
        (project / "scripts/lib").mkdir(parents=True)
        (project / ".dev/g5d-host").mkdir(parents=True)
        (project / ".dev/g5d-host/host.pid").write_text("12345\n")
        for relative in ["scripts/run-dev-stack-tests.sh", "scripts/stack_secrets.py",
                         "scripts/lib/local-stack-secrets.sh", "scripts/lib/jwt-keys.sh"]:
            shutil.copyfile(secrets.ROOT / relative, project / relative)
        # Exercise the whole wrapper, substituting only external transports.
        commands = {
            "docker": '#!/bin/sh\ncase "$1" in inspect) echo true;; port) echo "127.0.0.1:7500";; *) exit 1;; esac\n',
            "ps": '#!/bin/sh\necho "g5d host"\n',
            "curl": '#!/bin/sh\nexit 0\n',
            "cargo": '''#!/usr/bin/env python3
import json, os, sys
expected = os.environ.get("EXPECTED_EXTERNAL_SIGNER") or json.load(open(os.environ["G5_LOCAL_STACK_SECRETS_FILE"]))["G5_JWT_PRIVATE_KEY"]
assert os.environ.get("G5_E2E_JWT_PRIVATE_KEY") == expected, "dev runner has no matching signer"
assert "host_loss" in sys.argv
''',
        }
        for name, source in commands.items():
            executable = self.root / name
            executable.write_text(source)
            executable.chmod(0o755)
        self.install(json.dumps(self.bundle))
        for external in [False, True]:
            with self.subTest(external=external):
                # The fixture environment is loaded by seed_dev_fixture, after
                # .env: an explicit external signer there must still win.
                (project / ".env.test").write_text(
                    "G5_E2E_JWT_PRIVATE_KEY=external-fixture-signer\n" if external else "")
                env = {**self.env, "G5_DEV_E2E_SKIP_SEED": "true"}
                if external:
                    self.path.write_text("corrupt retained state")
                    env["EXPECTED_EXTERNAL_SIGNER"] = "external-fixture-signer"
                result = subprocess.run(["bash", "scripts/run-dev-stack-tests.sh", "rust", "host_loss"],
                                        cwd=project, env=env, text=True, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                for value in self.bundle.values():
                    self.assertNotIn(value, result.stdout + result.stderr)

    def test_directory_flush_failure_refuses_use_and_retry_keeps_keys(self):
        real_fsync = os.fsync

        def fail_directory(fd):
            if stat.S_ISDIR(os.fstat(fd).st_mode):
                raise OSError("injected directory flush failure")
            return real_fsync(fd)

        # Rename has already published a valid file, but credentials must not
        # escape until its directory entry is durable, including on retry.
        with patch.object(secrets.os, "fsync", side_effect=fail_directory):
            with self.assertRaisesRegex(OSError, "directory flush"):
                secrets.retained_bundle(self.path, local=False)
            original = self.path.read_bytes()
            secrets.validate(json.loads(original))
            with self.assertRaisesRegex(OSError, "directory flush"):
                secrets.retained_bundle(self.path, local=False)
            self.assertEqual(self.path.read_bytes(), original)
        restored = secrets.retained_bundle(self.path, local=False)
        self.assertEqual(restored, json.loads(original))
        self.assertEqual(self.path.read_bytes(), original)
        self.assertEqual(list(self.root.glob(".g5-secrets-*")), [])

    def test_ancestor_flush_failure_refuses_new_bundle_and_retained_retry(self):
        self.path = self.root / "new" / "config" / "g5" / "credentials.json"
        ancestor = self.path.parent.parent
        real_fsync = os.fsync
        flushed = set()

        def sync(fd, *, fail):
            current = os.fstat(fd)
            if stat.S_ISDIR(current.st_mode):
                identity = (current.st_dev, current.st_ino)
                flushed.add(identity)
                expected = ancestor.stat()
                if fail and identity == (expected.st_dev, expected.st_ino):
                    raise OSError("injected ancestor flush failure")
            return real_fsync(fd)

        with patch.object(secrets.os, "fsync", side_effect=lambda fd: sync(fd, fail=True)):
            with self.assertRaisesRegex(OSError, "ancestor flush"):
                secrets.retained_bundle(self.path, local=False)
            original = self.path.read_bytes()
            # The retry sees existing directories and a complete file. It must
            # still flush the ancestor that the previous initializer missed.
            with self.assertRaisesRegex(OSError, "ancestor flush"):
                secrets.retained_bundle(self.path, local=False)
            self.assertEqual(self.path.read_bytes(), original)
        flushed.clear()
        with patch.object(secrets.os, "fsync", side_effect=lambda fd: sync(fd, fail=False)):
            self.assertEqual(secrets.retained_bundle(self.path, local=False), json.loads(original))
        for directory in self.path.parents:
            info = directory.stat()
            self.assertIn((info.st_dev, info.st_ino), flushed)
        self.assertEqual(self.path.read_bytes(), original)

    def test_unrelated_compose_filenames_do_not_block_first_generation(self):
        for basename in ["docker-compose.dev.yml", "docker-compose.test.yml"]:
            with self.subTest(basename=basename):
                labels = {"com.docker.compose.project": "another-app",
                          "com.docker.compose.project.config_files": f"/another/app/{basename}"}
                result = self.cli(FAKE_DOCKER_LABELS=json.dumps(labels),
                                  FAKE_DOCKER_NETWORKS=json.dumps({"another-app_default": {}}))
                self.assertEqual(result.returncode, 0, result.stderr)
                secrets.validate(json.loads(self.path.read_text()))
                self.path.unlink()

    def test_g5_identity_detects_retained_partial_and_custom_stacks(self):
        for project, extra, networks in [
            ("g5-dev", {}, {}),
            ("g5-test", {}, {}),
            ("g5-local-0123456789", {}, {}),
            ("custom-qa", {"io.g5.validation.project": "custom-qa"}, {}),
            ("custom-disconnected", {"com.docker.compose.service": "g5-api"}, {}),
            ("custom-daemon", {"com.docker.compose.service": "g5-daemon"}, {}),
            ("custom-old-daemon", {"com.docker.compose.service": "g5-api-indexer"}, {}),
            # A stopped Postgres-only partial stack has no G5 image labels.
            ("custom-test", {}, {"custom-test_g5-test": {}}),
            ("custom-dev", {}, {"custom-dev_g5-dev": {}}),
        ]:
            with self.subTest(project=project):
                labels = {"com.docker.compose.project": project,
                          "com.docker.compose.service": "postgres",
                          "com.docker.compose.project.config_files": "/deleted/checkout/infra/docker/docker-compose.test.yml",
                          **extra}
                result = self.cli(FAKE_DOCKER_LABELS=json.dumps(labels),
                                  FAKE_DOCKER_NETWORKS=json.dumps(networks))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("retained G5 local stack", result.stderr)
                self.assertEqual(result.stdout, "")
                self.assertFalse(self.path.exists())

    def test_corrupt_or_partial_retained_file_is_not_regenerated(self):
        for contents in ['{"G5_JWT_PRIVATE_KEY":', '{}', 'null']:
            with self.subTest(contents=contents):
                self.install(contents)
                result = self.cli()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.path.read_text(), contents)
                self.assertEqual(result.stdout, "")

    def test_fixture_keys_are_rejected_including_reformatted_pem(self):
        fixture = secrets.ROOT / "scripts/ci"
        private = (fixture / "e2e-ci-jwt-private.pem").read_text().replace("\n", "\r\n")
        public = (fixture / "e2e-ci-jwt-public.pem").read_text()
        for bundle in [
            {**self.bundle, secrets.PRIVATE: private, secrets.PUBLIC: public},
            {**self.bundle, secrets.SCM: secrets.FIXTURE_SCM.upper()},
        ]:
            result = self.cli(**bundle, G5_E2E_AUTH="true")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("fixture", result.stderr)
            self.assertFalse(self.path.exists())
            self.assertEqual(result.stdout, "")

    def test_mismatched_pair_and_signer_alias_do_not_publish(self):
        other = secrets.generate()
        for overrides in [
            {secrets.PUBLIC: other[secrets.PUBLIC]},
            {secrets.USER_SIGNER: other[secrets.PRIVATE]},
        ]:
            result = self.cli(**{**self.bundle, **overrides})
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(self.path.exists())

    def test_existing_data_or_failed_probe_refuses_first_generation(self):
        for values in [
            {"FAKE_DOCKER_VOLUMES": "g5-lean_pgdata_dev"},
            {"FAKE_DOCKER_VOLUMES": "g5-lean_api_repos"},
            {"FAKE_DOCKER_LABELS": json.dumps({"com.docker.compose.project": "g5-local-0123456789", "com.docker.compose.project.config_files": "/another/checkout/infra/docker/docker-compose.test.yml"})},
            {"FAKE_DOCKER_FAIL": "1"},
        ]:
            with self.subTest(values=values):
                result = self.cli(**values)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")
                self.assertFalse(self.path.exists())

    def test_failed_publication_leaves_no_partial_bundle_and_retry_succeeds(self):
        with patch.object(secrets.os, "replace", side_effect=OSError("injected failure")):
            with self.assertRaises(OSError):
                secrets.retained_bundle(self.path, local=False)
        self.assertFalse(self.path.exists())
        self.assertEqual(list(self.root.glob(".g5-secrets-*")), [])
        secrets.validate(secrets.retained_bundle(self.path, local=False))

    def test_unsafe_file_permissions_and_symlinks_fail_without_replacement(self):
        contents = json.dumps(self.bundle)
        self.install(contents)
        self.path.chmod(0o644)
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertEqual(self.path.read_text(), contents)
        self.path.unlink()
        target = self.root / "other.json"
        target.write_text(contents)
        target.chmod(0o600)
        self.path.symlink_to(target)
        self.assertNotEqual(self.cli().returncode, 0)
        self.assertEqual(target.read_text(), contents)

    def test_shell_loader_does_not_trace_secret_values(self):
        self.install(json.dumps(self.bundle))
        result = subprocess.run(
            ["bash", "-eux", "-c", 'source "$1"; g5_load_local_stack_secrets; '
             'set +x; test "$G5_JWT_SECRET" = "$G5_JWT_PRIVATE_KEY"', "bash",
             str(secrets.ROOT / "scripts/lib/local-stack-secrets.sh")],
            env=self.env, text=True, capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        for value in self.bundle.values():
            self.assertNotIn(value, result.stdout + result.stderr)

    def test_dotenv_credentials_are_resolved_before_validation_identity(self):
        (self.root / ".env").write_text(secrets.shell_exports(self.bundle))
        resolved = secrets.configured_environment(self.root, self.env)
        self.assertEqual({key: resolved[key] for key in secrets.KEYS}, self.bundle)
        self.assertEqual(resolved[secrets.USER_SIGNER], self.bundle[secrets.PRIVATE])
        self.assertFalse(self.path.exists(), "explicit credentials must not create managed state")

    def test_dotenv_docker_context_controls_existing_data_probe(self):
        (self.root / ".env").write_text("export FAKE_DOCKER_VOLUMES=g5-lean_pgdata_dev\n")
        with self.assertRaisesRegex(ValueError, "Existing G5 dev data"):
            secrets.configured_environment(self.root, self.env)
        self.assertFalse(self.path.exists())

    def test_post_snapshot_credential_change_fails_before_export(self):
        digest = secrets.bundle_digest(self.bundle)
        result = self.cli(**self.bundle, G5_CI_STACK_SECRETS_DIGEST=digest)
        self.assertEqual(result.returncode, 0, result.stderr)
        changed = {**self.bundle, secrets.SCM: "ab" * 32}
        result = self.cli(**changed, G5_CI_STACK_SECRETS_DIGEST=digest)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertIn("changed after validation", result.stderr)
        self.assertFalse(self.path.exists())


if __name__ == "__main__":
    unittest.main()
