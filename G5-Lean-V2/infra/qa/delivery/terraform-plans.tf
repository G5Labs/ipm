# PR-controlled target plans must not run as the trusted release account or
# broaden the existing QA planner. Create this identity before target bootstrap;
# target grants live in release-bootstrap, not the QA deployment root.
resource "google_service_account" "release_plan" {
  count        = var.qa_release_authority_reviewed ? 1 : 0
  project      = local.project_id
  account_id   = "g5-lean-release-plan"
  display_name = "G5 Lean staging and production Terraform planner"
  depends_on   = [google_service_account.release]
}

resource "google_project_iam_member" "release_plan_logs" {
  count   = var.qa_release_authority_reviewed ? 1 : 0
  project = local.project_id
  role    = "roles/logging.logWriter"
  member  = "serviceAccount:${google_service_account.release_plan[0].email}"
}

# As in G5, every configured target gets a PR plan independently of the release
# queue. Populate targets only after a trusted operator initializes both states
# and installs the planner grants; a planner cannot create backend state.
resource "google_cloudbuild_trigger" "terraform_release_plan" {
  for_each    = local.release_targets
  project     = local.project_id
  location    = local.region
  name        = "g5-terraform-plan-${each.key}"
  description = "Plan the ${each.key} infrastructure from pull request source without applying or promoting"
  repository_event_config {
    repository = local.repository
    pull_request {
      branch          = "^main$"
      comment_control = "COMMENTS_ENABLED_FOR_EXTERNAL_CONTRIBUTORS_ONLY"
    }
  }
  disabled        = false
  filename        = "infra/ci/cloudbuild-terraform-release.yaml"
  service_account = google_service_account.release_plan[0].name
  approval_config { approval_required = false }
  substitutions = {
    _TF_ACTION            = "plan"
    _TF_WORKSPACE         = each.key
    _TF_BUNDLE_MODE       = "source"
    _TF_BUNDLE_REF        = ""
    _AR_PROJECT           = var.artifact_project
    _AR_REPOSITORY        = "g5"
    _REGION               = each.value.region
    _STATE_BUCKET         = "${each.value.project_id}-lean-tfstate"
    _INFRA_DIR            = "infra"
    _NEXT_TRIGGER         = ""
    _NEXT_TRIGGER_PROJECT = ""
    _TF_CONFIG_JSON       = jsonencode({ project_id = each.value.project_id, region = each.value.region, settings = each.value.terraform_settings })
  }
  included_files     = local.terraform_included_files
  include_build_logs = "INCLUDE_BUILD_LOGS_WITH_STATUS"
  depends_on         = [google_service_account_iam_member.terraform_attach, google_project_iam_member.release_plan_logs]
}

output "terraform_release_plan_triggers" {
  value = { for env, trigger in google_cloudbuild_trigger.terraform_release_plan : env => trigger.trigger_id }
}
