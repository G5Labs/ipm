# =============================================================================
# Cloud SQL Module — PostgreSQL 17 with pgvector, private IP
# =============================================================================

variable "project_id" { type = string }
variable "region" { type = string }
variable "instance_name" { type = string }
variable "network_id" { type = string }
variable "private_ip_range_id" { type = string }
variable "database_version" { type = string }
variable "tier" { type = string }
variable "disk_size_gb" { type = number }
variable "disk_autoresize" { type = bool }
variable "availability_type" { type = string }
variable "backup_enabled" { type = bool }
variable "point_in_time_recovery_enabled" { type = bool }
variable "database_name" { type = string }
variable "db_user" { type = string }
variable "database_flags" {
  type = list(object({
    name  = string
    value = string
  }))
  default = []
}

variable "encryption_key_name" {
  description = "KMS crypto key self-link for CMEK disk encryption. If empty, uses Google-managed keys."
  type        = string
  default     = ""
}

# -----------------------------------------------------------------------------
# Cloud SQL Instance
# -----------------------------------------------------------------------------
resource "google_sql_database_instance" "main" {
  name             = var.instance_name
  project          = var.project_id
  region           = var.region
  database_version = var.database_version

  # Prevent accidental deletion
  deletion_protection = true

  settings {
    tier              = var.tier
    disk_size         = var.disk_size_gb
    disk_autoresize   = var.disk_autoresize
    disk_type         = "PD_SSD"
    availability_type = var.availability_type

    # Private IP only — no public endpoint
    ip_configuration {
      ipv4_enabled                                  = false
      private_network                               = var.network_id
      enable_private_path_for_google_cloud_services = true

      # Enforce TLS for all connections
      ssl_mode = "ENCRYPTED_ONLY"
    }

    # Automated backups with PITR
    backup_configuration {
      enabled                        = var.backup_enabled
      point_in_time_recovery_enabled = var.point_in_time_recovery_enabled
      start_time                     = "03:00" # 3 AM UTC
      transaction_log_retention_days = 7

      backup_retention_settings {
        retained_backups = 30
        retention_unit   = "COUNT"
      }
    }

    # Database flags (pgvector, etc.)
    dynamic "database_flags" {
      for_each = var.database_flags
      content {
        name  = database_flags.value.name
        value = database_flags.value.value
      }
    }

    # Maintenance window: Sunday 4-5 AM UTC
    maintenance_window {
      day          = 7 # Sunday
      hour         = 4
      update_track = "stable"
    }

    # Insights for query performance monitoring
    insights_config {
      query_insights_enabled  = true
      query_plans_per_minute  = 5
      query_string_length     = 1024
      record_application_tags = true
      record_client_address   = false
    }
  }

  depends_on = [var.private_ip_range_id]

  # CMEK — customer-managed encryption key for disk + backups
  encryption_key_name = var.encryption_key_name != "" ? var.encryption_key_name : null

  # disk_size_gb is a creation-time floor only. With disk_autoresize, Cloud SQL
  # grows the live disk past it and disks cannot shrink; tracking the attribute
  # would make the first post-growth plan propose a shrink, which forces
  # whole-instance replacement (2026-08-10 dev outage, issue #1768).
  # prevent_destroy fails any destroy/replacement plan at PLAN time — before
  # provisioners run — unlike deletion_protection, which Cloud SQL enforces
  # mid-apply after earlier destroys have already executed. An intentional
  # replacement requires removing the guard first (terraform_cloudsql [D9]).
  lifecycle {
    prevent_destroy = true
    ignore_changes  = [settings[0].disk_size]
  }
}

# -----------------------------------------------------------------------------
# Database
# -----------------------------------------------------------------------------
resource "google_sql_database" "main" {
  name     = var.database_name
  project  = var.project_id
  instance = google_sql_database_instance.main.name
}

# -----------------------------------------------------------------------------
# Random passwords for database users
# -----------------------------------------------------------------------------
resource "random_password" "db_password" {
  length  = 32
  special = false
}

resource "random_password" "authz_writer_password" {
  length  = 32
  special = false
}

resource "random_password" "runtime_system_password" {
  length  = 32
  special = false
}

resource "random_password" "runtime_tenant_password" {
  length  = 32
  special = false
}

# -----------------------------------------------------------------------------
# Database User — primary application user
# -----------------------------------------------------------------------------
resource "google_sql_user" "main" {
  name     = var.db_user
  project  = var.project_id
  instance = google_sql_database_instance.main.name
  password = random_password.db_password.result
}

# -----------------------------------------------------------------------------
# Database Users — authz roles (M2 multi-org RLS)
# -----------------------------------------------------------------------------
# authz_writer: manages session rows in authz.user_sessions.
# Used by the authz pool (InProcessAuthz::authz_pool).
resource "google_sql_user" "authz_writer" {
  name     = "authz_writer"
  project  = var.project_id
  instance = google_sql_database_instance.main.name
  password = random_password.authz_writer_password.result
}

# authz_ctx_writer (per-connection RLS session context) is not a Cloud SQL
# user. g5-api only reaches it with SET ROLE inside ScopedDb transactions, and
# the migrate job (helm job-migrate.yaml) creates it and forces NOLOGIN on
# every deploy. Cloud SQL does not report a NOLOGIN role as a user, so a
# google_sql_user here was dropped at each refresh and its re-create then
# failed with "role already exists". Terraform forgets the old resources
# without destroying anything; the database role is untouched.
removed {
  from = google_sql_user.authz_ctx_writer
  lifecycle {
    destroy = false
  }
}

removed {
  from = random_password.authz_ctx_writer_password
  lifecycle {
    destroy = false
  }
}

# -----------------------------------------------------------------------------
# Outputs
# -----------------------------------------------------------------------------
output "connection_name" {
  value = google_sql_database_instance.main.connection_name
}

output "private_ip" {
  value     = google_sql_database_instance.main.private_ip_address
  sensitive = true
}

output "instance_name" {
  value = google_sql_database_instance.main.name
}

output "db_password" {
  value     = random_password.db_password.result
  sensitive = true
}

output "authz_writer_password" {
  value     = random_password.authz_writer_password.result
  sensitive = true
}

output "runtime_system_password" {
  value     = random_password.runtime_system_password.result
  sensitive = true
}

output "runtime_tenant_password" {
  value     = random_password.runtime_tenant_password.result
  sensitive = true
}
