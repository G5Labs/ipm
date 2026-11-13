"""QA Cloud Build -> Slack notifier contracts (ontology
g5.quality.delivery.failure-alerts and g5.quality.delivery.approval-alerts).

Pins the parts of infra/qa/delivery/slack-notifier.tf whose breakage is silent:
a non-PR trigger missing from the watched list never reports, a string status
literal or a wrapped Block Kit payload deploys a notifier that never posts,
and a config change without a new revision keeps the old filter running.

Text-based wiring checks (stdlib only, like the other CI contract scripts).
Actual Go rendering is covered by scripts/slack_notifier_template_test.go.
Run with: python3 scripts/test_qa_slack_notifier.py
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DELIVERY = ROOT / "infra/qa/delivery"


def top_level_blocks(text: str, kind: str, rtype: str) -> dict:
    """Map resource name -> body for each top-level `kind "rtype" "name" {`
    block; a block ends at the first line that is exactly `}`."""
    blocks = {}
    pattern = re.compile(rf'^{kind} "{re.escape(rtype)}" "(\w+)" \{{\n(.*?)^\}}\n', re.M | re.S)
    for match in pattern.finditer(text):
        blocks[match.group(1)] = match.group(2)
    return blocks


def locals_entry(text: str, name: str) -> str:
    match = re.search(rf"^  {name} = (.*?)^  \)\n", text, re.M | re.S)
    if not match:
        raise AssertionError(f"local {name} not found")
    return match.group(1)


class SlackNotifierContractTest(unittest.TestCase):
    def setUp(self):
        self.main = "\n".join(path.read_text() for path in DELIVERY.glob("*.tf"))
        self.notifier = (DELIVERY / "slack-notifier.tf").read_text()
        self.config = (DELIVERY / "slack-notifier-config.yaml.tftpl").read_text()
        self.triggers = top_level_blocks(self.main, "resource", "google_cloudbuild_trigger")
        self.watched = set(
            re.findall(
                r"google_cloudbuild_trigger\.(\w+)",
                locals_entry(self.notifier, "slack_notifier_trigger_names"),
            )
        )

    def test_every_non_pr_trigger_is_watched_and_no_pr_trigger_is(self):
        self.assertEqual(
            set(self.triggers),
            {"ci", "deploy", "worker_image", "bootstrap_builder", "terraform_plan", "terraform_apply", "promote", "terraform_promote", "release_train", "terraform_release_plan", "deploy_cutoff"},
            "trigger parsing must cover the current delivery surface",
        )
        pr = {name for name, body in self.triggers.items() if "pull_request {" in body}
        non_pr = set(self.triggers) - pr
        self.assertEqual(pr, {"ci", "terraform_plan", "terraform_release_plan"})
        self.assertEqual(self.watched, non_pr)

    def test_filter_matches_terminal_failures_by_enum(self):
        filter_line = next(line for line in self.config.splitlines() if line.strip().startswith("filter:"))
        failures, approvals = filter_line.split(" || ")
        self.assertIn("build.substitutions['TRIGGER_NAME'] in [${TRIGGER_NAMES_CEL}]", failures)
        statuses = re.findall(r"Build\.Status\.(\w+)", failures)
        self.assertEqual(statuses, ["FAILURE", "INTERNAL_ERROR", "TIMEOUT", "EXPIRED"])
        self.assertIn("build.substitutions['TRIGGER_NAME'] in [${APPROVAL_TRIGGER_NAMES_CEL}]", approvals)
        self.assertEqual(re.findall(r"Build\.Status\.(\w+)", approvals), ["WORKING"])
        self.assertNotRegex(filter_line, r"status\s*(?:==|in)\s*\[?\s*['\"]")

    def test_approval_notifications_follow_only_the_staging_gates(self):
        approved = locals_entry(self.notifier, "slack_approval_trigger_names")
        for resource in ("promote", "terraform_promote"):
            self.assertRegex(
                approved,
                rf'for environment, trigger in google_cloudbuild_trigger\.{resource} : trigger.name if environment == "staging"',
            )
            self.assertIn('approval_config { approval_required = each.key == "staging" }', self.triggers[resource])
        self.assertEqual(set(re.findall(r"google_cloudbuild_trigger\.(\w+)", approved)), {"promote", "terraform_promote"})
        self.assertIn("for name in local.slack_approval_trigger_names", self.notifier)

    def test_template_is_a_bare_block_array(self):
        content = self.config.split("content: |\n", 1)[1].split("\n  secrets:", 1)[0]
        self.assertTrue(content.strip().startswith("["), "Block Kit template must be a bare JSON array")
        self.assertNotIn('"blocks"', content)

    def test_config_change_rolls_a_new_revision(self):
        self.assertIn(
            '"g5/config-md5" = md5(google_storage_bucket_object.slack_notifier_config.content)',
            self.notifier,
        )

    def test_one_request_per_instance(self):
        self.assertIn("max_instance_request_concurrency = 1", self.notifier)

    def test_approvals_share_the_existing_notifier(self):
        self.assertEqual(set(top_level_blocks(self.notifier, "resource", "google_cloud_run_v2_service")), {"slack_notifier"})
        self.assertEqual(set(top_level_blocks(self.notifier, "resource", "google_pubsub_subscription")), {"slack_notifier"})

    def test_webhook_version_is_pinned_so_rotation_rolls_a_revision(self):
        self.assertRegex(self.notifier, r'slack_webhook_version = "[1-9][0-9]*"')
        self.assertIn("/versions/${local.slack_webhook_version}", self.notifier)
        self.assertNotIn("versions/latest", self.notifier)

    def test_webhook_value_stays_out_of_terraform(self):
        self.assertIn('data "google_secret_manager_secret" "slack_webhook"', self.notifier)
        for path in DELIVERY.glob("*.tf"):
            self.assertNotIn("google_secret_manager_secret_version", path.read_text(), path.name)

    def test_push_reaches_cloud_run_with_a_token(self):
        self.assertIn("roles/iam.serviceAccountTokenCreator", self.notifier)
        self.assertIn("@gcp-sa-pubsub.iam.gserviceaccount.com", self.notifier)
        self.assertIn('role     = "roles/run.invoker"', self.notifier)
        self.assertIn('name    = "cloud-builds"', self.notifier)


if __name__ == "__main__":
    unittest.main()
