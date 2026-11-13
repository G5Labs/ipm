terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = { source = "hashicorp/google" }
  }
}

variable "project_id" {
  description = "The approved new Lean project for this environment; never QA, admin or an original G5 project."
  type        = string
  validation {
    condition = var.project_id == lookup({
      staging = "g5lean-staging-1ccc"
      prod    = "g5lean-prod-1ccc"
    }, var.environment, "INVALID_ENVIRONMENT")
    error_message = "Bootstrap may only target g5lean-staging-1ccc for staging or g5lean-prod-1ccc for prod."
  }
}
variable "region" {
  type = string
  validation {
    condition     = var.region == "us-central1"
    error_message = "The approved staging/prod region is us-central1."
  }
}
variable "environment" {
  type = string
  validation {
    condition     = contains(["staging", "prod"], var.environment)
    error_message = "Only staging and prod are release bootstrap environments."
  }
}

locals {
  # Created by the separate QA delivery setup after QA identity hardening.
  # Do not parameterize this into an arbitrary cross-project grant recipient.
  release_account = "g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"
  planner_account = "g5-lean-release-plan@g5-qa-1ccc.iam.gserviceaccount.com"
  release_roles = toset([
    "roles/certificatemanager.editor",
    "roles/cloudkms.admin",
    "roles/cloudscheduler.admin",
    "roles/cloudsql.admin",
    "roles/compute.admin",
    "roles/container.admin",
    "roles/dns.admin",
    "roles/file.editor",
    "roles/iam.serviceAccountAdmin",
    "roles/iam.serviceAccountUser",
    "roles/logging.admin",
    "roles/monitoring.admin",
    "roles/pubsub.admin",
    "roles/resourcemanager.projectIamAdmin",
    "roles/servicenetworking.networksAdmin",
    "roles/serviceusage.serviceUsageAdmin",
    "roles/storage.admin",
    "roles/workflows.editor",
  ])
}

# Project creation/billing and Cloudflare parent delegation are administrator
# tasks. The target platform manages its Google child zone and records.
resource "google_project_service" "required" {
  for_each = toset([
    "certificatemanager.googleapis.com", "cloudkms.googleapis.com",
    "cloudresourcemanager.googleapis.com", "cloudscheduler.googleapis.com", "compute.googleapis.com",
    "container.googleapis.com", "dns.googleapis.com", "file.googleapis.com", "iam.googleapis.com",
    "iamcredentials.googleapis.com", "logging.googleapis.com",
    "monitoring.googleapis.com", "pubsub.googleapis.com",
    "secretmanager.googleapis.com", "servicenetworking.googleapis.com",
    "serviceusage.googleapis.com", "sqladmin.googleapis.com",
    "storage.googleapis.com", "workflowexecutions.googleapis.com", "workflows.googleapis.com",
  ])
  project            = var.project_id
  service            = each.value
  disable_on_destroy = false
}

module "worker_host_roles" {
  source     = "../worker-host-roles"
  project_id = var.project_id
  depends_on = [google_project_service.required]
}

# Bound by the Filestore backup identities the platform root manages. The
# roles exist live: import them with the commands in
# ../filestore-backup/README.md before applying a target bootstrap root.
module "filestore_backup_roles" {
  source     = "../filestore-backup-roles"
  project_id = var.project_id
  depends_on = [google_project_service.required]
}

resource "google_storage_bucket" "state" {
  project                     = var.project_id
  name                        = "${var.project_id}-lean-tfstate"
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = false
  versioning { enabled = true }
  lifecycle { prevent_destroy = true }
  depends_on = [google_project_service.required]
}

# Terraform state contains credentials. Remove only the implicit Project Viewer
# payload grant; preserve explicit planner reads and Owner/Editor authority.
resource "google_storage_bucket_iam_binding" "state_legacy_object_readers" {
  bucket = google_storage_bucket.state.name
  role   = "roles/storage.legacyObjectReader"
  # Owners retain existing object-owner access; keep a nonempty binding without Viewers.
  members = ["projectOwner:${var.project_id}"]
}

# Exact scope approved for code implementation in docs/production-pipeline-iam-review.md.
# An administrator applies bootstrap; the release account must already exist.
# Non-authoritative members preserve unrelated principals. Secret payload reads
# belong on the four managed database secrets and named release-check fixture,
# never in this project-wide role set. Admin artifacts remain a separate owner.
resource "google_project_iam_member" "release" {
  for_each = local.release_roles
  project  = var.project_id
  role     = each.value
  member   = "serviceAccount:${local.release_account}"

  depends_on = [google_project_service.required]
}

# The predefined Secret Manager Admin role includes versions.access. Keep secret
# management separate from the per-secret payload grants approved for this line.
# This does not sandbox a trusted project IAM administrator against self-grants.
resource "google_project_iam_custom_role" "secret_management" {
  project     = var.project_id
  role_id     = "g5LeanSecretManagement"
  title       = "G5 Lean secret management without payload access"
  description = "Manage target secret containers, policy and versions; payload reads require separate per-secret grants."
  permissions = [
    "resourcemanager.projects.get",
    "secretmanager.locations.get",
    "secretmanager.locations.list",
    "secretmanager.secrets.create",
    "secretmanager.secrets.delete",
    "secretmanager.secrets.get",
    "secretmanager.secrets.getIamPolicy",
    "secretmanager.secrets.list",
    "secretmanager.secrets.setIamPolicy",
    "secretmanager.secrets.update",
    "secretmanager.versions.add",
    "secretmanager.versions.destroy",
    "secretmanager.versions.disable",
    "secretmanager.versions.enable",
    "secretmanager.versions.get",
    "secretmanager.versions.list",
  ]
  depends_on = [google_project_service.required]
}

resource "google_project_iam_member" "release_secret_management" {
  project = var.project_id
  role    = "projects/${var.project_id}/roles/${google_project_iam_custom_role.secret_management.role_id}"
  member  = "serviceAccount:${local.release_account}"
}

# PR-controlled plans use a separate identity from both the QA planner and the
# privileged release runner. These are metadata-read roles, not apply authority.
# The existing Viewer role includes Cloud DNS zone/record reads, so DNS planning
# needs no additional grant and must never receive the release account's DNS role.
resource "google_project_iam_member" "planner" {
  for_each = toset([
    "roles/viewer", "roles/iam.securityReviewer", "roles/secretmanager.viewer",
  ])
  project = var.project_id
  role    = each.value
  member  = "serviceAccount:${local.planner_account}"

  depends_on = [google_project_service.required]
}

# State contains database passwords: state read access is sensitive even without
# Secret Manager payload permissions. The trusted release runner must initialize
# both states first; PR plans may never create or overwrite a state object.
resource "google_storage_bucket_iam_member" "planner_state_read" {
  bucket = google_storage_bucket.state.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${local.planner_account}"
}

resource "google_storage_bucket_iam_member" "planner_state_locks" {
  bucket = google_storage_bucket.state.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${local.planner_account}"
  condition {
    title       = "release_plan_state_locks_only"
    description = "The PR planner may mutate only the platform and addons default-workspace locks."
    expression  = "resource.type == 'storage.googleapis.com/Object' && resource.name in ['projects/_/buckets/${google_storage_bucket.state.name}/objects/platform/default.tflock', 'projects/_/buckets/${google_storage_bucket.state.name}/objects/addons/default.tflock']"
  }
}

output "state_bucket" { value = google_storage_bucket.state.name }
output "release_account" { value = local.release_account }
