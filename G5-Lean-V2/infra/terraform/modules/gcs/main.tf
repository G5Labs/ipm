# =============================================================================
# GCS Module — Storage buckets with tenant/workspace prefix structure
# =============================================================================
# Bucket structure:
#   gs://<bucket>/tenants/<tenant>/workspaces/<workspace>/repo_snapshots/
#   gs://<bucket>/tenants/<tenant>/workspaces/<workspace>/build_cache/
#   gs://<bucket>/tenants/<tenant>/workspaces/<workspace>/toolchains/
#   gs://<bucket>/tenants/<tenant>/workspaces/<workspace>/artifacts/
#   gs://<bucket>/tenants/<tenant>/workspaces/<workspace>/logs/
# =============================================================================

variable "project_id" { type = string }
variable "region" { type = string }
variable "bucket_name" { type = string }
variable "cache_max_age_days" { type = number }
variable "artifact_max_age_days" { type = number }
variable "labels" {
  description = "Labels applied to the storage bucket."
  type        = map(string)
  default = {
    environment = "production"
    service     = "g5"
  }
}
variable "encryption_key_name" {
  description = "KMS crypto key self-link for default bucket encryption. If empty, uses Google-managed keys."
  type        = string
  default     = ""
}

# -----------------------------------------------------------------------------
# Primary Storage Bucket
# -----------------------------------------------------------------------------
resource "google_storage_bucket" "main" {
  name          = var.bucket_name
  project       = var.project_id
  location      = var.region
  storage_class = "STANDARD"

  # Prevent accidental deletion
  force_destroy = false

  # Uniform bucket-level access (no per-object ACLs)
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"

  # Versioning for critical data
  versioning {
    enabled = true
  }

  # Lifecycle rules for cache eviction
  lifecycle_rule {
    condition {
      age            = var.cache_max_age_days
      matches_prefix = ["tenants/"]
      matches_suffix = ["/build_cache/"]
      with_state     = "ANY"
    }
    action {
      type = "Delete"
    }
  }

  # Lifecycle rule for toolchain cache (longer retention)
  lifecycle_rule {
    condition {
      age = 90 # Toolchains are rebuilt weekly; keep 3 months
    }
    action {
      type = "Delete"
    }
  }

  # Lifecycle rule for artifacts
  lifecycle_rule {
    condition {
      age = var.artifact_max_age_days
    }
    action {
      type = "Delete"
    }
  }

  # Delete non-current versions after 7 days
  lifecycle_rule {
    condition {
      num_newer_versions = 3
      with_state         = "ARCHIVED"
    }
    action {
      type = "Delete"
    }
  }

  # CORS configuration for direct browser uploads (if needed)
  cors {
    origin          = ["*"]
    method          = ["GET", "HEAD"]
    response_header = ["Content-Type", "Content-Range"]
    max_age_seconds = 3600
  }

  labels = var.labels

  # CMEK — customer-managed encryption key for bucket objects
  dynamic "encryption" {
    for_each = var.encryption_key_name != "" ? [var.encryption_key_name] : []
    content {
      default_kms_key_name = encryption.value
    }
  }
}

# -----------------------------------------------------------------------------
# Outputs
# -----------------------------------------------------------------------------
output "bucket_name" {
  value = google_storage_bucket.main.name
}

output "bucket_url" {
  value = google_storage_bucket.main.url
}

output "bucket_self_link" {
  value = google_storage_bucket.main.self_link
}
