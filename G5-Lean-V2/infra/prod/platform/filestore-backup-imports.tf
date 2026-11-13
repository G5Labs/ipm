# Adopt the backup automation configured directly in GCP on 2026-11-04. Import
# blocks are no-ops once the resources are in state. The plan must show only
# these imports, in-place updates that the pull request explains, the creation
# of attachment grants that did not exist live, and one creation for the
# workflow: the google provider cannot import google_workflows_workflow, so an
# operator deletes the live workflow of the same name immediately before the
# apply that recreates it (module README). The daily backups themselves are
# not Terraform resources and are unaffected by that replacement.
import {
  to = module.environment.module.filestore_backup.google_service_account.backup
  id = "projects/g5lean-prod-1ccc/serviceAccounts/g5-filestore-backup@g5lean-prod-1ccc.iam.gserviceaccount.com"
}
import {
  to = module.environment.module.filestore_backup.google_service_account.scheduler
  id = "projects/g5lean-prod-1ccc/serviceAccounts/g5-filestore-scheduler@g5lean-prod-1ccc.iam.gserviceaccount.com"
}
import {
  to = module.environment.module.filestore_backup.google_project_iam_member.backup_runner
  id = "g5lean-prod-1ccc projects/g5lean-prod-1ccc/roles/g5FilestoreBackupRunner serviceAccount:g5-filestore-backup@g5lean-prod-1ccc.iam.gserviceaccount.com"
}
import {
  to = module.environment.module.filestore_backup.google_project_iam_member.scheduler_invoker
  id = "g5lean-prod-1ccc projects/g5lean-prod-1ccc/roles/g5FilestoreBackupScheduler serviceAccount:g5-filestore-scheduler@g5lean-prod-1ccc.iam.gserviceaccount.com"
}
import {
  to = module.environment.module.filestore_backup.google_cloud_scheduler_job.backup
  id = "projects/g5lean-prod-1ccc/locations/us-central1/jobs/g5-filestore-backup"
}
