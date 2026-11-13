# Separate resource address/provider: never change the protected Basic instance
# into a different tier. No Kubernetes bindings are created by preparation.
terraform {
  required_providers {
    google-beta = { source = "hashicorp/google-beta", version = "~> 7.0" }
  }
}

variable "project_id" { type = string }
variable "instance_name" { type = string }
variable "network_name" { type = string }
variable "labels" {
  type    = map(string)
  default = {}
}
variable "kms_key_name" {
  type = string
  validation {
    condition     = can(regex("^projects/[^/]+/locations/us-central1/keyRings/[^/]+/cryptoKeys/[^/]+$", var.kms_key_name))
    error_message = "The Zonal candidate requires an existing CMEK in us-central1."
  }
}

resource "google_filestore_instance" "repos" {
  provider = google-beta
  project  = var.project_id
  name     = var.instance_name
  location = "us-central1-a"
  tier     = "ZONAL"
  protocol = "NFS_V3"

  file_shares {
    name        = "g5_repos"
    capacity_gb = 1024
  }
  performance_config {
    fixed_iops { max_iops = 4000 }
  }
  networks {
    network      = var.network_name
    modes        = ["MODE_IPV4"]
    connect_mode = "DIRECT_PEERING"
    # Omit reserved_ip_range: allocate a new free /26 for ZONAL, never reuse
    # the Basic instance's /29 or reserve a guessed range in its VPC.
  }

  kms_key_name                = var.kms_key_name
  deletion_protection_enabled = true
  labels = merge(var.labels, {
    managed_by = "terraform"
    component  = "filestore"
  })
  lifecycle { prevent_destroy = true }
}

output "instance_name" { value = google_filestore_instance.repos.name }
output "instance_id" { value = google_filestore_instance.repos.id }
output "ip_address" { value = google_filestore_instance.repos.networks[0].ip_addresses[0] }
