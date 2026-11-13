# =============================================================================
# Filestore Module — Managed NFS for Multi-Worker Shared Repository Storage
# =============================================================================
# Provisions a Google Cloud Filestore instance that provides an NFS share
# for shared repository storage across multiple g5d worker pods.
#
# Architecture:
#   - Filestore instance in the same VPC as GKE
#   - ReadWriteMany PersistentVolume via NFS CSI driver
#   - Workers mount `/mnt/g5-repos/` for O_CREAT|O_EXCL lockfile coordination
#   - Firewall rules restrict NFS (port 2049) to GKE node CIDR only
#
# Implements §4.7 Change 1 of SPEC_CHANGE_PROPOSAL.md.
# =============================================================================

# -----------------------------------------------------------------------------
# Filestore Instance
# -----------------------------------------------------------------------------
resource "google_filestore_instance" "repos" {
  project  = var.project_id
  name     = var.instance_name
  location = var.zone
  tier     = var.tier

  file_shares {
    name        = var.share_name
    capacity_gb = var.capacity_gb
  }

  networks {
    network           = var.network_name
    modes             = ["MODE_IPV4"]
    reserved_ip_range = var.reserved_ip_range
  }

  labels = merge(var.labels, {
    managed_by = "terraform"
    component  = "filestore"
  })

  # CMEK — encrypt NFS volume with customer-managed key
  # Note: Only ENTERPRISE / HIGH_SCALE_SSD tiers support CMEK. BASIC_* tiers do not.
  kms_key_name = !startswith(var.tier, "BASIC") && var.kms_key_name != "" ? var.kms_key_name : null

  lifecycle {
    prevent_destroy = true
  }
}

# -----------------------------------------------------------------------------
# Firewall Rule — Allow NFS from GKE nodes only
# -----------------------------------------------------------------------------
resource "google_compute_firewall" "allow_nfs_from_gke" {
  project = var.project_id
  name    = "${var.instance_name}-allow-nfs"
  network = var.network_id != "" ? var.network_id : var.network_name

  allow {
    protocol = "tcp"
    ports    = ["2049"]
  }

  # Restrict to GKE node subnet CIDR ranges
  source_ranges = var.gke_node_cidrs

  target_tags = ["nfs-client"]

  description = "Allow NFS (port 2049/tcp) from GKE node subnets to Filestore"
}

# -----------------------------------------------------------------------------
# Kubernetes PersistentVolume + PersistentVolumeClaim (ReadWriteMany)
# -----------------------------------------------------------------------------
# These are output as rendered manifests. Apply via kubectl or Helm chart.
# Alternatively, use the Filestore CSI driver for dynamic provisioning.

resource "kubernetes_persistent_volume" "repos" {
  count = var.create_k8s_resources ? 1 : 0

  metadata {
    name = "${var.instance_name}-pv"

    labels = {
      app       = "g5"
      component = "filestore"
    }
  }

  spec {
    capacity = {
      storage = "${var.capacity_gb}Gi"
    }

    access_modes       = ["ReadWriteMany"]
    storage_class_name = ""

    persistent_volume_reclaim_policy = "Retain"

    persistent_volume_source {
      nfs {
        server = google_filestore_instance.repos.networks[0].ip_addresses[0]
        path   = "/${var.share_name}"
      }
    }
  }
}

resource "kubernetes_persistent_volume_claim" "repos" {
  count = var.create_k8s_resources ? 1 : 0

  metadata {
    name      = "${var.instance_name}-pvc"
    namespace = var.k8s_namespace

    labels = {
      app       = "g5"
      component = "filestore"
    }
  }

  spec {
    access_modes       = ["ReadWriteMany"]
    storage_class_name = ""
    volume_name        = kubernetes_persistent_volume.repos[0].metadata[0].name

    resources {
      requests = {
        storage = "${var.capacity_gb}Gi"
      }
    }
  }
}
