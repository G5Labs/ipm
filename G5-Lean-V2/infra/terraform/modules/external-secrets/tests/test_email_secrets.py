"""Contract test — outbound email settings reach g5-api safely.

Outbound email (`.g5/ontology/software_architecture/email.g5.md`) is configured
by `G5_EMAIL_TRANSPORT`, `G5_EMAIL_FROM_DOMAIN` and `G5_EMAIL_RESEND_API_KEY`. They are
delivered like the G5 GitHub App credentials:

  * three `g5-email-*` Secret Manager containers (modules/secrets), bootstrapped
    with an inert "disabled" transport and blank sender/key so the
    all-or-nothing ExternalSecret syncs and g5-api starts with email off;
  * an isolated, optional `g5-email-secrets` ExternalSecret
    (modules/external-secrets) with a create-time apply only; and
  * an optional `envFrom` of that Secret shared by g5-api and g5-daemon
    (helm `g5.apiSecretEnvFrom`): the API records requests, the daemon sends.

Static assertions, like the sibling tests.

Run with: python3 -m unittest test_email_secrets
"""
from __future__ import annotations

import re
import unittest

from test_github_app_secrets import (
    API_CONFIG,
    API_WORKLOADS,
    ESO_MAIN,
    HELM_HELPERS,
    SECRETS_MAIN,
    _block,
)

OPTIONAL_EMAIL_REF = re.compile(
    r"- secretRef:\s*\n\s*name: g5-email-secrets\s*\n\s*optional: true"
)

# Secret Manager container -> (g5-api setting, inert bootstrap value).
SETTINGS = {
    "g5-email-transport": ("G5_EMAIL_TRANSPORT", "disabled"),
    "g5-email-from-domain": ("G5_EMAIL_FROM_DOMAIN", " "),
    "g5-email-resend-api-key": ("G5_EMAIL_RESEND_API_KEY", " "),
}


class EmailSecretsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.eso = ESO_MAIN.read_text()
        cls.secrets = SECRETS_MAIN.read_text()
        cls.helpers = HELM_HELPERS.read_text()
        cls.config = API_CONFIG.read_text()

    def test_every_container_exists_and_is_bootstrapped_inert(self):
        seed = _block(self.secrets, r"email_default_seed\s*=\s*\{")
        for container, (_, inert) in SETTINGS.items():
            self.assertRegex(
                self.secrets,
                r'"%s"\s*=\s*\{\s*description' % re.escape(container),
                f"{container} must be a managed Secret Manager container",
            )
            self.assertRegex(
                seed,
                r'"%s"\s*=\s*"%s"' % (re.escape(container), re.escape(inert)),
                f"{container} must be bootstrapped as {inert!r}",
            )
        bootstrap = _block(
            self.secrets,
            r'resource\s+"terraform_data"\s+"email_default_bootstrap"\s*\{',
        )
        self.assertIn("for_each = local.email_default_seed", bootstrap)
        self.assertIn("scripts/bootstrap_platform_issue_default.py", bootstrap)
        self.assertIn("BOOTSTRAP_INERT_VALUE = each.value", bootstrap)
        self.assertNotIn("versions access", bootstrap)

    def test_external_secret_maps_each_container_to_its_setting(self):
        """Keep email projection settings aligned with the shared IAM reference list."""
        manifest = _block(self.eso, r"email_external_secret_yamls\s*=\s*\{")
        refs = re.search(r"email_secret_remote_refs\s*=\s*\[(.*?)\n  \]", self.eso, re.S)[1]
        self.assertIn("data = local.email_secret_remote_refs", manifest)
        self.assertIn('name      = "g5-email-secrets"', manifest)
        for container, (setting, _) in SETTINGS.items():
            key = container.replace("-", "_")
            self.assertRegex(manifest, r'%s\s*=\s*"\{\{ \.%s \}\}"' % (setting, key))
            self.assertIn(
                f'{{ secretKey = "{key}", remoteRef = {{ key = "{container}" }} }}',
                refs,
            )
        remote_refs = re.findall(r'remoteRef = \{ key = "([^"]+)" \}', refs)
        self.assertEqual(sorted(remote_refs), sorted(SETTINGS))

    def test_external_secret_is_applied_only_at_create_time(self):
        body = _block(
            self.eso, r'resource\s+"null_resource"\s+"email_external_secret"\s*\{'
        )
        self.assertIn(
            "for_each = var.sync_api_secret ? toset([var.namespace]) : toset([])", body
        )
        self.assertIn("kubectl apply -f -", body)
        self.assertNotIn("when = destroy", body)
        self.assertNotIn("kubectl delete", body)

    def test_api_and_daemon_load_the_secret_optionally(self):
        env_from = _block(self.helpers, r'define "g5\.apiSecretEnvFrom" -\}\}')
        self.assertRegex(env_from, OPTIONAL_EMAIL_REF)
        for workload, template in API_WORKLOADS.items():
            self.assertRegex(
                template.read_text(),
                r'envFrom:\s*\n\s*\{\{- include "g5\.apiSecretEnvFrom" \. \| nindent 12 \}\}',
                f"{workload} must take its envFrom from g5.apiSecretEnvFrom",
            )

    def test_settings_match_the_api_configuration(self):
        for setting, inert in SETTINGS.values():
            self.assertIn(f'env = "{setting}"', self.config)
        # The inert transport must be a value g5-api accepts and treats as off.
        self.assertIn("Disabled,", self.config)
        self.assertIn("default_value_t = EmailTransportKind::Disabled", self.config)


if __name__ == "__main__":
    unittest.main()
