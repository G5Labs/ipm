# Exercise the real delivery root with mock cloud reads. Seed only synthetic
# service accounts, then verify the full root with plans: Terraform 1.9 keeps
# computed identity fields unknown until apply, even with override_resource.
# The protected release bucket is never applied or included in test teardown.
mock_provider "google" {
  mock_resource "google_service_account" {
    defaults = {
      name  = "projects/g5-qa-1ccc/serviceAccounts/mock-account@g5-qa-1ccc.iam.gserviceaccount.com"
      email = "mock-account@g5-qa-1ccc.iam.gserviceaccount.com"
    }
  }
  mock_data "google_service_account" {
    defaults = {
      name  = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-ci@g5-qa-1ccc.iam.gserviceaccount.com"
      email = "g5-lean-ci@g5-qa-1ccc.iam.gserviceaccount.com"
    }
  }
  mock_data "google_project" {
    defaults = { number = "123456789012" }
  }
}

override_data {
  target = data.terraform_remote_state.foundation
  values = {
    outputs = {
      ci_pool = "projects/g5-qa-1ccc/locations/us-central1/workerPools/g5-ci"
    }
  }
}


variables {
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

override_resource {
  target = google_service_account.deploy_cutoff[0]
  values = {
    name  = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-deploy-cutoff@g5-qa-1ccc.iam.gserviceaccount.com"
    email = "g5-lean-deploy-cutoff@g5-qa-1ccc.iam.gserviceaccount.com"
  }
}

override_resource {
  target = google_service_account.release_scheduler[0]
  values = {
    name  = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-release-scheduler@g5-qa-1ccc.iam.gserviceaccount.com"
    email = "g5-lean-release-scheduler@g5-qa-1ccc.iam.gserviceaccount.com"
  }
}

override_resource {
  target = google_service_account.release_train[0]
  values = {
    name  = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-release-train@g5-qa-1ccc.iam.gserviceaccount.com"
    email = "g5-lean-release-train@g5-qa-1ccc.iam.gserviceaccount.com"
  }
}

run "no_cutoff_resources_before_promotion" {
  command = plan

  assert {
    condition = (
      length(google_service_account.deploy_cutoff) == 0 &&
      length(google_project_iam_member.deploy_cutoff) == 0 &&
      length(google_secret_manager_secret_iam_member.deploy_cutoff_webhook) == 0 &&
      length(google_service_account_iam_member.scheduler_cutoff) == 0 &&
      length(google_cloudbuild_trigger.deploy_cutoff) == 0 &&
      length(google_cloud_scheduler_job.deploy_cutoff) == 0 &&
      !contains(output.slack_notifier_trigger_names, "g5-deploy-cutoff-notify")
    )
    error_message = "Configured targets alone must not create the cutoff reader, grants, trigger or schedule."
  }
}

run "bootstrap_does_not_activate_cutoff" {
  command = plan
  variables {
    enable_application_triggers = true
    enable_release_bootstrap    = true
  }

  assert {
    condition = (
      length(google_cloudbuild_trigger.deploy_cutoff) == 0 &&
      length(google_cloud_scheduler_job.deploy_cutoff) == 0 &&
      length(google_service_account.deploy_cutoff) == 0 &&
      length(google_project_iam_member.deploy_cutoff) == 0 &&
      length(google_secret_manager_secret_iam_member.deploy_cutoff_webhook) == 0 &&
      length(google_service_account_iam_member.scheduler_cutoff) == 0
    )
    error_message = "Bootstrap releases must not create cutoff notification resources."
  }
}

run "disabled_train_disables_cutoff" {
  command = plan
  variables {
    enable_application_triggers = true
    enable_promotion            = true
    enable_release_train        = false
  }

  assert {
    condition = (
      length(google_cloudbuild_trigger.release_train) == 0 &&
      length(google_cloud_scheduler_job.release_train) == 0 &&
      length(google_cloudbuild_trigger.deploy_cutoff) == 0 &&
      length(google_cloud_scheduler_job.deploy_cutoff) == 0 &&
      length(google_service_account.deploy_cutoff) == 0 &&
      length(google_project_iam_member.deploy_cutoff) == 0 &&
      length(google_secret_manager_secret_iam_member.deploy_cutoff_webhook) == 0 &&
      length(google_service_account_iam_member.scheduler_cutoff) == 0
    )
    error_message = "Disabling the weekly train must also remove its cutoff notification resources."
  }
}

run "seed_mock_notification_identities" {
  command = apply
  variables {
    enable_application_triggers = true
    enable_promotion            = true
  }
  plan_options {
    target = [
      google_service_account.deploy_cutoff,
      google_service_account.release_scheduler,
      google_service_account.release_train,
    ]
  }
}

run "cutoff_uses_existing_schedule_secret_and_isolated_reader" {
  command = plan
  variables {
    enable_application_triggers = true
    enable_promotion            = true
  }

  assert {
    condition = (
      google_cloud_scheduler_job.deploy_cutoff[0].schedule == "0 0 * * 3" &&
      google_cloud_scheduler_job.deploy_cutoff[0].time_zone == "America/Los_Angeles" &&
      google_cloud_scheduler_job.release_train[0].schedule == "0 23 * * 4" &&
      google_cloud_scheduler_job.release_train[0].time_zone == "America/New_York" &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_CUTOFF_DAY"] == "tuesday" &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_CUTOFF_TZ"] == google_cloud_scheduler_job.deploy_cutoff[0].time_zone &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_CUTOFF_DAY"] == google_cloudbuild_trigger.release_train[0].substitutions["_CUTOFF_DAY"] &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_CUTOFF_TZ"] == google_cloudbuild_trigger.release_train[0].substitutions["_CUTOFF_TZ"] &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_DEPLOY_DAY"] == "thursday" &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_DEPLOY_TIME"] == "23:00" &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_DEPLOY_TZ"] == google_cloud_scheduler_job.release_train[0].time_zone
    )
    error_message = "Announce at Tuesday's end in PT and advertise the same Thursday 23:00 New York release that is actually scheduled."
  }

  assert {
    condition = (
      google_cloudbuild_trigger.deploy_cutoff[0].name == "g5-deploy-cutoff-notify" &&
      google_cloudbuild_trigger.deploy_cutoff[0].project == "g5-qa-1ccc" &&
      google_cloudbuild_trigger.deploy_cutoff[0].location == "us-central1" &&
      google_cloudbuild_trigger.deploy_cutoff[0].git_file_source[0].path == "infra/ci/cloudbuild-deploy-cutoff.yaml" &&
      google_cloudbuild_trigger.deploy_cutoff[0].git_file_source[0].revision == "refs/heads/main" &&
      google_cloudbuild_trigger.deploy_cutoff[0].source_to_build[0].ref == "refs/heads/main" &&
      google_cloudbuild_trigger.deploy_cutoff[0].git_file_source[0].repository == google_cloudbuild_trigger.release_train[0].git_file_source[0].repository &&
      !google_cloudbuild_trigger.deploy_cutoff[0].approval_config[0].approval_required &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_GITHUB_CONNECTION"] == "G5labs-G5-QA" &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_GITHUB_CONNECTION_REPOSITORY"] == "G5Labs-G5-Lean" &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_GITHUB_OWNER"] == "G5Labs" &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_GITHUB_REPO"] == "G5-Lean" &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_DRY_RUN"] == "false"
    )
    error_message = "The notification must run main's Lean config and existing QA repository connection without requiring release approval."
  }

  assert {
    condition = (
      google_service_account.deploy_cutoff[0].account_id == "g5-lean-deploy-cutoff" &&
      google_cloudbuild_trigger.deploy_cutoff[0].service_account == google_service_account.deploy_cutoff[0].name &&
      google_cloudbuild_trigger.deploy_cutoff[0].service_account != google_cloudbuild_trigger.release_train[0].service_account &&
      google_cloudbuild_trigger.deploy_cutoff[0].service_account != google_cloudbuild_trigger.promote["staging"].service_account &&
      toset(keys(google_project_iam_member.deploy_cutoff)) == toset([
        "roles/cloudbuild.builds.viewer", "roles/logging.logWriter", "roles/cloudbuild.readTokenAccessor"
      ]) &&
      alltrue([for role, grant in google_project_iam_member.deploy_cutoff :
        grant.role == role && grant.project == "g5-qa-1ccc" &&
        grant.member == "serviceAccount:${google_service_account.deploy_cutoff[0].email}"
      ])
    )
    error_message = "A dedicated reader must receive exactly build reads, log writes and project-level read tokens, with no build submission, release approval or deployment roles."
  }

  assert {
    condition = (
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_SLACK_WEBHOOK_SECRET"] == data.google_secret_manager_secret.slack_webhook.secret_id &&
      can(regex("^[1-9][0-9]*$", google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_SLACK_WEBHOOK_VERSION"])) &&
      strcontains(google_storage_bucket_object.slack_notifier_config.content,
      "/secrets/${data.google_secret_manager_secret.slack_webhook.secret_id}/versions/${google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_SLACK_WEBHOOK_VERSION"]}") &&
      google_secret_manager_secret_iam_member.deploy_cutoff_webhook[0].secret_id == data.google_secret_manager_secret.slack_webhook.secret_id &&
      google_secret_manager_secret_iam_member.deploy_cutoff_webhook[0].role == "roles/secretmanager.secretAccessor" &&
      google_secret_manager_secret_iam_member.deploy_cutoff_webhook[0].member == "serviceAccount:${google_service_account.deploy_cutoff[0].email}" &&
      contains(output.slack_notifier_trigger_names, google_cloudbuild_trigger.deploy_cutoff[0].name) &&
      strcontains(google_storage_bucket_object.slack_notifier_config.content, "'g5-deploy-cutoff-notify'")
    )
    error_message = "Cutoff notifications must share the existing pinned Slack secret version and be included in out-of-build failure alerts."
  }

  assert {
    condition = (
      google_cloud_scheduler_job.deploy_cutoff[0].http_target[0].oauth_token[0].service_account_email == google_service_account.release_scheduler[0].email &&
      google_cloud_scheduler_job.deploy_cutoff[0].http_target[0].oauth_token[0].service_account_email == google_cloud_scheduler_job.release_train[0].http_target[0].oauth_token[0].service_account_email &&
      google_service_account_iam_member.scheduler_cutoff[0].service_account_id == google_service_account.deploy_cutoff[0].name &&
      google_service_account_iam_member.scheduler_cutoff[0].role == "roles/iam.serviceAccountUser" &&
      google_service_account_iam_member.scheduler_cutoff[0].member == "serviceAccount:${google_service_account.release_scheduler[0].email}" &&
      google_cloud_scheduler_job.deploy_cutoff[0].http_target[0].http_method == "POST" &&
      jsondecode(base64decode(google_cloud_scheduler_job.deploy_cutoff[0].http_target[0].body)).source.branchName == "main"
    )
    error_message = "Reuse the release scheduler and scope its additional actAs grant to the dedicated notification reader; schedule a main-branch invocation."
  }

  assert {
    condition = (
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_TF_TRIGGER"] == google_cloudbuild_trigger.terraform_promote["staging"].name &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_TF_ORIGIN_TRIGGER"] == "g5-terraform-apply-qa" &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_APP_TRIGGER"] == google_cloudbuild_trigger.promote["staging"].name &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_APP_ORIGIN_TRIGGER"] == "g5-deploy-qa" &&
      google_cloudbuild_trigger.deploy_cutoff[0].substitutions["_DEPLOYED_TRIGGER"] == google_cloudbuild_trigger.promote["prod"].name &&
      alltrue([for env, trigger in google_cloudbuild_trigger.promote : trigger.approval_config[0].approval_required == (env == "staging")]) &&
      alltrue([for env, trigger in google_cloudbuild_trigger.terraform_promote : trigger.approval_config[0].approval_required == (env == "staging")])
    )
    error_message = "Predict the approval-gated staging queues from QA origins while using production only as the deployed baseline."
  }
}
