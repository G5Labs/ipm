#!/usr/bin/env python3
"""QA reset regressions. All cloud, Kubernetes, SQL and API calls are fake."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("reset_qa", Path(__file__).with_name("reset-qa.py"))
qa = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(qa)
SHA = "a" * 40
ORG = "10000000-0000-4000-8000-000000000001"
OPERATOR_ORG = "10000000-0000-4000-8000-000000000002"
HOST = "20000000-0000-4000-8000-000000000001"
IMAGE = f"projects/{qa.PROJECT}/global/images/g5-worker-cutover"


def options(directory):
    return argparse.Namespace(commit=SHA, admin_email="admin@example.test", external_issues="on",
                              state_dir=directory, admin_token_file="unused-fake-token", execute=True)


def merged_pr():
    return {"state": "MERGED", "mergeCommit": {"oid": SHA}, "headRefOid": SHA,
            "statusCheckRollup": [{"name": lane + " (g5-qa-1ccc)", "status": "COMPLETED", "conclusion": "SUCCESS"}
                                 for lane in ("g5-ci-rust", "g5-ci-e2e")]}


class FakeClients:
    def __init__(self):
        self.calls, self.sql_calls, self.builds = [], [], {}
        self.main = SHA
        self.prs = {str(n): merged_pr() for n in qa.REQUIRED_PRS}
        self.paused = True
        self.extra_builds = []
        self.replicas = {name: 1 for name in qa.WRITERS}
        self.worker = {"name": "g5-worker-old", "zone": "zones/us-central1-a",
                       "labels": {"g5_host_id": "c" * 32}, "tags": {"items": ["g5-worker"]}}
        self.workers = [self.worker]
        self.lock = None
        self.helper = None
        self.fail_create_database = False
        self.fail_after_submit = False
        self.worker_build_status = "SUCCESS"
        self.seed_count = 0
        self.org_count = 1
        self.org_owner = "admin@example.test"

    def __call__(self, args, **kwargs):
        self.calls.append(args)
        if args[0] == "gh":
            if args[1] == "api":
                return self.main if "/git/ref/" in args[2] else json.dumps({"id": 1234, "clone_url": qa.CODE_REMOTE, "default_branch": "main"})
            return json.dumps(self.prs[args[3]])
        if args[0] == "git":
            if "pack-objects" in args:
                Path(kwargs["output_file"]).write_bytes(b"fake pack")
                return ""
            if "--is-shallow-repository" in args:
                return "false"
            if args[1] == "show":
                path = args[2].split(":", 1)[1]
                if path.endswith("config_service.rs"):
                    return "external_issues_enabled"
                return (qa.ROOT / path).read_text()
            return SHA if args[1] == "rev-parse" else ""
        if args[0] == "bash":
            self.seed_count += 1
            env = kwargs["env"]
            assert env["GCP_PROJECT"] == qa.PROJECT
            assert env["DB_NAME"] == qa.DATABASE
            assert env["K8S_CONTEXT"] == qa.CONTEXT
            assert "DB_PASSWORD" not in env
            assert env["DEFAULT_ADMIN_EMAIL"] == "admin@example.test"
            for retired in ("ROTATE_WORKER_TOKEN", "EMIT_SHELL_EXPORTS", "DEFAULT_ORG_SLUG", "DEFAULT_ORG_NAME"):
                assert retired not in env
            return ""
        if args[0] == "gcloud":
            assert f"--project={qa.PROJECT}" in args
            command = args[1:4]
            if command == ["builds", "triggers", "describe"]:
                return json.dumps({"disabled": self.paused})
            if command[:2] == ["builds", "list"]:
                if any("_QA_RESET_RUN_ID" in a for a in args):
                    return json.dumps(list(self.builds.values()))
                return json.dumps(self.extra_builds)
            if command == ["builds", "triggers", "run"]:
                name = args[4]
                substitutions = next(a.split("=", 1)[1] for a in args if a.startswith("--substitutions="))
                subs = dict(pair.split("=", 1) for pair in substitutions.split(","))
                assert f"--sha={SHA}" in args
                status = self.worker_build_status if name == qa.WORKER_TRIGGER else "SUCCESS"
                build = {"id": name, "status": status, "substitutions": {**subs, "COMMIT_SHA": SHA}}
                self.builds[name] = build
                if self.fail_after_submit:
                    self.fail_after_submit = False
                    raise RuntimeError("connection lost after submission")
                return json.dumps({"metadata": {"build": build}})
            if command[:2] == ["builds", "describe"]:
                return json.dumps(self.builds[args[3]])
            if command == ["sql", "instances", "describe"]:
                return json.dumps({"region": qa.REGION, "state": "RUNNABLE", "ipAddresses": [{"type": "PRIVATE", "ipAddress": "10.1.2.3"}]})
            if command == ["container", "clusters", "get-credentials"]:
                assert kwargs["env"]["KUBECONFIG"].endswith("/kubeconfig")
                return ""
            if command == ["compute", "instances", "list"]:
                return json.dumps(self.workers)
            if command == ["compute", "instances", "delete"]:
                self.workers = []
                return "{}"
            if command == ["compute", "instance-groups", "managed"]:
                return "[]"
            if command == ["compute", "images", "list"]:
                return json.dumps([{"name": "g5-worker-cutover", "labels": {"g5_git_short_sha": SHA[:7]}}])
        if args[0] == "kubectl":
            assert args[1:3] == [f"--context={qa.CONTEXT}", f"--namespace={qa.NAMESPACE}"]
            a = args[3:]
            if a[:2] == ["get", "deployments,statefulsets,daemonsets,cronjobs,jobs,hpa,pods"]:
                return json.dumps({"items": [{"kind": "Deployment", "metadata": {"name": n}} for n in (*qa.WRITERS, "g5-edge")]})
            if a[:3] == ["get", "configmap", "qa-database-reset"]:
                return json.dumps(self.lock) if self.lock else ""
            if a[:3] == ["create", "configmap", "qa-database-reset"]:
                run_id = next(x.split("=", 2)[2] for x in a if x.startswith("--from-literal=run_id="))
                self.lock = {"data": {"run_id": run_id}}
                return ""
            if a[:2] == ["get", "pods"]:
                return json.dumps({"items": []})
            if a[:2] == ["create", "-f"]:
                self.helper = json.loads(kwargs["input"])
                return ""
            if a[0] == "scale":
                self.replicas[a[1].split("/")[1]] = 0
                return ""
            if a[0] in ("delete", "wait", "rollout"):
                return ""
            if a[:2] == ["get", "deployment"]:
                return json.dumps({"spec": {"replicas": self.replicas[a[2]], "template": {"spec": {"containers": [{
                    "name": "g5-api", "env": [{"name": "G5_WORKER_IMAGE", "value": IMAGE}]}]}}}})
            if a[0] == "exec":
                if "deploy/g5-api" in a:
                    assert a[-1] == qa.CODE_REMOTE
                    assert Path(kwargs["input_file"]).read_bytes() == b"fake pack"
                    return SHA
                sql = kwargs["input"]
                self.sql_calls.append(sql)
                if "pg_get_userbyid" in sql:
                    return json.dumps({"owner": "g5", "encoding": "UTF8", "collate": "en_US.UTF8", "ctype": "en_US.UTF8"})
                if "SELECT count(*) FROM pg_stat_activity" in sql:
                    return "0"
                if "CREATE DATABASE" in sql and self.fail_create_database:
                    raise RuntimeError("create failed")
                if "FROM _sqlx_migrations" in sql:
                    checksums = (qa.ROOT / "backend/g5-api/src/db/migrations/checksums.sha384").read_text().splitlines()
                    return json.dumps([{"version": int(line.split()[1].split("_")[0]), "checksum": line.split()[0], "success": True} for line in checksums if line.strip()])
                image = f"{qa.REGION}-docker.pkg.dev/{qa.PROJECT}/g5/g5d-g5dev@sha256:" + "b" * 64
                if "FROM orgs WHERE kind='operator'" in sql:
                    return json.dumps({"id": OPERATOR_ORG, "image": image})
                if "SELECT count(*) FROM orgs;" in sql:
                    return str(self.org_count)
                if "FROM orgs o WHERE o.id=" in sql:
                    return json.dumps({"kind": "tenant", "image": image, "admin": self.org_owner})
                return ""
        raise AssertionError(f"Unexpected external command: {args}")


class ResetTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.args = options(self.temp.name)
        self.clients = FakeClients()
        self.reset = qa.Reset(self.args, runner=self.clients, sleep=lambda _: None)
        self.api_calls = []
        self.platform_orgs = []
        self.lose_create_response = False

        def api(method, path, data=None):
            self.api_calls.append((method, path, data))
            if path == "/platform/orgs":
                if method == "GET":
                    return list(self.platform_orgs)
                self.platform_orgs.append({"id": ORG, "slug": data["slug"], "name": data["name"]})
                if self.lose_create_response:
                    self.lose_create_response = False
                    raise RuntimeError("QA API returned HTTP 502; verify the fresh admin session")
                return {"org": {"id": ORG, "slug": data["slug"]},
                        "admin": {"userId": HOST, "email": data["adminEmail"].upper(), "accountCreated": False}}
            if path.endswith("/repos"):
                return [{"id": self.reset.state["code_repo_id"], "gitUrl": qa.CODE_REMOTE, "defaultBranch": "main"}]
            if path.endswith("/workspaces"):
                return [{"id": "base", "isBase": True}]
            if path.endswith("/branches"):
                return [{"repoId": self.reset.state["code_repo_id"], "trackingMode": "remote_tracking",
                         "remoteBranch": "main", "materializationStatus": "materialized", "headCommit": SHA}]
            if path.endswith("/config"):
                return {"settings": {"external_issues_enabled": True}}
            if method == "POST":
                return {"hostId": HOST, "hostJwt": "never-persist-this"}
            return {"hosts": [{"hostId": HOST, "status": "ready", "lastHeartbeat": "now"}]}
        self.reset.api = api

    def test_full_cutover_and_resume_do_not_wipe_or_seed_twice(self):
        self.reset.execute()
        self.reset.execute()
        self.assertEqual(sum("DROP DATABASE" in q for q in self.clients.sql_calls), 1)
        self.assertEqual(self.clients.seed_count, 1)
        self.assertEqual(len(self.clients.builds), 2)
        posts = [(path, data) for method, path, data in self.api_calls if method == "POST"]
        self.assertEqual(len(posts), 2)
        self.assertEqual(posts[0], ("/platform/orgs", {"name": "g5labs", "slug": "g5", "adminEmail": "admin@example.test"}))
        self.assertEqual(self.reset.state["org_id"], ORG)
        self.assertEqual(self.reset.state["operator_org_id"], OPERATOR_ORG)
        self.assertNotIn("never-persist-this", self.reset.path.read_text())
        worker = self.clients.builds[qa.WORKER_TRIGGER]["substitutions"]
        self.assertEqual(worker["_REPIN_DEV_API_WORKER_IMAGE"], "false")
        self.assertEqual(self.clients.builds[qa.DEPLOY_TRIGGER]["substitutions"]["_QA_DATABASE_RESET"], "true")
        deploy = self.clients.builds[qa.DEPLOY_TRIGGER]["substitutions"]
        for retired in ("_PR1884_CUTOVER_REQUIRED", "_RUN_TUNGSTEN_CANARY", "_RUN_PREVIEW_CANARY"):
            self.assertNotIn(retired, deploy)
        self.assertIn("finish_bootstrap", self.reset.state["done"])
        self.assertIn(("PUT", f"/orgs/{ORG}/config", {"settings": {"external_issues_enabled": True}}), self.api_calls)
        code_sql = next(q for q in self.clients.sql_calls if "INSERT INTO remote_integrations" in q)
        self.assertIn(qa.CODE_REMOTE, code_sql)
        # The cascade's untouched local code repository is replaced, guarded.
        # It is found by Base's code binding and slug, never by its URL.
        self.assertIn("m.workspace_id=base AND m.role='code'", code_sql)
        self.assertIn("r.slug='code' AND r.provider IS NULL", code_sql)
        self.assertNotIn("git_url='g5-local:", code_sql)
        # ...and only while it is still G5-hosted, in any URL shape.
        self.assertIn("r.git_url LIKE 'g5-local://%'", code_sql)
        self.assertLess(code_sql.index("DELETE FROM repos"), code_sql.index("INSERT INTO repos"))
        self.assertIn("Base already has another code repository", code_sql)
        self.assertFalse(any(c[:2] == ["gcloud", "secrets"] for c in self.clients.calls))
        self.assertIn("'code','code'", code_sql)
        self.assertIn("'main',true", code_sql)
        calls = self.clients.calls
        self.assertFalse(any("import" in c or "push" in c or "commit" in c for c in calls))

    def test_both_prs_must_be_merged_and_green_before_mutation(self):
        for number in ("186", "213"):
            for bad in ("open", "failed", "missing"):
                with self.subTest(pr=number, bad=bad):
                    pr = merged_pr()
                    if bad == "open":
                        pr["state"] = "OPEN"
                    elif bad == "failed":
                        pr["statusCheckRollup"][0]["conclusion"] = "FAILURE"
                    else:
                        pr["statusCheckRollup"] = []
                    self.clients.prs[number] = pr
                    self.reset.load()
                    with self.assertRaises(RuntimeError):
                        self.reset.preflight()
                    self.assertFalse(self.clients.sql_calls)
                    self.assertFalse(any(c[0] == "kubectl" for c in self.clients.calls))
                    self.clients.prs[number] = merged_pr()

    def test_enabled_trigger_or_changed_main_stops_before_shutdown(self):
        self.reset.load()
        self.clients.paused = False
        with self.assertRaisesRegex(RuntimeError, "Pause"):
            self.reset.preflight()
        self.clients.paused = True
        self.clients.main = "b" * 40
        with self.assertRaisesRegex(RuntimeError, "main moved"):
            self.reset.preflight()
        self.assertFalse(any("scale" in c for c in self.clients.calls))

    def test_active_foreign_deployment_blocks_reset(self):
        self.reset.load()
        self.clients.extra_builds = [{"id": "other", "substitutions": {"TRIGGER_NAME": qa.DEPLOY_TRIGGER}}]
        with self.assertRaisesRegex(RuntimeError, "Another delivery"):
            self.reset.preflight()

    def test_ambiguous_database_failure_never_repeats_drop(self):
        self.clients.fail_create_database = True
        with self.assertRaisesRegex(RuntimeError, "create failed"):
            self.reset.execute()
        with self.assertRaisesRegex(RuntimeError, "refusing to wipe twice"):
            self.reset.execute()
        self.assertEqual(sum("DROP DATABASE" in q for q in self.clients.sql_calls), 1)
        self.assertEqual(self.clients.replicas, {n: 0 for n in qa.WRITERS})

    def test_worker_build_failure_never_starts_deploy(self):
        self.clients.worker_build_status = "FAILURE"
        with self.assertRaisesRegex(RuntimeError, "ended FAILURE"):
            self.reset.execute()
        self.assertNotIn(qa.DEPLOY_TRIGGER, self.clients.builds)

    def test_lost_build_response_recovers_existing_build(self):
        self.clients.fail_after_submit = True
        with self.assertRaisesRegex(RuntimeError, "connection lost"):
            self.reset.execute()
        self.reset.execute()
        starts = [c for c in self.clients.calls if c[:4] == ["gcloud", "builds", "triggers", "run"]]
        self.assertEqual(len(starts), 2)

    def test_bootstrap_waits_for_fresh_admin_session(self):
        self.args.admin_token_file = None
        self.reset.execute()
        self.assertIn("seed", self.reset.state["done"])
        for phase in ("create_org", "seed_code_repository", "finish_bootstrap"):
            self.assertNotIn(phase, self.reset.state["done"])
        self.assertFalse(self.api_calls)
        self.assertFalse(any("INSERT INTO remote_integrations" in q for q in self.clients.sql_calls))
        self.assertFalse(any("deploy/g5-api" in c for c in self.clients.calls))

    def test_lost_org_creation_response_adopts_the_created_org(self):
        self.lose_create_response = True
        with self.assertRaisesRegex(RuntimeError, "HTTP 502"):
            self.reset.execute()
        self.assertNotIn("create_org", self.reset.state["done"])
        self.reset.execute()
        posts = [path for method, path, _ in self.api_calls if method == "POST" and path == "/platform/orgs"]
        self.assertEqual(len(posts), 1)
        self.assertEqual(self.reset.state["org_id"], ORG)
        self.assertIn("finish_bootstrap", self.reset.state["done"])

    def test_seed_that_leaves_a_tenant_org_stops_before_org_creation(self):
        self.clients.org_count = 2
        with self.assertRaisesRegex(RuntimeError, "only the operator org"):
            self.reset.execute()
        self.assertFalse(self.api_calls)

    def test_org_without_the_named_active_admin_is_refused(self):
        self.clients.org_owner = "intruder@example.test"
        with self.assertRaisesRegex(RuntimeError, "administered by the named admin"):
            self.reset.execute()
        self.assertNotIn("org_id", self.reset.state)
        self.assertFalse(any("INSERT INTO remote_integrations" in q for q in self.clients.sql_calls))

    def test_other_cluster_lock_blocks_before_shutdown(self):
        self.clients.lock = {"data": {"run_id": "other-cutover"}}
        with self.assertRaisesRegex(RuntimeError, "Another cutover"):
            self.reset.execute()
        self.assertFalse(self.clients.sql_calls)

    def test_worker_inventory_rejects_unknown_and_gke_instances(self):
        self.assertEqual(qa.select_workers([{"name": "gke-node", "zone": "zones/us-central1-a"}]), [])
        bad = dict(self.clients.worker, labels={"g5_host_id": "c" * 32, "goog-k8s-cluster-name": "g5"})
        with self.assertRaises(RuntimeError):
            qa.select_workers([bad])
        with self.assertRaises(RuntimeError):
            qa.select_workers([dict(self.clients.worker, labels={})])

    def test_unexpected_controllers_block_reset(self):
        for kind in ("HorizontalPodAutoscaler", "CronJob", "StatefulSet", "DaemonSet"):
            with self.subTest(kind=kind), self.assertRaises(RuntimeError):
                qa.check_workloads([{"kind": kind, "metadata": {"name": "writer"}}], "helper")

    def test_prod_environment_is_rejected(self):
        with patch.dict(os.environ, {"GCP_PROJECT": "g5-prod-1ccc"}), self.assertRaisesRegex(RuntimeError, "conflicting"):
            qa.validate_options(self.args)

    def test_preview_does_not_invoke_any_external_command(self):
        with patch.object(qa, "run", side_effect=AssertionError("external command")), patch("sys.argv", [
            "reset-qa.py", "--commit", SHA, "--admin-email", "admin@example.test"]), patch.dict(os.environ, {}, clear=True):
            self.assertEqual(qa.main(), 0)

    def test_sql_connection_guard_runs_before_psql(self):
        self.reset.helper = "helper"
        self.reset.db_host = "10.1.2.3"
        self.reset.sql("SELECT 1;")
        call = self.clients.calls[-1]
        shell = call[call.index("-c") + 1]
        client = Path(self.temp.name) / "psql"
        client.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n')
        client.chmod(0o755)
        env = {"PATH": self.temp.name + ":/usr/bin:/bin", "G5_ADMIN_DATABASE_URL": "postgres://g5@10.1.2.3:5432/g5_lean_qa?password=FAKE&sslmode=require"}
        command = ["/bin/sh", "-c", shell, "test", "10.1.2.3", "postgres"]
        result = subprocess.run(command, env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("/postgres?password=FAKE&sslmode=require", result.stdout)
        env["G5_ADMIN_DATABASE_URL"] = "postgres://g5@10.9.9.9:5432/g5?password=FAKE&sslmode=require"
        result = subprocess.run(command, env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")


class HelmResetTests(unittest.TestCase):
    def test_reset_flag_refuses_non_qa_targets_or_missing_run_identity(self):
        source = (qa.ROOT / "infra/ci/cloudbuild-deploy.yaml").read_text()
        start = 'case "${_QA_DATABASE_RESET}" in'
        guard = start + source.split(start, 1)[1].split("        esac", 1)[0] + "esac"
        valid = {"PROJECT_ID": qa.PROJECT, "_GKE_CLUSTER": qa.CLUSTER, "_ENVIRONMENT": "qa",
                 "_HELM_NAMESPACE": qa.NAMESPACE, "_QA_RESET_RUN_ID": "test-run", "_QA_DATABASE_RESET": "true"}
        for override in ({}, {"PROJECT_ID": "g5-prod-1ccc"}, {"_GKE_CLUSTER": "prod"}, {"_ENVIRONMENT": "prod"},
                         {"_HELM_NAMESPACE": "other"}, {"_QA_RESET_RUN_ID": ""}, {"_QA_DATABASE_RESET": "yes"}):
            with self.subTest(override=override):
                result = subprocess.run(["/bin/bash", "-c", guard], env={**valid, **override}, capture_output=True)
                self.assertEqual(result.returncode == 0, not override)

    def test_normal_and_reset_deployments_wait_without_automatic_rollback(self):
        source = (qa.ROOT / "infra/ci/cloudbuild-deploy.yaml").read_text()
        setup = source.split("        HELM_ARGS=()", 1)[1].split("        # Filestore IP", 1)[0]
        function = source.split("        deploy_control_plane() {", 1)[1].split("        # The in-cluster worker deploy", 1)[0]
        script = 'HELM_ARGS=()\n' + setup + '\ndeploy_control_plane() {' + function + '\ndeploy_control_plane\n'
        for mode in ("true", "false"):
            with self.subTest(reset=mode):
                replacements = {"_QA_DATABASE_RESET": mode, "_GKE_CLUSTER": qa.CLUSTER, "_GKE_ZONE": qa.REGION,
                                "_HELM_RELEASE": "g5", "_HELM_NAMESPACE": qa.NAMESPACE, "_ENVIRONMENT": "qa",
                                "_FILESTORE_PV_NAME": "g5-repos-pv", "_FILESTORE_PVC_NAME": "g5-repos-pvc"}
                rendered = script
                for key, value in replacements.items():
                    rendered = rendered.replace("${" + key + "}", value)
                rendered = rendered.replace("$${", "${")
                # This test isolates Helm rollback policy; the real storage
                # guard + refused Helm invocation is covered by deployment tests.
                prelude = ('set -eu\ngcloud() { :; }\nhelm() { printf "%s\\n" "$@"; }\n'
                           'python3() { test "$1" = scripts/check_filestore_binding.py; }\n')
                env = {"PATH": "/usr/bin:/bin", "WORKER_IMAGE_REF": IMAGE,
                       "FILESTORE_IP": "10.0.0.1",
                       "TARGET_PROJECT": qa.PROJECT, "AR": "qa-registry", "IMAGE_VERSION": SHA,
                       "API_IMAGE_REF": "api@sha256:fake", "G5D_IMAGE_REF": "worker@sha256:fake", "EDGE_IMAGE_REF": "edge@sha256:fake"}
                result = subprocess.run(["/bin/bash", "-c", prelude + rendered], env=env, text=True, capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("--wait", result.stdout.splitlines())
                self.assertNotIn("--atomic", result.stdout.splitlines())


class CodeMirrorTests(unittest.TestCase):
    def test_real_git_seed_preserves_history_and_refuses_overwriting_a_moved_base(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, pack = root / "source", root / "code.pack"
            qa.run(["git", "init", "-q", "-b", "main", str(source)])
            git = ["git", "-C", str(source), "-c", "user.name=QA Fixture", "-c", "user.email=qa@example.test",
                   "-c", "commit.gpgSign=false", "-c", "core.hooksPath=/dev/null"]
            for content in ("first\n", "second\n"):
                (source / "README.md").write_text(content)
                qa.run([*git, "add", "README.md"])
                qa.run([*git, "commit", "-qm", content.strip()])
            head = qa.run([*git, "rev-parse", "HEAD"])
            parent = qa.run([*git, "rev-parse", "HEAD^"])
            qa.run([*git, "pack-objects", "--stdout", "--revs"], input=head + "\n", output_file=pack)
            org_id = "20000000-0000-4000-8000-000000000001"
            repo_id = "30000000-0000-4000-8000-000000000001"
            env = {**os.environ, "G5_STORAGE_MOUNT_POINT": str(root / "storage"), "G5_STORAGE_TENANT_PREFIX": "repos"}
            command = ["/bin/sh", "-c", qa.INSTALL_CODE_MIRROR, "test", org_id, repo_id, head, qa.CODE_REMOTE]
            self.assertEqual(qa.run(command, env=env, input_file=pack), head)
            self.assertEqual(qa.run(command, env=env, input_file=pack), head)
            bare = ["git", "--git-dir=" + str(root / "storage" / "repos" / org_id / repo_id)]
            self.assertEqual(qa.run([*bare, "rev-parse", "main^"]), parent)
            self.assertEqual(qa.run([*bare, "rev-parse", "refs/g5-tracking/main/baseline"]), head)
            self.assertEqual(qa.run([*bare, "rev-parse", "refs/remotes/origin/main"]), head)
            self.assertEqual(qa.run([*bare, "config", "--get", "remote.origin.url"]), qa.CODE_REMOTE)
            qa.run([*bare, "update-ref", "refs/heads/main", parent])
            with self.assertRaises(RuntimeError):
                qa.run(command, env=env, input_file=pack)
            self.assertEqual(qa.run([*bare, "rev-parse", "main"]), parent)


if __name__ == "__main__":
    unittest.main()
