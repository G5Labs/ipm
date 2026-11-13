mock_provider "google" {}
variables { project_id = "g5-qa-1ccc" }

run "exact_backup_permissions" {
  command = plan
  assert {
    condition = toset(google_project_iam_custom_role.runner.permissions) == toset([
      "file.backups.create", "file.backups.delete", "file.backups.get", "file.backups.list",
      "file.operations.get", "logging.logEntries.create",
    ])
    error_message = "Adoption preserves the deployed backup lifecycle, operation read and log-write permissions without adding instance access."
  }
  assert {
    condition     = toset(google_project_iam_custom_role.scheduler.permissions) == toset(["workflows.executions.create"])
    error_message = "The scheduler may only start workflow executions."
  }
  assert {
    condition = (
      google_project_iam_custom_role.runner.role_id == "g5FilestoreBackupRunner" &&
      google_project_iam_custom_role.scheduler.role_id == "g5FilestoreBackupScheduler" &&
      google_project_iam_custom_role.runner.project == "g5-qa-1ccc" &&
      google_project_iam_custom_role.scheduler.project == "g5-qa-1ccc" &&
      output.runner_role == "projects/g5-qa-1ccc/roles/g5FilestoreBackupRunner" &&
      output.scheduler_role == "projects/g5-qa-1ccc/roles/g5FilestoreBackupScheduler"
    )
    error_message = "Role IDs must match the live roles so the bootstrap import blocks adopt them without replacement."
  }
}
