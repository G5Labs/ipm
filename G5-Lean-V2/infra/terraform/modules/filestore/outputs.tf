# =============================================================================
# Filestore Module — Outputs
# =============================================================================

output "instance_name" {
  description = "Filestore instance name"
  value       = google_filestore_instance.repos.name
}

output "instance_id" {
  description = "Filestore instance ID"
  value       = google_filestore_instance.repos.id
}

output "ip_address" {
  description = "IP address of the Filestore instance (NFS server)"
  value       = google_filestore_instance.repos.networks[0].ip_addresses[0]
}

output "share_name" {
  description = "NFS share name"
  value       = var.share_name
}

output "nfs_mount_path" {
  description = "Full NFS mount path (server:path)"
  value       = "${google_filestore_instance.repos.networks[0].ip_addresses[0]}:/${var.share_name}"
}

output "capacity_gb" {
  description = "Provisioned capacity in GB"
  value       = var.capacity_gb
}

output "pv_name" {
  description = "Kubernetes PersistentVolume name (if created)"
  value       = var.create_k8s_resources ? kubernetes_persistent_volume.repos[0].metadata[0].name : null
}

output "pvc_name" {
  description = "Kubernetes PersistentVolumeClaim name (if created)"
  value       = var.create_k8s_resources ? kubernetes_persistent_volume_claim.repos[0].metadata[0].name : null
}
