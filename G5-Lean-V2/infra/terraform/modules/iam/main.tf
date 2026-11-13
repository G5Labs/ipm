# =============================================================================
# IAM Module — Service accounts, Workload Identity bindings
# =============================================================================
# Application service accounts bound via Workload Identity, plus the dedicated
# outer worker-host identity. The worker base identity has bucket-wide object
# access; this module does not establish per-workspace GCS isolation.
# =============================================================================

variable "project_id" { type = string }
variable "gke_cluster_name" { type = string }
variable "gke_location" { type = string }
variable "api_sa_name" { type = string }
variable "worker_sa_name" { type = string }
variable "gcs_bucket_name" { type = string }
variable "k8s_namespaces" {
  description = "Kubernetes namespaces for workload identity bindings"
  type        = list(string)
  default     = ["g5"]
}
variable "enable_api_managed_worker_hosts" {
  description = "Create the dedicated host identity and bind bootstrap-owned worker-host Compute roles. Apply worker-host-roles in operator bootstrap before enabling."
  type        = bool
  default     = false
}
variable "enable_legacy_workspace_identity_admin" {
  description = "Legacy workspace service-account administration. Current hosted runtime does not use it; disable before introducing privileged release identities. Registry token minting uses a separate named-puller binding."
  type        = bool
  default     = true
}

# -----------------------------------------------------------------------------
# API Server Service Account
# Used by g5-api pods for Cloud SQL and GCS object operations
# -----------------------------------------------------------------------------
resource "google_service_account" "api" {
  account_id   = var.api_sa_name
  project      = var.project_id
  display_name = "G5 API Server"
  description  = "Service account for g5-api pods (Cloud SQL, GCS objects)"
}

# Cloud SQL Client role
resource "google_project_iam_member" "api_cloudsql" {
  project = var.project_id
  role    = "roles/cloudsql.client"
  member  = "serviceAccount:${google_service_account.api.email}"
}

# Preserve object access without runtime bucket configuration or IAM authority.
resource "google_storage_bucket_iam_member" "api_gcs_objects" {
  bucket = var.gcs_bucket_name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.api.email}"
  # Replacing storage.admin must install both narrower grants before removal.
  # Effective IAM propagation still needs the staged rollout checks.
  depends_on = [google_storage_bucket_iam_member.api_gcs_metadata]
  lifecycle { create_before_destroy = true }
}
resource "google_storage_bucket_iam_member" "api_gcs_metadata" {
  bucket = var.gcs_bucket_name
  role   = "roles/storage.bucketViewer"
  member = "serviceAccount:${google_service_account.api.email}"
}
moved {
  from = google_storage_bucket_iam_member.api_gcs_admin
  to   = google_storage_bucket_iam_member.api_gcs_objects
}

# Workload Identity binding: K8s SA "g5-api" in each application namespace
resource "google_service_account_iam_member" "api_workload_identity" {
  for_each           = toset(var.k8s_namespaces)
  service_account_id = google_service_account.api.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "serviceAccount:${var.project_id}.svc.id.goog[${each.value}/g5-api]"
}

# -----------------------------------------------------------------------------
# Worker-host provisioning roles (microVM with-identity worker default)
# -----------------------------------------------------------------------------
# Only the API binds these bootstrap-owned roles. The typed resource condition
# protects existing infrastructure VMs/disks/firewalls. Supporting network
# authority remains project-wide; a host-prefixed resource can still be created
# in another VPC. See worker-host-roles for the exact permission rationale.
# -----------------------------------------------------------------------------
locals {
  api_worker_host_roles = var.enable_api_managed_worker_hosts ? toset([
    "g5LeanWorkerHostResources",
    "g5LeanWorkerHostSupport",
  ]) : toset([])
}
resource "google_project_iam_member" "api_worker_host_provisioning" {
  for_each = local.api_worker_host_roles
  project  = var.project_id
  role     = "projects/${var.project_id}/roles/${each.value}"
  member   = "serviceAccount:${google_service_account.api.email}"
  dynamic "condition" {
    for_each = each.value == "g5LeanWorkerHostResources" ? [true] : []
    content {
      title       = "worker_host_resources_only"
      description = "Named host instances/boot disks and organization firewalls; supporting networking is a separate grant."
      expression = join(" || ", [
        "(resource.type == 'compute.googleapis.com/Instance' && resource.name.startsWith('projects/${var.project_id}/zones/') && resource.name.extract('/instances/{name}').startsWith('g5-host-'))",
        "(resource.type == 'compute.googleapis.com/Disk' && resource.name.startsWith('projects/${var.project_id}/zones/') && resource.name.extract('/disks/{name}').startsWith('g5-host-'))",
        "(resource.type == 'compute.googleapis.com/Firewall' && resource.name.startsWith('projects/${var.project_id}/global/firewalls/g5-org-'))",
      ])
    }
  }
}

# Host identity is separate from GKE's node SA. Artifact/cache readers belong
# to the roots that own those resources; host telemetry is project-local.
resource "google_service_account" "host" {
  count        = var.enable_api_managed_worker_hosts ? 1 : 0
  project      = var.project_id
  account_id   = "${trimsuffix(var.api_sa_name, "-api")}-host"
  display_name = "G5 worker host"
  description  = "Outer worker VM artifact reads and telemetry; never a GKE node identity."
}
resource "google_service_account_iam_member" "api_actas_host" {
  count              = var.enable_api_managed_worker_hosts ? 1 : 0
  service_account_id = google_service_account.host[0].name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${google_service_account.api.email}"
}
resource "google_project_iam_member" "host_telemetry" {
  for_each = var.enable_api_managed_worker_hosts ? toset(["roles/logging.logWriter", "roles/monitoring.metricWriter"]) : toset([])
  project  = var.project_id
  role     = each.key
  member   = "serviceAccount:${google_service_account.host[0].email}"
}

# -----------------------------------------------------------------------------
# Worker Base Service Account
# Used by worker pods for bucket-wide GCS cache access
# -----------------------------------------------------------------------------
resource "google_service_account" "worker" {
  account_id   = var.worker_sa_name
  project      = var.project_id
  display_name = "G5 Worker (Base)"
  description  = "Base service account for g5d worker pods (GCS cache access)"
}

# Bucket-wide GCS Object Viewer; this binding has no workspace condition.
resource "google_storage_bucket_iam_member" "worker_gcs_viewer" {
  bucket = var.gcs_bucket_name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.worker.email}"
}

# GCS Object Creator at bucket level (for uploading cache artifacts)
resource "google_storage_bucket_iam_member" "worker_gcs_creator" {
  bucket = var.gcs_bucket_name
  role   = "roles/storage.objectCreator"
  member = "serviceAccount:${google_service_account.worker.email}"
}

# Workload Identity binding: K8s SA "g5-worker" in each application namespace
resource "google_service_account_iam_member" "worker_workload_identity" {
  for_each           = toset(var.k8s_namespaces)
  service_account_id = google_service_account.worker.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "serviceAccount:${var.project_id}.svc.id.goog[${each.value}/g5-worker]"
}


# -----------------------------------------------------------------------------
# Legacy workspace identity administration compatibility
# Current hosted runtime does not create per-workspace service accounts. These
# optional project-wide grants remain only for existing legacy consumers.
# -----------------------------------------------------------------------------

# Legacy API service-account creation authority
resource "google_project_iam_member" "api_sa_creator" {
  count   = var.enable_legacy_workspace_identity_admin ? 1 : 0
  project = var.project_id
  role    = "roles/iam.serviceAccountCreator"
  member  = "serviceAccount:${google_service_account.api.email}"
}

# Legacy API project-wide token creation authority
resource "google_project_iam_member" "api_token_creator" {
  count   = var.enable_legacy_workspace_identity_admin ? 1 : 0
  project = var.project_id
  role    = "roles/iam.serviceAccountTokenCreator"
  member  = "serviceAccount:${google_service_account.api.email}"
}

# Legacy API workload identity pool administration authority
resource "google_project_iam_member" "api_wi_admin" {
  count   = var.enable_legacy_workspace_identity_admin ? 1 : 0
  project = var.project_id
  role    = "roles/iam.workloadIdentityPoolAdmin"
  member  = "serviceAccount:${google_service_account.api.email}"
}

# Preserve state addresses for existing consumers that retain the old setting.
# Environments opting out remove only these obsolete broad grants.
moved {
  from = google_project_iam_member.api_sa_creator
  to   = google_project_iam_member.api_sa_creator[0]
}
moved {
  from = google_project_iam_member.api_token_creator
  to   = google_project_iam_member.api_token_creator[0]
}
moved {
  from = google_project_iam_member.api_wi_admin
  to   = google_project_iam_member.api_wi_admin[0]
}

# -----------------------------------------------------------------------------
# Outputs
# -----------------------------------------------------------------------------
output "api_sa_email" {
  value      = "${google_service_account.api.account_id}@${var.project_id}.iam.gserviceaccount.com"
  depends_on = [google_service_account.api]
}

output "api_sa_name" {
  value = google_service_account.api.name
}

output "worker_sa_email" {
  value      = "${google_service_account.worker.account_id}@${var.project_id}.iam.gserviceaccount.com"
  depends_on = [google_service_account.worker]
}

output "worker_sa_name" {
  value = google_service_account.worker.name
}

output "host_sa_email" {
  value      = try("${google_service_account.host[0].account_id}@${var.project_id}.iam.gserviceaccount.com", null)
  depends_on = [google_service_account.host]
}

output "release_identity_isolation_ready" {
  description = "True only when the API's legacy project-wide identity-administration bindings are absent."
  value       = !var.enable_legacy_workspace_identity_admin
  depends_on = [
    google_project_iam_member.api_sa_creator,
    google_project_iam_member.api_token_creator,
    google_project_iam_member.api_wi_admin,
  ]
}
