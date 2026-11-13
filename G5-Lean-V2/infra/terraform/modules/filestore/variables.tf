# =============================================================================
# Filestore Module — Input Variables
# =============================================================================

variable "project_id" {
  description = "GCP project ID"
  type        = string
}

variable "zone" {
  description = "GCP zone for the Filestore instance (e.g., us-central1-a)"
  type        = string
}

variable "instance_name" {
  description = "Name of the Filestore instance"
  type        = string
  default     = "g5-repos"
}

variable "tier" {
  description = "Filestore service tier: BASIC_HDD, BASIC_SSD, HIGH_SCALE_SSD, ZONAL, REGIONAL, ENTERPRISE"
  type        = string
  default     = "BASIC_HDD"

  validation {
    condition     = contains(["BASIC_HDD", "BASIC_SSD", "HIGH_SCALE_SSD", "ZONAL", "REGIONAL", "ENTERPRISE"], var.tier)
    error_message = "Tier must be one of: BASIC_HDD, BASIC_SSD, HIGH_SCALE_SSD, ZONAL, REGIONAL, ENTERPRISE"
  }
}

variable "share_name" {
  description = "Name of the NFS file share"
  type        = string
  default     = "g5_repos"
}

variable "capacity_gb" {
  description = "Capacity of the file share in GB (minimum 1024 for BASIC_HDD/ZONAL/REGIONAL/ENTERPRISE, 2560 for BASIC_SSD, 10240 for HIGH_SCALE_SSD)"
  type        = number
  default     = 1024

  validation {
    condition = (
      var.capacity_gb >= 1024
      && (var.tier != "BASIC_SSD" || var.capacity_gb >= 2560)
      && (var.tier != "HIGH_SCALE_SSD" || var.capacity_gb >= 10240)
    )
    error_message = "Filestore capacity must be at least 1024 GB, at least 2560 GB for BASIC_SSD, and at least 10240 GB for HIGH_SCALE_SSD."
  }
}

variable "network_name" {
  description = "VPC network name (must be in the same project)"
  type        = string
}

variable "network_id" {
  description = "VPC network self-link/ID for firewall rules"
  type        = string
  default     = ""
}

variable "reserved_ip_range" {
  description = "Reserved IP range for the Filestore instance (e.g., 10.0.32.0/29). Leave empty for auto-allocation."
  type        = string
  default     = ""
}

variable "gke_node_cidrs" {
  description = "List of GKE node subnet CIDR ranges allowed to access NFS"
  type        = list(string)
}

variable "labels" {
  description = "Labels to apply to the Filestore instance"
  type        = map(string)
  default     = {}
}

variable "create_k8s_resources" {
  description = "Whether to create Kubernetes PV/PVC resources via Terraform"
  type        = bool
  default     = false
}

variable "k8s_namespace" {
  description = "Kubernetes namespace for the PersistentVolumeClaim"
  type        = string
  default     = "g5"
}

variable "kms_key_name" {
  description = "Cloud KMS key self_link for CMEK encryption of the Filestore instance. Leave empty for Google-managed encryption."
  type        = string
  default     = ""
}
