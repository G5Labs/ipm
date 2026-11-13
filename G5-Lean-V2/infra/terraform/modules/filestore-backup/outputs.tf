output "workflow_id" { value = google_workflows_workflow.backup.id }
output "scheduler_job_id" { value = google_cloud_scheduler_job.backup.id }
output "backup_service_account" { value = google_service_account.backup.email }
output "scheduler_service_account" { value = google_service_account.scheduler.email }
output "daily_backup_name_pattern" { value = "${var.source_instance}-daily-YYYY-MM-DD" }
# The fixed source and schedule, for root tests and operator review.
output "configuration" {
  value = {
    source_instance = var.source_instance
    source_zone     = var.source_zone
    share_name      = var.share_name
    daily_time_utc  = var.daily_time_utc
    schedule        = local.daily_cron
    retention_days  = var.retention_days
    attach_members  = var.attach_members
  }
}
