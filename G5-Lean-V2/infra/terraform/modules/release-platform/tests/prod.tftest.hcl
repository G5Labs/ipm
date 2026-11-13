# Plan-only: shared secrets contain terraform_data provisioners that must never
# execute in an offline test, even when every cloud provider is mocked.
override_module {
  target = module.filestore
  outputs = {
    instance_name = "g5-lean-prod-repos"
    ip_address    = "10.70.0.2"
    share_name    = "g5_repos"
  }
}
override_module {
  target = module.gke
  outputs = {
    cluster_name           = "g5-lean-prod"
    cluster_location       = "us-central1"
    cluster_endpoint       = "10.0.0.1"
    cluster_ca_certificate = "bW9jay1jYQ=="
  }
}
mock_provider "google" {
  mock_data "google_compute_default_service_account" {
    defaults = { email = "177811684595-compute@developer.gserviceaccount.com" }
  }
  mock_data "google_project" {
    defaults = { number = "177811684595" }
  }
  mock_data "google_storage_project_service_account" {
    defaults = { email_address = "service-177811684595@gs-project-accounts.iam.gserviceaccount.com" }
  }
  mock_resource "google_service_account" {
    defaults = {
      email = "mock-runtime@g5lean-prod-1ccc.iam.gserviceaccount.com"
      name  = "projects/g5lean-prod-1ccc/serviceAccounts/mock-runtime@g5lean-prod-1ccc.iam.gserviceaccount.com"
    }
  }
  mock_resource "google_kms_crypto_key" {
    defaults = { id = "projects/g5lean-prod-1ccc/locations/us-central1/keyRings/mock/cryptoKeys/mock" }
  }
}
mock_provider "google-beta" {
  mock_resource "google_container_cluster" {
    defaults = { master_auth = [{ cluster_ca_certificate = "bW9jay1jYQ==" }] }
  }
}
mock_provider "random" {}
mock_provider "kubernetes" {}

variables {
  project_id  = "g5lean-prod-1ccc"
  region      = "us-central1"
  environment = "prod"
}

run "prod_backup_wiring" {
  command = plan
  assert {
    condition     = module.filestore_backup.configuration.source_instance == "g5-lean-prod-repos-zonal" && module.filestore_backup.configuration.schedule == "0 5 * * *" && module.filestore_backup.configuration.retention_days == 30
    error_message = "Production backs up its own repository instance daily at 05:00 UTC with 30-day retention."
  }
}

run "prod_uses_its_own_resources" {
  command = plan
  assert {
    condition = (
      google_storage_bucket_iam_binding.customer_data_legacy_object_readers.bucket == "g5lean-prod-1ccc-lean-storage" &&
      google_storage_bucket_iam_binding.customer_data_legacy_object_readers.role == "roles/storage.legacyObjectReader" &&
      google_storage_bucket_iam_binding.customer_data_legacy_object_readers.members == toset(["projectOwner:g5lean-prod-1ccc"]) &&
      length(google_storage_bucket_iam_binding.customer_data_legacy_object_readers.condition) == 0
    )
    error_message = "Production customer data must not inherit payload reads from basic Project Viewer membership."
  }
  variables {
    project_id  = "g5lean-prod-1ccc"
    environment = "prod"
    settings = {
      app_hostname                 = "app.g5labs.ai"
      enable_release_check_fixture = true
    }
  }
  assert {
    condition     = output.application.database_name == "g5_lean_prod" && output.cluster.name == "g5-lean-prod" && google_storage_bucket.customer_data.name == "g5lean-prod-1ccc-lean-storage"
    error_message = "Production must use its own cluster, database and durable bucket."
  }
  assert {
    condition     = output.application.external_url == "https://app.g5labs.ai" && !output.application.external_access_enabled && toset(keys(output.dns_records_to_publish)) == toset(["ingress:app.g5labs.ai"]) && output.application.release_check_secret == "g5-release-checks"
    error_message = "A configured private origin must prepare its DNS without enabling application ingress."
  }
  assert {
    condition = (
      google_dns_managed_zone.application.project == "g5lean-prod-1ccc" &&
      google_dns_managed_zone.application.name == "g5-lean-prod" &&
      google_dns_managed_zone.application.dns_name == "app.g5labs.ai." &&
      output.dns_delegation.project_id == "g5lean-prod-1ccc" &&
      output.dns_delegation.zone_name == "g5-lean-prod" &&
      output.dns_delegation.dns_name == "app.g5labs.ai."
    )
    error_message = "Production must own only its production child and expose its actual delegation."
  }
}

run "prod_prepares_public_dns_without_opening_ingress" {
  command = plan
  variables {
    settings = {
      app_hostname     = "app.g5labs.ai"
      preview_hostname = "preview.app.g5labs.ai"
    }
  }
  assert {
    condition = (
      !output.application.external_access_enabled &&
      output.application.certificate_map_name == "g5-lean-prod-ip-cert-map" &&
      toset(keys(output.dns_records_to_publish)) == toset([
        "ingress:app.g5labs.ai", "preview:preview.app.g5labs.ai",
        "authorization:app.g5labs.ai", "authorization:preview.app.g5labs.ai",
      ])
    )
    error_message = "Fresh production must plan its complete child records and certificates before ingress/delegation."
  }
}

run "prod_rejects_staging_child" {
  command = plan
  variables { settings = { app_hostname = "lean.g5labs.ai" } }
  expect_failures = [var.settings]
}

run "committed_map_provisions_and_selects_zonal_storage" {
  command = plan
  assert {
    condition = (
      length(module.filestore_zonal) == 1 &&
      output.filestore_migration.provision_zonal &&
      output.filestore_migration.active == "zonal" &&
      output.application.filestore_instance == "g5-lean-prod-repos-zonal" &&
      output.application.filestore_pvc_name == "g5-repos-zonal-pvc" &&
      module.filestore_backup.configuration.source_instance == output.application.filestore_instance &&
      module.filestore_backup.configuration.retention_days == 30
    )
    error_message = "The committed production map must provision one zonal instance and select it for deployment outputs and daily backups."
  }
}

run "opt_in_provisioning_keeps_legacy_active" {
  command = plan
  override_module {
    target  = module.filestore_selection
    outputs = { configuration = { "provision_zonal" : true, "active" : "legacy", "legacy" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-prod-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" }, "zonal" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-prod-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" }, "selected" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-prod-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" } } }
  }
  assert {
    condition = (
      length(module.filestore_zonal) == 1 &&
      output.filestore_migration.active == "legacy" &&
      output.application.filestore_instance == "g5-lean-prod-repos" &&
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
    outputs = { configuration = { "provision_zonal" : true, "active" : "zonal", "legacy" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-prod-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" }, "zonal" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-prod-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" }, "selected" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-prod-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" } } }
  }
  override_module {
    target  = module.filestore_zonal[0]
    outputs = { ip_address = "10.90.0.2" }
  }
  assert {
    condition = (
      output.application.filestore_instance == "g5-lean-prod-repos-zonal" &&
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
