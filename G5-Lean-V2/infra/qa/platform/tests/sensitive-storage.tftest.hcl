# IAM's own mocked tests inspect resources; keep its identity outputs known
# here so plan-only root tests can verify the downstream consumers.
override_module {
  target = module.iam
  outputs = {
    api_sa_email                     = "g5-lean-qa-api@g5-qa-1ccc.iam.gserviceaccount.com"
    api_sa_name                      = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-qa-api@g5-qa-1ccc.iam.gserviceaccount.com"
    worker_sa_email                  = "g5-lean-qa-worker@g5-qa-1ccc.iam.gserviceaccount.com"
    worker_sa_name                   = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-qa-worker@g5-qa-1ccc.iam.gserviceaccount.com"
    host_sa_email                    = "g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com"
    release_identity_isolation_ready = true
  }
}
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
    defaults = { email = "g5-lean-terraform-plan@g5-qa-1ccc.iam.gserviceaccount.com" }
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

run "customer_objects_require_explicit_read_access" {
  command = plan

  assert {
    condition = (
      google_storage_bucket_iam_binding.customer_data_legacy_object_readers.bucket == "g5-qa-1ccc-lean-storage" &&
      google_storage_bucket_iam_binding.customer_data_legacy_object_readers.role == "roles/storage.legacyObjectReader" &&
      google_storage_bucket_iam_binding.customer_data_legacy_object_readers.members == toset(["projectOwner:g5-qa-1ccc"]) &&
      length(google_storage_bucket_iam_binding.customer_data_legacy_object_readers.condition) == 0
    )
    error_message = "QA customer data must remove Project Viewer legacy object reads, not metadata or explicit workload roles."
  }

  assert {
    condition = (
      google_storage_bucket_iam_member.host_guest_sccache_reader.role == "roles/storage.objectViewer" &&
      google_storage_bucket_iam_member.host_guest_sccache_reader.bucket == "g5-qa-1ccc-deploy-artifacts" &&
      google_storage_bucket_iam_member.host_guest_sccache_reader.member == "serviceAccount:g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com"
    )
    error_message = "Removing the customer-data convenience grant must preserve the worker host's separately scoped cache read."
  }
}

run "host_and_node_reads_remain_separate" {
  command = plan
  assert {
    condition = (
      output.application.artifact_readers == toset([
        "serviceAccount:g5-lean-qa-api@g5-qa-1ccc.iam.gserviceaccount.com",
        "serviceAccount:g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com",
        "serviceAccount:89784110315-compute@developer.gserviceaccount.com",
      ]) &&
      google_project_iam_member.compute_registry_reader.member == "serviceAccount:89784110315-compute@developer.gserviceaccount.com" &&
      google_storage_bucket_iam_member.compute_guest_sccache_reader.member == "serviceAccount:89784110315-compute@developer.gserviceaccount.com" &&
      google_storage_bucket_iam_member.compute_guest_sccache_reader.bucket == google_storage_bucket_iam_member.host_guest_sccache_reader.bucket &&
      google_storage_bucket_iam_member.compute_guest_sccache_reader.role == google_storage_bucket_iam_member.host_guest_sccache_reader.role &&
      google_storage_bucket_iam_member.compute_guest_sccache_reader.condition == google_storage_bucket_iam_member.host_guest_sccache_reader.condition &&
      toset(keys(google_artifact_registry_repository_iam_member.host_registry_reader)) == toset(["g5", "g5-tiers", "g5-worker-images"]) &&
      alltrue([for grant in google_artifact_registry_repository_iam_member.host_registry_reader : grant.role == "roles/artifactregistry.reader" && grant.member == "serviceAccount:g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com"]) &&
      google_storage_bucket_iam_member.host_worker_artifact_reader.condition[0].expression == "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/g5-qa-1ccc-g5-worker-images/objects/g5-worker-v')" &&
      google_storage_bucket_iam_member.host_guest_sccache_reader.condition[0].expression == "resource.name.startsWith(\"projects/_/buckets/g5-qa-1ccc-deploy-artifacts/objects/guest-sccache/\")"
    )
    error_message = "Host artifact/cache reads must be additive: retain pre-rotation host cache access and GKE node pulls without widening either cache grant."
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
