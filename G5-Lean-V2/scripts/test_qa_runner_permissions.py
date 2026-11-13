"""QA Terraform attachment/dependency contracts complementary to mocked plans.

Computed new-account IDs are unknown during plan; inspect their source references
and the creation dependencies here. This is not a Google IAM evaluator.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parent.parent


def blocks(text, kind="resource"):
    """Index top-level HCL blocks by type and name for source contract checks."""
    return {
        (match[1], match[2]): match[3]
        for match in re.finditer(
            rf'^{kind} "([^"]+)" "([^"]+)" \{{\n(.*?)^\}}',
            text, re.MULTILINE | re.DOTALL,
        )
    }


def depends_on(body, dependency):
    """Check whether an HCL body names the exact dependency in its explicit list."""
    match = re.search(r"\bdepends_on\s*=\s*\[([^]]*)\]", body)
    return bool(
        match and dependency in [item.strip() for item in match[1].split(",")]
    )


class RunnerPermissionTests(unittest.TestCase):
    def test_every_delivery_attachment_has_a_matching_grant_and_dependency(self):
        """Require a named actAs grant before each managed delivery attachment."""
        delivery = ROOT / "infra/qa/delivery"
        text = "\n".join(path.read_text() for path in delivery.glob("*.tf"))
        resources = blocks(text)
        account_map = (delivery / "terraform-runner.tf").read_text()
        accounts = set(re.findall(
            r"^\s+\w+\s*=\s*((?:data\.)?google_service_account\.[^\s]+\.name)\s*$",
            account_map, re.MULTILINE,
        ))
        # The build map passes each data account's name through without rewriting
        # identities. Mock plans separately pin the allowed instance keys.
        self.assertIn(
            "{ for key, account in data.google_service_account.build : key => account.name }",
            account_map,
        )
        accounts.update(f'data.google_service_account.build["{key}"].name'
                        for key in ("ci", "deploy", "images"))
        central = 'google_service_account.release["central"].name'
        grant = resources[("google_service_account_iam_member", "qa_terraform_stage")]
        self.assertRegex(grant, r"service_account_id\s*=\s*" + re.escape(central))

        attached_types = {
            "google_cloudbuild_trigger", "google_cloud_run_v2_service",
            "google_cloud_scheduler_job", "google_pubsub_subscription",
        }
        checked = set()
        for (kind, name), body in resources.items():
            if kind not in attached_types:
                continue
            match = re.search(r"\bservice_account(?:_email)?\s*=\s*([^\s]+)", body)
            self.assertIsNotNone(match, f"Review the identity attachment for {kind}.{name}")
            account = re.sub(r"\.email$", ".name", match[1])
            with self.subTest(resource=f"{kind}.{name}"):
                self.assertIn(account, accounts | {central})
                dependency = "google_service_account_iam_member." + (
                    "qa_terraform_stage" if account == central else "terraform_attach"
                )
                self.assertTrue(depends_on(body, dependency), "actAs must precede attachment")
            checked.add((kind, name))
        self.assertEqual(len(checked), 15, "Review new delivery attachment types/resources")

    def test_database_version_creation_waits_for_payload_read_grants(self):
        """Order database accessor grants before the provider creates and reads versions."""
        text = (ROOT / "infra/qa/platform/main.tf").read_text()
        versions = {name: body for (kind, name), body in blocks(text).items()
                    if kind == "google_secret_manager_secret_version"}
        self.assertEqual(set(versions), {
            "db_password", "authz_writer_db_password",
            "runtime_system_db_password", "runtime_tenant_db_password",
        })
        for name, body in versions.items():
            with self.subTest(version=name):
                self.assertTrue(depends_on(
                    body, "google_secret_manager_secret_iam_member.terraform_runner_accessor"
                ), "The provider reads payloads immediately after creating a version")

    def test_node_attachment_waits_for_exact_node_grant(self):
        """Order the default node account's actAs grant before GKE provisioning."""
        text = (ROOT / "infra/qa/platform/main.tf").read_text()
        module = re.search(r'^module "gke" \{\n(.*?)^\}', text, re.MULTILINE | re.DOTALL)[1]
        self.assertTrue(depends_on(module, "google_service_account_iam_member.terraform_node_act_as"))


if __name__ == "__main__":
    unittest.main()
