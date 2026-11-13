# =============================================================================
# KMS Module — Customer-Managed Encryption Keys (CMEK)
# =============================================================================
# Provides a KMS keyring and per-service crypto keys for encrypting data at
# rest across all GCP services. Keys are rotated automatically every 90 days.
#
# Keys provisioned:
#   - cloudsql   — Cloud SQL disk + backup encryption
#   - gke        — GKE etcd / application-layer secrets encryption
#   - gcs        — GCS bucket default encryption
#   - filestore  — Filestore NFS volume encryption
#   - secrets    — GCP Secret Manager envelope encryption
# =============================================================================

variable "project_id" { type = string }
variable "region" { type = string }
variable "keyring_name" {
  type    = string
  default = "g5-keyring"
}
variable "rotation_period" {
  description = "Key rotation interval (default 90 days)"
  type        = string
  default     = "7776000s" # 90 days
}
variable "labels" {
  type    = map(string)
  default = {}
}

# -----------------------------------------------------------------------------
# KMS Keyring — single ring per region
# -----------------------------------------------------------------------------
resource "google_kms_key_ring" "main" {
  name     = var.keyring_name
  project  = var.project_id
  location = var.region
}

# -----------------------------------------------------------------------------
# Crypto Keys — one per service, auto-rotated
# -----------------------------------------------------------------------------
resource "google_kms_crypto_key" "cloudsql" {
  name            = "cloudsql"
  key_ring        = google_kms_key_ring.main.id
  purpose         = "ENCRYPT_DECRYPT"
  rotation_period = var.rotation_period

  version_template {
    algorithm        = "GOOGLE_SYMMETRIC_ENCRYPTION"
    protection_level = "SOFTWARE"
  }

  labels = var.labels

  lifecycle {
    prevent_destroy = true
  }
}

resource "google_kms_crypto_key" "gke" {
  name            = "gke"
  key_ring        = google_kms_key_ring.main.id
  purpose         = "ENCRYPT_DECRYPT"
  rotation_period = var.rotation_period

  version_template {
    algorithm        = "GOOGLE_SYMMETRIC_ENCRYPTION"
    protection_level = "SOFTWARE"
  }

  labels = var.labels

  lifecycle {
    prevent_destroy = true
  }
}

resource "google_kms_crypto_key" "gcs" {
  name            = "gcs"
  key_ring        = google_kms_key_ring.main.id
  purpose         = "ENCRYPT_DECRYPT"
  rotation_period = var.rotation_period

  version_template {
    algorithm        = "GOOGLE_SYMMETRIC_ENCRYPTION"
    protection_level = "SOFTWARE"
  }

  labels = var.labels

  lifecycle {
    prevent_destroy = true
  }
}

resource "google_kms_crypto_key" "filestore" {
  name            = "filestore"
  key_ring        = google_kms_key_ring.main.id
  purpose         = "ENCRYPT_DECRYPT"
  rotation_period = var.rotation_period

  version_template {
    algorithm        = "GOOGLE_SYMMETRIC_ENCRYPTION"
    protection_level = "SOFTWARE"
  }

  labels = var.labels

  lifecycle {
    prevent_destroy = true
  }
}

resource "google_kms_crypto_key" "secrets" {
  name            = "secrets"
  key_ring        = google_kms_key_ring.main.id
  purpose         = "ENCRYPT_DECRYPT"
  rotation_period = var.rotation_period

  version_template {
    algorithm        = "GOOGLE_SYMMETRIC_ENCRYPTION"
    protection_level = "SOFTWARE"
  }

  labels = var.labels

  lifecycle {
    prevent_destroy = true
  }
}

# -----------------------------------------------------------------------------
# IAM — Grant GCP service agents access to use CMEK keys
# -----------------------------------------------------------------------------

# Cloud SQL service agent needs encrypter/decrypter role
data "google_project" "current" {
  project_id = var.project_id
}

# Service identities — ensure GCP service agents exist before granting KMS access.
# Without this, IAM bindings fail with "service account does not exist" on new projects.
resource "google_project_service_identity" "cloudsql" {
  provider = google-beta
  project  = var.project_id
  service  = "sqladmin.googleapis.com"
}

# GCS uses a per-project service account, not google_project_service_identity
data "google_storage_project_service_account" "gcs" {
  project = var.project_id
}

resource "google_project_service_identity" "secretmanager" {
  provider = google-beta
  project  = var.project_id
  service  = "secretmanager.googleapis.com"
}

# Cloud SQL service agent
resource "google_kms_crypto_key_iam_member" "cloudsql_sa" {
  crypto_key_id = google_kms_crypto_key.cloudsql.id
  role          = "roles/cloudkms.cryptoKeyEncrypterDecrypter"
  member        = "serviceAccount:${google_project_service_identity.cloudsql.email}"
}

# GKE service agent (auto-created when container.googleapis.com is enabled)
resource "google_kms_crypto_key_iam_member" "gke_sa" {
  crypto_key_id = google_kms_crypto_key.gke.id
  role          = "roles/cloudkms.cryptoKeyEncrypterDecrypter"
  member        = "serviceAccount:service-${data.google_project.current.number}@container-engine-robot.iam.gserviceaccount.com"
}

# GCS service agent
resource "google_kms_crypto_key_iam_member" "gcs_sa" {
  crypto_key_id = google_kms_crypto_key.gcs.id
  role          = "roles/cloudkms.cryptoKeyEncrypterDecrypter"
  member        = "serviceAccount:${data.google_storage_project_service_account.gcs.email_address}"
}

# Filestore service agent
resource "google_kms_crypto_key_iam_member" "filestore_sa" {
  crypto_key_id = google_kms_crypto_key.filestore.id
  role          = "roles/cloudkms.cryptoKeyEncrypterDecrypter"
  member        = "serviceAccount:service-${data.google_project.current.number}@cloud-filer.iam.gserviceaccount.com"
}

# Secret Manager service agent
resource "google_kms_crypto_key_iam_member" "secretmanager_sa" {
  crypto_key_id = google_kms_crypto_key.secrets.id
  role          = "roles/cloudkms.cryptoKeyEncrypterDecrypter"
  member        = "serviceAccount:${google_project_service_identity.secretmanager.email}"
}

# -----------------------------------------------------------------------------
# Outputs
# -----------------------------------------------------------------------------
output "keyring_id" {
  value = google_kms_key_ring.main.id
}

output "cloudsql_key_id" {
  value = google_kms_crypto_key.cloudsql.id
}

output "gke_key_id" {
  value = google_kms_crypto_key.gke.id
}

output "gcs_key_id" {
  description = "GCS CMEK key ID. Depends on GCS SA IAM binding being ready."
  value       = google_kms_crypto_key.gcs.id
  depends_on  = [google_kms_crypto_key_iam_member.gcs_sa]
}

output "filestore_key_id" {
  value = google_kms_crypto_key.filestore.id
}

output "secrets_key_id" {
  description = "Secret Manager CMEK key ID. Depends on SM SA IAM binding being ready."
  value       = google_kms_crypto_key.secrets.id
  depends_on  = [google_kms_crypto_key_iam_member.secretmanager_sa]
}

# Expose the SM service identity email so the secrets module can grant pubsub access
output "secretmanager_sa_email" {
  value = google_project_service_identity.secretmanager.email
}
