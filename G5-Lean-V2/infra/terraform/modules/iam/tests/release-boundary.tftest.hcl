override_resource {
  target = google_service_account.host[0]
  values = {
    email = "g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com"
    name  = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com"
  }
}
mock_provider "google" {
  mock_resource "google_service_account" {
    defaults = {
      email = "g5-test-api@g5-qa-1ccc.iam.gserviceaccount.com"
      name  = "projects/g5-qa-1ccc/serviceAccounts/g5-test-api@g5-qa-1ccc.iam.gserviceaccount.com"
    }
  }
  mock_data "google_project" {
    defaults = { number = "123456789" }
  }
}

variables {
  project_id                      = "g5-qa-1ccc"
  gke_cluster_name                = "g5-test"
  gke_location                    = "us-central1"
  api_sa_name                     = "g5-test-api"
  worker_sa_name                  = "g5-test-worker"
  gcs_bucket_name                 = "g5-test-storage"
  enable_api_managed_worker_hosts = true
}

run "hosted_release_boundary" {
  command = plan
  variables { enable_legacy_workspace_identity_admin = false }
  assert {
    condition     = length(google_project_iam_member.api_token_creator) == 0 && length(google_project_iam_member.api_sa_creator) == 0 && length(google_project_iam_member.api_wi_admin) == 0
    error_message = "Hosted API must not gain project-wide identity administration or impersonate release identities."
  }
  assert {
    condition     = length(google_service_account_iam_member.api_actas_host) == 1
    error_message = "Worker provisioning must use its dedicated host identity."
  }
  assert {
    condition     = output.release_identity_isolation_ready
    error_message = "Release setup must receive evidence of the reconciled runtime identity boundary."
  }
}

run "legacy_consumer_compatibility" {
  command = plan
  assert {
    condition     = length(google_project_iam_member.api_token_creator) == 1 && length(google_project_iam_member.api_sa_creator) == 1 && length(google_project_iam_member.api_wi_admin) == 1
    error_message = "Unchanged module consumers must retain their existing policy until explicitly migrated."
  }
  assert {
    condition     = !output.release_identity_isolation_ready
    error_message = "Legacy runtime identity authority must block release identity activation."
  }
}

run "runtime_privileges_are_object_only_and_host_scoped" {
  # Apply is entirely mocked: resolve generated service-account attributes.
  command = apply
  variables { enable_legacy_workspace_identity_admin = false }
  assert {
    condition = (
      google_storage_bucket_iam_member.api_gcs_objects.role == "roles/storage.objectAdmin" &&
      google_storage_bucket_iam_member.api_gcs_metadata.role == "roles/storage.bucketViewer" &&
      google_storage_bucket_iam_member.api_gcs_objects.bucket == var.gcs_bucket_name &&
      google_storage_bucket_iam_member.api_gcs_metadata.bucket == var.gcs_bucket_name
    )
    error_message = "API compatibility reads/writes must not include bucket policy or configuration administration."
  }
  assert {
    condition = (
      google_service_account.host[0].account_id == "g5-test-host" &&
      google_service_account_iam_member.api_actas_host[0].service_account_id == google_service_account.host[0].name &&
      google_service_account_iam_member.api_actas_host[0].role == "roles/iam.serviceAccountUser" &&
      toset(keys(google_project_iam_member.host_telemetry)) == toset(["roles/logging.logWriter", "roles/monitoring.metricWriter"])
    )
    error_message = "Only the dedicated host may be attached; host project authority is telemetry only."
  }
  assert {
    condition = (
      toset(keys(google_project_iam_member.api_worker_host_provisioning)) == toset(["g5LeanWorkerHostResources", "g5LeanWorkerHostSupport"]) &&
      google_project_iam_member.api_worker_host_provisioning["g5LeanWorkerHostResources"].role == "projects/g5-qa-1ccc/roles/g5LeanWorkerHostResources" &&
      length(google_project_iam_member.api_worker_host_provisioning["g5LeanWorkerHostSupport"].condition) == 0 &&
      google_project_iam_member.api_worker_host_provisioning["g5LeanWorkerHostResources"].condition[0].expression == join(" || ", [
        "(resource.type == 'compute.googleapis.com/Instance' && resource.name.startsWith('projects/g5-qa-1ccc/zones/') && resource.name.extract('/instances/{name}').startsWith('g5-host-'))",
        "(resource.type == 'compute.googleapis.com/Disk' && resource.name.startsWith('projects/g5-qa-1ccc/zones/') && resource.name.extract('/disks/{name}').startsWith('g5-host-'))",
        "(resource.type == 'compute.googleapis.com/Firewall' && resource.name.startsWith('projects/g5-qa-1ccc/global/firewalls/g5-org-'))",
      ])
    )
    error_message = "Typed prefixes protect host resources; unsupported network conditions and list-prefix claims are forbidden."
  }
}

run "disabled_host_provisioning_has_no_host_authority" {
  command = plan
  variables { enable_api_managed_worker_hosts = false }
  assert {
    condition = (
      length(google_service_account.host) == 0 &&
      length(google_service_account_iam_member.api_actas_host) == 0 &&
      length(google_project_iam_member.host_telemetry) == 0 &&
      length(google_project_iam_member.api_worker_host_provisioning) == 0
    )
    error_message = "Disabling API-managed hosts must omit their identity and Compute/attachment grants."
  }
}
