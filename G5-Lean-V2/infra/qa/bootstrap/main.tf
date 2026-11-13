# Run once as a QA project administrator. This root does not create projects,
# change billing, or grant access in any other project. Keep its local state
# alongside the bootstrap root; it owns the bucket itself and therefore cannot
# use that bucket as its backend.
terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 5.0"
    }
  }
}

locals {
  project_id = "g5-qa-1ccc"
  region     = "us-central1"
}

provider "google" {
  project = local.project_id
  region  = local.region
}

# Automatic scanning covers new Docker pushes across this artifact project,
# including shared CI/cache images. Enablement adds Artifact Analysis charges.
resource "google_project_service" "required" {
  for_each = toset([
    "artifactregistry.googleapis.com", "bigquery.googleapis.com",
    "containeranalysis.googleapis.com", "containerscanning.googleapis.com",
    "certificatemanager.googleapis.com", "cloudbuild.googleapis.com",
    "cloudkms.googleapis.com", "cloudresourcemanager.googleapis.com",
    "compute.googleapis.com", "container.googleapis.com", "dns.googleapis.com",
    "file.googleapis.com", "iam.googleapis.com", "iamcredentials.googleapis.com",
    "logging.googleapis.com", "monitoring.googleapis.com", "pubsub.googleapis.com",
    "run.googleapis.com", "secretmanager.googleapis.com",
    "servicenetworking.googleapis.com", "serviceusage.googleapis.com",
    "sqladmin.googleapis.com", "storage.googleapis.com",
    "workflowexecutions.googleapis.com", "workflows.googleapis.com",
  ])
  project            = local.project_id
  service            = each.value
  disable_on_destroy = false
}

resource "google_storage_bucket" "state" {
  project                     = local.project_id
  name                        = "g5-qa-1ccc-lean-tfstate"
  location                    = local.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = false
  versioning { enabled = true }
  lifecycle { prevent_destroy = true }
}

# The QEMU worker build imports a raw GCS archive; it never creates a VM or
# attaches a service account. Bootstrap owns this role so the automated runner
# does not need custom-role administration. Apply bootstrap before foundation.
resource "google_project_iam_custom_role" "worker_image_publisher" {
  project     = local.project_id
  role_id     = "g5LeanWorkerImagePublisher"
  title       = "G5 Lean worker image publisher"
  description = "Publish and inspect worker images from raw GCS archives."
  permissions = [
    "compute.images.create", "compute.images.get", "compute.images.list",
    "compute.images.setLabels", "compute.globalOperations.get",
  ]
  depends_on = [google_project_service.required]
}

# Keep runtime custom roles outside the automated platform runner's authority.
module "worker_host_roles" {
  source     = "../../terraform/modules/worker-host-roles"
  project_id = local.project_id
  depends_on = [google_project_service.required]
}

# The Filestore backup identities (platform root) bind these fixed roles. Both
# roles already exist live: import them with the commands in
# infra/terraform/modules/filestore-backup/README.md before applying.
module "filestore_backup_roles" {
  source     = "../../terraform/modules/filestore-backup-roles"
  project_id = local.project_id
  depends_on = [google_project_service.required]
}

# Upload clients resolve exact destinations by listing the bucket. Keep this
# metadata-only permission separate from their prefix-scoped object authority.
resource "google_project_iam_custom_role" "artifact_object_lister" {
  project     = local.project_id
  role_id     = "g5LeanArtifactObjectLister"
  title       = "G5 Lean artifact object lister"
  description = "List artifact object names and metadata for upload destination discovery."
  permissions = ["storage.objects.list"]
  depends_on  = [google_project_service.required]
}

# The identity the QA Terraform Cloud Build triggers run as
# (g5-terraform-plan-qa / g5-terraform-apply-qa in the delivery root), the
# counterpart of dev's `g5-terraform` runner from infra/terraform/bootstrap.
# It is created here, out of band, so it exists before the first automated
# apply of any remote-state root and never has to grant itself its own roles
# in the apply that needs them. Roles are project-local to g5-qa-1ccc; the set
# is dev's runner set reduced to the services this project enables above.
resource "google_service_account" "terraform" {
  project      = local.project_id
  account_id   = "g5-lean-terraform"
  display_name = "G5 Lean QA Terraform runner"
  description  = "Runs the QA Terraform plan/apply Cloud Build triggers"
  depends_on   = [google_project_service.required]
}

# State credentials require explicit reads, not basic Project Viewer membership.
# Manage only the legacy object-reader role; preserve all other bucket roles.
resource "google_storage_bucket_iam_binding" "state_legacy_object_readers" {
  bucket = google_storage_bucket.state.name
  role   = "roles/storage.legacyObjectReader"
  # Owners retain existing object-owner access; keep a nonempty binding without Viewers.
  members = ["projectOwner:${local.project_id}"]
}

resource "google_storage_bucket_iam_member" "terraform_state" {
  bucket = google_storage_bucket.state.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.terraform.email}"
}

resource "google_project_iam_member" "terraform_runner" {
  for_each = toset([
    "roles/artifactregistry.admin",
    "roles/bigquery.admin",
    "roles/certificatemanager.editor",
    "roles/cloudbuild.builds.builder",
    "roles/cloudbuild.builds.editor",
    "roles/cloudbuild.tokenAccessor",
    "roles/cloudbuild.workerPoolOwner",
    "roles/cloudkms.admin",
    "roles/cloudscheduler.admin",
    "roles/cloudsql.admin",
    "roles/compute.admin",
    "roles/container.admin",
    "roles/dns.admin",
    "roles/file.editor",
    "roles/iam.serviceAccountAdmin",
    "roles/logging.admin",
    "roles/logging.logWriter",
    "roles/monitoring.admin",
    "roles/pubsub.admin",
    "roles/resourcemanager.projectIamAdmin",
    "roles/run.admin",
    "roles/servicenetworking.networksAdmin",
    "roles/serviceusage.serviceUsageAdmin",
    "roles/storage.admin",
    # Deploy the Filestore backup workflow (platform root); Cloud Scheduler is
    # already covered above. Workflow IAM is not managed, so editor suffices.
    "roles/workflows.editor",
  ])
  project = local.project_id
  role    = each.value
  member  = "serviceAccount:${google_service_account.terraform.email}"
}

# Secret Manager Admin includes payload access. Match the target bootstrap's
# management-only role; platform grants reads on its four managed DB secrets.
# This reduces direct grants, not the trusted project IAM admin's self-grants.
resource "google_project_iam_custom_role" "terraform_secret_management" {
  project     = local.project_id
  role_id     = "g5LeanSecretManagement"
  title       = "G5 Lean secret management without payload access"
  description = "Manage secret containers, policies and versions; payload reads require separate per-secret grants."
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

resource "google_project_iam_member" "terraform_secret_management" {
  project = local.project_id
  role    = "projects/${local.project_id}/roles/${google_project_iam_custom_role.terraform_secret_management.role_id}"
  member  = "serviceAccount:${google_service_account.terraform.email}"
}

# The identity the pull-request plan trigger runs as. A PR controls the
# pipeline steps it runs (and any `external` data source in its Terraform),
# so the plan must never hold the apply identity's write roles: this account
# can read every resource the roots manage and take the state lock, nothing
# more. Only this separation protects against a plan-time escalation.
resource "google_service_account" "terraform_plan" {
  project      = local.project_id
  account_id   = "g5-lean-terraform-plan"
  display_name = "G5 Lean QA Terraform planner"
  description  = "Runs the read-only QA Terraform plan Cloud Build trigger"
  depends_on   = [google_project_service.required]
}

# PR code runs as the planner, so it must not be able to overwrite trusted
# delivery/IAM state. Read/list state separately; allow mutations only on GCS
# backend lock objects. Terraform 1.9.8 uses <prefix>/<workspace>.tflock and
# creates/deletes the lock with generation preconditions:
# https://github.com/hashicorp/terraform/blob/v1.9.8/internal/backend/remote-state/gcs/backend_state.go
# https://github.com/hashicorp/terraform/blob/v1.9.8/internal/backend/remote-state/gcs/client.go
# GCS supports resource-name prefix/suffix conditions; listing is bucket-scoped:
# https://cloud.google.com/storage/docs/access-control/iam#conditions
# Apply this bootstrap change and the QA API IAM restriction BEFORE creating
# release identities. Check the old unconditional grant is gone. A live planner
# lock/unlock + denied state-write check remains an activation prerequisite;
# mock tests do not validate live IAM. New state/workspaces require the trusted
# apply identity to initialize them first, because initialization writes state.
resource "google_storage_bucket_iam_member" "terraform_plan_state_read" {
  bucket = google_storage_bucket.state.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.terraform_plan.email}"
}
resource "google_storage_bucket_iam_member" "terraform_plan_state" {
  bucket = google_storage_bucket.state.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.terraform_plan.email}"
  condition {
    title       = "terraform_state_locks_only"
    description = "The PR planner may create/delete locks, never Terraform state objects."
    expression  = "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/${google_storage_bucket.state.name}/objects/') && resource.name.endsWith('.tflock')"
  }
}

resource "google_project_iam_member" "terraform_planner" {
  for_each = toset([
    "roles/viewer",
    "roles/iam.securityReviewer",
    "roles/secretmanager.viewer",
    "roles/logging.logWriter",
  ])
  project = local.project_id
  role    = each.value
  member  = "serviceAccount:${google_service_account.terraform_plan.email}"
}

output "state_bucket" { value = google_storage_bucket.state.name }
output "terraform_runner" { value = google_service_account.terraform.email }
output "terraform_planner" { value = google_service_account.terraform_plan.email }
