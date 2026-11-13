# Exercise the real QA root with cloud calls and provisioners disabled.
override_module {
  target = module.gke
  outputs = {
    cluster_name           = "g5-lean-qa"
    cluster_location       = "us-central1"
    cluster_endpoint       = "10.0.0.1"
    cluster_ca_certificate = "bW9jay1jYQ=="
  }
}
override_module {
  target = module.filestore
  outputs = {
    instance_name = "g5-lean-qa-repos"
    ip_address    = "10.40.0.2"
    share_name    = "g5_repos"
  }
}
mock_provider "google" {
  mock_data "google_dns_managed_zone" {
    defaults = { dns_name = "qa.g5labs.ai." }
  }
  mock_data "google_project" {
    defaults = { number = "89784110315" }
  }
  mock_data "google_service_account" {
    defaults = { email = "unrelated@g5-qa-1ccc.iam.gserviceaccount.com" }
  }
  mock_data "google_storage_project_service_account" {
    defaults = { email_address = "service-89784110315@gs-project-accounts.iam.gserviceaccount.com" }
  }
  mock_resource "google_service_account" {
    defaults = {
      email = "mock-runtime@g5-qa-1ccc.iam.gserviceaccount.com"
      name  = "projects/g5-qa-1ccc/serviceAccounts/mock-runtime@g5-qa-1ccc.iam.gserviceaccount.com"
    }
  }
  mock_resource "google_kms_crypto_key" {
    defaults = { id = "projects/g5-qa-1ccc/locations/us-central1/keyRings/mock/cryptoKeys/mock" }
  }
}
mock_provider "google-beta" {}
mock_provider "random" {}
mock_provider "kubernetes" {}

override_data {
  target = data.google_service_account.terraform
  values = { email = "g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com" }
}
override_data {
  target = data.google_service_account.terraform_plan
  values = { email = "g5-lean-terraform-plan@g5-qa-1ccc.iam.gserviceaccount.com" }
}

run "runner_reads_only_managed_database_versions_and_attaches_only_nodes" {
  command = plan
  assert {
    condition = (
      toset(keys(google_secret_manager_secret_iam_member.terraform_runner_accessor)) == toset([
        "g5-db-password", "g5-authz-writer-db-password",
        "g5-runtime-system-db-password", "g5-runtime-tenant-db-password",
      ]) &&
      alltrue([for id, grant in google_secret_manager_secret_iam_member.terraform_runner_accessor :
        grant.secret_id == id && grant.project == "g5-qa-1ccc" &&
        grant.role == "roles/secretmanager.secretAccessor" &&
        grant.member == "serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com"
      ]) &&
      toset(keys(google_secret_manager_secret_iam_member.terraform_plan_accessor)) ==
      toset(keys(google_secret_manager_secret_iam_member.terraform_runner_accessor)) &&
      alltrue([for id, grant in google_secret_manager_secret_iam_member.terraform_plan_accessor :
        grant.secret_id == id && grant.project == "g5-qa-1ccc" &&
        grant.role == "roles/secretmanager.secretAccessor" &&
        grant.member == "serviceAccount:g5-lean-terraform-plan@g5-qa-1ccc.iam.gserviceaccount.com"
      ])
    )
    error_message = "Both Terraform identities need refresh access to exactly the four DB payloads, never OAuth, webhook, or other operator-seeded secrets."
  }
  assert {
    condition = (
      google_service_account_iam_member.terraform_node_act_as.service_account_id == "projects/g5-qa-1ccc/serviceAccounts/89784110315-compute@developer.gserviceaccount.com" &&
      google_service_account_iam_member.terraform_node_act_as.role == "roles/iam.serviceAccountUser" &&
      google_service_account_iam_member.terraform_node_act_as.member == "serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com"
    )
    error_message = "GKE creation needs actAs on the node account; runtime API, worker and arbitrary accounts must not receive this grant."
  }
  assert {
    condition = module.filestore_backup.configuration == {
      source_instance = "g5-lean-qa-repos-zonal"
      source_zone     = "us-central1-a"
      share_name      = "g5_repos"
      daily_time_utc  = "04:00"
      schedule        = "0 4 * * *"
      retention_days  = 30
      attach_members  = toset(["serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com"])
    }
    error_message = "QA backs up its own repository instance and share daily at 04:00 UTC with 30-day retention, attaching only the runner."
  }
}

# Adoption imports (filestore-backup-imports.tf) cannot run through mock
# providers; overriding the imported resources keeps these plan-only tests
# offline without changing what they assert.
override_resource {
  target = module.filestore_backup.google_service_account.backup
  values = {
    email = "g5-filestore-backup@g5-qa-1ccc.iam.gserviceaccount.com"
    name  = "projects/g5-qa-1ccc/serviceAccounts/g5-filestore-backup@g5-qa-1ccc.iam.gserviceaccount.com"
  }
}
override_resource {
  target = module.filestore_backup.google_service_account.scheduler
  values = {
    email = "g5-filestore-scheduler@g5-qa-1ccc.iam.gserviceaccount.com"
    name  = "projects/g5-qa-1ccc/serviceAccounts/g5-filestore-scheduler@g5-qa-1ccc.iam.gserviceaccount.com"
  }
}
override_resource {
  target = module.filestore_backup.google_project_iam_member.backup_runner
}
override_resource {
  target = module.filestore_backup.google_project_iam_member.scheduler_invoker
}
override_resource {
  target = module.filestore_backup.google_cloud_scheduler_job.backup
}

run "committed_map_provisions_and_selects_zonal_storage" {
  command = plan
  assert {
    condition = (
      length(module.filestore_zonal) == 1 &&
      output.filestore_migration.provision_zonal &&
      output.filestore_migration.active == "zonal" &&
      output.application.filestore_instance == "g5-lean-qa-repos-zonal" &&
      output.application.filestore_pvc_name == "g5-repos-zonal-pvc" &&
      module.filestore_backup.configuration.source_instance == output.application.filestore_instance &&
      module.filestore_backup.configuration.retention_days == 30
    )
    error_message = "The committed QA map must provision one zonal instance and select it for deployment outputs and daily backups."
  }
}

run "opt_in_provisioning_keeps_legacy_active" {
  command = plan
  override_module {
    target  = module.filestore_selection
    outputs = { configuration = { "provision_zonal" : true, "active" : "legacy", "legacy" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-qa-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" }, "zonal" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-qa-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" }, "selected" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-qa-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" } } }
  }
  assert {
    condition = (
      length(module.filestore_zonal) == 1 &&
      output.filestore_migration.active == "legacy" &&
      output.application.filestore_instance == "g5-lean-qa-repos" &&
      output.application.filestore_pvc_name == "g5-repos-pvc" &&
      module.filestore_backup.configuration.source_instance == output.application.filestore_instance &&
      module.filestore_backup.configuration.retention_days == 30
    )
    error_message = "Explicit provisioning must add a candidate while deployment outputs and daily backups both remain on legacy storage."
  }
}

run "selected_zonal_drives_backup_and_application_together" {
  command = plan
  override_module {
    target  = module.filestore_selection
    outputs = { configuration = { "provision_zonal" : true, "active" : "zonal", "legacy" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-qa-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" }, "zonal" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-qa-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" }, "selected" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-qa-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" } } }
  }
  override_module {
    target  = module.filestore_zonal[0]
    outputs = { ip_address = "10.90.0.2" }
  }
  assert {
    condition = (
      output.application.filestore_instance == "g5-lean-qa-repos-zonal" &&
      output.application.filestore_ip == "10.90.0.2" &&
      output.application.filestore_pv_name == "g5-repos-zonal-pv" &&
      output.application.filestore_pvc_name == "g5-repos-zonal-pvc" &&
      output.application.filestore_storage == "1Ti" &&
      module.filestore_backup.configuration.source_instance == output.application.filestore_instance &&
      module.filestore_backup.configuration.source_zone == output.application.filestore_zone &&
      module.filestore_backup.configuration.retention_days == 30
    )
    error_message = "The one selected identity must drive both backups and deploy outputs without changing the backup policy."
  }
}
