# Plan-only: shared secrets contain terraform_data provisioners that must never
# execute in an offline test, even when every cloud provider is mocked.
override_module {
  target = module.filestore
  outputs = {
    instance_name = "g5-lean-staging-repos"
    ip_address    = "10.70.0.2"
    share_name    = "g5_repos"
  }
}
override_module {
  target = module.gke
  outputs = {
    cluster_name           = "g5-lean-staging"
    cluster_location       = "us-central1"
    cluster_endpoint       = "10.0.0.1"
    cluster_ca_certificate = "bW9jay1jYQ=="
  }
}
mock_provider "google" {
  mock_data "google_compute_default_service_account" {
    defaults = { email = "237625147788-compute@developer.gserviceaccount.com" }
  }
  mock_data "google_project" {
    defaults = { number = "237625147788" }
  }
  mock_data "google_storage_project_service_account" {
    defaults = { email_address = "service-237625147788@gs-project-accounts.iam.gserviceaccount.com" }
  }
  mock_resource "google_service_account" {
    defaults = {
      email = "mock-runtime@g5lean-staging-1ccc.iam.gserviceaccount.com"
      name  = "projects/g5lean-staging-1ccc/serviceAccounts/mock-runtime@g5lean-staging-1ccc.iam.gserviceaccount.com"
    }
  }
  mock_resource "google_kms_crypto_key" {
    defaults = { id = "projects/g5lean-staging-1ccc/locations/us-central1/keyRings/mock/cryptoKeys/mock" }
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
  project_id  = "g5lean-staging-1ccc"
  region      = "us-central1"
  environment = "staging"
}

run "staging_backup_wiring" {
  command = plan
  assert {
    condition = (
      module.filestore_backup.configuration.source_instance == "g5-lean-staging-repos-zonal" &&
      module.filestore_backup.configuration.source_zone == "us-central1-a" &&
      module.filestore_backup.configuration.share_name == "g5_repos" &&
      module.filestore_backup.configuration.daily_time_utc == "04:30" &&
      module.filestore_backup.configuration.schedule == "30 4 * * *" &&
      module.filestore_backup.configuration.retention_days == 30 &&
      length(module.filestore_backup.configuration.attach_members) == 0
    )
    error_message = "Staging backs up its own repository instance daily at 04:30 UTC with 30-day retention and no extra attachment grant."
  }
}

run "activation_defaults_closed" {
  command = plan
  assert {
    condition = output.application.artifact_readers == toset([
      "serviceAccount:g5-lean-staging-api@g5lean-staging-1ccc.iam.gserviceaccount.com",
      "serviceAccount:g5-lean-staging-host@g5lean-staging-1ccc.iam.gserviceaccount.com",
      "serviceAccount:237625147788-compute@developer.gserviceaccount.com",
    ])
    error_message = "First-plan readers must be known and preserve node pulls alongside API and dedicated host access."
  }
  assert {
    condition = (
      google_storage_bucket_iam_binding.customer_data_legacy_object_readers.bucket == "g5lean-staging-1ccc-lean-storage" &&
      google_storage_bucket_iam_binding.customer_data_legacy_object_readers.role == "roles/storage.legacyObjectReader" &&
      google_storage_bucket_iam_binding.customer_data_legacy_object_readers.members == toset(["projectOwner:${var.project_id}"]) &&
      length(google_storage_bucket_iam_binding.customer_data_legacy_object_readers.condition) == 0
    )
    error_message = "Staging customer data must not inherit payload reads from basic Project Viewer membership."
  }
  assert {
    condition     = !can(output.application.auth_allowed_emails)
    error_message = "The retired login allowlist must not reach the application settings."
  }
  assert {
    condition     = !var.settings.external_access_enabled && var.settings.app_hostname == "" && var.settings.preview_hostname == ""
    error_message = "Fresh environments must not publish an inferred hostname."
  }
  assert {
    condition     = !output.application.external_access_enabled && output.application.external_url == "https://unconfigured.invalid" && length(output.dns_records_to_publish) == 0
    error_message = "Unconfigured targets must not imply a usable public origin or DNS activation."
  }
  assert {
    condition = (
      google_dns_managed_zone.application.project == "g5lean-staging-1ccc" &&
      google_dns_managed_zone.application.name == "g5-lean-staging" &&
      google_dns_managed_zone.application.dns_name == "lean.g5labs.ai." &&
      google_dns_managed_zone.application.visibility == "public" &&
      !google_dns_managed_zone.application.force_destroy &&
      output.dns_delegation.project_id == "g5lean-staging-1ccc" &&
      output.dns_delegation.zone_name == "g5-lean-staging" &&
      output.dns_delegation.dns_name == "lean.g5labs.ai."
    )
    error_message = "Private bootstrap must own the staging child zone and expose its actual delegation, without parent DNS writes."
  }
  assert {
    condition     = output.application.project_id == "g5lean-staging-1ccc" && output.application.database_name == "g5_lean_staging" && output.cluster.name == "g5-lean-staging"
    error_message = "Runtime and data names must belong to the staging target."
  }
  assert {
    condition     = google_storage_bucket.customer_data.versioning[0].enabled && !google_storage_bucket.customer_data.force_destroy && google_storage_bucket.customer_data.public_access_prevention == "enforced" && length(google_storage_bucket.customer_data.lifecycle_rule) == 0
    error_message = "Customer storage must be private, versioned and free of cache expiry rules."
  }
  assert {
    condition     = google_project_iam_member.node_services.project == "g5lean-staging-1ccc" && google_project_iam_member.node_services.role == "roles/container.defaultNodeServiceAccount" && google_project_iam_member.node_services.member == "serviceAccount:237625147788-compute@developer.gserviceaccount.com"
    error_message = "Explicit node permissions must target the actual Compute identity in this target only."
  }
  assert {
    condition     = output.release_identity_isolation_ready && output.application.release_check_secret == null
    error_message = "Legacy API impersonation must stay off and the release fixture must remain optional."
  }
  assert {
    condition = toset(keys(google_secret_manager_secret_version.database)) == toset([
      "g5-db-password", "g5-authz-writer-db-password",
      "g5-runtime-system-db-password", "g5-runtime-tenant-db-password",
    ])
    error_message = "Only the four managed database credentials are seeded by platform."
  }
}

run "activation_requires_hostnames" {
  command = plan
  variables { settings = { external_access_enabled = true } }
  expect_failures = [var.settings]
}

run "existing_customer_hostname_is_forbidden" {
  command = plan
  variables { settings = { app_hostname = "g5labs.ai" } }
  expect_failures = [var.settings]
}

run "public_configuration_prepares_existing_g5_certificates" {
  command = plan
  variables {
    settings = {
      external_access_enabled = true
      app_hostname            = "lean.g5labs.ai"
      preview_hostname        = "preview.lean.g5labs.ai"
    }
  }
  assert {
    condition     = output.application.external_access_enabled && output.application.preview_hostname == "preview.lean.g5labs.ai" && output.application.certificate_map_name == "g5-lean-staging-ip-cert-map"
    error_message = "Public settings must expose the existing target map and preview configuration."
  }
  assert {
    condition = toset(keys(output.dns_records_to_publish)) == toset([
      "ingress:lean.g5labs.ai", "preview:preview.lean.g5labs.ai",
      "authorization:lean.g5labs.ai", "authorization:preview.lean.g5labs.ai",
    ])
    error_message = "External DNS output must contain app, wildcard and both DNS authorization records."
  }
}

run "public_names_can_change_with_reviewed_settings" {
  command = plan
  variables {
    settings = {
      external_access_enabled = true
      app_hostname            = "next.lean.g5labs.ai"
      preview_hostname        = "preview.next.lean.g5labs.ai"
    }
  }
  assert {
    condition     = output.application.external_url == "https://next.lean.g5labs.ai" && length(output.dns_records_to_publish) == 4 && google_dns_managed_zone.application.dns_name == "lean.g5labs.ai."
    error_message = "Hostnames may change inside the existing child without replacing its delegation."
  }
}

run "private_configuration_retains_dns_and_certificate_resources" {
  command = plan
  variables {
    settings = {
      external_access_enabled = false
      app_hostname            = "lean.g5labs.ai"
      preview_hostname        = "preview.lean.g5labs.ai"
    }
  }
  assert {
    condition = (
      !output.application.external_access_enabled &&
      toset(keys(output.dns_records_to_publish)) == toset(keys(run.public_configuration_prepares_existing_g5_certificates.dns_records_to_publish)) &&
      output.application.certificate_map_name == run.public_configuration_prepares_existing_g5_certificates.application.certificate_map_name &&
      output.dns_delegation.zone_name == run.public_configuration_prepares_existing_g5_certificates.dns_delegation.zone_name
    )
    error_message = "Disabling ingress with unchanged hostnames must retain the same zone, DNS records and certificate map."
  }
}

run "reject_former_staging_child" {
  command = plan
  variables { settings = { app_hostname = "lean-staging.g5labs.ai" } }
  expect_failures = [var.settings]
}

run "reject_suffix_without_label_boundary" {
  command = plan
  variables { settings = { preview_hostname = "notlean.g5labs.ai" } }
  expect_failures = [var.settings]
}

run "reject_malformed_private_hostname" {
  command = plan
  variables { settings = { app_hostname = "invalid..lean.g5labs.ai" } }
  expect_failures = [var.settings]
}

run "reject_equal_private_app_and_preview" {
  command = plan
  variables {
    settings = {
      app_hostname     = "lean.g5labs.ai"
      preview_hostname = "lean.g5labs.ai"
    }
  }
  expect_failures = [var.settings]
}

run "reject_other_target_preview_name" {
  command = plan
  variables { settings = { preview_hostname = "preview.app.g5labs.ai" } }
  expect_failures = [var.settings]
}

run "ignores_retired_allowlist_input" {
  # A reviewed input object from before the allowlist retirement still plans;
  # the attribute is discarded and never reaches the application output.
  command = plan
  variables { settings = { auth_allowed_emails = "@example.com,person@example.org" } }
  assert {
    condition     = !can(output.application.auth_allowed_emails)
    error_message = "The retired login allowlist must be ignored."
  }
}

run "reject_swapped_target" {
  command = plan
  variables { project_id = "g5lean-prod-1ccc" }
  expect_failures = [var.project_id]
}

run "reject_admin_target" {
  command = plan
  variables { project_id = "g5lean-admin-1ccc" }
  expect_failures = [var.project_id]
}

run "reject_unreviewed_region" {
  command = plan
  variables { region = "europe-west1" }
  expect_failures = [var.region]
}

run "committed_map_provisions_and_selects_zonal_storage" {
  command = plan
  assert {
    condition = (
      length(module.filestore_zonal) == 1 &&
      output.filestore_migration.provision_zonal &&
      output.filestore_migration.active == "zonal" &&
      output.application.filestore_instance == "g5-lean-staging-repos-zonal" &&
      output.application.filestore_pvc_name == "g5-repos-zonal-pvc" &&
      module.filestore_backup.configuration.source_instance == output.application.filestore_instance &&
      module.filestore_backup.configuration.retention_days == 30
    )
    error_message = "The committed staging map must provision one zonal instance and select it for deployment outputs and daily backups."
  }
}

run "opt_in_provisioning_keeps_legacy_active" {
  command = plan
  override_module {
    target  = module.filestore_selection
    outputs = { configuration = { "provision_zonal" : true, "active" : "legacy", "legacy" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-staging-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" }, "zonal" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-staging-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" }, "selected" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-staging-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" } } }
  }
  assert {
    condition = (
      length(module.filestore_zonal) == 1 &&
      output.filestore_migration.active == "legacy" &&
      output.application.filestore_instance == "g5-lean-staging-repos" &&
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
    outputs = { configuration = { "provision_zonal" : true, "active" : "zonal", "legacy" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-staging-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" }, "zonal" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-staging-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" }, "selected" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-staging-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" } } }
  }
  override_module {
    target  = module.filestore_zonal[0]
    outputs = { ip_address = "10.90.0.2" }
  }
  assert {
    condition = (
      output.application.filestore_instance == "g5-lean-staging-repos-zonal" &&
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
