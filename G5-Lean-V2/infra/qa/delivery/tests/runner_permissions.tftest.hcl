# Plan-only coverage: generated account references/order are checked in
# scripts/test_qa_runner_permissions.py, without applying protected resources.
mock_provider "google" {
  mock_data "google_project" { defaults = { number = "89784110315" } }
}
override_data {
  target = data.terraform_remote_state.foundation
  values = { outputs = { ci_pool = "projects/g5-qa-1ccc/locations/us-central1/workerPools/g5-ci-pool" } }
}
override_data {
  target = data.google_service_account.build["ci"]
  values = { name = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-ci@g5-qa-1ccc.iam.gserviceaccount.com", email = "g5-lean-ci@g5-qa-1ccc.iam.gserviceaccount.com" }
}
override_data {
  target = data.google_service_account.build["deploy"]
  values = { name = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-deploy@g5-qa-1ccc.iam.gserviceaccount.com", email = "g5-lean-deploy@g5-qa-1ccc.iam.gserviceaccount.com" }
}
override_data {
  target = data.google_service_account.build["images"]
  values = { name = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-images@g5-qa-1ccc.iam.gserviceaccount.com", email = "g5-lean-images@g5-qa-1ccc.iam.gserviceaccount.com" }
}
override_data {
  target = data.google_service_account.terraform
  values = { name = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com", email = "g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com" }
}
override_data {
  target = data.google_service_account.terraform_plan
  values = { name = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-terraform-plan@g5-qa-1ccc.iam.gserviceaccount.com", email = "g5-lean-terraform-plan@g5-qa-1ccc.iam.gserviceaccount.com" }
}
run "qa_attachments_without_release_authority" {
  command = plan
  variables { enable_application_triggers = true }
  assert {
    condition = (
      length(google_project_iam_member.qa_deploy_chain) == 1 &&
      google_project_iam_member.qa_deploy_chain[0].project == "g5-qa-1ccc" &&
      google_project_iam_member.qa_deploy_chain[0].role == "roles/cloudbuild.builds.editor" &&
      google_project_iam_member.qa_deploy_chain[0].member == "serviceAccount:g5-lean-deploy@g5-qa-1ccc.iam.gserviceaccount.com" &&
      length(google_service_account_iam_member.qa_deploy_worker_image) == 1 &&
      google_service_account_iam_member.qa_deploy_worker_image[0].service_account_id == "projects/g5-qa-1ccc/serviceAccounts/g5-lean-images@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_service_account_iam_member.qa_deploy_worker_image[0].role == "roles/iam.serviceAccountUser" &&
      google_service_account_iam_member.qa_deploy_worker_image[0].member == "serviceAccount:g5-lean-deploy@g5-qa-1ccc.iam.gserviceaccount.com"
    )
    error_message = "QA must be able to recover a skipped worker bake with attachment scoped to the images account, independently of promotion targets."
  }
  assert {
    condition = (
      contains(google_cloudbuild_trigger.terraform_plan.included_files, "scripts/terraform-qa-freshness.sh") &&
      contains(google_cloudbuild_trigger.terraform_apply.included_files, "scripts/terraform-qa-freshness.sh")
    )
    error_message = "Changes to the shared QA freshness guard must trigger both the PR plan and main apply pipelines."
  }
  assert {
    condition = (
      toset(keys(google_service_account_iam_member.terraform_attach)) == toset(["ci", "deploy", "images", "terraform", "terraform_plan", "slack_notifier", "slack_notifier_invoker"]) &&
      alltrue([for grant in google_service_account_iam_member.terraform_attach :
        grant.role == "roles/iam.serviceAccountUser" &&
        grant.member == "serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com"
      ]) && length(google_service_account_iam_member.qa_terraform_stage) == 0
    )
    error_message = "QA attaches only named build/planner/notifier identities before release authority exists."
  }
  assert {
    condition = alltrue([for account in concat(
      [for trigger in google_cloudbuild_trigger.ci : trigger.service_account],
      [for trigger in google_cloudbuild_trigger.deploy : trigger.service_account],
      [for trigger in google_cloudbuild_trigger.worker_image : trigger.service_account],
      [google_cloudbuild_trigger.bootstrap_builder.service_account,
        google_cloudbuild_trigger.terraform_plan.service_account,
      google_cloudbuild_trigger.terraform_apply.service_account]
    ) : contains([for grant in google_service_account_iam_member.terraform_attach : grant.service_account_id], account)])
    error_message = "Every active QA build and notifier attachment must have an exact account-scoped grant."
  }
}
run "release_identity_before_targets" {
  command = plan
  variables { qa_release_authority_reviewed = true }
  override_data {
    target = data.terraform_remote_state.release_isolation[0]
    values = { outputs = { release_identity_isolation_ready = true } }
  }
  assert {
    condition = (
      length(google_project_iam_member.qa_deploy_chain) == 0 &&
      length(google_service_account_iam_member.qa_deploy_worker_image) == 0
    )
    error_message = "Release identity bootstrap without application triggers must not add worker-bake authority."
  }
  assert {
    condition = (
      toset(keys(google_service_account_iam_member.terraform_attach)) == toset(["ci", "deploy", "images", "terraform", "terraform_plan", "slack_notifier", "slack_notifier_invoker", "release_plan"]) &&
      length(google_service_account_iam_member.qa_terraform_stage) == 1 &&
      google_service_account.release["central"].account_id == "g5-lean-release" &&
      google_service_account_iam_member.qa_terraform_stage[0].member == "serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com"
    )
    error_message = "Release attachments need exact grants and must preserve the single existing central-release binding."
  }
}
run "release_train_all_attachments" {
  command = plan
  variables {
    enable_application_triggers = true
    enable_promotion            = true
    enable_release_train        = true

    qa_release_authority_reviewed = true
    promotion_targets = {
      staging = {
        project_id             = "g5lean-staging-1ccc"
        cluster                = "g5-lean-staging"
        filestore_instance     = "g5-lean-staging-repos"
        filestore_zone         = "us-central1-a"
        release_check_base_url = "https://staging.example.test"
        release_check_secret   = "g5-release-check"
        artifact_readers = [
          "serviceAccount:g5-lean-staging-api@g5lean-staging-1ccc.iam.gserviceaccount.com",
        ]
      }
      prod = {
        project_id             = "g5lean-prod-1ccc"
        cluster                = "g5-lean-prod"
        filestore_instance     = "g5-lean-prod-repos"
        filestore_zone         = "us-central1-a"
        release_check_base_url = "https://app.example.test"
        release_check_secret   = "g5-release-check"
        artifact_readers = [
          "serviceAccount:g5-lean-prod-api@g5lean-prod-1ccc.iam.gserviceaccount.com",
        ]
      }
    }
  }
  override_data {
    target = data.terraform_remote_state.release_isolation[0]
    values = { outputs = { release_identity_isolation_ready = true } }
  }
  assert {
    condition = (
      length(google_project_iam_member.qa_deploy_chain) == 1 &&
      length(google_service_account_iam_member.qa_deploy_worker_image) == 1 &&
      google_service_account_iam_member.qa_deploy_worker_image[0].service_account_id == "projects/g5-qa-1ccc/serviceAccounts/g5-lean-images@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_service_account_iam_member.qa_deploy_worker_image[0].member == "serviceAccount:g5-lean-deploy@g5-qa-1ccc.iam.gserviceaccount.com"
    )
    error_message = "Worker-bake recovery authority must also be present with the release train enabled."
  }
  assert {
    condition = (
      toset(keys(google_service_account_iam_member.terraform_attach)) == toset(["ci", "deploy", "images", "terraform", "terraform_plan", "slack_notifier", "slack_notifier_invoker", "release_plan", "release_train", "release_scheduler", "deploy_cutoff"]) &&
      length(google_service_account_iam_member.qa_terraform_stage) == 1 &&
      google_service_account.release["central"].account_id == "g5-lean-release" &&
      google_service_account_iam_member.qa_terraform_stage[0].member == "serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com"
    )
    error_message = "Release attachments need exact grants and must preserve the single existing central-release binding."
  }
}
