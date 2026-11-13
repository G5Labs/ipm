# Notification-only companion to the existing weekly release train.
# This identity cannot deploy or approve. Read-token authority covers
# the QA project, matching G5; requests use the existing QA connection only.
resource "google_service_account" "deploy_cutoff" {
  count        = local.release_train_enabled ? 1 : 0
  project      = local.project_id
  account_id   = "g5-lean-deploy-cutoff"
  display_name = "G5 Lean release cutoff notification reader"
}

resource "google_project_iam_member" "deploy_cutoff" {
  for_each = local.release_train_enabled ? toset([
    "roles/cloudbuild.builds.viewer", "roles/logging.logWriter",
    "roles/cloudbuild.readTokenAccessor",
  ]) : toset([])
  project = local.project_id
  role    = each.key
  member  = "serviceAccount:${google_service_account.deploy_cutoff[0].email}"
}

resource "google_secret_manager_secret_iam_member" "deploy_cutoff_webhook" {
  count     = local.release_train_enabled ? 1 : 0
  project   = local.project_id
  secret_id = data.google_secret_manager_secret.slack_webhook.secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.deploy_cutoff[0].email}"
}

resource "google_service_account_iam_member" "scheduler_cutoff" {
  count              = local.release_train_enabled ? 1 : 0
  service_account_id = google_service_account.deploy_cutoff[0].name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.release_scheduler[0].email}"
}

resource "google_cloudbuild_trigger" "deploy_cutoff" {
  count       = local.release_train_enabled ? 1 : 0
  project     = local.project_id
  location    = local.region
  name        = "g5-deploy-cutoff-notify"
  description = "Preview the weekly release PRs at merge cutoff; never approve or deploy"
  source_to_build {
    repository = local.repository
    ref        = "refs/heads/main"
    repo_type  = "GITHUB"
  }
  git_file_source {
    path       = "infra/ci/cloudbuild-deploy-cutoff.yaml"
    repository = local.repository
    revision   = "refs/heads/main"
    repo_type  = "GITHUB"
  }
  service_account = google_service_account.deploy_cutoff[0].name
  approval_config { approval_required = false }
  substitutions = {
    _REGION                       = local.region
    _CUTOFF_DAY                   = local.release_cutoff_day
    _CUTOFF_TZ                    = local.release_cutoff_tz
    _DEPLOY_DAY                   = local.release_deploy_day
    _DEPLOY_TIME                  = local.release_deploy_time
    _DEPLOY_TZ                    = local.release_deploy_tz
    _TF_TRIGGER                   = google_cloudbuild_trigger.terraform_promote["staging"].name
    _TF_ORIGIN_TRIGGER            = google_cloudbuild_trigger.terraform_apply.name
    _APP_TRIGGER                  = google_cloudbuild_trigger.promote["staging"].name
    _DEPLOYED_TRIGGER             = google_cloudbuild_trigger.promote["prod"].name
    _APP_ORIGIN_TRIGGER           = "g5-deploy-qa"
    _GITHUB_OWNER                 = "G5Labs"
    _GITHUB_REPO                  = "G5-Lean"
    _GITHUB_CONNECTION            = local.connection_name
    _GITHUB_CONNECTION_REPOSITORY = local.repository_name
    _SLACK_WEBHOOK_SECRET         = data.google_secret_manager_secret.slack_webhook.secret_id
    _SLACK_WEBHOOK_VERSION        = local.slack_webhook_version
    _DRY_RUN                      = "false"
  }
  depends_on = [
    google_service_account_iam_member.terraform_attach,
    google_project_iam_member.deploy_cutoff,
    google_secret_manager_secret_iam_member.deploy_cutoff_webhook,
  ]
}

resource "google_cloud_scheduler_job" "deploy_cutoff" {
  count            = local.release_train_enabled ? 1 : 0
  project          = local.project_id
  region           = local.region
  name             = "g5-deploy-cutoff-notify"
  schedule         = local.release_cutoff_cron
  time_zone        = local.release_cutoff_tz
  attempt_deadline = "320s"
  retry_config { retry_count = 1 }
  http_target {
    http_method = "POST"
    uri         = "https://cloudbuild.googleapis.com/v1/projects/${local.project_id}/locations/${local.region}/triggers/${google_cloudbuild_trigger.deploy_cutoff[0].trigger_id}:run"
    body        = base64encode(jsonencode({ source = { branchName = "main" } }))
    headers     = { "Content-Type" = "application/json" }
    oauth_token { service_account_email = google_service_account.release_scheduler[0].email }
  }
  depends_on = [
    google_service_account_iam_member.terraform_attach,
    google_project_service.release_scheduler,
    google_service_account_iam_member.scheduler_cutoff,
  ]
}
