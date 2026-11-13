"""Offline QA planner permission contract, complementary to Terraform mock plan.

This evaluates only the deliberately small CEL subset used by the checked-in
binding. It does NOT emulate Google IAM inheritance/authorization or prove live
GCS lock operations; those remain a preactivation acceptance requirement.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parent.parent
SOURCE = (ROOT / "infra/qa/bootstrap/main.tf").read_text()


def block(kind, name):
    return re.search(r'^resource "' + kind + '" "' + name + r'" \{\n(.*?)^\}',
                     SOURCE, re.MULTILINE | re.DOTALL)[1]


def lock_condition(resource_type, resource_name):
    text = block("google_storage_bucket_iam_member", "terraform_plan_state")
    expression = re.search(r'^\s*expression\s*=\s*"(.*)"$', text, re.MULTILINE)[1]
    bucket = re.search(r'^\s*name\s*=\s*"([^"]+)"', block("google_storage_bucket", "state"), re.MULTILINE)[1]
    expression = expression.replace("${google_storage_bucket.state.name}", bucket)
    results = []
    for term in expression.split(" && "):
        if match := re.fullmatch(r"resource.type == '([^']+)'", term):
            results.append(resource_type == match[1])
        elif match := re.fullmatch(r"resource.name.(startsWith|endsWith)\('([^']+)'\)", term):
            operation = resource_name.startswith if match[1] == "startsWith" else resource_name.endswith
            results.append(operation(match[2]))
        else:
            raise AssertionError(f"Unexpected condition syntax needs review: {term}")
    return all(results)


class PlannerPermissionTests(unittest.TestCase):
    def test_planner_grants_are_read_only_or_lock_conditional(self):
        resources = re.findall(r'^resource "google_storage_bucket_iam_member" "([^"]+)" \{\n(.*?)^\}',
                               SOURCE, re.MULTILINE | re.DOTALL)
        planner = [(name, body) for name, body in resources
                   if 'member = "serviceAccount:${google_service_account.terraform_plan.email}"' in body]
        self.assertEqual({name for name, _ in planner}, {"terraform_plan_state_read", "terraform_plan_state"})
        for name, body in planner:
            self.assertRegex(body, r'bucket\s*=\s*google_storage_bucket.state.name')
            role = re.search(r'role\s*=\s*"([^"]+)"', body)[1]
            if role != "roles/storage.objectViewer":
                self.assertEqual(role, "roles/storage.objectAdmin")
                self.assertIn("condition {", body)
                self.assertIn("resource.name.endsWith('.tflock')", body)
        trusted = block("google_storage_bucket_iam_member", "terraform_state")
        self.assertIn('member = "serviceAccount:${google_service_account.terraform.email}"', trusted)
        self.assertNotIn("condition {", trusted)

    def test_existing_backend_lock_objects_are_writable(self):
        # Terraform 1.9.8 GCS backend_state.go uses prefix/workspace.tflock.
        prefix = "projects/_/buckets/g5-qa-1ccc-lean-tfstate/objects/"
        for root in ("foundation", "delivery", "platform", "addons"):
            for workspace in ("default", "release-review"):
                with self.subTest(root=root, workspace=workspace):
                    self.assertTrue(lock_condition("storage.googleapis.com/Object", f"{prefix}{root}/{workspace}.tflock"))

    def test_state_overwrite_delete_and_foreign_bucket_objects_are_denied(self):
        prefix = "projects/_/buckets/g5-qa-1ccc-lean-tfstate/objects/"
        for suffix in ("delivery/default.tfstate", "delivery/default.tfstate.backup", "delivery/default.tflock/child",
                       "delivery/default.tflock.json", "delivery/default.TFLOCK", "delivery/default.tflock%00.tfstate"):
            with self.subTest(suffix=suffix):
                self.assertFalse(lock_condition("storage.googleapis.com/Object", prefix + suffix))
        for name in (prefix.replace("g5-qa-1ccc-lean-tfstate", "g5lean-prod-1ccc-lean-tfstate") + "delivery/default.tflock",
                     prefix.replace("g5-qa-1ccc-lean-tfstate", "g5-qa-1ccc-lean-tfstate-evil") + "delivery/default.tflock",
                     "projects/_/buckets/g5-qa-1ccc-lean-tfstate"):
            with self.subTest(name=name):
                self.assertFalse(lock_condition("storage.googleapis.com/Object", name))
        self.assertFalse(lock_condition("storage.googleapis.com/Bucket", prefix + "delivery/default.tflock"))


if __name__ == "__main__":
    unittest.main()
