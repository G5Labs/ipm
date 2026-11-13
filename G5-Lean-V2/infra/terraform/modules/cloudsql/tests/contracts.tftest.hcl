# Contract tests for the Cloud SQL module (terraform_cloudsql [D1]/[D8]).
#
# [D8]: disk_size_gb is a creation-time floor only. With disk_autoresize, Cloud
# SQL grows the live disk past the configured value and disks cannot shrink, so
# a recorded disk above the configured floor must plan NO change to the disk
# attribute — a tracked shrink forces whole-instance replacement, which is what
# cascaded into the g5-api-secrets deletion in the 2026-08-10 dev outage
# (issue #1768).
#
# Auto-discovered by the tf-test-modules Cloud Build step. Mock providers only;
# no GCP credentials needed.

mock_provider "google" {}
mock_provider "random" {}

variables {
  project_id                     = "test-project"
  region                         = "us-central1"
  instance_name                  = "test-postgres"
  network_id                     = "projects/test-project/global/networks/test-net"
  private_ip_range_id            = "test-range"
  database_version               = "POSTGRES_17"
  tier                           = "db-custom-2-8192"
  disk_size_gb                   = 20
  disk_autoresize                = true
  availability_type              = "ZONAL"
  backup_enabled                 = true
  point_in_time_recovery_enabled = true
  database_name                  = "g5"
  db_user                        = "g5"
}

# Creation honors the configured size (lifecycle ignore_changes never applies
# to initial create). Applying at 37 also seeds the shared test state with a
# recorded disk ABOVE the 20GB file-default floor — the exact post-autoresize
# divergence of the incident — for the next run.
run "create_honors_configured_disk_size" {
  command = apply

  variables {
    disk_size_gb = 37
  }

  assert {
    condition     = google_sql_database_instance.main.settings[0].disk_size == 37
    error_message = "instance creation must honor the configured disk_size_gb"
  }

  assert {
    condition     = google_sql_database_instance.main.deletion_protection == true
    error_message = "deletion_protection must stay hard-coded to true (terraform_cloudsql [D1])"
  }

  assert {
    condition     = google_sql_database_instance.main.settings[0].disk_autoresize == true
    error_message = "disk_autoresize must flow through from the caller"
  }
}

# The incident shape: recorded disk 37GB, configured floor 20GB. With
# ignore_changes on settings[0].disk_size the plan must keep the recorded 37 —
# no shrink, and therefore no shrink-forced replacement. Without ignore_changes
# this plan adopts 20 and forces whole-instance replacement (issue #1768).
run "recorded_disk_above_config_floor_plans_no_shrink" {
  command = plan

  assert {
    condition     = google_sql_database_instance.main.settings[0].disk_size == 37
    error_message = "settings[0].disk_size must be under lifecycle ignore_changes: a recorded disk above the configured floor must never plan a shrink or replacement (issue #1768, terraform_cloudsql [D8])"
  }
}
