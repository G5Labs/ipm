# An existing infrastructure operator runs this root. No build or release
# identity is granted access to this state bucket or to admin infrastructure.
# Bootstrap state stays local because this root creates the remote backend.
terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = { source = "hashicorp/google", version = "~> 5.0" }
  }
}

variable "project_id" {
  type    = string
  default = "g5lean-admin-1ccc"
  validation {
    condition     = var.project_id == "g5lean-admin-1ccc"
    error_message = "This bootstrap root may manage only g5lean-admin-1ccc."
  }
}
variable "region" {
  type    = string
  default = "us-central1"
  validation {
    condition     = var.region == "us-central1"
    error_message = "Shared Lean artifacts are provisioned in us-central1."
  }
}

provider "google" {
  project = var.project_id
  region  = var.region
}

# Automatic scanning covers new Docker pushes across this artifact project,
# including shared CI/cache images. Enablement adds Artifact Analysis charges.
resource "google_project_service" "required" {
  for_each = toset([
    "artifactregistry.googleapis.com", "cloudresourcemanager.googleapis.com",
    "containeranalysis.googleapis.com", "containerscanning.googleapis.com",
    "compute.googleapis.com", "iam.googleapis.com",
    "serviceusage.googleapis.com", "storage.googleapis.com",
  ])
  project            = var.project_id
  service            = each.value
  disable_on_destroy = false
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
  labels     = { application = "g5-lean", environment = "admin", purpose = "terraform-state" }
  depends_on = [google_project_service.required]
}

# State access requires an explicit grant, not basic Project Viewer membership.
# This role-only binding preserves operator Owner/Editor and other bucket roles.
resource "google_storage_bucket_iam_binding" "state_legacy_object_readers" {
  bucket = google_storage_bucket.state.name
  role   = "roles/storage.legacyObjectReader"
  # Owners retain existing object-owner access; keep a nonempty binding without Viewers.
  members = ["projectOwner:${var.project_id}"]
}

output "state_bucket" { value = google_storage_bucket.state.name }
