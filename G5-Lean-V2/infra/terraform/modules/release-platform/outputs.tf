output "cluster" {
  value = {
    name     = module.gke.cluster_name
    location = module.gke.cluster_location
    endpoint = module.gke.cluster_endpoint
    ca       = module.gke.cluster_ca_certificate
  }
  sensitive = true
}

output "application" {
  value = {
    project_id              = var.project_id
    region                  = var.region
    external_url            = local.external_url
    external_access_enabled = var.settings.external_access_enabled
    preview_hostname        = var.settings.preview_hostname
    certificate_map_name    = module.networking.preview_certificate_map_name
    static_ip               = module.networking.ingress_static_ip
    static_ip_name          = module.networking.ingress_static_ip_name
    database_ip             = module.cloudsql.private_ip
    database_name           = local.database_name
    database_user           = "g5"
    storage_bucket          = google_storage_bucket.customer_data.name
    filestore_ip            = local.active_filestore_ip
    filestore_instance      = local.filestore_selection.selected.instance_name
    filestore_zone          = local.filestore_selection.selected.zone
    filestore_storage       = local.filestore_selection.selected.storage
    filestore_pv_name       = local.filestore_selection.selected.pv_name
    filestore_pvc_name      = local.filestore_selection.selected.pvc_name
    api_sa_email            = module.iam.api_sa_email
    worker_sa_email         = module.iam.worker_sa_email
    host_sa_email           = module.iam.host_sa_email
    node_sa_email           = data.google_compute_default_service_account.runtime.email
    artifact_readers = toset([
      "serviceAccount:${module.iam.api_sa_email}",
      "serviceAccount:${module.iam.host_sa_email}",
      "serviceAccount:${data.google_compute_default_service_account.runtime.email}",
    ])
    release_check_secret = module.secrets.release_check_secret_id
  }
  sensitive = true
}

output "filestore_migration" {
  description = "Selected and candidate identities for the controlled migration; provisioning does not activate a candidate."
  value = merge(local.filestore_selection, {
    candidate_ip = try(module.filestore_zonal[0].ip_address, null)
  })
  precondition {
    condition = (
      local.filestore_zone == local.filestore_selection.legacy.zone &&
      var.settings.filestore_tier == "BASIC_HDD" &&
      var.settings.filestore_capacity_gb == local.filestore_selection.legacy.capacity_gb
    )
    error_message = "Preserve the legacy Basic HDD instance unchanged; the committed migration map selects a separate Zonal candidate."
  }
}

output "dns_records_to_publish" {
  description = "Terraform-managed child record inventory. Publish only the dns_delegation NS set in Cloudflare."
  value       = module.networking.dns_records_to_publish
}

output "dns_delegation" {
  description = "Actual child nameservers for the reviewed manual Cloudflare parent NS delegation."
  value = {
    project_id   = var.project_id
    zone_name    = google_dns_managed_zone.application.name
    dns_name     = google_dns_managed_zone.application.dns_name
    name_servers = google_dns_managed_zone.application.name_servers
  }
}

output "release_identity_isolation_ready" {
  value = module.iam.release_identity_isolation_ready
}
