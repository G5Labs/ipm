# Operator-applied bootstrap owns these definitions, like worker-host-roles:
# the automated platform runners bind the fixed role names and never need
# permission to edit custom roles. Both roles exist live in QA, staging and
# production (created on 2026-11-04 outside Terraform); adopt them with the
# exact import commands in the module README before applying bootstrap.
terraform {
  required_providers {
    google = { source = "hashicorp/google" }
  }
}

variable "project_id" { type = string }

# Preserve the exact deployed role during adoption, including its existing
# log-write permission. No instance read/mutation, snapshot or restore authority.
resource "google_project_iam_custom_role" "runner" {
  project     = var.project_id
  role_id     = "g5FilestoreBackupRunner"
  title       = "G5 Filestore backup runner"
  description = "Native backup lifecycle only; no instance mutation or restore."
  permissions = [
    "file.backups.create", "file.backups.delete", "file.backups.get", "file.backups.list",
    "file.operations.get", "logging.logEntries.create",
  ]
}

# Cloud Scheduler only starts executions; it cannot read, cancel or deploy.
resource "google_project_iam_custom_role" "scheduler" {
  project     = var.project_id
  role_id     = "g5FilestoreBackupScheduler"
  title       = "G5 Filestore backup scheduler"
  description = "Start workflow executions in this project; no cancel or callbacks."
  permissions = ["workflows.executions.create"]
}

output "runner_role" { value = "projects/${var.project_id}/roles/${google_project_iam_custom_role.runner.role_id}" }
output "scheduler_role" { value = "projects/${var.project_id}/roles/${google_project_iam_custom_role.scheduler.role_id}" }
