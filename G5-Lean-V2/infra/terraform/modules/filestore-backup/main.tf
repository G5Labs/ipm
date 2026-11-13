# Daily native Filestore backup automation: one Workflow, its daily Cloud
# Scheduler trigger and two dedicated identities per environment. The custom
# roles they bind are owned by the operator-applied bootstrap roots through
# ../filestore-backup-roles; the APIs (file, workflows, workflowexecutions,
# cloudscheduler) are owned by the existing API owners of each project. See
# README.md for the adoption (import) procedure and the permission audit.
terraform {
  required_providers {
    google = { source = "hashicorp/google" }
  }
}

locals {
  workflow_name  = "g5-filestore-backup"
  clock          = split(":", var.daily_time_utc)
  daily_cron     = "${tonumber(local.clock[1])} ${tonumber(local.clock[0])} * * *"
  executions_api = "https://workflowexecutions.googleapis.com/v1/projects/${var.project_id}/locations/${var.region}/workflows/${local.workflow_name}/executions"
}

# Display names match the live accounts (verified by the 2026-11-04 adoption
# plan) so importing them changes nothing.
resource "google_service_account" "backup" {
  project      = var.project_id
  account_id   = "g5-filestore-backup"
  display_name = "g5-filestore-backup"
  lifecycle { prevent_destroy = true }
}

resource "google_service_account" "scheduler" {
  project      = var.project_id
  account_id   = "g5-filestore-scheduler"
  display_name = "g5-filestore-scheduler"
  lifecycle { prevent_destroy = true }
}

# Single-member grants preserve every other principal on these roles.
resource "google_project_iam_member" "backup_runner" {
  project = var.project_id
  role    = "projects/${var.project_id}/roles/${var.runner_role_id}"
  member  = "serviceAccount:${google_service_account.backup.email}"
}

resource "google_project_iam_member" "scheduler_invoker" {
  project = var.project_id
  role    = "projects/${var.project_id}/roles/${var.scheduler_role_id}"
  member  = "serviceAccount:${google_service_account.scheduler.email}"
}

# Deploying the workflow and the job attaches their identities, which needs
# actAs on each. Explicit per-account attachment, as for every other QA
# managed workload; the grant precedes the resources that need it.
resource "google_service_account_iam_member" "attach" {
  for_each = { for pair in setproduct(["backup", "scheduler"], var.attach_members) :
  "${pair[0]}:${pair[1]}" => { account = pair[0], member = pair[1] } }
  service_account_id = each.value.account == "backup" ? google_service_account.backup.name : google_service_account.scheduler.name
  role               = "roles/iam.serviceAccountUser"
  member             = each.value.member
}

# The source is fixed in the deployed definition; an execution argument cannot
# redirect it. Each daily backup is created by the workflow, not by Terraform.
# The provider cannot import workflows: adoption recreates this one under its
# live name after an operator deletes the hand-configured definition.
resource "google_workflows_workflow" "backup" {
  project         = var.project_id
  region          = var.region
  name            = local.workflow_name
  description     = "Native daily Filestore-only backup with guarded 30-day retention"
  service_account = google_service_account.backup.name
  source_contents = file("${path.module}/workflow.yaml")
  user_env_vars = {
    SOURCE_INSTANCE = "projects/${var.project_id}/locations/${var.source_zone}/instances/${var.source_instance}"
    SOURCE_SHARE    = var.share_name
    BACKUP_PARENT   = "projects/${var.project_id}/locations/${var.region}"
    BACKUP_PREFIX   = var.source_instance
    RETENTION_DAYS  = tostring(var.retention_days)
  }
  call_log_level = "LOG_ERRORS_ONLY"
  labels         = { "managed-by" = "g5-filestore-backup" }
  depends_on     = [google_service_account_iam_member.attach, google_project_iam_member.backup_runner]
  lifecycle { prevent_destroy = true }
}

resource "google_cloud_scheduler_job" "backup" {
  project = var.project_id
  region  = var.region
  name    = local.workflow_name
  # Description, retries and body match the live jobs (2026-11-04 adoption
  # plans showed every other attribute equal), so importing changes nothing.
  # Retries of the start request are safe: the workflow is idempotent for the
  # day (an existing backup is reused), so a repeat cannot duplicate.
  description      = "Daily native Filestore backup; retain owned backups for ${var.retention_days} days"
  schedule         = local.daily_cron
  time_zone        = "Etc/UTC"
  attempt_deadline = "180s"
  retry_config { retry_count = 3 }
  http_target {
    http_method = "POST"
    uri         = local.executions_api
    body        = base64encode("{}")
    headers     = { "Content-Type" = "application/json" }
    oauth_token { service_account_email = google_service_account.scheduler.email }
  }
  depends_on = [google_service_account_iam_member.attach, google_project_iam_member.scheduler_invoker, google_workflows_workflow.backup]
  lifecycle { prevent_destroy = true }
}
