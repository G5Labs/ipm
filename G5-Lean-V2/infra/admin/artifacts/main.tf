terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = { source = "hashicorp/google", version = "~> 5.0" }
  }
  backend "gcs" {}
}

provider "google" {
  project = var.project_id
  region  = var.region
}

locals {
  labels         = { application = "g5-lean", environment = "admin" }
  release_member = "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"
  deploy_member  = "serviceAccount:g5-lean-deploy@g5-qa-1ccc.iam.gserviceaccount.com"
  images_member  = "serviceAccount:g5-lean-images@g5-qa-1ccc.iam.gserviceaccount.com"
  tf_member      = "serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com"
  runtime_projects = {
    "g5-qa-1ccc"          = "g5-lean-qa-api"
    "g5lean-staging-1ccc" = "g5-lean-staging-api"
    "g5lean-prod-1ccc"    = "g5-lean-prod-api"
  }
  allowed_image_users = toset([for project_id, api_name in local.runtime_projects : "serviceAccount:${api_name}@${project_id}.iam.gserviceaccount.com"])
  allowed_runtime_members = toset(flatten([
    for project_id, api_name in local.runtime_projects : [
      "serviceAccount:${api_name}@${project_id}.iam.gserviceaccount.com",
      "serviceAccount:${trimsuffix(api_name, "-api")}-host@${project_id}.iam.gserviceaccount.com",
      "serviceAccount:${data.google_project.runtime[project_id].number}-compute@developer.gserviceaccount.com",
    ]
  ]))
}

data "google_project" "runtime" {
  for_each   = local.runtime_projects
  project_id = each.key
}

resource "google_artifact_registry_repository" "docker" {
  for_each      = toset(["g5", "g5-tiers", "g5-worker-images"])
  project       = var.project_id
  location      = var.region
  repository_id = each.value
  format        = "DOCKER"
  labels        = local.labels
  lifecycle { prevent_destroy = true }
}

# Versioning protects operator recovery; publishers cannot delete/overwrite.
# No cache expiry, cleanup policies or destructive lifecycle rules are inherited.
resource "google_storage_bucket" "artifacts" {
  for_each                    = toset(["lean-releases", "g5-worker-images"])
  project                     = var.project_id
  name                        = "${var.project_id}-${each.key}"
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = false
  versioning { enabled = true }
  lifecycle { prevent_destroy = true }
  labels = local.labels
}

# Fixed trusted publishers; PR CI, runtime and central release identities cannot
# be supplied as writers. The worker builder's tools/cache remain in QA's g5-builders repo.
resource "google_artifact_registry_repository_iam_member" "publisher" {
  for_each = {
    g5               = local.deploy_member
    g5-tiers         = local.deploy_member
    g5-worker-images = local.images_member
  }
  project    = var.project_id
  location   = var.region
  repository = google_artifact_registry_repository.docker[each.key].name
  role       = "roles/artifactregistry.writer"
  member     = each.value
}

# G5 publishes its Terraform carrier alongside application images. The trusted
# QA Terraform runner replaces its former GCS source-bundle writer grant.
resource "google_artifact_registry_repository_iam_member" "terraform_publisher" {
  project    = var.project_id
  location   = var.region
  repository = google_artifact_registry_repository.docker["g5"].repository_id
  role       = "roles/artifactregistry.writer"
  member     = local.tf_member
}

resource "google_artifact_registry_repository_iam_member" "reader" {
  for_each = {
    for pair in setproduct(toset(["g5", "g5-tiers", "g5-worker-images"]), setunion(
      var.runtime_registry_readers, toset([local.release_member]),
    )) : "${pair[0]}:${pair[1]}" => pair
  }
  project    = var.project_id
  location   = var.region
  repository = google_artifact_registry_repository.docker[each.value[0]].name
  role       = "roles/artifactregistry.reader"
  member     = each.value[1]
}

# Analysis occurrences have project-level IAM; central release only reads them.
# No scanner administration, findings writes or new artifact writes are granted.
resource "google_project_iam_member" "release_image_scan_reads" {
  project = var.project_id
  role    = "roles/containeranalysis.occurrences.viewer"
  member  = local.release_member
}

locals {
  object_publishers = {
    app = {
      bucket = "lean-releases", prefix = "app-releases/", member = local.deploy_member
    }
    worker = {
      bucket = "g5-worker-images", prefix = "g5-worker-v", member = local.images_member
    }
  }
}

# gcloud storage cp performs destination discovery at bucket scope, including
# app-release manifests and fresh worker versions. List only names/metadata;
# leave all payload reads and mutations under the existing object-prefix grants.
resource "google_project_iam_custom_role" "artifact_object_lister" {
  project     = var.project_id
  role_id     = "g5LeanArtifactObjectLister"
  title       = "G5 Lean artifact object lister"
  description = "List artifact object names and metadata for upload destination discovery."
  permissions = ["storage.objects.list"]
}
resource "google_storage_bucket_iam_member" "publisher_object_lister" {
  for_each = local.object_publishers
  bucket   = google_storage_bucket.artifacts[each.value.bucket].name
  role     = "projects/${var.project_id}/roles/${google_project_iam_custom_role.artifact_object_lister.role_id}"
  member   = each.value.member
}

# Conditional objectViewer supports exact-name get/stat; listing is separate.
# Upload retries must compare an existing object's digest; they cannot overwrite
# it. latest.json, ISO caches and temporary upload objects must stay in QA.
resource "google_storage_bucket_iam_member" "publisher" {
  for_each = {
    for pair in setproduct(toset(keys(local.object_publishers)), toset([
      "roles/storage.objectCreator", "roles/storage.objectViewer",
      ])) : "${pair[0]}:${pair[1]}" => {
      publisher = local.object_publishers[pair[0]], role = pair[1]
    }
  }
  bucket = google_storage_bucket.artifacts[each.value.publisher.bucket].name
  role   = each.value.role
  member = each.value.publisher.member
  condition {
    title       = "publisher_version_objects_only"
    description = "Only this producer's immutable artifact payloads; no other producer prefixes."
    expression  = "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/${google_storage_bucket.artifacts[each.value.publisher.bucket].name}/objects/${each.value.publisher.prefix}')"
  }
}

# Existing worker publication checks bucket existence before uploading.
resource "google_storage_bucket_iam_member" "worker_bucket_metadata" {
  bucket = google_storage_bucket.artifacts["g5-worker-images"].name
  role   = "roles/storage.bucketViewer"
  member = local.images_member
}

resource "google_storage_bucket_iam_member" "release_reader" {
  for_each = google_storage_bucket.artifacts
  bucket   = each.value.name
  role     = "roles/storage.objectViewer"
  member   = local.release_member
}

resource "google_storage_bucket_iam_member" "runtime_worker_reader" {
  for_each = var.runtime_worker_artifact_readers
  bucket   = google_storage_bucket.artifacts["g5-worker-images"].name
  role     = "roles/storage.objectViewer"
  member   = each.key
  condition {
    title       = "worker_version_objects_only"
    description = "Read versioned worker artifacts, without mutable pointers or cache state."
    expression  = "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/${google_storage_bucket.artifacts["g5-worker-images"].name}/objects/g5-worker-v')"
  }
}

# The pipeline creates an image directly from its raw-disk archive. No import VM,
# service-account attachment, image deletion, project IAM or Compute admin grant.
# images.insert also checks setLabels; gcloud waits for the global operation.
resource "google_project_iam_custom_role" "worker_image_publisher" {
  project     = var.project_id
  role_id     = "g5LeanWorkerImagePublisher"
  title       = "G5 Lean worker image publisher"
  description = "Publish and inspect worker images from immutable GCS archives."
  permissions = [
    "compute.images.create", "compute.images.get", "compute.images.list",
    "compute.images.setLabels", "compute.globalOperations.get",
  ]
}
resource "google_project_iam_member" "worker_image_publisher" {
  project = var.project_id
  role    = "projects/${var.project_id}/roles/${google_project_iam_custom_role.worker_image_publisher.role_id}"
  member  = local.images_member
}

# QA deploy resolves the gated image before creating its release manifest.
resource "google_project_iam_member" "image_reader" {
  for_each = setunion(var.runtime_image_users, toset([local.release_member, local.deploy_member]))
  project  = var.project_id
  role     = "roles/compute.imageUser"
  member   = each.key
}
