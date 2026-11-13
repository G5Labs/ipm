variable "project_id" {
  description = "Project that owns the source instance, the backups and the automation identities."
  type        = string
}

variable "region" {
  description = "Region of the backups, the workflow and the scheduler job."
  type        = string
  validation {
    condition     = can(regex("^[a-z]+-[a-z]+[0-9]+$", var.region))
    error_message = "Region must be a Google Cloud region such as us-central1."
  }
}

variable "source_instance" {
  description = "Fixed Filestore instance name; the workflow backs up this instance only."
  type        = string
  validation {
    condition     = can(regex("^[a-z]([-a-z0-9]{0,38}[a-z0-9])?$", var.source_instance))
    error_message = "The source instance must be a valid Filestore instance name (at most 40 characters) so the daily backup name stays under 63 characters."
  }
}

variable "source_zone" {
  description = "Zone of the source instance; must lie in the backup region."
  type        = string
  validation {
    condition     = startswith(var.source_zone, "${var.region}-")
    error_message = "The source zone must belong to the backup region."
  }
}

variable "share_name" {
  description = "Fixed file share to back up; the workflow verifies that the created/reused backup has this source share."
  type        = string
  validation {
    condition     = can(regex("^[a-z][a-z0-9_]{0,15}$", var.share_name))
    error_message = "The share name must be a valid Filestore share name."
  }
}

variable "daily_time_utc" {
  description = "Daily start time of the backup run, HH:MM in UTC."
  type        = string
  validation {
    condition     = can(regex("^([01][0-9]|2[0-3]):[0-5][0-9]$", var.daily_time_utc))
    error_message = "Daily time must be HH:MM in 24-hour UTC."
  }
}

variable "retention_days" {
  description = "Preserved 30-day retention for owned daily/initial backups after today's backup is READY."
  type        = number
  default     = 30
  validation {
    condition     = var.retention_days == 30
    error_message = "Adoption preserves the deployed 30-day retention policy."
  }
}

variable "runner_role_id" {
  description = "Project custom role the backup identity binds; defined by the filestore-backup-roles module in bootstrap."
  type        = string
  default     = "g5FilestoreBackupRunner"
}

variable "scheduler_role_id" {
  description = "Project custom role the scheduler identity binds; defined by the filestore-backup-roles module in bootstrap."
  type        = string
  default     = "g5FilestoreBackupScheduler"
}

variable "attach_members" {
  description = "Members that deploy the workflow and scheduler job and therefore need serviceAccountUser on both identities (QA's per-account attachment pattern). Empty where the applying identity already holds project-wide attachment."
  type        = set(string)
  default     = []
  validation {
    condition     = alltrue([for member in var.attach_members : startswith(member, "serviceAccount:")])
    error_message = "Only service accounts deploy this automation."
  }
}
