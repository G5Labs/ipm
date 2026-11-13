# Same cadence as G5 dev: Thursday 23:00 New York, shipping merges through
# Tuesday end-of-day Los Angeles, once both release targets are enabled.
locals {
  release_train_enabled = var.enable_release_train && var.enable_promotion && length(local.release_targets) == 2
  # One schedule definition supplies both the train and its cutoff announcement.
  release_weekdays     = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"]
  release_cutoff_day   = "tuesday"
  release_cutoff_tz    = "America/Los_Angeles"
  release_deploy_day   = "thursday"
  release_deploy_time  = "23:00"
  release_deploy_tz    = "America/New_York"
  release_deploy_clock = split(":", local.release_deploy_time)
  release_train_cron   = "${tonumber(local.release_deploy_clock[1])} ${tonumber(local.release_deploy_clock[0])} * * ${index(local.release_weekdays, local.release_deploy_day)}"
  release_cutoff_cron  = "0 0 * * ${(index(local.release_weekdays, local.release_cutoff_day) + 1) % 7}"
}

resource "google_project_service" "release_scheduler" {
  count              = local.release_train_enabled ? 1 : 0
  project            = local.project_id
  service            = "cloudscheduler.googleapis.com"
  disable_on_destroy = false
}
resource "google_service_account" "release_train" {
  count        = local.release_train_enabled ? 1 : 0
  project      = local.project_id
  account_id   = "g5-lean-release-train"
  display_name = "G5 Lean weekly release approver"
  depends_on   = [google_service_account.release]
}
resource "google_project_iam_member" "train_permissions" {
  for_each = local.release_train_enabled ? toset([
    "roles/cloudbuild.builds.approver", "roles/logging.logWriter",
  ]) : toset([])
  project = local.project_id
  role    = each.key
  member  = "serviceAccount:${google_service_account.release_train[0].email}"
}
resource "google_service_account" "release_scheduler" {
  count        = local.release_train_enabled ? 1 : 0
  project      = local.project_id
  account_id   = "g5-lean-release-scheduler"
  display_name = "G5 Lean release train scheduler"
  depends_on   = [google_service_account.release]
}
resource "google_project_iam_member" "scheduler_trigger" {
  count   = local.release_train_enabled ? 1 : 0
  project = local.project_id
  role    = "roles/cloudbuild.builds.editor"
  member  = "serviceAccount:${google_service_account.release_scheduler[0].email}"
}
resource "google_service_account_iam_member" "scheduler_train" {
  count              = local.release_train_enabled ? 1 : 0
  service_account_id = google_service_account.release_train[0].name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.release_scheduler[0].email}"
}
resource "google_cloudbuild_trigger" "release_train" {
  count       = local.release_train_enabled ? 1 : 0
  project     = local.project_id
  location    = local.region
  name        = "g5-release-train"
  description = "Approve staging infrastructure, await prod infrastructure, then approve staging app"
  source_to_build {
    repository = local.repository
    ref        = "refs/heads/main"
    repo_type  = "GITHUB"
  }
  git_file_source {
    path       = "infra/ci/cloudbuild-release-train.yaml"
    repository = local.repository
    revision   = "refs/heads/main"
    repo_type  = "GITHUB"
  }
  service_account = google_service_account.release_train[0].name
  approval_config { approval_required = false }
  substitutions = {
    _REGION     = local.region
    _CUTOFF_DAY = local.release_cutoff_day
    _CUTOFF_TZ  = local.release_cutoff_tz
  }
  depends_on = [google_service_account_iam_member.terraform_attach]
}
resource "google_cloud_scheduler_job" "release_train" {
  count            = local.release_train_enabled ? 1 : 0
  project          = local.project_id
  region           = local.region
  name             = "g5-release-train"
  schedule         = local.release_train_cron
  time_zone        = local.release_deploy_tz
  attempt_deadline = "320s"
  retry_config { retry_count = 1 }
  http_target {
    http_method = "POST"
    uri         = "https://cloudbuild.googleapis.com/v1/projects/${local.project_id}/locations/${local.region}/triggers/${google_cloudbuild_trigger.release_train[0].trigger_id}:run"
    body        = base64encode(jsonencode({ source = { branchName = "main" } }))
    headers     = { "Content-Type" = "application/json" }
    oauth_token { service_account_email = google_service_account.release_scheduler[0].email }
  }
  depends_on = [google_service_account_iam_member.terraform_attach, google_project_service.release_scheduler]
}
