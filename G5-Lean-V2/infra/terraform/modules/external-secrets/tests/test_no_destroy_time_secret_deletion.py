"""Contract test — a failed apply must never delete a working secret projection.

Regression guard for the 2026-08-10 dev outage (issue #1768): a planned Cloud
SQL replacement made `database_private_ip` unknown, flipping the g5-api-secrets
manifest md5 triggers to (known after apply) and forcing replacement of the
`api_external_secret` null_resources. Replacement is destroy-before-create and
Terraform is non-transactional: the destroy-time `kubectl delete externalsecret`
provisioners deleted the live ExternalSecrets (and, via the target's
`creationPolicy: Owner`, their rendered Kubernetes Secrets) in the `g5` and
`default` namespaces, then the protected Cloud SQL destroy aborted the apply
before the create-time provisioners could restore them — API/daemon pods
failed with `CreateContainerConfigError`.

Ontology terraform_secrets [D12]: the api/worker/platform-issue ExternalSecret
null_resources carry NO destroy-time provisioner. The create-time
`kubectl apply` is the only mutation (in-place and idempotent on replacement);
retiring a sync target is the explicit operator procedure in [I4]. The
`cluster_secret_store` destroy-time delete is retained deliberately — deleting
a ClusterSecretStore breaks refresh but does not garbage-collect
already-rendered target Secrets.

Like test_platform_issue_reporting_default.py this is a STATIC assertion test:
the module declares a helm provider whose plan reads live release state, so
`terraform test` cannot plan it offline. Executed in CI by the tf-test-static
step of cloudbuild-terraform.yaml.

Run with: python3 -m unittest test_no_destroy_time_secret_deletion
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

MODULE_MAIN = Path(__file__).resolve().parents[1] / "main.tf"

EXTERNAL_SECRET_RESOURCES = (
    "api_external_secret",
    "platform_issue_reporting_external_secret",
    "github_app_external_secret",
    "worker_external_secret",
)


def _balanced_body(source: str, open_brace_index: int) -> str:
    """Return the brace-balanced body starting just after `open_brace_index`."""
    depth = 1
    index = open_brace_index
    while index < len(source) and depth:
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
        index += 1
    return source[open_brace_index : index - 1]


def _resource_body(source: str, resource_type: str, name: str) -> str:
    """Return the brace-balanced body of a named resource block."""
    header = re.search(
        r'resource\s+"%s"\s+"%s"\s*\{' % (re.escape(resource_type), re.escape(name)),
        source,
    )
    if not header:
        raise AssertionError(f'resource "{resource_type}" "{name}" not found')
    return _balanced_body(source, header.end())


class NoDestroyTimeSecretDeletionTest(unittest.TestCase):
    def setUp(self):
        self.module = MODULE_MAIN.read_text(encoding="utf-8")

    def test_externalsecret_resources_have_no_destroy_provisioner(self):
        for name in EXTERNAL_SECRET_RESOURCES:
            body = _resource_body(self.module, "null_resource", name)
            self.assertNotRegex(
                body,
                r"when\s*=\s*destroy",
                f"null_resource.{name} must not carry a destroy-time provisioner: "
                "replacement is destroy-before-create, so a mid-apply failure "
                "would strand the cluster secret-less (issue #1768, "
                "terraform_secrets [D12])",
            )
            self.assertNotIn(
                "kubectl delete",
                body,
                f"null_resource.{name} must never delete its ExternalSecret",
            )

    def test_externalsecret_resources_keep_create_time_apply(self):
        # Removing the destroy hook must not remove the in-place apply that
        # creates/updates the projection.
        for name in EXTERNAL_SECRET_RESOURCES:
            body = _resource_body(self.module, "null_resource", name)
            self.assertIn(
                "kubectl apply -f -",
                body,
                f"null_resource.{name} must keep its create-time kubectl apply",
            )

    def test_no_externalsecret_delete_anywhere_in_module(self):
        # Comments may cite the forbidden command when explaining why it is
        # forbidden; only executable (non-comment) lines are asserted.
        code_lines = [
            line
            for line in self.module.splitlines()
            if not line.lstrip().startswith(("#", "//"))
        ]
        self.assertNotIn(
            "kubectl delete externalsecret",
            "\n".join(code_lines),
            "no destroy path in the module may delete an ExternalSecret "
            "(issue #1768, terraform_secrets [D12])",
        )

    def test_destroy_provisioners_confined_to_cluster_secret_store(self):
        # The retained ClusterSecretStore destroy delete is the only permitted
        # destroy-time provisioner; any new one is a re-introduction of the
        # incident pattern and needs an explicit ontology decision first.
        css_body = _resource_body(self.module, "null_resource", "cluster_secret_store")
        outside = self.module.replace(css_body, "")
        self.assertNotRegex(
            outside,
            r"when\s*=\s*destroy",
            "destroy-time provisioners are confined to "
            "null_resource.cluster_secret_store (terraform_secrets [D12])",
        )


if __name__ == "__main__":
    unittest.main()
