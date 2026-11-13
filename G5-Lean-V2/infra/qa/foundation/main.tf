terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 5.0"
    }
    google-beta = {
      source  = "hashicorp/google-beta"
      version = "~> 7.0"
    }
  }
  backend "gcs" {
    bucket = "g5-qa-1ccc-lean-tfstate"
    prefix = "foundation"
  }
}

locals {
  project_id = "g5-qa-1ccc"
  region     = "us-central1"
  labels     = { application = "g5-lean", environment = "qa" }
}

provider "google" {
  project = local.project_id
  region  = local.region
}
provider "google-beta" {
  project = local.project_id
  region  = local.region
}

# PR builds use a separate network and cannot reach the application data plane.
# Only Cloud Build's service-producer network is peered here.
resource "google_compute_network" "ci" {
  project                 = local.project_id
  name                    = "g5-lean-ci"
  auto_create_subnetworks = false
}
resource "google_compute_global_address" "ci_services" {
  project       = local.project_id
  name          = "g5-lean-ci-services"
  purpose       = "VPC_PEERING"
  address_type  = "INTERNAL"
  prefix_length = 20
  network       = google_compute_network.ci.id
}
resource "google_service_networking_connection" "ci" {
  network                 = google_compute_network.ci.id
  service                 = "servicenetworking.googleapis.com"
  reserved_peering_ranges = [google_compute_global_address.ci_services.name]
}
# The pinned provider cannot set workerRelease; see ../README.md for the
# separately managed CI host OS and its strict VM qualification requirements.
resource "google_cloudbuild_worker_pool" "ci" {
  provider = google-beta
  project  = local.project_id
  name     = "g5-ci-pool"
  location = local.region
  worker_config {
    machine_type                 = "c3-highcpu-22"
    disk_size_gb                 = 200
    enable_nested_virtualization = true
    no_external_ip               = false
  }
  network_config {
    peered_network          = google_compute_network.ci.id
    peered_network_ip_range = "/24"
  }
  depends_on = [google_service_networking_connection.ci]
}

resource "google_artifact_registry_repository" "docker" {
  for_each      = toset(["g5", "g5-ci", "g5-tiers", "g5-worker-images", "g5-builders"])
  project       = local.project_id
  location      = local.region
  repository_id = each.value
  format        = "DOCKER"
  labels        = local.labels
}
# Rollback retention only: current runtime builds never read or publish here.
# Keep this managed resource and its access until old releases are retired;
# deleting it from Terraform would destroy their retained firmware artifacts.
resource "google_artifact_registry_repository" "firmware" {
  project       = local.project_id
  location      = local.region
  repository_id = "libkrunfw"
  format        = "GENERIC"
  labels        = local.labels
}
resource "google_storage_bucket" "artifacts" {
  for_each                    = toset(["build-artifacts", "deploy-artifacts", "g5-worker-images"])
  project                     = local.project_id
  name                        = "${local.project_id}-${each.value}"
  location                    = local.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = false
  labels                      = local.labels
}

resource "google_service_account" "build" {
  for_each     = toset(["ci", "deploy", "images"])
  project      = local.project_id
  account_id   = "g5-lean-${each.value}"
  display_name = "G5 Lean QA ${each.value} builds"
}

# Agents use this identity to inspect QA Cloud Build failures.
resource "google_service_account" "build_log_reader" {
  project      = local.project_id
  account_id   = "g5-build-log-reader"
  display_name = "G5 Lean QA Cloud Build log reader"
  description  = "Read-only access to QA Cloud Build metadata and logs"
}
resource "google_project_iam_member" "build_log_reader" {
  for_each = toset([
    "roles/cloudbuild.builds.viewer",
    "roles/serviceusage.serviceUsageConsumer",
  ])
  project = local.project_id
  role    = each.value
  member  = "serviceAccount:${google_service_account.build_log_reader.email}"
}
resource "google_logging_log_view" "build_logs" {
  parent      = "projects/${local.project_id}"
  location    = "global"
  bucket      = "_Default"
  name        = "g5-build-logs"
  description = "QA Cloud Build logs for the read-only agent"
  filter      = "SOURCE(\"projects/${local.project_id}\") AND resource.type = \"build\" AND LOG_ID(\"cloudbuild\")"
}
resource "google_logging_log_view_iam_member" "build_log_reader" {
  parent   = "projects/${local.project_id}"
  location = "global"
  bucket   = "_Default"
  name     = "g5-build-logs"
  role     = "roles/logging.viewAccessor"
  member   = "serviceAccount:${google_service_account.build_log_reader.email}"

  depends_on = [google_logging_log_view.build_logs]
}

resource "google_project_iam_member" "build_logs" {
  for_each = google_service_account.build
  project  = local.project_id
  role     = "roles/logging.logWriter"
  member   = "serviceAccount:${each.value.email}"
}
resource "google_project_iam_member" "pool_user" {
  for_each = google_service_account.build
  project  = local.project_id
  role     = "roles/cloudbuild.workerPoolUser"
  member   = "serviceAccount:${each.value.email}"
}
# Queue/freshness checks need reads on every build identity.
resource "google_project_iam_member" "build_reads" {
  for_each = google_service_account.build
  project  = local.project_id
  role     = "roles/cloudbuild.builds.viewer"
  member   = "serviceAccount:${each.value.email}"
}
# The PR CI identity cancels the stale builds of its own PR head
# (`cancel-stale-pr-builds` in cloudbuild-ci-{rust,e2e}.yaml), which needs
# cloudbuild.builds.update; builds.viewer alone made that step fail with
# PERMISSION_DENIED on every run and left superseded rounds burning the pool.
# builds.editor also carries builds.create, so this identity must never gain
# iam.serviceAccountUser on a trusted build identity: a PR could otherwise
# submit builds under it. The deploy and images identities keep reads only.
resource "google_project_iam_member" "ci_build_editor" {
  project = local.project_id
  role    = "roles/cloudbuild.builds.editor"
  member  = "serviceAccount:${google_service_account.build["ci"].email}"
}
resource "google_artifact_registry_repository_iam_member" "ci_writer" {
  project    = local.project_id
  location   = local.region
  repository = google_artifact_registry_repository.docker["g5-ci"].name
  role       = "roles/artifactregistry.writer"
  member     = "serviceAccount:${google_service_account.build["ci"].email}"
}
resource "google_artifact_registry_repository_iam_member" "trusted_writer" {
  for_each = {
    deploy = "g5"
    images = "g5-worker-images"
  }
  project    = local.project_id
  location   = local.region
  repository = google_artifact_registry_repository.docker[each.value].name
  role       = "roles/artifactregistry.writer"
  member     = "serviceAccount:${google_service_account.build[each.key].email}"
}
resource "google_artifact_registry_repository_iam_member" "firmware_reader" {
  for_each   = google_service_account.build
  project    = local.project_id
  location   = local.region
  repository = google_artifact_registry_repository.firmware.name
  role       = "roles/artifactregistry.reader"
  member     = "serviceAccount:${each.value.email}"
}
resource "google_artifact_registry_repository_iam_member" "firmware_writer" {
  project    = local.project_id
  location   = local.region
  repository = google_artifact_registry_repository.firmware.name
  role       = "roles/artifactregistry.writer"
  member     = "serviceAccount:${google_service_account.build["images"].email}"
}
# The trusted bootstrap builder has a dedicated repository, separate from
# application artifacts and the PR-writable g5-ci cache.
resource "google_artifact_registry_repository_iam_member" "images_builder" {
  project    = local.project_id
  location   = local.region
  repository = google_artifact_registry_repository.docker["g5-builders"].repository_id
  role       = "roles/artifactregistry.writer"
  member     = "serviceAccount:${google_service_account.build["images"].email}"
}
# The deploy pipeline publishes the base tiers a workspace guest's dev-up
# pulls through its host's cache door (publish-base-tiers step; ontology
# g5.runtime.cache.fleet) into g5-tiers, a repository only the deploy
# identity writes: g5-ci is PR CI's own cache, which any pull request can
# write, so nothing that runs inside a workspace VM or the deploy pipeline
# may be pulled from it.
resource "google_artifact_registry_repository_iam_member" "deploy_tiers_writer" {
  project    = local.project_id
  location   = local.region
  repository = google_artifact_registry_repository.docker["g5-tiers"].name
  role       = "roles/artifactregistry.writer"
  member     = "serviceAccount:${google_service_account.build["deploy"].email}"
}
resource "google_storage_bucket_iam_member" "artifact_writer" {
  for_each = {
    ci     = "build-artifacts"
    deploy = "deploy-artifacts"
  }
  bucket = google_storage_bucket.artifacts[each.value].name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.build[each.key].email}"
}
# gcloud/gsutil cp list the destination bucket to distinguish object names from
# folders. Object-prefix grants cannot authorize that bucket-level check. This
# bootstrap-owned role reveals names/metadata, never additional object payloads.
resource "google_storage_bucket_iam_member" "images_object_lister" {
  bucket = google_storage_bucket.artifacts["g5-worker-images"].name
  role   = "projects/${local.project_id}/roles/g5LeanArtifactObjectLister"
  member = "serviceAccount:${google_service_account.build["images"].email}"
}
# Versioned artifacts cannot be overwritten or deleted by the publisher.
# Exact-name reads support create-only retries and the signed summary checks.
resource "google_storage_bucket_iam_member" "images_versions" {
  for_each = toset(["roles/storage.objectCreator", "roles/storage.objectViewer"])
  bucket   = google_storage_bucket.artifacts["g5-worker-images"].name
  role     = each.value
  member   = "serviceAccount:${google_service_account.build["images"].email}"
  condition {
    title       = "worker_version_objects_only"
    description = "Create and verify versioned artifacts without deletion or overwrite."
    expression  = "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/${google_storage_bucket.artifacts["g5-worker-images"].name}/objects/g5-worker-v')"
  }
}
# The existing publisher overwrites the operational latest pointer and ISO cache.
# Concurrent cold builds can both upload the same ISO, so cache writes retain
# overwrite access; neither exception reaches versioned artifacts.
resource "google_storage_bucket_iam_member" "images_mutable" {
  bucket = google_storage_bucket.artifacts["g5-worker-images"].name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.build["images"].email}"
  condition {
    title       = "worker_pointer_and_iso_cache_only"
    description = "Mutable QA discovery pointer and build cache; never release versions."
    expression  = "resource.type == 'storage.googleapis.com/Object' && (resource.name == 'projects/_/buckets/${google_storage_bucket.artifacts["g5-worker-images"].name}/objects/latest.json' || resource.name.startsWith('projects/_/buckets/${google_storage_bucket.artifacts["g5-worker-images"].name}/objects/iso/'))"
  }
}
# The worker-image publisher checks that its Terraform-owned bucket exists
# with `gsutil ls -b` before uploading. Object Admin deliberately omits
# storage.buckets.get, so grant that metadata read separately and only on the
# dedicated QA worker-images bucket.
resource "google_storage_bucket_iam_member" "images_bucket_viewer" {
  bucket = google_storage_bucket.artifacts["g5-worker-images"].name
  role   = "roles/storage.bucketViewer"
  member = "serviceAccount:${google_service_account.build["images"].email}"
}
# Cloud Build's top-level artifacts block runs after every worker-image step
# and publishes the small manifest, SBOM and summary to build-artifacts. Keep
# that final upload scoped to the images identity rather than broadening its
# project permissions. Object Creator permits the upload, Object Viewer permits
# the destination read that Cloud Build performs before creating the object,
# and Bucket Viewer supplies storage.buckets.get for the bucket preflight.
resource "google_storage_bucket_iam_member" "images_build_artifacts_writer" {
  bucket = google_storage_bucket.artifacts["build-artifacts"].name
  role   = "roles/storage.objectCreator"
  member = "serviceAccount:${google_service_account.build["images"].email}"
}
resource "google_storage_bucket_iam_member" "images_build_artifacts_reader" {
  bucket = google_storage_bucket.artifacts["build-artifacts"].name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.build["images"].email}"
}
resource "google_storage_bucket_iam_member" "images_build_artifacts_viewer" {
  bucket = google_storage_bucket.artifacts["build-artifacts"].name
  role   = "roles/storage.bucketViewer"
  member = "serviceAccount:${google_service_account.build["images"].email}"
}
# roles/cloudbuild.readTokenAccessor: the worker-image gate
# (scripts/await_worker_image.py) compares worker-image commits with the
# promoted commit through the Cloud Build connection's GitHub read token,
# because the shallow deploy checkout lacks their history; without the
# permission the gate fails closed on every deploy without an exact image.
resource "google_project_iam_member" "deployer" {
  for_each = toset([
    "roles/container.developer",
    "roles/compute.viewer",
    "roles/file.viewer",
    "roles/cloudbuild.readTokenAccessor",
  ])
  project = local.project_id
  role    = each.value
  member  = "serviceAccount:${google_service_account.build["deploy"].email}"
}
# Operator-applied bootstrap creates this role before foundation. QEMU builds
# need image publication only; the gated deploy owns all Kubernetes changes.
# Removing the former Compute Admin, project-wide actAs and cluster grants is an
# intentional IAM migration requiring review under the existing deletion guard.
resource "google_project_iam_member" "image_publisher" {
  project = local.project_id
  role    = "projects/${local.project_id}/roles/g5LeanWorkerImagePublisher"
  member  = "serviceAccount:${google_service_account.build["images"].email}"
}
resource "google_kms_key_ring" "worker_signer" {
  project  = local.project_id
  location = local.region
  name     = "g5-worker-signer"
}
resource "google_kms_crypto_key" "cosign" {
  name     = "cosign"
  key_ring = google_kms_key_ring.worker_signer.id
  purpose  = "ASYMMETRIC_SIGN"
  version_template {
    algorithm        = "EC_SIGN_P256_SHA256"
    protection_level = "SOFTWARE"
  }
  lifecycle { prevent_destroy = true }
}
resource "google_kms_crypto_key_iam_member" "image_signer" {
  crypto_key_id = google_kms_crypto_key.cosign.id
  role          = "roles/cloudkms.signerVerifier"
  member        = "serviceAccount:${google_service_account.build["images"].email}"
}
resource "google_kms_crypto_key_iam_member" "image_signer_viewer" {
  crypto_key_id = google_kms_crypto_key.cosign.id
  role          = "roles/cloudkms.viewer"
  member        = "serviceAccount:${google_service_account.build["images"].email}"
}

# Cloudflare delegates this child after its existing records and TLS are verified.
# Preserve the original QA state address and assigned nameservers.
resource "google_dns_managed_zone" "qa" {
  project    = local.project_id
  name       = "g5-lean-qa"
  dns_name   = "qa.g5labs.ai."
  visibility = "public"
  labels     = local.labels
  lifecycle { prevent_destroy = true }
}
output "dns_delegation" {
  value = { name = "qa.g5labs.ai.", type = "NS", ttl = 300, rrdatas = google_dns_managed_zone.qa.name_servers }
}
output "ci_pool" { value = google_cloudbuild_worker_pool.ci.id }
output "build_service_accounts" {
  value = { for name, account in google_service_account.build : name => account.email }
}
