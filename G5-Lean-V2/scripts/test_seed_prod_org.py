#!/usr/bin/env python3
"""Exercise seed target selection with every external client stubbed."""

from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/seed-prod-org.sh"
QA_CONTEXT = "gke_g5-qa-1ccc_us-central1_g5-lean-qa"
TARGET = {
    "GCP_PROJECT": "g5-qa-1ccc",
    "DB_INSTANCE_NAME": "g5-lean-qa-postgres",
    "DB_NAME": "g5_lean_qa",
    "DB_HOST": "10.1.2.3",
    "DEFAULT_ADMIN_EMAIL": "admin@example.test",
}
CODE_REPO_ID = "20000000-0000-4000-8000-000000000002"
DOCS_REPO_ID = "10000000-0000-4000-8000-000000000001"
# The fake psql answers every scalar with this id, the operator org included.
ORG_ID = "10000000-0000-4000-8000-000000000001"
CLIENT = r'''
import json
import os
from pathlib import Path
import subprocess
import sys

name = Path(sys.argv[0]).name
args = sys.argv[1:]
if name == "brew":
    sys.exit(1)
with open(os.environ["SEED_TEST_CALLS"], "a") as log:
    log.write(json.dumps([name, *args]) + "\n")


def record_sql(text):
    with open(os.environ["SEED_TEST_SQL"], "a") as log:
        log.write(text + "\n")


def answer_sql(text):
    if os.environ.get("SEED_TEST_FAIL_SQL"):
        sys.exit(7)
    # The Base code-member step answers `id|default_branch`, or nothing when
    # Base already has another code repository.
    if "|| '|' ||" in text:
        row = os.environ.get("SEED_TEST_CODE_ROW", "20000000-0000-4000-8000-000000000002|main")
        if row:
            print(row)
        return
    print("10000000-0000-4000-8000-000000000001")


if name == "kubectl":
    if args and args[0].startswith("--context="):
        args = args[1:]
    if args[:2] == ["get", "pod"]:
        sys.exit(0 if os.environ.get("SEED_TEST_EXISTING_POD") else 1)
    if args and args[0] == "exec":
        stdin = sys.stdin.read()
        if "psql" in args:
            record_sql(stdin)
            answer_sql(stdin)
        elif os.environ.get("SEED_TEST_REPO_ROOT"):
            # Run the API-pod shell against a local repository storage root.
            env = {
                key: value for key, value in os.environ.items()
                if not key.startswith(("GIT_AUTHOR_", "GIT_COMMITTER_"))
            }
            env.update(
                G5_STORAGE_MOUNT_POINT=os.environ["SEED_TEST_REPO_ROOT"],
                G5_STORAGE_TENANT_PREFIX="repos",
                PATH=os.environ["SEED_TEST_GIT_PATH"],
            )
            shell = args[args.index("sh"):]
            result = subprocess.run(shell, input=stdin, env=env, text=True, capture_output=True)
            sys.stdout.write(result.stdout)
            sys.stderr.write(result.stderr)
            sys.exit(result.returncode)
        else:
            print("a" * 40)
elif name == "psql":
    text = args[args.index("-tAc") + 1] if "-tAc" in args else sys.stdin.read()
    record_sql(text)
    answer_sql(text)
elif name == "gcloud":
    if args[:3] == ["secrets", "versions", "access"]:
        print("fake-db-password")
'''


class SeedTargetTests(unittest.TestCase):
    def run_seed(self, target, *args):
        return self.run_seed_with_sql(target, *args)[:2]

    def run_seed_with_sql(self, target, *args):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("brew", "gcloud", "kubectl", "psql", "cloud-sql-proxy", "sleep"):
                client = root / name
                client.write_text(f"#!{sys.executable}\n{CLIENT}")
                client.chmod(0o755)
            log = root / "calls.jsonl"
            sql_log = root / "statements.sql"
            # Do not inherit cloud credentials or the operator's configuration.
            env = {
                "PATH": f"{root}:/usr/bin:/bin",
                "SEED_TEST_CALLS": str(log),
                "SEED_TEST_SQL": str(sql_log),
                "FORCE_DEFAULT_BASE_IMAGE": "1",
                "SEED_TEST_GIT_PATH": f"{Path(shutil.which('git')).parent}:/usr/bin:/bin",
                **target,
            }
            completed = subprocess.run(
                ["/bin/bash", str(SCRIPT), *args], env=env,
                capture_output=True, text=True, timeout=15,
            )
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            sql = sql_log.read_text() if sql_log.exists() else ""
            return completed, calls, sql

    def assert_rejected_before_clients(self, target, *args):
        result, calls = self.run_seed(target, *args)
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [], result.stderr)

    def test_bare_invocation_has_no_default_target(self):
        self.assert_rejected_before_clients({})

    def test_partial_target_never_falls_back(self):
        for key in TARGET:
            with self.subTest(missing=key):
                target = dict(TARGET)
                del target[key]
                self.assert_rejected_before_clients(target)
            if key != "DEFAULT_ADMIN_EMAIL":
                with self.subTest(empty=key):
                    self.assert_rejected_before_clients({**TARGET, key: ""})

    def test_cluster_requires_context_in_selected_project(self):
        for context in ("", "gke_g5-prod-1ccc_us-central1_g5-prod"):
            with self.subTest(context=context):
                self.assert_rejected_before_clients({
                    **TARGET, "USE_K8S_PSQL": "1", "K8S_CONTEXT": context,
                })

    def test_invalid_connection_modes_fail_before_clients(self):
        self.assert_rejected_before_clients(TARGET, "--unknown")
        self.assert_rejected_before_clients(TARGET, "--proxy", "extra")
        self.assert_rejected_before_clients(TARGET, "--proxy")
        self.assert_rejected_before_clients({**TARGET, "USE_K8S_PSQL": "true"})
        for host in ("127.0.0.1", "localhost", "::1"):
            with self.subTest(host=host):
                self.assert_rejected_before_clients({
                    **TARGET, "DB_HOST": host, "USE_K8S_PSQL": "1", "K8S_CONTEXT": QA_CONTEXT,
                })

    def test_cluster_seed_pins_every_client_to_explicit_target(self):
        result, calls = self.run_seed({**TARGET, "USE_K8S_PSQL": "1", "K8S_CONTEXT": QA_CONTEXT})
        self.assertEqual(result.returncode, 0, result.stderr)
        kube_calls = [call for call in calls if call[0] == "kubectl"]
        self.assertTrue(kube_calls)
        self.assertTrue(all(call[1] == f"--context={QA_CONTEXT}" for call in kube_calls))
        self.assertTrue(any("deploy/g5-api" in call for call in kube_calls))
        self.assertTrue(any(call[2:4] == ["get", "deploy"] for call in kube_calls))
        self.assertEqual(sum(call[2:4] == ["delete", "pod"] for call in kube_calls), 1)
        sql_calls = [call for call in kube_calls if "psql" in call]
        self.assertTrue(sql_calls)
        for call in sql_calls:
            self.assertEqual(call[call.index("-h") + 1], TARGET["DB_HOST"])
            self.assertEqual(call[call.index("-d") + 1], TARGET["DB_NAME"])
        cloud_calls = [call for call in calls if call[0] == "gcloud"]
        self.assertTrue(cloud_calls)
        self.assertTrue(all("--project=g5-qa-1ccc" in call for call in cloud_calls))

    def test_failed_sql_cleans_up_owned_pod_in_selected_context(self):
        result, calls = self.run_seed({
            **TARGET, "USE_K8S_PSQL": "1", "K8S_CONTEXT": QA_CONTEXT, "SEED_TEST_FAIL_SQL": "1",
        })
        self.assertEqual(result.returncode, 7, result.stderr)
        deletes = [call for call in calls if "delete" in call]
        self.assertEqual(len(deletes), 1)
        self.assertEqual(deletes[0][1], f"--context={QA_CONTEXT}")

    def test_existing_client_pod_is_not_deleted(self):
        result, calls = self.run_seed({
            **TARGET, "USE_K8S_PSQL": "1", "K8S_CONTEXT": QA_CONTEXT, "SEED_TEST_EXISTING_POD": "1",
        })
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(any("delete" in call for call in calls))

    def test_direct_connection_does_not_touch_kubernetes(self):
        result, calls = self.run_seed({**TARGET, "DEFAULT_ADMIN_EMAIL": ""})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any(call[0] == "psql" for call in calls))
        self.assertFalse(any(call[0] == "kubectl" for call in calls))

    def test_seed_provisions_only_the_operator_org(self):
        # Retired overrides must not select a tenant org.
        result, calls, sql = self.run_seed_with_sql({
            **TARGET, "DEFAULT_ORG_SLUG": "g5", "DEFAULT_ORG_NAME": "g5labs",
        })
        self.assertEqual(result.returncode, 0, result.stderr)
        normalized = " ".join(sql.split())
        self.assertIn("WHERE kind = 'operator'", normalized)
        self.assertIn("INSERT INTO orgs (name, slug, kind) SELECT 'Platform operator', 'operator', 'operator'", normalized)
        self.assertNotIn("'g5'", normalized)
        self.assertNotIn("g5labs'", normalized)
        # The service user holds only the Base-owner member role and mints no
        # worker token.
        self.assertIn("INSERT INTO org_memberships (org_id, user_id, role) VALUES ('10000000-0000-4000-8000-000000000001', '10000000-0000-4000-8000-000000000001', 'member')", normalized)
        self.assertNotIn("'admin')", normalized.split("INSERT INTO org_invitations")[0])
        self.assertNotIn("worker_tokens", normalized)
        self.assertIn("INSERT INTO org_invitations", normalized)
        self.assertIn("'admin@example.test', 'admin'", normalized)
        self.assertIn("'g5-local://operator/docs'", normalized)
        # gcloud only reads the database password; no secret is written.
        cloud_calls = [call for call in calls if call[0] == "gcloud"]
        self.assertEqual(cloud_calls, [[
            "gcloud", "secrets", "versions", "access", "latest",
            "--secret=g5-db-password", "--project=g5-qa-1ccc",
        ]])
        self.assertNotIn("export ", result.stdout)

    def test_empty_admin_email_skips_the_invitation(self):
        result, _, sql = self.run_seed_with_sql({**TARGET, "DEFAULT_ADMIN_EMAIL": ""})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("org_invitations", sql)

    def test_proxy_uses_explicit_instance_and_local_psql(self):
        target = dict(TARGET)
        del target["DB_HOST"]
        result, calls = self.run_seed(target, "--proxy")
        self.assertEqual(result.returncode, 0, result.stderr)
        proxy_calls = [call for call in calls if call[0] == "cloud-sql-proxy"]
        self.assertEqual(len(proxy_calls), 1)
        self.assertIn("g5-qa-1ccc:us-central1:g5-lean-qa-postgres", proxy_calls[0])
        self.assertFalse(any(call[0] == "kubectl" for call in calls))
        for call in calls:
            if call[0] == "psql":
                self.assertEqual(call[call.index("-h") + 1], "127.0.0.1")


    def test_base_gets_the_tenant_cascade_code_repository_without_rewrites(self):
        result, _, sql = self.run_seed_with_sql(TARGET)
        self.assertEqual(result.returncode, 0, result.stderr)
        normalized = " ".join(sql.split())
        step = normalized[normalized.index("RAISE EXCEPTION 'The operator org code repository is provider-backed"):]
        # The tenant cascade's identity, inserted only while Base has no code
        # member and never updated.
        self.assertIn(
            "INSERT INTO repos (org_id, name, slug, git_url, default_branch) "
            f"SELECT '{DOCS_REPO_ID}', 'code', 'code', 'g5-local://operator/code', 'main' "
            "WHERE NOT EXISTS ( SELECT 1 FROM workspace_repos "
            f"WHERE workspace_id = '{DOCS_REPO_ID}' AND role = 'code' ) "
            "ON CONFLICT (org_id, slug) DO NOTHING;", step)
        self.assertIn(f"SELECT '{DOCS_REPO_ID}', '{DOCS_REPO_ID}', r.id, 'code', 'code'", step)
        self.assertIn("AND r.provider IS NULL", step)
        self.assertIn("ON CONFLICT (workspace_id, repo_id) DO NOTHING;", step)
        self.assertNotIn("DO UPDATE", step.split("COMMIT;")[0])
        self.assertIn(f"code_repo_id={CODE_REPO_ID}", result.stderr)

    def run_cluster_seed(self, repo_root, **extra):
        return self.run_seed_with_sql({
            **TARGET, "USE_K8S_PSQL": "1", "K8S_CONTEXT": QA_CONTEXT,
            "SEED_TEST_REPO_ROOT": str(repo_root), **extra,
        })[0]

    @staticmethod
    def git(repo, *args):
        return subprocess.run(
            ["git", f"--git-dir={repo}", *args], check=True, capture_output=True, text=True,
        ).stdout.strip()

    def test_mirrors_get_first_commits_and_reruns_keep_history(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            docs, code = root / "repos" / ORG_ID / DOCS_REPO_ID, root / "repos" / ORG_ID / CODE_REPO_ID
            result = self.run_cluster_seed(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(self.git(docs, "ls-tree", "-r", "--name-only", "main"), "")
            self.assertEqual(self.git(code, "ls-tree", "-r", "--name-only", "main"), "README.md")
            self.assertEqual(
                self.git(code, "show", "main:README.md"),
                "# code\n\nThis is a G5-managed local project repository.",
            )
            self.assertEqual(self.git(code, "symbolic-ref", "HEAD"), "refs/heads/main")
            heads = {repo: self.git(repo, "rev-parse", "main") for repo in (docs, code)}
            self.assertIn(f"code mirror ready at {heads[code]}", result.stderr)

            # A rerun changes no ref.
            result = self.run_cluster_seed(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual({repo: self.git(repo, "rev-parse", "main") for repo in heads}, heads)

            # History written after seeding is kept.
            work = root / "work"
            subprocess.run(["git", "clone", "-q", str(code), str(work)], check=True)
            (work / "app.txt").write_text("app\n")
            for args in (["add", "app.txt"],
                         ["-c", "user.name=t", "-c", "user.email=t@example.test", "commit", "-qm", "work"],
                         ["push", "-q", "origin", "main"]):
                subprocess.run(["git", "-C", str(work), *args], check=True)
            advanced = self.git(code, "rev-parse", "main")
            result = self.run_cluster_seed(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(self.git(code, "rev-parse", "main"), advanced)
            self.assertEqual(self.git(code, "rev-list", "--count", "main"), "2")

    def test_nonempty_mirror_without_the_branch_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            code = root / "repos" / ORG_ID / CODE_REPO_ID
            code.parent.mkdir(parents=True)
            subprocess.run(["git", "init", "-q", "--bare", str(code)], check=True)
            work = root / "work"
            subprocess.run(["git", "init", "-q", "-b", "other", str(work)], check=True)
            subprocess.run(["git", "-C", str(work), "-c", "user.name=t", "-c", "user.email=t@example.test",
                            "commit", "-q", "--allow-empty", "-m", "x"], check=True)
            subprocess.run(["git", "-C", str(work), "push", "-q", str(code), "other:other"], check=True)
            result = self.run_cluster_seed(root)
            self.assertNotEqual(result.returncode, 0, result.stderr)
            self.assertIn("code mirror initialisation failed", result.stderr)
            self.assertEqual(self.git(code, "for-each-ref", "--format=%(refname)"), "refs/heads/other")

    def test_another_base_code_member_is_kept(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            result = self.run_cluster_seed(root, SEED_TEST_CODE_ROW="")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Base already has another code repository member; kept unchanged", result.stderr)
            self.assertEqual(sorted(path.name for path in (root / "repos" / ORG_ID).iterdir()), [DOCS_REPO_ID])

    def test_mirror_initialisation_can_be_skipped_with_the_former_name(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("REPO_MIRROR_INIT", "DOCS_MIRROR_INIT"):
                with self.subTest(name=name):
                    result = self.run_cluster_seed(root, **{name: "skip"})
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn("repository mirror initialisation skipped", result.stderr)
                    self.assertFalse((root / "repos").exists())


if __name__ == "__main__":
    unittest.main()
