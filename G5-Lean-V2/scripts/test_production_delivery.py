"""Release topology and privileged identity boundaries, without cloud credentials."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ProductionDeliveryContract(unittest.TestCase):
    def test_worker_publisher_cannot_attach_service_accounts_or_administer_runtime(self):
        """Reject direct or shared grants that expand the image publisher's authority."""
        # Check every direct images project grant, not only the resource being
        # replaced, so reintroducing an equivalent broad binding is rejected.
        text = (ROOT / "infra/qa/foundation/main.tf").read_text()
        grants = re.findall(r'resource "google_project_iam_member" "[^"]+" \{(.*?)\n\}', text, re.S)
        image_grants = [body for body in grants if 'google_service_account.build["images"].email' in body]
        self.assertEqual(len(image_grants), 1)
        self.assertIn('role    = "projects/${local.project_id}/roles/g5LeanWorkerImagePublisher"', image_grants[0])
        self.assertNotIn('resource "google_project_iam_custom_role"', text)
        # Shared grants to all build accounts must not add another privilege path.
        shared = [body for body in grants if re.search(r'for_each\s*=\s*google_service_account.build\s', body)]
        self.assertEqual({re.search(r'role\s*=\s*"([^"]+)"', body)[1] for body in shared}, {
            "roles/logging.logWriter", "roles/cloudbuild.workerPoolUser", "roles/cloudbuild.builds.viewer",
        })

    def test_qa_and_target_apis_use_eso_without_direct_secret_reads(self):
        """Require hosted APIs to consume ESO projections without direct GSM grants."""
        for path in ("infra/qa/platform/main.tf", "infra/terraform/modules/release-platform/main.tf"):
            with self.subTest(path=path):
                text = (ROOT / path).read_text()
                secrets = re.search(r'module "secrets" \{(.*?)\n\}', text, re.S)[1]
                self.assertRegex(secrets, r'api_secret_ids\s*=\s*\[\]')

    def test_every_qa_bucket_enforces_private_uniform_access(self):
        """Check every QA bucket for uniform access and public-access prevention."""
        paths = list((ROOT / "infra/qa").glob("*/*.tf"))
        paths.append(ROOT / "infra/terraform/modules/gcs/main.tf")
        buckets = []
        for path in paths:
            for name, body in re.findall(r'resource "google_storage_bucket" "([^"]+)" \{(.*?)\n\}', path.read_text(), re.S):
                with self.subTest(path=path, bucket=name):
                    self.assertRegex(body, r'uniform_bucket_level_access\s*=\s*true')
                    self.assertRegex(body, r'public_access_prevention\s*=\s*"enforced"')
                    buckets.append((path, name))
        self.assertEqual(len(buckets), 5)

    def test_bootstrap_builder_publication_has_its_own_repository(self):
        """Keep the bootstrap builder's trigger and writer grant in its own registry."""
        config = (ROOT / "infra/ci/cloudbuild-bootstrap-builder.yaml").read_text()
        self.assertRegex(config, r'_AR_REPOSITORY:\s+g5-builders\n')
        delivery = (ROOT / "infra/qa/delivery/main.tf").read_text()
        trigger = re.search(r'resource "google_cloudbuild_trigger" "bootstrap_builder" \{(.*?)\n\}', delivery, re.S)[1]
        self.assertRegex(trigger, r'_AR_REPOSITORY\s*=\s*"g5-builders"')
        self.assertIn('data.google_service_account.build["images"].name', trigger)
        foundation = (ROOT / "infra/qa/foundation/main.tf").read_text()
        grants = re.findall(r'resource "google_artifact_registry_repository_iam_member" "[^"]+" \{(.*?)\n\}', foundation, re.S)
        direct = [body for body in grants if 'google_service_account.build["images"].email' in body]
        self.assertFalse(any('docker["g5"]' in body for body in direct))
        self.assertTrue(any('docker["g5-builders"]' in body for body in direct))

    def test_qa_destination_listing_is_bucket_scoped_to_images_only(self):
        """Limit upload discovery to the images identity's worker-artifact bucket."""
        text = (ROOT / "infra/qa/foundation/main.tf").read_text()
        grant = re.search(r'resource "google_storage_bucket_iam_member" "images_object_lister" \{(.*?)\n\}', text, re.S)[1]
        self.assertIn('google_storage_bucket.artifacts["g5-worker-images"].name', grant)
        self.assertIn('google_service_account.build["images"].email', grant)
        self.assertIn('"projects/${local.project_id}/roles/g5LeanArtifactObjectLister"', grant)
        self.assertNotIn('condition', grant)
        # A project policy can hide grants behind policy_data references and
        # would also replace unrelated bindings; foundation uses additive IAM.
        self.assertNotRegex(text, r'resource\s+"google_project_iam_policy"')
        project_grants = re.findall(r'resource\s+"google_project_iam_(?:member|binding)"\s+"[^"]+"\s*\{(.*?)\n\}', text, re.S)
        self.assertFalse(any("g5LeanArtifactObjectLister" in grant for grant in project_grants))

    def test_eso_has_no_project_payload_grant(self):
        """Require per-secret ESO grants before creating the cluster secret store."""
        eso = (ROOT / "infra/terraform/modules/external-secrets/main.tf").read_text()
        self.assertNotRegex(eso, r'resource\s+"google_project_iam_(?:member|binding|policy)"')
        self.assertIn('depends_on = [helm_release.external_secrets, google_secret_manager_secret_iam_member.eso_secret_accessor]', eso)

    def test_reader_validation_needs_no_new_cross_project_metadata_grants(self):
        """Keep artifact-reader validation local to the known target project IDs."""
        # The QA planner/runner have no target metadata grants. Validate inputs
        # locally; a target-project data read would break automated QA plans.
        delivery = (ROOT / "infra/qa/delivery/promotion.tf").read_text()
        self.assertNotIn('data "google_project"', delivery)
        variables = (ROOT / "infra/qa/delivery/variables.tf").read_text()
        self.assertIn('staging = { id = "g5lean-staging-1ccc", number = "237625147788" }', variables)
        self.assertIn('prod    = { id = "g5lean-prod-1ccc", number = "177811684595" }', variables)

    def test_artifact_selection_preserves_qa_resources_and_gates_writes(self):
        variables = (ROOT / "infra/qa/delivery/variables.tf").read_text()
        block = re.search(r'variable "artifact_project"\s*\{(.*?)(?=\nvariable|\Z)', variables, re.S).group(1)
        self.assertRegex(block, r'default\s*=\s*"g5-qa-1ccc"')
        self.assertIn('["g5-qa-1ccc", "g5lean-admin-1ccc"]', block)
        delivery = (ROOT / "infra/qa/delivery/promotion.tf").read_text()
        bucket = delivery.split('resource "google_storage_bucket" "releases" {', 1)[1].split('\n}', 1)[0]
        self.assertIn('"${local.project_id}-lean-releases"', bucket)
        self.assertNotIn('var.artifact_project', bucket)
        self.assertIn('release_bucket        = "${var.artifact_project}-lean-releases"', delivery)
        app = (ROOT / "infra/ci/cloudbuild-deploy.yaml").read_text()
        publisher = app.split('  - id: publish-base-tiers\n', 1)[1].split('\n  - id:', 1)[0]
        self.assertIn("waitFor: ['validate-release-policy']", publisher)
        guard = app.split('  - id: validate-release-policy\n', 1)[1].split('\n  - id:', 1)[0]
        self.assertIn("args: ['scripts/app_release.py', 'policy']", guard)
        self.assertIn("waitFor: ['validate-inputs']", guard)
        qa = (ROOT / "infra/ci/cloudbuild-terraform-qa.yaml").read_text()
        self.assertIn("TF_VAR_artifact_project=${_ARTIFACT_PROJECT}", qa)
        guard = qa.split('  - id: validate-inputs\n', 1)[1].split('\n  - id:', 1)[0]
        self.assertIn('scripts/terraform_release.py policy', guard)

    def test_terraform_uses_g5_main_runner_and_app_keeps_invoked_source(self):
        text = (ROOT / "infra/qa/delivery/promotion.tf").read_text()
        app, terraform = text.split('resource "google_cloudbuild_trigger" "terraform_promote"', 1)
        app_source = re.search(r"git_file_source\s*\{(.*?)\}", app, re.S).group(1)
        tf_source = re.search(r"git_file_source\s*\{(.*?)\}", terraform, re.S).group(1)
        self.assertNotIn("revision", app_source)
        self.assertIn('revision   = "refs/heads/main"', tf_source)
        self.assertIn('approval_required = each.key == "staging"', text)
        self.assertIn('each.key == "staging" && var.enable_promotion ? "g5-promote-prod" : ""', text)

    def test_weekly_preference_on_without_premature_activation(self):
        text = (ROOT / "infra/qa/delivery/variables.tf").read_text()
        for name in ("enable_release_bootstrap", "enable_promotion"):
            body = re.search(r'variable "' + name + r'"\s*\{(.*?)(?=\nvariable|\Z)', text, re.S).group(1)
            self.assertRegex(body, r"default\s*=\s*false")
        train = re.search(r'variable "enable_release_train"\s*\{(.*?)(?=\nvariable|\Z)', text, re.S).group(1)
        self.assertRegex(train, r"default\s*=\s*true")
        schedule = (ROOT / "infra/qa/delivery/release-train.tf").read_text()
        self.assertIn("var.enable_release_train && var.enable_promotion && length(local.release_targets) == 2", schedule)
        self.assertIn("schedule         = local.release_train_cron", schedule)
        self.assertIn("time_zone        = local.release_deploy_tz", schedule)
        self.assertIn("_CUTOFF_TZ  = local.release_cutoff_tz", schedule)
        self.assertIn("_CUTOFF_DAY = local.release_cutoff_day", schedule)
        self.assertIn('revision   = "refs/heads/main"', schedule)
        self.assertNotIn('roles/cloudbuild.builds.viewer', schedule)
        bootstrap = (ROOT / "infra/qa/bootstrap/main.tf").read_text()
        runner = re.search(r'resource "google_project_iam_member" "terraform_runner" \{(.*?)\n\}', bootstrap, re.S).group(1)
        planner = re.search(r'resource "google_project_iam_member" "terraform_planner" \{(.*?)\n\}', bootstrap, re.S).group(1)
        self.assertIn('"roles/cloudscheduler.admin"', runner)
        self.assertNotIn('"roles/cloudscheduler.admin"', planner)

    def test_example_acceptance_targets_provision_their_fixture_secret(self):
        text = (ROOT / "infra/qa/delivery/terraform.tfvars.example").read_text()
        example = "\n".join(line.removeprefix("# ") for line in text.splitlines() if line.startswith("# "))
        for environment in ("staging", "prod"):
            with self.subTest(environment=environment):
                target = re.search(r'^  ' + environment + r' = \{(.*?)^  \}', example, re.M | re.S).group(1)
                self.assertRegex(target, r'release_check_secret\s*=\s*"g5-release-checks"')
                settings = re.search(r'terraform_settings\s*=\s*\{(.*?)\}', target, re.S).group(1)
                self.assertRegex(settings, r'enable_release_check_fixture\s*=\s*true')

    def test_pr_identity_gets_no_release_permissions(self):
        for name in ("promotion.tf", "release-train.tf"):
            text = (ROOT / "infra/qa/delivery" / name).read_text()
            self.assertNotIn('build["ci"]', text)
            self.assertNotIn('terraform_plan.email', text)
        text = (ROOT / "infra/qa/delivery/promotion.tf").read_text()
        self.assertIn('role   = "roles/storage.objectCreator"', text)
        self.assertIn('versioning { enabled = true }', text)
        self.assertNotIn('roles/storage.admin', text)
        self.assertNotIn('roles/editor', text)

    def test_target_pr_plans_use_dedicated_identity_without_release_authority(self):
        """Keep target PR plans on their dedicated account without release grants."""
        # Terraform 1.9 mock plans leave resource-computed account names unknown.
        # Check the identity references here; root mock plans check trigger values.
        text = (ROOT / "infra/qa/delivery/terraform-plans.tf").read_text()
        trigger = text.split('resource "google_cloudbuild_trigger" "terraform_release_plan" {', 1)[1].split('\n}', 1)[0]
        self.assertRegex(trigger, r'service_account\s*=\s*google_service_account.release_plan\[0\].name')
        self.assertNotIn('google_service_account.release[', trigger)
        self.assertNotIn('terraform_plan.name', trigger)
        self.assertNotIn('git_file_source', trigger)
        self.assertNotIn('source_to_build', trigger)
        self.assertIn('depends_on   = [google_service_account.release]', text)
        self.assertEqual(re.findall(r'role\s*=\s*"([^"]+)"', text), ["roles/logging.logWriter"])
        # The trigger waits for the runner's exact attachment grant; that
        # reference does not grant the PR planner impersonation authority.
        self.assertNotRegex(text, r'resource\s+"google_service_account_iam_(?:member|binding|policy)"')
        self.assertNotIn('artifact_registry', text)

    def test_qa_self_apply_preserves_activation_config(self):
        text = (ROOT / "infra/ci/cloudbuild-terraform-qa.yaml").read_text()
        for name in ("enable_release_bootstrap", "enable_promotion", "enable_release_train", "promotion_targets"):
            self.assertIn(f"TF_VAR_{name}=", text)
        self.assertIn("waitFor: ['tf-roots']", text)
        delivery = (ROOT / "infra/qa/delivery/main.tf").read_text()
        self.assertIn("included_files     = local.release_queue_enabled ? [] : local.terraform_included_files", delivery)

    def test_one_central_identity_drives_application_and_terraform_releases(self):
        text = (ROOT / "infra/qa/delivery/promotion.tf").read_text()
        self.assertIn('{ central = "g5-lean-release" }', text)
        self.assertEqual(text.count('service_account = google_service_account.release["central"].name'), 2)
        self.assertNotIn('g5-lean-tf-release-', text)
        self.assertNotIn('"app-${each.key}"', text)
        self.assertNotIn('"tf-${each.key}"', text)
        self.assertIn('service_account_id = each.value.name', text)
        self.assertIn('member             = "serviceAccount:${each.value.email}"', text)
        runner = (ROOT / "scripts/terraform_release.py").read_text()
        self.assertNotIn('--impersonate-service-account=', runner)
        self.assertNotIn('-backend-config=impersonate_service_account=', runner)
        self.assertNotIn('g5-lean-terraform@{args.target}', runner)

    def test_central_identity_can_be_created_before_target_platforms(self):
        """Allow reviewed release identity setup while target queues remain inactive."""
        text = (ROOT / "infra/qa/delivery/promotion.tf").read_text()
        self.assertRegex(text, r'release_identities\s*=\s*var.qa_release_authority_reviewed \? \{ central = "g5-lean-release" \} : \{\}')
        self.assertRegex(text, r'resource "google_storage_bucket" "releases" \{\s*count\s*=\s*var.qa_release_authority_reviewed')
        self.assertIn('for_each    = local.release_targets', text)
        self.assertRegex(text, r'for_each\s*=\s*local.artifact_readers')
        self.assertIn('disabled    = !local.release_queue_enabled', text)
        self.assertIn('release_queue_enabled = var.enable_release_bootstrap || var.enable_promotion', text)

    def test_release_identities_require_previously_applied_isolation(self):
        text = (ROOT / "infra/qa/delivery/promotion.tf").read_text()
        self.assertIn('data "terraform_remote_state" "release_isolation"', text)
        self.assertIn('condition = var.qa_release_authority_reviewed && try(', text)
        self.assertIn('outputs.release_identity_isolation_ready, false', text)

    def test_bundle_release_reuses_g5_oci_and_native_state(self):
        text = (ROOT / "infra/ci/cloudbuild-terraform-release.yaml").read_text()
        helper = (ROOT / "scripts/terraform_release.py").read_text()
        self.assertIn('docker pull "$${BUNDLE_SOURCE}"', text)
        self.assertIn('docker cp "$${CONTAINER_ID}:/bundle/."', text)
        self.assertIn('TF_PLAN_REFRESH="false"', text)
        self.assertIn('-lock-timeout=10m', text)
        self.assertIn('--sha="${COMMIT_SHA}"', text)
        self.assertNotIn('release-locks/', text + helper)
        self.assertNotIn('no_destructive_actions', helper)
        self.assertNotIn('terraform-releases/', text + helper)
        self.assertNotIn('infrastructure_prerequisite', (ROOT / "scripts/app_release.py").read_text())


if __name__ == "__main__":
    unittest.main()
