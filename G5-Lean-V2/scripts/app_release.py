#!/usr/bin/env python3
"""Trusted QA -> staging -> production artifact handoff; no compilation on promotion.

Runs inside Cloud Build. The generation-pinned manifest is written only by QA's
trusted deploy identity in the dedicated release bucket (never PR CI's bucket).
Build API provenance, not caller-supplied COMMIT_SHA, establishes source identity.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

import await_worker_image as worker_gate
import resolve_worker_image as worker_images
import release_artifacts as artifacts

IMAGES = {
    "API_IMAGE_REF": "api", "G5D_IMAGE_REF": "g5d", "EDGE_IMAGE_REF": "edge",
    "G5D_TUNGSTEN_IMAGE_REF": "g5d-tungsten",
    "G5D_PAIRSOFT_PREVIEW_IMAGE_REF": "g5d-pairsoft-preview",
    "G5D_PNC_BANKWORKFLOW_IMAGE_REF": "g5d-pnc-bankworkflow",
    "G5D_PNC_CASH_RECEIPT_IMAGE_REF": "g5d-pnc-cash-receipt",
    "G5D_G5DEV_IMAGE_REF": "g5d-g5dev",
}
SHA = re.compile(r"[0-9a-f]{40}")
BUILD_ID = re.compile(r"[0-9a-f-]{36}")
TARGETS = {
    "staging": ("g5lean-staging-1ccc", "g5-lean-staging"),
    "prod": ("g5lean-prod-1ccc", "g5-lean-prod"),
}
# These are deployment policy, not artifact-selection inputs. A caller may
# supply the immutable manifest/refs, but cannot override a trigger's target,
# acceptance policy or destination and still claim that trigger's provenance.
POLICY_KEYS = (
    "_ENVIRONMENT", "_TARGET_PROJECT", "_GKE_CLUSTER", "_GKE_ZONE",
    "_HELM_RELEASE", "_HELM_NAMESPACE", "_RELEASE_BUCKET",
    "_ARTIFACT_PROJECT", "_AR_PROJECT", "_AR_REGION", "_AR_REPOSITORY", "_WORKER_IMAGE_PROJECT",
    "_RELEASE_CHECKS_REQUIRED", "_RELEASE_CHECK_BASE_URL", "_RELEASE_CHECK_SECRET",
    "_NEXT_TRIGGER", "_PROMOTION_MODE", "_FILESTORE_INSTANCE", "_FILESTORE_ZONE",
    "_FILESTORE_PV_NAME", "_FILESTORE_PVC_NAME", "_FILESTORE_STORAGE",
    "_EXTERNAL_ACCESS_ENABLED", "_APP_HOSTNAME", "_PREVIEW_HOSTNAME",
    "_INGRESS_STATIC_IP_NAME", "_CERTIFICATE_MAP_NAME",
)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def public_settings(environment, settings):
    """Existing G5 ingress values, derived from the reviewed Terraform settings."""
    require(environment in TARGETS and isinstance(settings, dict), "Invalid target public configuration")
    enabled = settings.get("external_access_enabled", False)
    app, preview = settings.get("app_hostname", ""), settings.get("preview_hostname", "")
    require(type(enabled) is bool, "External access must be boolean")
    zone = "lean.g5labs.ai" if environment == "staging" else "app.g5labs.ai"
    for host in (app, preview):
        require(isinstance(host, str), "Hostnames must be strings")
        if host:
            require(len(host) <= 253 and "." in host and all(
                re.fullmatch(r"[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?", label) for label in host.split(".")),
                "Hostnames must be explicit lowercase DNS names")
            require(host == zone or host.endswith("." + zone),
                    "Hostnames must belong to this environment's delegated DNS child")
    require(not app or not preview or app != preview, "App and preview hostnames must be distinct")
    require(not enabled or (app and preview), "Public ingress requires app and preview hostnames")
    return {"_EXTERNAL_ACCESS_ENABLED": str(enabled).lower(), "_APP_HOSTNAME": app,
            "_PREVIEW_HOSTNAME": preview, "_INGRESS_STATIC_IP_NAME": f"g5-lean-{environment}-ip",
            "_CERTIFICATE_MAP_NAME": f"g5-lean-{environment}-ip-cert-map"}


def run(*args):
    # Never echo command arguments: some sibling release checks use credentials.
    return subprocess.check_output(args, text=True).strip()


def gcloud(*args):
    return run("gcloud", *args)


def build_source(build):
    provenance = build.get("sourceProvenance") or {}
    source = build.get("source") or {}
    for block, key in ((provenance.get("resolvedRepoSource", {}), "commitSha"),
                       (provenance.get("resolvedGitSource", {}), "revision"),
                       (provenance.get("resolvedConnectedRepository", {}), "revision"),
                       (source.get("gitSource", {}), "revision"),
                       (source.get("connectedRepository", {}), "revision")):
        value = block.get(key, "")
        if SHA.fullmatch(value):
            return value
    raise ValueError("Build API did not resolve an immutable source revision")


def validate_refs(refs, registry, worker_project, require_all=True):
    for key, repository in IMAGES.items():
        value = refs.get(key, "")
        if not value and not require_all and key not in {"API_IMAGE_REF", "G5D_IMAGE_REF", "EDGE_IMAGE_REF"}:
            continue
        require(re.fullmatch(re.escape(f"{registry}/{repository}@sha256:") + r"[0-9a-f]{64}", value),
                f"{key} must be a digest in the selected trusted artifact repository")
    require(re.fullmatch(re.escape(f"projects/{worker_project}/global/images/") +
                         r"[a-z](?:[-a-z0-9]{0,61}[a-z0-9])?", refs.get("WORKER_IMAGE_REF", "")),
            "Worker image must be concrete and owned by the selected artifact project; families are forbidden")


def read_env(path):
    result = {}
    for line in Path(path).read_text().splitlines():
        if line.startswith("export "):
            line = line[7:]
        if "=" in line:
            key, value = line.split("=", 1)
            result[key] = value
    return result


class Release:
    def __init__(self, env=None):
        self.env = dict(os.environ if env is None else env)
        self.project = self.env["PROJECT_ID"]
        self.region = self.env["_AR_REGION"]
        self.commit = self.env["COMMIT_SHA"]
        self.environment = self.env["_ENVIRONMENT"]
        self.workspace = Path(self.env.get("WORKSPACE_DIR", "/workspace"))
        self.artifact_project = artifacts.validate_layout(
            self.project, self.env.get("_ARTIFACT_PROJECT", artifacts.QA_BUILD_PROJECT),
            self.env.get("_AR_PROJECT", artifacts.QA_BUILD_PROJECT),
            self.env.get("_WORKER_IMAGE_PROJECT", artifacts.QA_BUILD_PROJECT),
            self.env.get("_RELEASE_BUCKET", ""), require_bucket=self.environment != "qa")
        self.registry = f"{self.region}-docker.pkg.dev/{self.artifact_project}/{self.env['_AR_REPOSITORY']}"

    def artifact_binding(self, trigger, build, *, current=False):
        # Legacy QA triggers omit these new keys; they cannot select admin artifacts.
        policy = {"_ARTIFACT_PROJECT": artifacts.QA_BUILD_PROJECT, "_RELEASE_BUCKET": "",
                  **(trigger.get("substitutions") or {})}
        actual = build.get("substitutions") or {}
        artifacts.validate_layout(self.project, policy.get("_ARTIFACT_PROJECT"),
            policy.get("_AR_PROJECT"), policy.get("_WORKER_IMAGE_PROJECT"),
            policy.get("_RELEASE_BUCKET", ""), require_bucket=bool(policy.get("_NEXT_TRIGGER")))
        for key in ("_ARTIFACT_PROJECT", "_AR_PROJECT", "_WORKER_IMAGE_PROJECT", "_RELEASE_BUCKET"):
            require(key in policy and actual.get(key) == policy[key], f"Build overrides trusted artifact policy: {key}")
            if current:
                require(self.env.get(key) == actual[key], f"Process differs from trusted artifact policy: {key}")
        require(actual["_ARTIFACT_PROJECT"] == self.artifact_project,
                "Upstream artifacts belong to a different selected artifact project")

    def producer_binding(self, build, *, current=False):
        trigger = json.loads(gcloud("builds", "triggers", "describe", "g5-deploy-qa",
            f"--project={self.project}", f"--region={self.region}", "--format=json"))
        require(trigger.get("name") == "g5-deploy-qa" and trigger.get("id")
                and build.get("buildTriggerId") == trigger["id"] and not trigger.get("disabled", False),
                "Artifact producer must use the enabled trusted QA deploy trigger")
        identity = f"g5-lean-deploy@{self.project}.iam.gserviceaccount.com"
        for record in (trigger, build):
            require(record.get("serviceAccount", "").split("/")[-1] == identity,
                    "Artifact producer must run as the QA deploy identity")
        self.artifact_binding(trigger, build, current=current)

    def policy(self):
        """Check live trigger policy before compiling or writing any artifacts."""
        require(self.project == artifacts.QA_BUILD_PROJECT and self.region == "us-central1",
                "Unexpected release coordinator")
        current = self.describe(self.env["BUILD_ID"])
        require(build_source(current) == self.commit, "Build source differs from selected revision")
        if self.environment == "qa":
            self.producer_binding(current, current=True)
        else:
            self.promotion_binding(current, self.environment, current=True)

    def describe(self, build_id):
        require(BUILD_ID.fullmatch(build_id), "Invalid upstream build ID")
        return json.loads(gcloud("builds", "describe", build_id, f"--project={self.project}",
                                 f"--region={self.region}", "--format=json"))

    def trigger_id(self, name):
        return gcloud("builds", "triggers", "describe", name, f"--project={self.project}",
                      f"--region={self.region}", "--format=value(id)")

    def promotion_binding(self, build, environment, *, current=False):
        """Bind a build to the live trusted trigger, independently of overrides.

        Trigger policy changing while a build waits fails closed: requeue under
        the new policy. In particular, a prior unchecked bootstrap deployment
        cannot become staging acceptance when normal promotion is activated.
        """
        require(self.project == "g5-qa-1ccc" and self.region == "us-central1",
                "Unexpected release coordinator")
        name = f"g5-promote-{environment}"
        trigger = json.loads(gcloud("builds", "triggers", "describe", name,
            f"--project={self.project}", f"--region={self.region}", "--format=json"))
        require(trigger.get("name") == name and trigger.get("id")
                and build.get("buildTriggerId") == trigger["id"],
                "Build did not run the expected environment promotion trigger")
        require(not trigger.get("disabled", False), "Promotion trigger is disabled")
        central = f"g5-lean-release@{self.project}.iam.gserviceaccount.com"
        for record in (trigger, build):
            require(record.get("serviceAccount", "").split("/")[-1] == central,
                    "Promotion must execute as the configured central release identity")
        approved = trigger.get("approvalConfig", {}).get("approvalRequired", False)
        require(approved == (environment == "staging"), "Unexpected promotion approval policy")
        if environment == "staging":
            require(build.get("approval", {}).get("state") == "APPROVED",
                    "Staging deployment was not approved")
        policy = trigger.get("substitutions") or {}
        actual = build.get("substitutions") or {}
        self.artifact_binding(trigger, build, current=current)
        project, cluster = TARGETS[environment]
        fixed = {
            "_ENVIRONMENT": environment, "_TARGET_PROJECT": project, "_GKE_CLUSTER": cluster,
            "_GKE_ZONE": "us-central1", "_HELM_RELEASE": "g5", "_HELM_NAMESPACE": "g5",
            "_RELEASE_BUCKET": artifacts.release_bucket(self.artifact_project),
            "_ARTIFACT_PROJECT": self.artifact_project,
            "_AR_PROJECT": self.artifact_project, "_AR_REGION": self.region, "_AR_REPOSITORY": "g5",
            "_WORKER_IMAGE_PROJECT": self.artifact_project, "_PROMOTION_MODE": "deploy",
        }
        for key, expected in fixed.items():
            require(policy.get(key) == expected, f"Trusted trigger has invalid {key}")
        require(policy.get("_EXTERNAL_ACCESS_ENABLED") in {"true", "false"}, "Invalid public access policy")
        configured = public_settings(environment, {
            "external_access_enabled": policy["_EXTERNAL_ACCESS_ENABLED"] == "true",
            "app_hostname": policy.get("_APP_HOSTNAME", ""),
            "preview_hostname": policy.get("_PREVIEW_HOSTNAME", ""),
        })
        for key, expected in configured.items():
            require(policy.get(key) == expected, f"Trusted trigger has invalid {key}")
        for key in POLICY_KEYS:
            require(key in policy and actual.get(key) == policy[key],
                    f"Build overrides trusted trigger policy: {key}")
            if current and key in self.env:
                require(self.env[key] == actual[key], f"Process differs from Build API policy: {key}")
        checks = policy["_RELEASE_CHECKS_REQUIRED"]
        next_trigger = policy["_NEXT_TRIGGER"]
        require(checks in {"true", "false"}, "Invalid release check policy")
        require(next_trigger in ({"", "g5-promote-prod"} if environment == "staging" else {""}),
                "Unexpected downstream promotion")
        if checks == "false":
            require(not next_trigger, "Bootstrap cannot automatically promote unchecked application artifacts")
        else:
            require(policy["_RELEASE_CHECK_BASE_URL"] and policy["_RELEASE_CHECK_SECRET"],
                    "Required acceptance lacks a configured endpoint or fixture secret")
            if policy["_EXTERNAL_ACCESS_ENABLED"] == "true":
                require(policy["_RELEASE_CHECK_BASE_URL"].rstrip("/") == "https://" + policy["_APP_HOSTNAME"],
                        "Required acceptance must test this target's configured app origin")
        return actual

    def successful_build(self, build_id, trigger_name):
        trigger = self.trigger_id(trigger_name)
        # The parent schedules its child as its final step, so it can still be
        # WORKING when the child starts. Never accept FAILED or green-skipped
        # unrelated sources; the manifest and trigger are checked separately.
        for attempt in range(20):
            build = self.describe(build_id)
            require(build.get("buildTriggerId") == trigger, "Upstream build used the wrong trigger")
            require(build_source(build) == self.commit, "Upstream source differs from selected revision")
            if build.get("status") == "SUCCESS":
                return build
            require(build.get("status") in {"QUEUED", "WORKING"}, "Upstream build did not succeed")
            if attempt < 19:
                time.sleep(15)
        raise ValueError("Upstream build did not finish successfully within five minutes")

    def manifest(self):
        uri = self.env.get("_RELEASE_MANIFEST", "")
        bucket = self.env.get("_RELEASE_BUCKET", "")
        require(bucket and re.fullmatch(re.escape(f"gs://{bucket}/app-releases/") +
                r"[0-9a-f-]{36}\.json#[0-9]+", uri), "A generation-pinned release manifest is required")
        data = json.loads(gcloud("storage", "cat", uri))
        require(data.get("version") == 1 and data.get("commit") == self.commit,
                "Manifest schema/revision mismatch")
        require(data.get("project") == self.project, "Manifest belongs to another build project")
        require(data.get("artifact_project", artifacts.QA_BUILD_PROJECT) == self.artifact_project,
                "Manifest belongs to another artifact project")
        require(uri.split("/")[-1].split(".json")[0] == data.get("qa_build_id"), "Manifest origin/path mismatch")
        validate_refs(data.get("refs", {}), self.registry, self.artifact_project)
        return data

    def preflight(self):
        if self.environment == "qa":
            return
        require(run("git", "rev-parse", "HEAD") == self.commit,
                "Checked-out scripts/chart differ from candidate; run trigger with --sha")
        current = self.describe(self.env["BUILD_ID"])
        require(build_source(current) == self.commit, "Current build source is not the selected revision")
        current_policy = self.promotion_binding(current, self.environment, current=True)
        data = self.manifest()
        origin = self.successful_build(data["qa_build_id"], "g5-deploy-qa")
        self.producer_binding(origin)
        previous = "g5-deploy-qa" if self.environment == "staging" else "g5-promote-staging"
        parent = self.successful_build(self.env.get("_SOURCE_BUILD_ID", ""), previous)
        if self.environment == "staging":
            require(parent.get("id") == data["qa_build_id"], "Staging parent is not manifest origin")
        else:
            parent_policy = self.promotion_binding(parent, "staging")
            if current_policy["_RELEASE_CHECKS_REQUIRED"] == "true":
                require(parent_policy["_RELEASE_CHECKS_REQUIRED"] == "true",
                        "Production acceptance requires a checked staging deployment; bootstrap is insufficient")
            require((parent.get("substitutions") or {}).get("_RELEASE_MANIFEST") == self.env["_RELEASE_MANIFEST"],
                    "Production manifest differs from the tested staging manifest")
        refs = read_env(self.workspace / "image-refs.env")
        for key in IMAGES:
            require(refs.get(key) == data["refs"].get(key), f"{key} differs from the QA manifest")
        require(self.env.get("_WORKER_IMAGE_REF") == data["refs"]["WORKER_IMAGE_REF"],
                "Worker image differs from the QA manifest")
        print("Release provenance and source checkout verified")

    def freshness(self):
        """Promotion ordering uses Git ancestry, never later rebuild timestamps.

        A same-SHA retry is allowed: readiness/check failures must be recoverable
        without rebuilding artifacts. This rechecks the manifest on every run.
        """
        gcloud("container", "clusters", "get-credentials", self.env["_GKE_CLUSTER"],
               f"--location={self.env['_GKE_ZONE']}", f"--project={self.env['_TARGET_PROJECT']}")
        raw = run("kubectl", "get", "deployment", "g5-api", "-n", self.env["_HELM_NAMESPACE"],
                  "--ignore-not-found", "-o", "json")
        if raw:
            deployment = json.loads(raw)
            deployed = deployment.get("metadata", {}).get("annotations", {}).get("g5.g5labs.ai/deployed-commit", "")
            require(SHA.fullmatch(deployed), "Existing deployment lacks a trustworthy commit marker")
            if deployed != self.commit:
                require(self.is_forward(deployed),
                        "Selected commit would rewind/diverge from deployed schema; use forward recovery")
        (self.workspace / "deploy-control.env").write_text(
            f"export SKIP_DEPLOY=false\nexport ARTIFACT_COMMIT={self.commit}\nexport DEPLOY_FRESHNESS_HEAD={self.commit}\n")

    def is_forward(self, deployed):
        # Cloud Build uses a shallow checkout without git-fetch credentials.
        # Reuse the worker-image gate's read-only comparison: local history
        # where complete, then the configured connection's GitHub read token.
        runner = worker_images._default_runner
        shallow = worker_images.is_shallow_repo(".", runner)
        span = worker_gate.local_span(deployed, self.commit, ".", shallow, runner)
        if span is None:
            compare = worker_gate.GitHubCompare(
                project=self.project, region=self.region,
                connection=self.env["_GITHUB_CONNECTION"],
                repository=self.env["_GITHUB_CONNECTION_REPOSITORY"],
                owner=self.env["_GITHUB_OWNER"], repo=self.env["_GITHUB_REPO"], runner=runner,
            )
            span = compare(deployed, self.commit)
        return span.ancestor

    def promote(self):
        next_trigger = self.env.get("_NEXT_TRIGGER", "")
        if not next_trigger or read_env(self.workspace / "deploy-control.env").get("SKIP_DEPLOY") == "true":
            print("No downstream release queued")
            return
        expected = {"qa": "g5-promote-staging", "staging": "g5-promote-prod"}
        require(expected.get(self.environment) == next_trigger, "Unexpected downstream trigger")
        refs = read_env(self.workspace / "image-refs.env")
        refs.update(read_env(self.workspace / "worker-image.env"))
        refs = {key: refs.get(key, "") for key in (*IMAGES, "WORKER_IMAGE_REF")}
        validate_refs(refs, self.registry, self.artifact_project)
        if self.environment == "qa":
            require(run("git", "rev-parse", "HEAD") == self.commit, "QA checkout does not match candidate")
            current = self.describe(self.env["BUILD_ID"])
            self.producer_binding(current, current=True)
            require(build_source(current) == self.commit,
                    "QA source provenance does not match candidate")
            data = {"version": 1, "commit": self.commit, "project": self.project,
                    "artifact_project": self.artifact_project,
                    "qa_build_id": self.env["BUILD_ID"], "refs": refs}
            path = self.workspace / "app-release.json"
            path.write_text(json.dumps(data, sort_keys=True) + "\n")
            uri = f"gs://{self.env['_RELEASE_BUCKET']}/app-releases/{self.env['BUILD_ID']}.json"
            gcloud("storage", "cp", str(path), uri, "--if-generation-match=0")
            generation = gcloud("storage", "objects", "describe", uri, "--format=value(generation)")
            require(generation.isdigit(), "Release manifest generation was not returned")
            uri += "#" + generation
        else:
            # A staging retry forwards the same immutable QA manifest.
            data = self.manifest()
            require(data["refs"] == refs, "Staging resolved refs changed before promotion")
            uri = self.env["_RELEASE_MANIFEST"]
        substitutions = {"_RELEASE_MANIFEST": uri, "_SOURCE_BUILD_ID": self.env["BUILD_ID"]}
        substitutions.update({"_" + key: value for key, value in refs.items()})
        gcloud("builds", "triggers", "run", next_trigger, f"--project={self.project}",
               f"--region={self.region}", f"--sha={self.commit}",
               "--substitutions=" + ",".join(f"{key}={value}" for key, value in substitutions.items()))
        print(f"Queued {next_trigger} at {self.commit} using the same immutable artifacts")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("policy", "preflight", "freshness", "promote"))
    args = parser.parse_args(argv)
    try:
        getattr(Release(), args.action)()
    except (ValueError, KeyError, OSError, RuntimeError, subprocess.CalledProcessError) as exc:
        # subprocess repr can include command args; release operations never
        # carry secret payloads, but print only the operation's status anyway.
        message = str(exc) if not isinstance(exc, subprocess.CalledProcessError) else "External release operation failed"
        print(f"Release refused: {message}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
