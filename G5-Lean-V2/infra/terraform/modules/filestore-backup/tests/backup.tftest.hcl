mock_provider "google" {
  mock_resource "google_service_account" {
    defaults = {
      email = "mock-scheduler@g5-qa-1ccc.iam.gserviceaccount.com"
      name  = "projects/g5-qa-1ccc/serviceAccounts/mock-scheduler@g5-qa-1ccc.iam.gserviceaccount.com"
    }
  }
}

variables {
  project_id      = "g5-qa-1ccc"
  region          = "us-central1"
  source_instance = "g5-lean-qa-repos"
  source_zone     = "us-central1-a"
  share_name      = "g5_repos"
  daily_time_utc  = "04:00"
  attach_members  = ["serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com"]
}

run "live_names_and_fixed_source" {
  command = plan
  assert {
    condition = (
      google_service_account.backup.account_id == "g5-filestore-backup" &&
      google_service_account.scheduler.account_id == "g5-filestore-scheduler" &&
      google_workflows_workflow.backup.name == "g5-filestore-backup" &&
      google_cloud_scheduler_job.backup.name == "g5-filestore-backup" &&
      google_workflows_workflow.backup.region == "us-central1" &&
      google_cloud_scheduler_job.backup.region == "us-central1"
    )
    error_message = "Live identity/job names and the same-name Workflow handover target must be preserved."
  }
  assert {
    condition = (
      google_workflows_workflow.backup.user_env_vars == tomap({
        SOURCE_INSTANCE = "projects/g5-qa-1ccc/locations/us-central1-a/instances/g5-lean-qa-repos"
        SOURCE_SHARE    = "g5_repos"
        BACKUP_PARENT   = "projects/g5-qa-1ccc/locations/us-central1"
        BACKUP_PREFIX   = "g5-lean-qa-repos"
        RETENTION_DAYS  = "30"
      }) &&
      google_workflows_workflow.backup.source_contents == file("${path.module}/workflow.yaml") &&
      google_workflows_workflow.backup.description == "Native daily Filestore-only backup with guarded 30-day retention" &&
      google_workflows_workflow.backup.call_log_level == "LOG_ERRORS_ONLY" &&
      google_workflows_workflow.backup.labels == tomap({ "managed-by" = "g5-filestore-backup" })
    )
    error_message = "The deployed definition fixes the source instance, share, zone, 30-day retention and ownership label; the workflow source is the checked-in file."
  }
  assert {
    condition     = output.daily_backup_name_pattern == "g5-lean-qa-repos-daily-YYYY-MM-DD"
    error_message = "Daily backups are named deterministically from the instance and the UTC date."
  }
}

run "schedule_is_daily_utc" {
  command = plan
  assert {
    condition = (
      google_cloud_scheduler_job.backup.schedule == "0 4 * * *" &&
      google_cloud_scheduler_job.backup.time_zone == "Etc/UTC" &&
      google_cloud_scheduler_job.backup.attempt_deadline == "180s" &&
      google_cloud_scheduler_job.backup.http_target[0].headers == tomap({ "Content-Type" = "application/json" }) &&
      google_cloud_scheduler_job.backup.description == "Daily native Filestore backup; retain owned backups for 30 days" &&
      google_cloud_scheduler_job.backup.http_target[0].http_method == "POST" &&
      google_cloud_scheduler_job.backup.http_target[0].uri == "https://workflowexecutions.googleapis.com/v1/projects/g5-qa-1ccc/locations/us-central1/workflows/g5-filestore-backup/executions" &&
      length(google_cloud_scheduler_job.backup.http_target[0].oauth_token) == 1 &&
      google_cloud_scheduler_job.backup.retry_config[0].retry_count == 3 &&
      google_cloud_scheduler_job.backup.http_target[0].body == base64encode("{}")
    )
    error_message = "The job starts one execution of this project's workflow daily at the configured UTC time as the scheduler identity."
  }
}

run "staging_and_prod_times" {
  command = plan
  variables {
    project_id      = "g5lean-prod-1ccc"
    source_instance = "g5-lean-prod-repos"
    daily_time_utc  = "05:00"
    attach_members  = []
  }
  assert {
    condition     = google_cloud_scheduler_job.backup.schedule == "0 5 * * *" && length(google_service_account_iam_member.attach) == 0
    error_message = "Targets use their own daily time and need no per-account attachment grant for the central release identity."
  }
  assert {
    condition     = google_cloud_scheduler_job.backup.http_target[0].uri == "https://workflowexecutions.googleapis.com/v1/projects/g5lean-prod-1ccc/locations/us-central1/workflows/g5-filestore-backup/executions"
    error_message = "The scheduler must target the workflow of its own project."
  }
}

run "half_hour_schedule" {
  command = plan
  variables { daily_time_utc = "04:30" }
  assert {
    condition     = google_cloud_scheduler_job.backup.schedule == "30 4 * * *"
    error_message = "Minutes must carry into the cron expression."
  }
}

run "only_custom_roles_and_explicit_attachment" {
  command = plan
  assert {
    condition = (
      google_project_iam_member.backup_runner.role == "projects/g5-qa-1ccc/roles/g5FilestoreBackupRunner" &&
      google_project_iam_member.scheduler_invoker.role == "projects/g5-qa-1ccc/roles/g5FilestoreBackupScheduler" &&
      google_project_iam_member.backup_runner.project == "g5-qa-1ccc" &&
      google_project_iam_member.scheduler_invoker.project == "g5-qa-1ccc"
    )
    error_message = "Runtime identities bind only the two bootstrap-owned custom roles; no predefined role widens them."
  }
  assert {
    condition = (
      toset(keys(google_service_account_iam_member.attach)) == toset([
        "backup:serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com",
        "scheduler:serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com",
      ]) &&
      alltrue([for grant in google_service_account_iam_member.attach :
        grant.role == "roles/iam.serviceAccountUser" &&
        grant.member == "serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com"
      ])
    )
    error_message = "Only the named deploying identity may attach the two backup identities."
  }
}

run "reject_zone_outside_region" {
  command = plan
  variables { source_zone = "us-east1-b" }
  expect_failures = [var.source_zone]
}

run "reject_malformed_time" {
  command = plan
  variables { daily_time_utc = "4am" }
  expect_failures = [var.daily_time_utc]
}

run "reject_fractional_retention" {
  command = plan
  variables { retention_days = 0.5 }
  expect_failures = [var.retention_days]
}

run "reject_user_attachment" {
  command = plan
  variables { attach_members = ["user:someone@example.com"] }
  expect_failures = [var.attach_members]
}

run "two_digit_region" {
  command = plan
  variables {
    region      = "europe-west10"
    source_zone = "europe-west10-a"
  }
  assert {
    condition     = google_workflows_workflow.backup.region == "europe-west10"
    error_message = "Regions with two-digit suffixes are valid Filestore regions."
  }
}

run "reject_changed_retention" {
  command = plan
  variables { retention_days = 29 }
  expect_failures = [var.retention_days]
}
