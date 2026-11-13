# =============================================================================
# GKE Module — Cluster with a system node pool (the in-cluster worker plane is retired)
# =============================================================================

variable "project_id" { type = string }
variable "region" { type = string }
variable "cluster_name" { type = string }
variable "network_id" { type = string }
variable "subnetwork_id" { type = string }
variable "pod_cidr" { type = string }
variable "service_cidr" { type = string }
variable "pod_secondary_range_name" {
  description = "Secondary subnet range name for pod IPs"
  type        = string
  default     = "pods"
}
variable "service_secondary_range_name" {
  description = "Secondary subnet range name for service IPs"
  type        = string
  default     = "services"
}
variable "deletion_protection" {
  description = "Protect the cluster from deletion. Existing environments retain their current default."
  type        = bool
  default     = false
}
variable "system_node_count" { type = number }
variable "system_machine_type" { type = string }
variable "system_disk_size_gb" { type = number }
variable "system_disk_type" {
  description = "Boot disk type for system pool nodes (pd-ssd, pd-balanced, pd-standard)"
  type        = string
  default     = "pd-balanced"
}
variable "workload_identity_enabled" { type = bool }
variable "database_encryption_key" {
  description = "KMS crypto key ID for GKE application-layer secrets encryption. If empty, uses Google-managed keys."
  type        = string
  default     = ""
}
variable "enable_mesh" {
  description = "Enable Anthos Service Mesh (managed Istio) for mTLS between pods"
  type        = bool
  default     = false
}

variable "enable_private_cluster" {
  description = "Enable private cluster (private nodes + private control plane endpoint)"
  type        = bool
  default     = true
}

variable "master_ipv4_cidr_block" {
  description = "CIDR block for the GKE control plane VPC. Must be /28."
  type        = string
  default     = "172.16.0.0/28"
}

variable "master_authorized_cidr_blocks" {
  description = "CIDR blocks authorized to access the GKE control plane API"
  type = list(object({
    cidr_block   = string
    display_name = string
  }))
  default = []
}

# -----------------------------------------------------------------------------
# GKE Cluster
# -----------------------------------------------------------------------------
resource "google_container_cluster" "primary" {
  provider = google-beta

  name     = var.cluster_name
  project  = var.project_id
  location = var.region

  deletion_protection = var.deletion_protection

  # Use a separately managed node pool
  remove_default_node_pool = true
  initial_node_count       = 1

  # Temporary default pool config used only during cluster creation.
  # Keep it aligned with the system pool so Terraform does not churn on the
  # removed default pool settings after the real node pools take over.
  node_config {
    disk_type    = var.system_disk_type
    disk_size_gb = var.system_disk_size_gb
    tags         = ["g5-system"]
  }

  network    = var.network_id
  subnetwork = var.subnetwork_id

  ip_allocation_policy {
    cluster_secondary_range_name  = var.pod_secondary_range_name
    services_secondary_range_name = var.service_secondary_range_name
  }

  # Required for GKE `gce` Ingress resources and ManagedCertificate
  # provisioning. Without this addon, the ingress never gets a public
  # address, the reserved static IP stays unattached, and cert issuance
  # stalls at FailedNotVisible.
  addons_config {
    http_load_balancing {
      disabled = false
    }
  }

  # Workload Identity for pod-level IAM
  workload_identity_config {
    workload_pool = "${var.project_id}.svc.id.goog"
  }

  # Network policy (Calico) for worker pod isolation
  network_policy {
    enabled  = true
    provider = "CALICO"
  }

  # Binary Authorization (optional, for production hardening)
  # binary_authorization { evaluation_mode = "PROJECT_SINGLETON_POLICY_ENFORCE" }

  # Private cluster — nodes have internal IPs only, control plane has private endpoint
  dynamic "private_cluster_config" {
    for_each = var.enable_private_cluster ? [1] : []
    content {
      enable_private_nodes    = true
      enable_private_endpoint = false # Allow kubectl from authorized networks
      master_ipv4_cidr_block  = var.master_ipv4_cidr_block
    }
  }

  # Authorized networks — restrict who can reach the K8s API server
  dynamic "master_authorized_networks_config" {
    for_each = length(var.master_authorized_cidr_blocks) > 0 ? [1] : []
    content {
      dynamic "cidr_blocks" {
        for_each = var.master_authorized_cidr_blocks
        content {
          cidr_block   = cidr_blocks.value.cidr_block
          display_name = cidr_blocks.value.display_name
        }
      }
    }
  }

  # Maintenance window: Sunday 2-6 AM UTC
  maintenance_policy {
    daily_maintenance_window {
      start_time = "02:00"
    }
  }

  # Release channel: Regular for stability with reasonable update cadence
  release_channel {
    channel = "REGULAR"
  }

  # Application-layer secrets encryption (CMEK for etcd)
  dynamic "database_encryption" {
    for_each = var.database_encryption_key != "" ? [var.database_encryption_key] : []
    content {
      state    = "ENCRYPTED"
      key_name = database_encryption.value
    }
  }

  # Logging and monitoring
  logging_config {
    enable_components = ["SYSTEM_COMPONENTS", "WORKLOADS"]
  }

  monitoring_config {
    enable_components = ["SYSTEM_COMPONENTS"]
    managed_prometheus { enabled = true }
  }

  # GKE Gateway API support — prerequisite for Istio/ASM
  gateway_api_config {
    channel = "CHANNEL_STANDARD"
  }

  lifecycle {
    # GKE persists transient/default-pool fields on the cluster resource even
    # when we immediately delete that pool and manage real node pools
    # separately. Ignoring those fields keeps follow-up plans from trying to
    # replace the whole cluster just because GKE reflected worker/system pool
    # settings back onto the cluster object.
    ignore_changes = [
      initial_node_count,
      node_config,
      node_pool,
    ]
  }
}

# -----------------------------------------------------------------------------
# Anthos Service Mesh (managed Istio) — mTLS for all pod-to-pod traffic
# Enabled via the mesh_certificates fleet feature. Requires google-beta.
# This installs the managed Istio control plane on the cluster.
# After enabling, label your namespace for sidecar injection:
#   kubectl label namespace g5 istio-injection=enabled
# -----------------------------------------------------------------------------
resource "google_gke_hub_membership" "primary" {
  count = var.enable_mesh ? 1 : 0

  provider      = google-beta
  project       = var.project_id
  membership_id = var.cluster_name
  location      = var.region

  endpoint {
    gke_cluster {
      resource_link = "//container.googleapis.com/${google_container_cluster.primary.id}"
    }
  }

  # The Fleet membership API can reject clusters that are still busy creating
  # their managed node pools, so wait for the pools to settle first.
  depends_on = [
    google_container_node_pool.system,
  ]
}

resource "google_gke_hub_feature" "mesh" {
  count = var.enable_mesh ? 1 : 0

  provider = google-beta
  project  = var.project_id
  name     = "servicemesh"
  location = "global"
}

resource "google_gke_hub_feature_membership" "mesh" {
  count = var.enable_mesh ? 1 : 0

  provider            = google-beta
  project             = var.project_id
  feature             = google_gke_hub_feature.mesh[0].name
  location            = "global"
  membership          = google_gke_hub_membership.primary[0].membership_id
  membership_location = var.region

  mesh {
    management = "MANAGEMENT_AUTOMATIC"
  }
}

# -----------------------------------------------------------------------------
# System Node Pool — g5-api, control plane workloads
# -----------------------------------------------------------------------------
resource "google_container_node_pool" "system" {
  name     = "system"
  project  = var.project_id
  location = var.region
  cluster  = google_container_cluster.primary.name

  node_count = var.system_node_count

  node_config {
    machine_type = var.system_machine_type
    disk_size_gb = var.system_disk_size_gb
    disk_type    = var.system_disk_type

    oauth_scopes = [
      "https://www.googleapis.com/auth/cloud-platform",
    ]

    labels = {
      "g5/pool" = "system"
    }

    tags = ["g5-system"]

    # Workload Identity
    workload_metadata_config {
      mode = "GKE_METADATA"
    }

    shielded_instance_config {
      enable_secure_boot          = true
      enable_integrity_monitoring = true
    }
  }

  management {
    auto_repair  = true
    auto_upgrade = true
  }
}

# NOTE: The legacy in-platform "worker" node pool was deleted with the rest
# of the in-cluster worker plane (infra ontology D17). It had been kept at
# scale-to-zero to avoid churn, but the Kata strip forced a full pool
# replacement anyway — and replacement of the statically-named pool collides
# with itself at create time. Jobs run on bare g5-worker hosts.
# -----------------------------------------------------------------------------
# Outputs
# -----------------------------------------------------------------------------
output "cluster_name" {
  value = google_container_cluster.primary.name
}

output "cluster_endpoint" {
  value     = google_container_cluster.primary.endpoint
  sensitive = true
}

output "cluster_ca_certificate" {
  value     = google_container_cluster.primary.master_auth[0].cluster_ca_certificate
  sensitive = true
}

output "cluster_location" {
  value = google_container_cluster.primary.location
}
