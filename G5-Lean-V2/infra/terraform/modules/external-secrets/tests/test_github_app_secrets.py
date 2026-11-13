"""Contract test — the G5 GitHub App credentials reach g5-api safely.

The optional G5 GitHub App (external issues: organization bindings, issue
reads, comments) is configured by seven `G5_GITHUB_APP_*` settings. They are
delivered like platform issue reporting's credentials:

  * seven `g5-github-app-*` Secret Manager containers (modules/secrets), each
    bootstrapped with an inert single-space version by the metadata-only helper
    so the all-or-nothing ExternalSecret syncs before real values exist;
  * an isolated, optional `g5-github-app-secrets` ExternalSecret
    (modules/external-secrets) with a create-time apply only; and
  * an optional `envFrom` of that Secret shared by g5-api and g5-daemon
    (helm `g5.apiSecretEnvFrom`).

The environment names are checked against g5-api's own configuration so a
renamed setting cannot silently leave the App unconfigured.

Like the sibling tests this is a STATIC assertion test: the external-secrets
module declares a helm provider whose plan reads live release state.

Run with: python3 -m unittest test_github_app_secrets
"""
from __future__ import annotations

import re
import shutil
import subprocess
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[5]
ESO_MAIN = REPO / "infra" / "terraform" / "modules" / "external-secrets" / "main.tf"
SECRETS_MAIN = REPO / "infra" / "terraform" / "modules" / "secrets" / "main.tf"
HELM_CHART = REPO / "infra" / "helm" / "g5"
HELM_HELPERS = HELM_CHART / "templates" / "_helpers.tpl"
# The Deployments that run g5-api's binary and read its settings.
API_WORKLOADS = {
    "g5-api": HELM_CHART / "templates" / "deployment-api.yaml",
    "g5-daemon": HELM_CHART / "templates" / "deployment-daemon.yaml",
}
OPTIONAL_GITHUB_APP_REF = re.compile(
    r"- secretRef:\s*\n\s*name: g5-github-app-secrets\s*\n\s*optional: true"
)
API_CONFIG = REPO / "backend" / "g5-api-foundation" / "src" / "config.rs"

# Secret Manager container -> the g5-api environment setting it configures.
SETTINGS = {
    "g5-github-app-id": "G5_GITHUB_APP_ID",
    "g5-github-app-slug": "G5_GITHUB_APP_SLUG",
    "g5-github-app-private-key": "G5_GITHUB_APP_PRIVATE_KEY",
    "g5-github-app-oauth-client-id": "G5_GITHUB_APP_OAUTH_CLIENT_ID",
    "g5-github-app-oauth-client-secret": "G5_GITHUB_APP_OAUTH_CLIENT_SECRET",
    "g5-github-app-oauth-callback-url": "G5_GITHUB_APP_OAUTH_CALLBACK_URL",
    "g5-github-app-connection-state-secret": "G5_GITHUB_APP_CONNECTION_STATE_SECRET",
}


def _balanced_body(source: str, open_brace_index: int) -> str:
    depth = 1
    index = open_brace_index
    while index < len(source) and depth:
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
        index += 1
    return source[open_brace_index : index - 1]


def _block(source: str, pattern: str) -> str:
    header = re.search(pattern, source)
    if not header:
        raise AssertionError(f"block {pattern!r} not found")
    return _balanced_body(source, header.end())


class GithubAppSecretsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.eso = ESO_MAIN.read_text()
        cls.secrets = SECRETS_MAIN.read_text()
        cls.helpers = HELM_HELPERS.read_text()
        cls.config = API_CONFIG.read_text()

    def test_every_container_exists_and_is_bootstrapped_blank(self):
        seed = _block(self.secrets, r"github_app_default_seed\s*=\s*\{")
        for container in SETTINGS:
            self.assertRegex(
                self.secrets,
                r'"%s"\s*=\s*\{\s*description' % re.escape(container),
                f"{container} must be a managed Secret Manager container",
            )
            self.assertIn(f'"{container}"', seed, f"{container} must be bootstrapped")
        self.assertIn('secret_id => " "', seed, "the inert default is a single space")
        bootstrap = _block(
            self.secrets,
            r'resource\s+"terraform_data"\s+"github_app_default_bootstrap"\s*\{',
        )
        self.assertIn("for_each = local.github_app_default_seed", bootstrap)
        self.assertIn("scripts/bootstrap_platform_issue_default.py", bootstrap)
        # The value goes through the environment, never argv.
        self.assertIn("BOOTSTRAP_INERT_VALUE = each.value", bootstrap)
        self.assertNotIn("versions access", bootstrap)

    def test_external_secret_maps_each_container_to_its_setting(self):
        """Map every GitHub App secret through the manifest's shared IAM references."""
        manifest = _block(self.eso, r"github_app_external_secret_yamls\s*=\s*\{")
        refs = re.search(r"github_app_secret_remote_refs\s*=\s*\[(.*?)\n  \]", self.eso, re.S)[1]
        self.assertIn("data = local.github_app_secret_remote_refs", manifest)
        self.assertIn('name      = "g5-github-app-secrets"', manifest)
        for container, setting in SETTINGS.items():
            key = container.replace("-", "_")
            self.assertRegex(
                manifest,
                r'%s\s*=\s*"\{\{ \.%s \}\}"' % (setting, key),
                f"{setting} must render from {key}",
            )
            self.assertIn(
                f'{{ secretKey = "{key}", remoteRef = {{ key = "{container}" }} }}',
                refs,
            )
        remote_refs = re.findall(r'remoteRef = \{ key = "([^"]+)" \}', refs)
        self.assertEqual(sorted(remote_refs), sorted(SETTINGS))

    def test_external_secret_is_applied_only_at_create_time(self):
        body = _block(
            self.eso,
            r'resource\s+"null_resource"\s+"github_app_external_secret"\s*\{',
        )
        self.assertIn(
            "for_each = var.sync_api_secret ? toset([var.namespace]) : toset([])", body
        )
        self.assertIn("kubectl apply -f -", body)
        self.assertNotIn("when = destroy", body)
        self.assertNotIn("kubectl delete", body)

    def test_api_and_daemon_load_the_secret_optionally(self):
        env_from = _block(self.helpers, r'define "g5\.apiSecretEnvFrom" -\}\}')
        self.assertRegex(env_from, OPTIONAL_GITHUB_APP_REF)
        for workload, template in API_WORKLOADS.items():
            self.assertRegex(
                template.read_text(),
                r'envFrom:\s*\n\s*\{\{- include "g5\.apiSecretEnvFrom" \. \| nindent 12 \}\}',
                f"{workload} must take its envFrom from g5.apiSecretEnvFrom",
            )

    @unittest.skipUnless(shutil.which("helm"), "helm is not installed")
    def test_rendered_api_and_daemon_load_the_secret_optionally(self):
        try:
            rendered = subprocess.run(
                ["helm", "template", "g5", str(HELM_CHART)],
                check=True,
                capture_output=True,
                text=True,
                timeout=60,
            ).stdout
        except subprocess.TimeoutExpired:
            self.fail("helm template did not finish within 60 seconds")
        deployments = {}
        for document in rendered.split("\n---\n"):
            if not re.search(r"^kind: Deployment$", document, re.M):
                continue
            name = re.search(r"^metadata:\n(?:  .*\n)*?  name: (\S+)", document, re.M)
            deployments[name.group(1)] = document
        for workload in API_WORKLOADS:
            self.assertIn(workload, deployments, f"{workload} Deployment not rendered")
            env_from = re.search(
                r"^( +)envFrom:\n((?:\1 .*\n)+)", deployments[workload], re.M
            )
            self.assertIsNotNone(env_from, f"{workload} renders no envFrom")
            self.assertRegex(
                env_from.group(2),
                OPTIONAL_GITHUB_APP_REF,
                f"{workload} must load g5-github-app-secrets optionally",
            )

    def test_settings_match_the_api_configuration(self):
        for setting in SETTINGS.values():
            self.assertIn(
                f'env = "{setting}"',
                self.config,
                f"g5-api no longer reads {setting}",
            )


if __name__ == "__main__":
    unittest.main()
