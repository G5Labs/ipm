#!/usr/bin/env python3
"""Reset disposable QA at one merged commit. Preview by default; see reset-qa.md."""
from __future__ import annotations

import argparse
from contextlib import ExitStack
import fcntl
import ipaddress
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
PROJECT = "g5-qa-1ccc"
REGION = "us-central1"
CLUSTER = "g5-lean-qa"
CONTEXT = f"gke_{PROJECT}_{REGION}_{CLUSTER}"
DATABASE = "g5_lean_qa"
INSTANCE = "g5-lean-qa-postgres"
NAMESPACE = "g5"
REPO = "G5Labs/G5-Lean"
API = "https://qa.g5labs.ai/api/v1"
CODE_REMOTE = "https://github.com/G5Labs/G5-Lean.git"
# G5's own development org, created through the operator API after the
# platform seed (g5.organization.platform-operator.bootstrap).
TENANT_ORG_NAME = "g5labs"
TENANT_ORG_SLUG = "g5"
REQUIRED_PRS = (186, 213)
WORKER_TRIGGER = "g5-packer-worker-main-qa"
DEPLOY_TRIGGER = "g5-deploy-qa"
TRIGGERS = (WORKER_TRIGGER, DEPLOY_TRIGGER, "g5-terraform-apply-qa")
WRITERS = ("g5-api", "g5-daemon")
ACTIVE_BUILDS = {"QUEUED", "PENDING", "WORKING"}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def run(args, *, env=None, input=None, input_file=None, output_file=None, timeout=120):
    with ExitStack() as stack:
        stdin = stack.enter_context(open(input_file, "rb")) if input_file else None
        stdout = stack.enter_context(open(output_file, "wb")) if output_file else subprocess.PIPE
        result = subprocess.run(args, cwd=ROOT, env=env, input=input, stdin=stdin, text=True,
                                stdout=stdout, stderr=subprocess.PIPE, timeout=timeout)
    # Deliberately omit output: SQL connections and API responses can contain
    # credentials. Non-secret build IDs and journal stages locate diagnostics.
    require(result.returncode == 0, f"Command failed ({result.returncode}): {args[0]} {' '.join(args[1:3])}")
    return (result.stdout or "").strip()


INSTALL_CODE_MIRROR = r'''set -eu
org_id="$1"; repo_id="$2"; commit="$3"; remote="$4"
root="${G5_STORAGE_MOUNT_POINT:-/mnt/g5-repos}/${G5_STORAGE_TENANT_PREFIX:-repos}"
repo="$root/$org_id/$repo_id"
[ -d "$repo" ] || { mkdir -p "$root/$org_id"; git init -q --bare "$repo"; }
old=$(git --git-dir="$repo" rev-parse -q --verify refs/heads/main 2>/dev/null || true)
[ -z "$old" ] || [ "$old" = "$commit" ] || { echo "Base code ref moved; refusing to overwrite" >&2; exit 1; }
url=$(git --git-dir="$repo" config --get remote.origin.url || true)
[ -z "$url" ] || [ "$url" = "$remote" ] || { echo "Code remote changed" >&2; exit 1; }
git --git-dir="$repo" index-pack --stdin >/dev/null
git --git-dir="$repo" cat-file -e "$commit^{commit}"
git --git-dir="$repo" fsck --connectivity-only --no-reflogs >/dev/null 2>&1
git --git-dir="$repo" config remote.origin.url "$remote"
git --git-dir="$repo" config remote.origin.fetch '+refs/heads/*:refs/remotes/origin/*'
git --git-dir="$repo" update-ref refs/heads/main "$commit" "${old:-0000000000000000000000000000000000000000}"
git --git-dir="$repo" update-ref refs/remotes/origin/main "$commit"
git --git-dir="$repo" update-ref refs/g5-tracking/main/baseline "$commit"
git --git-dir="$repo" symbolic-ref HEAD refs/heads/main
git --git-dir="$repo" rev-parse --verify refs/heads/main
'''


def sql_literal(value):
    return "'" + value.replace("'", "''") + "'"


def validate_options(args):
    require(re.fullmatch(r"[0-9a-f]{40}", args.commit), "--commit must be a full 40-character commit SHA")
    require(re.fullmatch(r"[A-Za-z0-9._+%-]+@[A-Za-z0-9.-]+", args.admin_email), "Supply a valid --admin-email")
    # A copied prod shell must not silently look like a QA invocation.
    for key, expected in {"GCP_PROJECT": PROJECT, "DB_INSTANCE_NAME": INSTANCE,
                          "DB_NAME": DATABASE, "K8S_CONTEXT": CONTEXT}.items():
        require(key not in os.environ or os.environ[key] == expected,
                f"Unset conflicting {key}; this script only targets {PROJECT}")


def check_pr(pr, number):
    require(pr.get("state") == "MERGED", f"PR #{number} must be merged before cutover")
    merge = (pr.get("mergeCommit") or {}).get("oid", "")
    require(re.fullmatch(r"[0-9a-f]{40}", merge), f"Missing merge commit for PR #{number}")
    checks = pr.get("statusCheckRollup") or []
    for lane in ("g5-ci-rust", "g5-ci-e2e"):
        matches = [c for c in checks if c.get("name", "").startswith(lane + " ") or c.get("name") == lane]
        require(matches and all(c.get("status") == "COMPLETED" and c.get("conclusion") == "SUCCESS" for c in matches),
                f"PR #{number} needs successful current-head {lane}")
    return merge


def select_workers(instances):
    selected = []
    for instance in instances:
        labels = instance.get("labels", {})
        tags = instance.get("tags", {}).get("items", [])
        if "g5-worker" not in tags and not labels.get("g5_host_id"):
            continue
        require(not any(k.startswith(("goog-k8s", "goog-gke")) for k in labels), "Refusing a GKE-managed worker candidate")
        require("g5-worker" in tags and re.fullmatch(r"[0-9a-f]{32}", labels.get("g5_host_id", "")),
                f"Worker {instance['name']} lacks API-managed provenance; retire it manually first")
        require(re.fullmatch(r"[a-z][-a-z0-9]*", instance["name"]), "Invalid worker name")
        zone = instance["zone"].rsplit("/", 1)[-1]
        require(re.fullmatch(r"[a-z]+-[a-z]+[0-9]-[a-z]", zone), "Invalid worker zone")
        selected.append((instance["name"], zone))
    return selected


def check_workloads(items, helper):
    jobs = {i["metadata"]["name"] for i in items if i["kind"] == "Job"}
    for item in items:
        kind, name = item["kind"], item["metadata"]["name"]
        if kind == "Deployment":
            require(name in (*WRITERS, "g5-edge"), f"Unexpected deployment {name}; quiesce it manually")
        elif kind == "Job":
            require(item["metadata"].get("labels", {}).get("app.kubernetes.io/instance") == "g5",
                    f"Unexpected job {name}; remove it before cutover")
        elif kind == "Pod":
            if name == helper or item.get("status", {}).get("phase") in ("Succeeded", "Failed"):
                continue
            owners = item["metadata"].get("ownerReferences", [])
            owned_job = any(o["kind"] == "Job" and o["name"] in jobs for o in owners)
            app = item["metadata"].get("labels", {}).get("app")
            require(owners and (app in (*WRITERS, "g5-edge") or owned_job), f"Unmanaged pod {name}; stop it manually")
        else:
            require(False, f"Unsupported {kind} {name}; suspend/remove its writers before cutover")


class Reset:
    def __init__(self, args, runner=run, sleep=time.sleep):
        self.args, self.runner, self.sleep = args, runner, sleep
        self.directory = Path(args.state_dir or ROOT / ".dev" / "qa-reset" / args.commit).resolve()
        self.path = self.directory / "state.json"
        self.state = {}
        self.env = dict(os.environ)
        self.env["KUBECONFIG"] = str(self.directory / "kubeconfig")

    def call(self, *args, **kwargs):
        return self.runner(list(args), env=self.env, **kwargs)

    def cloud(self, *args):
        return self.call("gcloud", *args, f"--project={PROJECT}", "--quiet", "--format=json")

    def cloud_json(self, *args):
        return json.loads(self.cloud(*args))

    def kube(self, *args, **kwargs):
        return self.call("kubectl", f"--context={CONTEXT}", f"--namespace={NAMESPACE}", *args, **kwargs)

    def kube_json(self, *args):
        return json.loads(self.kube(*args, "-o", "json") or "{}")

    def save(self):
        temp = self.path.with_suffix(".tmp")
        temp.write_text(json.dumps(self.state, indent=2) + "\n")
        temp.chmod(0o600)
        temp.replace(self.path)

    def load(self):
        expected = {"commit": self.args.commit, "admin_email": self.args.admin_email,
                    "external_issues": self.args.external_issues, "project": PROJECT}
        if self.path.exists():
            self.state = json.loads(self.path.read_text())
            require(all(self.state.get(k) == v for k, v in expected.items()), "Journal belongs to different cutover inputs")
        else:
            self.state = {**expected, "run_id": uuid.uuid4().hex, "done": []}
            self.save()
        self.helper = "qa-reset-" + self.state["run_id"][:12]

    def phase(self, name, function):
        if name in self.state["done"]:
            print(f"Already complete: {name}", flush=True)
            return
        print(f"Starting: {name}", flush=True)
        self.state["phase"] = name
        self.save()
        function()
        self.state["done"].append(name)
        self.save()

    def guard_delivery(self):
        head = self.call("gh", "api", f"repos/{REPO}/git/ref/heads/main", "--jq", ".object.sha")
        require(head == self.args.commit, "main moved; keep the merge hold in place and review the cutover")
        identities = set()
        for trigger in TRIGGERS:
            data = self.cloud_json("builds", "triggers", "describe", trigger, f"--region={REGION}")
            require(data.get("disabled") is True, f"Pause {trigger} before running this script")
            identities.add(data.get("serviceAccount"))
        builds = self.cloud_json("builds", "list", f"--region={REGION}", "--filter=status=(QUEUED,PENDING,WORKING)")
        for build in builds:
            subs = build.get("substitutions", {})
            relevant = subs.get("TRIGGER_NAME") in TRIGGERS or build.get("serviceAccount") in identities - {None}
            ours = subs.get("_QA_RESET_RUN_ID") == self.state["run_id"]
            require(not relevant or ours, f"Another delivery/terraform build is active: {build.get('id')}")

    def release_file(self, path):
        return self.call("git", "show", f"{self.args.commit}:{path}") + "\n"

    def preflight(self):
        self.guard_delivery()
        for number in REQUIRED_PRS:
            pr = json.loads(self.call("gh", "pr", "view", str(number), "--repo", REPO,
                                     "--json", "state,mergeCommit,headRefOid,statusCheckRollup"))
            merge = check_pr(pr, number)
            self.call("git", "merge-base", "--is-ancestor", merge, self.args.commit)
        deploy = self.release_file("infra/ci/cloudbuild-deploy.yaml")
        require('_QA_DATABASE_RESET: "false"' in deploy,
                "Cutover commit must include the QA reset deployment option")
        require("external_issues_enabled" in self.release_file("backend/g5-api-core/src/services/agents/config_service.rs"),
                "Cutover source lacks PR #213")
        seed = self.release_file("scripts/seed-prod-org.sh")
        require("${GCP_PROJECT:?" in seed and "--context=" in seed, "Cutover needs the explicit-target seed script")
        (self.directory / "seed-org.sh").write_text(seed)
        repository = json.loads(self.call("gh", "api", f"repos/{REPO}"))
        require(repository.get("clone_url") == CODE_REMOTE and repository.get("default_branch") == "main",
                "Bootstrap code repository identity/default branch changed")
        require(isinstance(repository.get("id"), int), "Missing GitHub repository identity")
        self.state["provider_repo_id"] = str(repository["id"])
        self.save()
        require(self.call("git", "rev-parse", "--is-shallow-repository") == "false",
                "Fetch full repository history before cutover so the Base code mirror is complete")
        if "seed_code_repository" not in self.state["done"]:
            self.call("git", "--no-replace-objects", "pack-objects", "--stdout", "--revs",
                      input=self.args.commit + "\n", output_file=self.directory / "code.pack", timeout=600)
        instance = self.cloud_json("sql", "instances", "describe", INSTANCE)
        require(instance.get("region") == REGION and instance.get("state") == "RUNNABLE", "QA SQL instance is not ready")
        private = [x["ipAddress"] for x in instance.get("ipAddresses", []) if x.get("type") == "PRIVATE"]
        require(len(private) == 1 and ipaddress.ip_address(private[0]).is_private, "Missing unique Cloud SQL private IP")
        self.db_host = private[0]
        self.cloud("container", "clusters", "get-credentials", CLUSTER, f"--location={REGION}")
        workloads = self.kube_json("get", "deployments,statefulsets,daemonsets,cronjobs,jobs,hpa,pods")["items"]
        check_workloads(workloads, self.helper)
        select_workers(self.cloud_json("compute", "instances", "list"))
        groups = self.cloud_json("compute", "instance-groups", "managed", "list")
        require(all(g["name"].startswith("gke-") for g in groups), "Retire non-GKE managed instance groups before reset")

    def ensure_helper(self):
        existing = self.kube_json("get", "pods")["items"]
        match = next((p for p in existing if p["metadata"]["name"] == self.helper), None)
        if match:
            require(match["metadata"].get("labels", {}).get("qa-reset-run") == self.state["run_id"], "SQL helper ownership mismatch")
        else:
            pod = {"apiVersion": "v1", "kind": "Pod", "metadata": {
                "name": self.helper, "namespace": NAMESPACE, "labels": {"qa-reset-run": self.state["run_id"]}},
                "spec": {"restartPolicy": "Never", "nodeSelector": {"g5/pool": "system"}, "containers": [{
                    "name": "psql", "image": "postgres:17-alpine@sha256:266d3c257594e195b309ae9d1805e54c788f1d7b375f56bf6c367aac6c917e7b", "command": ["sleep", "86400"],
                    "env": [{"name": "G5_ADMIN_DATABASE_URL", "valueFrom": {"secretKeyRef": {
                        "name": "g5-api-secrets", "key": "G5_ADMIN_DATABASE_URL"}}}]}]}}
            self.kube("create", "-f", "-", input=json.dumps(pod))
        self.kube("wait", "--for=condition=Ready", f"pod/{self.helper}", "--timeout=180s", timeout=200)

    def sql(self, statement, database="postgres"):
        require(database in ("postgres", DATABASE), "Invalid SQL target")
        # Validate the secret's destination inside the pod, without downloading
        # its password. The QA ESO template stores password in the query string.
        shell = r'''set -eu
prefix="${G5_ADMIN_DATABASE_URL%%/g5_lean_qa\?*}"
[ "$prefix" = "postgres://g5@$1:5432" ] || { echo "QA admin URL mismatch" >&2; exit 1; }
query="${G5_ADMIN_DATABASE_URL#*\?}"
[ "$query" != "$G5_ADMIN_DATABASE_URL" ] || exit 1
exec psql -X -q -tA -v ON_ERROR_STOP=1 "$prefix/$2?$query"
'''
        return self.kube("exec", "-i", self.helper, "--", "sh", "-c", shell,
                         "qa-reset-psql", self.db_host, database, input=statement, timeout=180)

    def quiesce(self):
        self.guard_delivery()
        metadata = json.loads(self.sql("SELECT json_build_object('owner', pg_get_userbyid(datdba), "
            "'encoding', pg_encoding_to_char(encoding), 'collate', datcollate, 'ctype', datctype) "
            f"FROM pg_database WHERE datname = '{DATABASE}';"))
        require(metadata["owner"] == "g5", "Unexpected database owner; inspect before resetting")
        self.state["database_metadata"] = metadata
        self.save()
        for name in WRITERS:
            self.kube("scale", f"deployment/{name}", "--replicas=0")
        self.kube("delete", "jobs", "-l", "app.kubernetes.io/instance=g5", "--wait=true", "--timeout=180s", timeout=200)
        for name in WRITERS:
            self.wait_until(lambda n=name: not self.kube_json("get", "pods", "-l", f"app={n}")["items"], 300, f"{name} shutdown")
        workers = select_workers(self.cloud_json("compute", "instances", "list"))
        self.state["retired_workers"] = workers
        self.save()
        for name, zone in workers:
            print(f"Retiring QA worker {zone}/{name}", flush=True)
            self.cloud("compute", "instances", "delete", name, f"--zone={zone}")
        require(not select_workers(self.cloud_json("compute", "instances", "list")), "Worker instances remain")

    def reset_database(self):
        self.guard_delivery()
        require(not self.state.get("database_reset_started"),
                "Previous database reset has an uncertain outcome. Inspect the database and journal; refusing to wipe twice")
        for name in WRITERS:
            deployment = self.kube_json("get", "deployment", name)
            require(deployment["spec"].get("replicas") == 0, f"{name} restarted; stop it before reset")
            require(not self.kube_json("get", "pods", "-l", f"app={name}")["items"], f"{name} pods remain")
        require(not select_workers(self.cloud_json("compute", "instances", "list")), "Worker instances returned before reset")
        self.state["database_reset_started"] = True
        self.save()
        self.sql(f'ALTER DATABASE {DATABASE} ALLOW_CONNECTIONS false;\n'
                 f"SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{DATABASE}';")
        require(self.sql(f"SELECT count(*) FROM pg_stat_activity WHERE datname = '{DATABASE}';") == "0", "Database sessions remain")
        self.sql(f"DROP DATABASE {DATABASE};")
        m = self.state["database_metadata"]
        self.sql(f"CREATE DATABASE {DATABASE} OWNER g5 TEMPLATE template0 "
                 f"ENCODING {sql_literal(m['encoding'])} LC_COLLATE {sql_literal(m['collate'])} LC_CTYPE {sql_literal(m['ctype'])};")

    def wait_until(self, predicate, seconds, label):
        deadline = time.monotonic() + seconds
        while not predicate():
            require(time.monotonic() < deadline, f"Timed out waiting for {label}; resume with the same journal")
            self.sleep(15)

    def build(self, trigger, substitutions):
        self.guard_delivery()
        key = f"build_{trigger}"
        if key not in self.state:
            # Persist intent before submitting. After an interrupted submission,
            # find the unique run ID instead of accidentally starting two builds.
            self.state[key] = {"requested": True}
            self.save()
            result = self.cloud_json("builds", "triggers", "run", trigger, f"--region={REGION}",
                f"--sha={self.args.commit}", "--substitutions=" + ",".join(
                    f"{k}={v}" for k, v in {**substitutions, "_QA_RESET_RUN_ID": self.state["run_id"]}.items()))
            build = result.get("metadata", {}).get("build", result)
            if build.get("id"):
                self.state[key]["id"] = build["id"]
                self.save()
        if not self.state[key].get("id"):
            matches = self.cloud_json("builds", "list", f"--region={REGION}",
                f"--filter=substitutions._QA_RESET_RUN_ID={self.state['run_id']} AND substitutions.TRIGGER_NAME={trigger}")
            require(len(matches) == 1, "Build submission outcome is ambiguous; inspect Cloud Build before retrying")
            self.state[key]["id"] = matches[0]["id"]
            self.save()
        build_id = self.state[key]["id"]
        print(f"Build: https://console.cloud.google.com/cloud-build/builds;region={REGION}/{build_id}?project={PROJECT}", flush=True)

        def finished():
            build = self.cloud_json("builds", "describe", build_id, f"--region={REGION}")
            require(build.get("substitutions", {}).get("COMMIT_SHA") == self.args.commit, "Build resolved a different commit")
            status = build["status"]
            require(status in ACTIVE_BUILDS | {"SUCCESS"}, f"Build {build_id} ended {status}; inspect its logs (no automatic retry)")
            return status == "SUCCESS"
        self.wait_until(finished, 6 * 3600, trigger)

    def worker_image(self):
        self.build(WORKER_TRIGGER, {"_RUN_FULL_BUILD": "true", "_REPIN_DEV_API_WORKER_IMAGE": "false",
                                    "_TRIGGER_DEV_WORKER_ROLLOUT": "false"})
        images = self.cloud_json("compute", "images", "list", "--no-standard-images", "--filter=family=g5-worker AND status=READY")
        exact = []
        for image in images:
            sha = image.get("labels", {}).get("g5_git_short_sha", "")
            if re.fullmatch(r"[0-9a-f]{7,40}", sha) and self.args.commit.startswith(sha):
                require(self.call("git", "rev-parse", f"{sha}^{{commit}}") == self.args.commit, "Ambiguous worker source SHA")
                exact.append(image)
        require(exact, "No READY worker image for the exact cutover commit")
        chosen = max(exact, key=lambda x: x.get("creationTimestamp", ""))
        self.state["worker_image"] = f"projects/{PROJECT}/global/images/{chosen['name']}"
        self.save()

    def deploy(self):
        self.build(DEPLOY_TRIGGER, {"_QA_DATABASE_RESET": "true"})
        api = self.kube_json("get", "deployment", "g5-api")
        container = next(c for c in api["spec"]["template"]["spec"]["containers"] if c["name"] == "g5-api")
        settings = {x["name"]: x.get("value") for x in container.get("env", [])}
        require(settings.get("G5_WORKER_IMAGE") == self.state["worker_image"], "Deployed worker pin differs from cutover image")
        for name in (*WRITERS, "g5-edge"):
            self.kube("rollout", "status", f"deployment/{name}", "--timeout=600s", timeout=620)
        expected = {}
        for line in self.release_file("backend/g5-api/src/db/migrations/checksums.sha384").splitlines():
            if line.strip():
                checksum, filename = line.split()
                expected[int(filename.split("_", 1)[0])] = checksum
        rows = json.loads(self.sql("SELECT json_agg(json_build_object('version', version, 'checksum', encode(checksum,'hex'), 'success', success)) FROM _sqlx_migrations;", DATABASE))
        require(all(row["success"] for row in rows) and {r["version"]: r["checksum"] for r in rows} == expected,
                "Deployed migration ledger does not match the cutover source")

    def seed(self):
        self.sql("GRANT g5_app TO g5;", DATABASE)
        # Whitelist the shell environment: inherited seed options/passwords
        # must not override the image policy, admin or connection.
        env = {k: v for k, v in self.env.items() if k in {
            "PATH", "HOME", "KUBECONFIG", "CLOUDSDK_CONFIG", "CLOUDSDK_AUTH_CREDENTIAL_FILE_OVERRIDE",
            "GOOGLE_APPLICATION_CREDENTIALS", "GKE_GCLOUD_AUTH_PLUGIN_USE_ADC"}}
        env.update(GCP_PROJECT=PROJECT, GCP_REGION=REGION, DB_INSTANCE_NAME=INSTANCE,
                   DB_HOST=self.db_host, DB_NAME=DATABASE, K8S_NAMESPACE=NAMESPACE,
                   K8S_CONTEXT=CONTEXT, USE_K8S_PSQL="1", DEFAULT_ADMIN_EMAIL=self.args.admin_email,
                   DEFAULT_BASE_IMAGE_TRACK="g5d-g5dev")
        self.runner(["bash", str(self.directory / "seed-org.sh")], env=env, timeout=600)
        # The platform seed provisions only the operator org and the admin's
        # operator invitation; the g5 tenant org is created by that operator.
        org = json.loads(self.sql("SET ROLE g5_app; SELECT json_build_object('id',id,'image',default_base_image_ref) "
                                  "FROM orgs WHERE kind='operator' AND slug='operator';", DATABASE))
        require(org.get("image", "").startswith(f"{REGION}-docker.pkg.dev/{PROJECT}/g5/"), "Seed has no QA base image")
        require(self.sql("SET ROLE g5_app; SELECT count(*) FROM orgs;", DATABASE) == "1",
                "The platform seed must leave only the operator org on a fresh database")
        self.state["operator_org_id"] = str(uuid.UUID(org["id"]))
        self.save()

    def api(self, method, path, data=None):
        token = Path(self.args.admin_token_file).read_text().strip()
        require(token and "\n" not in token, "Invalid admin token file")
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *args, **kwargs):
                return None
        request = urllib.request.Request(API + path, method=method,
            headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
            data=json.dumps(data).encode() if data is not None else None)
        try:
            with urllib.request.build_opener(NoRedirect).open(request, timeout=60) as response:
                return json.load(response)["data"]
        except urllib.error.HTTPError as error:
            raise RuntimeError(f"QA API returned HTTP {error.code}; verify the fresh admin session") from None

    def create_org(self):
        """Create the g5 tenant org for the admin through the operator API."""
        self.guard_delivery()
        # The database was recreated by this run, so a g5 org can only be this
        # phase's own earlier request (e.g. a lost response); adopt it.
        existing = [o for o in self.api("GET", "/platform/orgs") if o.get("slug") == TENANT_ORG_SLUG]
        if existing:
            org_id = existing[0]["id"]
        else:
            created = self.api("POST", "/platform/orgs", {
                "name": TENANT_ORG_NAME, "slug": TENANT_ORG_SLUG, "adminEmail": self.args.admin_email})
            require(created["admin"]["email"].lower() == self.args.admin_email.lower(),
                    "Platform API made a different account the initial Admin")
            org_id = created["org"]["id"]
        org = str(uuid.UUID(org_id))
        facts = json.loads(self.sql(f"""SET ROLE g5_app; SELECT json_build_object(
  'kind', o.kind, 'image', o.default_base_image_ref,
  'admin', (SELECT lower(u.email) FROM org_memberships m JOIN users u ON u.id=m.user_id
            WHERE m.org_id=o.id AND m.role = 'admin' AND m.deactivated_at IS NULL
            AND lower(u.email)={sql_literal(self.args.admin_email.lower())})
) FROM orgs o WHERE o.id='{org}' AND o.slug='{TENANT_ORG_SLUG}';""", DATABASE))
        require(facts.get("kind") == "tenant" and facts.get("admin") == self.args.admin_email.lower(),
                "The g5 org is not a tenant administered by the named admin")
        require((facts.get("image") or "").startswith(f"{REGION}-docker.pkg.dev/{PROJECT}/g5/"),
                "The g5 org has no QA base image; check G5_NEW_ORG_DEFAULT_IMAGE_TRACK")
        self.state["org_id"] = org
        self.save()
        print(f"Created {TENANT_ORG_NAME} ({TENANT_ORG_SLUG}) for {self.args.admin_email}", flush=True)

    def seed_code_repository(self):
        self.guard_delivery()
        org = self.state["org_id"]
        if "code_repo_id" not in self.state:
            self.state["code_repo_id"] = str(uuid.uuid4())
            self.save()
        repo = str(uuid.UUID(self.state["code_repo_id"]))
        head = self.kube("exec", "-i", "deploy/g5-api", "-c", "g5-api", "--", "sh", "-c",
                         INSTALL_CODE_MIRROR, "qa-seed-code", org, repo, self.args.commit, CODE_REMOTE,
                         input_file=self.directory / "code.pack", timeout=600)
        require(head == self.args.commit, "Base code mirror did not materialize the cutover commit")
        # Bootstrap only: Git objects exist before publishing membership/policy.
        # Use the same scoped admin session pattern as seed-prod-org.sh, acting
        # as the org's owner (the admin); never overwrite a repo or code
        # membership that another actor created. The
        # creation cascade gave the fresh Base a local `code` repository; it is
        # replaced, not adopted, because Adopt-Remote needs provider access and
        # would reconcile unrelated histories through a merge.
        self.sql(f"""BEGIN;
INSERT INTO authz.session_ctx (pid,org_ids,user_id,scopes,set_at)
SELECT pg_backend_pid(), ARRAY['{org}'::uuid], id, ARRAY['*']::text[], now()
FROM users WHERE lower(email)=lower({sql_literal(self.args.admin_email)})
ON CONFLICT (pid) DO UPDATE SET org_ids=EXCLUDED.org_ids, user_id=EXCLUDED.user_id,
scopes=EXCLUDED.scopes, set_at=EXCLUDED.set_at;
DO $guard$ BEGIN
  IF (SELECT count(*) FROM workspaces WHERE org_id='{org}' AND is_base AND deleted_at IS NULL) <> 1 THEN
    RAISE EXCEPTION 'Expected one fresh Base workspace';
  END IF;
  IF EXISTS (SELECT 1 FROM workspaces child JOIN workspaces parent ON child.cloned_from_id=parent.id
             WHERE parent.org_id='{org}' AND parent.is_base AND child.deleted_at IS NULL) THEN
    RAISE EXCEPTION 'Finish fresh Base bootstrap before creating child workspaces';
  END IF;
END $guard$;
DO $replace$
DECLARE base uuid; local_repo uuid;
BEGIN
  SELECT id INTO base FROM workspaces WHERE org_id='{org}' AND is_base AND deleted_at IS NULL;
  FOR local_repo IN SELECT m.repo_id FROM workspace_repos m
                    WHERE m.org_id='{org}' AND m.workspace_id=base AND m.role='code' AND m.repo_id<>'{repo}' LOOP
    -- Only the cascade's untouched local repository is replaced.
    IF NOT EXISTS (SELECT 1 FROM repos r WHERE r.id=local_repo AND r.org_id='{org}'
                   AND r.name='code' AND r.slug='code' AND r.provider IS NULL
                   AND r.git_url LIKE 'g5-local://%')
       OR EXISTS (SELECT 1 FROM workspace_repos o WHERE o.repo_id=local_repo AND o.workspace_id<>base)
       OR EXISTS (SELECT 1 FROM remote_integrations i WHERE i.repo_id=local_repo) THEN
      RAISE EXCEPTION 'Base already has another code repository';
    END IF;
    DELETE FROM workspace_repos WHERE org_id='{org}' AND workspace_id=base AND repo_id=local_repo;
    DELETE FROM repos WHERE org_id='{org}' AND id=local_repo;
  END LOOP;
END $replace$;
INSERT INTO repos (id,org_id,name,slug,git_url,default_branch,provider,provider_repo_id,provider_full_name)
VALUES ('{repo}','{org}','G5-Lean','g5labs-g5-lean','{CODE_REMOTE}','main','github','{self.state['provider_repo_id']}','{REPO}')
ON CONFLICT (id) DO NOTHING;
DO $guard$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM repos WHERE id='{repo}' AND org_id='{org}' AND git_url='{CODE_REMOTE}'
                 AND default_branch='main' AND provider='github' AND provider_repo_id='{self.state['provider_repo_id']}') THEN
    RAISE EXCEPTION 'Bootstrap code repository identity changed';
  END IF;
END $guard$;
INSERT INTO workspace_repos (org_id,workspace_id,repo_id,root_path,role)
SELECT '{org}',id,'{repo}','code','code' FROM workspaces WHERE org_id='{org}' AND is_base AND deleted_at IS NULL
ON CONFLICT (workspace_id,repo_id) DO NOTHING;
INSERT INTO remote_integrations (org_id,workspace_id,repo_id,remote_branch,auto_pull_enabled)
SELECT '{org}',id,'{repo}','main',true FROM workspaces WHERE org_id='{org}' AND is_base AND deleted_at IS NULL
ON CONFLICT (workspace_id,repo_id) DO NOTHING;
DO $guard$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM workspace_repos m JOIN remote_integrations i
                 USING (org_id,workspace_id,repo_id)
                 WHERE m.org_id='{org}' AND m.repo_id='{repo}' AND m.role='code'
                   AND m.root_path='code' AND i.remote_branch='main') THEN
    RAISE EXCEPTION 'Base code membership or remote policy changed';
  END IF;
  IF (SELECT count(*) FROM workspace_repos m JOIN workspaces w ON w.id=m.workspace_id
      WHERE w.org_id='{org}' AND w.is_base AND m.role='code') <> 1 THEN
    RAISE EXCEPTION 'Base must have exactly the G5-Lean code repository';
  END IF;
END $guard$;
DO $clear$ BEGIN PERFORM authz.clear_ctx(); END $clear$;
COMMIT;""", DATABASE)
        print(f"Base code repository: {CODE_REMOTE}, main at {self.args.commit}", flush=True)

    def finish_bootstrap(self):
        org = self.state["org_id"]
        repos = self.api("GET", f"/orgs/{org}/repos")
        code = next((r for r in repos if r["id"] == self.state["code_repo_id"]), None)
        require(code and code.get("gitUrl") == CODE_REMOTE and code.get("defaultBranch") == "main",
                "Base code repository remote differs from the requested G5-Lean URL")
        bases = [w for w in self.api("GET", f"/orgs/{org}/workspaces") if w.get("isBase")]
        require(len(bases) == 1, "Expected one Base workspace after seeding")
        branches = self.api("GET", f"/workspaces/{bases[0]['id']}/branches")
        branch = next((b for b in branches if b["repoId"] == self.state["code_repo_id"]), None)
        require(branch and branch.get("trackingMode") == "remote_tracking" and branch.get("remoteBranch") == "main"
                and branch.get("materializationStatus") == "materialized" and branch.get("headCommit") == self.args.commit,
                "Base code membership, Git head, or remote tracking is not ready")
        self.api("PUT", f"/orgs/{org}/config", {"settings": {"external_issues_enabled": self.args.external_issues == "on"}})
        config = self.api("GET", f"/orgs/{org}/config")
        require(config.get("settings", {}).get("external_issues_enabled") is (self.args.external_issues == "on"),
                "External issues setting did not persist at organization scope")
        if not self.state.get("new_host_id"):
            require(not self.state.get("host_request_started"),
                    "Host request outcome is uncertain; inspect Worker Hosts before retrying (no duplicate provisioning)")
            self.state["host_request_started"] = True
            self.save()
            host = self.api("POST", f"/orgs/{org}/worker-hosts", {"region": REGION, "instanceType": "c3-standard-4", "imageTrack": "g5d-g5dev"})
            self.state["new_host_id"] = str(uuid.UUID(host["hostId"]))
            self.save()  # The response's hostJwt is deliberately not retained.
        host_id = self.state["new_host_id"]
        def ready():
            hosts = self.api("GET", f"/orgs/{org}/worker-hosts?limit=100")["hosts"]
            host = next((h for h in hosts if h["hostId"] == host_id), None)
            require(host, "New worker is missing from the organization")
            status = host["status"]
            require(status not in ("failed", "terminated"), f"New worker is {status}; inspect Worker Hosts")
            return status == "ready" and bool(host.get("lastHeartbeat"))
        self.wait_until(ready, 1800, "new QA worker readiness")

    def acquire_cluster_lock(self):
        locks = self.kube_json("get", "configmap", "qa-database-reset", "--ignore-not-found")
        if locks:
            require(locks.get("data", {}).get("run_id") == self.state["run_id"],
                    "Another cutover owns the cluster lock; resume its journal or inspect it before starting over")
            return
        self.kube("create", "configmap", "qa-database-reset", f"--from-literal=run_id={self.state['run_id']}",
                  f"--from-literal=commit={self.args.commit}")

    def execute(self):
        self.directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        with (self.directory / "lock").open("w") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise RuntimeError("Another process owns this cutover journal") from None
            self.load()
            self.preflight()
            self.acquire_cluster_lock()
            try:
                self.ensure_helper()
                self.phase("quiesce", self.quiesce)
                self.phase("reset_database", self.reset_database)
                self.phase("worker_image", self.worker_image)
                self.phase("deploy", self.deploy)
                self.phase("seed", self.seed)
                if "finish_bootstrap" not in self.state["done"] and not self.args.admin_token_file:
                    print("Platform seeded. Log in at https://qa.g5labs.ai as the invited admin to accept the operator invitation, "
                          "save a fresh admin token to a private file, and rerun with --admin-token-file. Completed phases will not repeat.")
                    return
                self.phase("create_org", self.create_org)
                self.phase("seed_code_repository", self.seed_code_repository)
                self.phase("finish_bootstrap", self.finish_bootstrap)
                self.kube("delete", "configmap", "qa-database-reset", "--ignore-not-found")
                print("QA reset complete. Bind the GitHub App in organization settings, select repositories, and run the smoke tests. Delivery triggers remain paused.")
            finally:
                try:
                    self.kube("delete", "pod", self.helper, "--ignore-not-found", "--wait=false")
                except (RuntimeError, OSError, subprocess.TimeoutExpired):
                    print(f"SQL helper cleanup failed; remove {NAMESPACE}/{self.helper} in {CONTEXT} after inspection.", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--commit", required=True, help="Final main SHA containing merged PRs 186 and 213")
    parser.add_argument("--admin-email", required=True)
    parser.add_argument("--external-issues", choices=("on", "off"), default="on")
    parser.add_argument("--state-dir", help="Private journal directory; reuse it when resuming")
    parser.add_argument("--admin-token-file", help="Fresh post-reset QA operator-admin session token; enables org creation and final bootstrap")
    parser.add_argument("--execute", action="store_true", help="Perform the destructive QA reset; default is an offline preview")
    args = parser.parse_args()
    try:
        validate_options(args)
        if not args.execute:
            print(f"PREVIEW ONLY: {PROJECT} / {CLUSTER} / {INSTANCE} / {DATABASE}")
            print(f"Commit: {args.commit}; admin: {args.admin_email}; external issues: {args.external_issues}")
            print("Require PRs 186 and 213 merged with green CI, main held at that commit, and deploy/worker/terraform-apply triggers paused.")
            print("4. Stop API/daemon/jobs and delete API-managed QA worker VMs.")
            print("5. Drop/recreate only g5_lean_qa, preserving owner, encoding and locale. No backup is taken.")
            print("6. Bake the exact worker image; deploy the same commit without Helm automatic rollback; verify migrations.")
            print("7. Seed the operator org (admin invitation, docs and code mirrors, image policy). After the admin signs in and supplies a fresh token,")
            print(f"   create the {TENANT_ORG_NAME} ({TENANT_ORG_SLUG}) org for the admin, replace its Base code repository with {CODE_REMOTE} main")
            print("   at the cutover SHA, set external issues and provision one worker.")
            print("GitHub App binding requires an admin browser login. Triggers stay paused for smoke tests. Add --execute to run.")
            return 0
        Reset(args).execute()
        return 0
    except (RuntimeError, subprocess.TimeoutExpired, OSError, ValueError) as error:
        print(f"STOPPED: {error}\nNo rollback, old-writer restart, or trigger re-enable is attempted. Keep the same journal for recovery.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
