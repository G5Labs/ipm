"""Render the real release overlays; requires Helm 3 and PyYAML, never a cluster."""

import os
from pathlib import Path
import shutil
import subprocess
from string import Template
import textwrap
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[2]
HELM = os.environ.get("HELM", shutil.which("helm") or "helm")
API_IMAGE = "us-central1-docker.pkg.dev/g5-qa-1ccc/g5/api@sha256:" + "a" * 64
EDGE_IMAGE = "us-central1-docker.pkg.dev/g5-qa-1ccc/g5/edge@sha256:" + "b" * 64
WORKER_IMAGE = "projects/g5-qa-1ccc/global/images/g5-worker-test"


def render(environment, include_worker=True, extra_args=()):
    command = [
        HELM, "template", "g5", "infra/helm/g5",
        "-f", f"infra/helm/g5/values-{environment}.yaml",
        "-f", "infra/helm/g5/values-control-plane.yaml",
    ]
    values = {
        "global.projectId": f"g5lean-{environment}-1ccc",
        "global.clusterName": f"g5-lean-{environment}",
        "filestore.ip": "10.60.0.10",
        "api.image.reference": API_IMAGE,
        "daemon.image.reference": API_IMAGE,
        "migration.image.reference": API_IMAGE,
        "edge.image.reference": EDGE_IMAGE,
    }
    if include_worker:
        values["api.env.G5_WORKER_IMAGE"] = WORKER_IMAGE
    for key, value in values.items():
        command.extend(["--set-string", f"{key}={value}"])
    command.extend(extra_args)
    return subprocess.run(command, cwd=ROOT, capture_output=True, text=True)


def release_ingress_args(environment, *, enabled=True, app="", preview=""):
    # Execute the actual pipeline argument assembly, then give those arguments
    # to real Helm. No fake renderer and no gcloud/kubectl operation is involved.
    pipeline = (ROOT / "infra/ci/cloudbuild-deploy.yaml").read_text()
    step = next(item for item in yaml.safe_load(pipeline)["steps"] if item["id"] == "helm-deploy")
    substitutions = {"_EXTERNAL_ACCESS_ENABLED": str(enabled).lower(),
                     "_APP_HOSTNAME": app, "_PREVIEW_HOSTNAME": preview,
                     "_INGRESS_STATIC_IP_NAME": f"g5-lean-{environment}-ip",
                     "_CERTIFICATE_MAP_NAME": f"g5-lean-{environment}-ip-cert-map"}
    # Use the actual step mappings: fixtures must not supply a missing binding.
    environment_values = dict(Template(value).substitute(substitutions).split("=", 1)
                              for value in step.get("env", []))
    fragment = pipeline.split("        # Follow G5's existing ingress configuration.", 1)[1]
    fragment = fragment.split("        # Filestore IP", 1)[0]
    fragment = textwrap.dedent("        # Follow G5's existing ingress configuration." + fragment)
    fragment = fragment.replace("${_ENVIRONMENT}", environment).replace("$$", "$")
    result = subprocess.run(["bash", "-c", "set -euo pipefail\nHELM_ARGS=()\n" + fragment +
                             '\nif (( ${#HELM_ARGS[@]} )); then printf "%s\\0" "${HELM_ARGS[@]}"; fi'],
        env={"PATH": os.defpath, **environment_values},
        capture_output=True, text=True, check=True)
    return result.stdout.rstrip("\0").split("\0") if result.stdout else []


class ReleaseEnvironmentRenderingTests(unittest.TestCase):
    def test_managed_workspace_capacity_reaches_api_environment(self):
        for environment in ("qa", "staging", "prod"):
            with self.subTest(environment=environment):
                result = render(environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                api = next(item for item in yaml.safe_load_all(result.stdout)
                           if item and item["kind"] == "Deployment" and item["metadata"]["name"] == "g5-api")
                values = {item["name"]: item.get("value")
                          for item in api["spec"]["template"]["spec"]["containers"][0]["env"]}
                self.assertNotIn("G5_WORKER_SCRATCH_DISK_MIB", values)

    def test_pipeline_public_arguments_render_existing_g5_ingress(self):
        for environment, app in (("staging", "lean.g5labs.ai"), ("prod", "app.g5labs.ai"),
                                 ("staging", "next.lean.g5labs.ai")):
            preview = "preview." + app
            with self.subTest(environment=environment, app=app):
                result = render(environment, extra_args=release_ingress_args(environment, app=app, preview=preview))
                self.assertEqual(result.returncode, 0, result.stderr)
                documents = [item for item in yaml.safe_load_all(result.stdout) if item]
                ingress = next(item for item in documents if item["kind"] == "Ingress")
                annotations = ingress["metadata"]["annotations"]
                self.assertEqual(annotations["kubernetes.io/ingress.global-static-ip-name"], f"g5-lean-{environment}-ip")
                self.assertEqual(annotations["networking.gke.io/managed-certificates"], f"g5-{environment}-cert")
                self.assertEqual(annotations["networking.gke.io/certmap"], f"g5-lean-{environment}-ip-cert-map")
                rules = {rule["host"]: rule["http"]["paths"] for rule in ingress["spec"]["rules"]}
                self.assertEqual(set(rules), {app, "*." + preview})
                paths = {path["path"]: path["backend"]["service"]["name"] for path in rules[app]}
                self.assertEqual(paths["/api/*"], "g5-api")
                self.assertEqual(paths["/git/*"], "g5-api")
                self.assertEqual(paths["/ws"], "g5-api")
                self.assertEqual(paths["/*"], "g5-edge")
                certificate = next(item for item in documents if item["kind"] == "ManagedCertificate")
                self.assertEqual(certificate["metadata"]["name"], f"g5-{environment}-cert")
                self.assertEqual(certificate["spec"]["domains"], [app])
                api = next(item for item in documents if item["kind"] == "Deployment" and item["metadata"]["name"] == "g5-api")
                values = {item["name"]: item.get("value") for item in api["spec"]["template"]["spec"]["containers"][0]["env"]}
                self.assertEqual(values["G5_PREVIEW_HOST_SUFFIX"], preview)

    def test_pipeline_preserves_private_and_qa_values(self):
        for environment in ("staging", "prod"):
            self.assertEqual(release_ingress_args(environment, enabled=False), [])
        self.assertEqual(release_ingress_args("qa", enabled=True, app="qa.g5labs.ai", preview="preview.qa.g5labs.ai"), [])

    def test_explicit_qa_hosts_ignore_release_fallback(self):
        result = render("qa", extra_args=["--set-string", "ingress.host=unused.example.com"])
        self.assertEqual(result.returncode, 0, result.stderr)
        ingress = next(item for item in yaml.safe_load_all(result.stdout) if item and item["kind"] == "Ingress")
        hosts = {rule["host"] for rule in ingress["spec"]["rules"]}
        self.assertIn("qa.g5labs.ai", hosts)
        self.assertNotIn("unused.example.com", hosts)

    def test_public_access_is_inactive_and_artifacts_are_exact(self):
        for environment in ("staging", "prod"):
            with self.subTest(environment=environment):
                result = render(environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                documents = [item for item in yaml.safe_load_all(result.stdout) if item]
                kinds = {item["kind"] for item in documents}
                self.assertNotIn("Ingress", kinds)
                self.assertNotIn("ManagedCertificate", kinds)
                deployments = {
                    item["metadata"]["name"]: item
                    for item in documents if item["kind"] == "Deployment"
                }
                api = deployments["g5-api"]
                self.assertEqual(api["spec"]["replicas"], 1)
                container = api["spec"]["template"]["spec"]["containers"][0]
                self.assertEqual(container["image"], API_IMAGE)
                environment_values = {
                    item["name"]: item.get("value") for item in container["env"]
                }
                self.assertEqual(environment_values["G5_WORKER_IMAGE"], WORKER_IMAGE)
                self.assertEqual(environment_values["G5_WORKER_ALLOW_SSH"], "0")
                self.assertEqual(environment_values["G5_E2E_AUTH"], "false")
                self.assertNotIn("G5_AUTH_ALLOWLIST_STRICT", environment_values)
                # The login allowlist is retired; admission is invitation or membership.
                self.assertNotIn("G5_AUTH_ALLOWED_EMAILS", environment_values)
                self.assertNotIn("G5_PREVIEW_HOST_SUFFIX", environment_values)
                self.assertNotIn("G5_WORKER_SCCACHE_BUCKET", environment_values)
                self.assertEqual(
                    deployments["g5-edge"]["spec"]["template"]["spec"]["containers"][0]["image"],
                    EDGE_IMAGE,
                )

    def test_worker_pin_is_required(self):
        for environment in ("staging", "prod"):
            with self.subTest(environment=environment):
                result = render(environment, include_worker=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("DEPLOY-INJECTED", result.stderr)


if __name__ == "__main__":
    unittest.main()
