terraform {
  required_providers {
    google      = { source = "hashicorp/google", version = "~> 5.0" }
    google-beta = { source = "hashicorp/google-beta", version = "~> 7.0" }
    random      = { source = "hashicorp/random", version = "~> 3.0" }
    kubernetes  = { source = "hashicorp/kubernetes", version = "~> 2.25" }
  }
}

locals {
  name           = "g5-lean-${var.environment}"
  database_name  = "g5_lean_${var.environment}"
  labels         = { application = "g5-lean", environment = var.environment }
  filestore_zone = var.settings.filestore_zone != "" ? var.settings.filestore_zone : "${var.region}-a"
  external_url   = var.settings.app_hostname != "" ? "https://${var.settings.app_hostname}" : "https://unconfigured.invalid"
  dns_domain = lookup({
    staging = "lean.g5labs.ai"
    prod    = "app.g5labs.ai"
  }, var.environment, "unconfigured.invalid")
}

# The child exists before public activation and survives ingress disablement.
# Only its parent NS delegation is published manually in Cloudflare.
resource "google_dns_managed_zone" "application" {
  project       = var.project_id
  name          = local.name
  dns_name      = "${local.dns_domain}."
  description   = "Delegated application DNS for G5 Lean ${var.environment}"
  visibility    = "public"
  force_destroy = false
  labels        = local.labels

  lifecycle { prevent_destroy = true }
}

# Bootstrap enables APIs before this root. Explicitly materialize agents whose
# addresses the shared KMS module otherwise derives from the project number.
resource "google_project_service_identity" "runtime" {
  provider = google-beta
  for_each = toset(["container.googleapis.com", "file.googleapis.com"])
  project  = var.project_id
  service  = each.value
}

data "google_compute_default_service_account" "runtime" {
  project = var.project_id
}

# New projects must not depend on automatic Editor grants to their default SA.
# GKE nodes retain this identity; API-created worker hosts use module.iam.host.
resource "google_project_iam_member" "node_services" {
  project = var.project_id
  role    = "roles/container.defaultNodeServiceAccount"
  member  = "serviceAccount:${data.google_compute_default_service_account.runtime.email}"
}

module "kms" {
  source       = "../kms"
  project_id   = var.project_id
  region       = var.region
  keyring_name = local.name
  labels       = local.labels
  depends_on   = [google_project_service_identity.runtime]
}

module "networking" {
  source                        = "../networking"
  project_id                    = var.project_id
  region                        = var.region
  network_name                  = local.name
  enable_worker_egress_firewall = false
  system_subnet_cidr            = var.settings.system_subnet_cidr
  worker_subnet_cidr            = var.settings.worker_subnet_cidr
  pod_cidr                      = var.settings.pod_cidr
  service_cidr                  = var.settings.service_cidr
  create_static_ip              = true
  static_ip_name                = "${local.name}-ip"
  # Keep DNS and certificate renewal independent of application ingress. Clearing
  # a configured hostname is a reviewed DNS/TLS teardown, not an ingress toggle.
  dns_admin_project_id    = var.project_id
  dns_zone_name           = google_dns_managed_zone.application.name
  dns_hostnames           = compact([var.settings.app_hostname])
  preview_subdomain_bases = compact([var.settings.preview_hostname])
}

module "gke" {
  source                        = "../gke"
  project_id                    = var.project_id
  region                        = var.region
  cluster_name                  = local.name
  network_id                    = module.networking.network_id
  subnetwork_id                 = module.networking.system_subnet_id
  pod_cidr                      = var.settings.pod_cidr
  service_cidr                  = var.settings.service_cidr
  system_node_count             = var.settings.system_node_count
  system_machine_type           = var.settings.system_machine_type
  system_disk_size_gb           = var.settings.system_disk_size_gb
  workload_identity_enabled     = true
  database_encryption_key       = module.kms.gke_key_id
  enable_mesh                   = false
  enable_private_cluster        = true
  deletion_protection           = true
  master_ipv4_cidr_block        = var.settings.master_ipv4_cidr_block
  master_authorized_cidr_blocks = var.settings.master_authorized_cidr_blocks
  depends_on                    = [module.kms, google_project_iam_member.node_services]
}

module "cloudsql" {
  source                         = "../cloudsql"
  project_id                     = var.project_id
  region                         = var.region
  instance_name                  = "${local.name}-postgres"
  network_id                     = module.networking.network_id
  private_ip_range_id            = module.networking.private_ip_range_id
  database_version               = "POSTGRES_17"
  tier                           = var.settings.database_tier
  disk_size_gb                   = var.settings.database_disk_size_gb
  disk_autoresize                = true
  availability_type              = var.settings.database_availability_type
  backup_enabled                 = true
  point_in_time_recovery_enabled = true
  database_name                  = local.database_name
  db_user                        = "g5"
  database_flags                 = []
  encryption_key_name            = module.kms.cloudsql_key_id
  # SQL needs the peering connection and the service-agent KMS grant, not just
  # a reserved range/key ID, before its first instance can be created.
  depends_on = [module.networking, module.kms]
}

# Customer data must not inherit the shared cache bucket's blanket age-based
# deletion rules. Retention/cleanup is a separate reviewed operational decision.
resource "google_storage_bucket" "customer_data" {
  project                     = var.project_id
  name                        = "${var.project_id}-lean-storage"
  location                    = var.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = false
  versioning { enabled = true }
  encryption { default_kms_key_name = module.kms.gcs_key_id }
  labels = local.labels
  lifecycle { prevent_destroy = true }
}

# Basic Project Viewer membership must not expose customer objects. Own only
# this legacy role; retain bucket metadata access and explicit runtime grants.
resource "google_storage_bucket_iam_binding" "customer_data_legacy_object_readers" {
  bucket = google_storage_bucket.customer_data.name
  role   = "roles/storage.legacyObjectReader"
  # Owners retain existing object-owner access; keep a nonempty binding without Viewers.
  members = ["projectOwner:${var.project_id}"]
}

module "iam" {
  source                                 = "../iam"
  project_id                             = var.project_id
  gke_cluster_name                       = module.gke.cluster_name
  gke_location                           = module.gke.cluster_location
  api_sa_name                            = "${local.name}-api"
  worker_sa_name                         = "${local.name}-worker"
  gcs_bucket_name                        = google_storage_bucket.customer_data.name
  k8s_namespaces                         = ["g5"]
  enable_api_managed_worker_hosts        = true
  enable_legacy_workspace_identity_admin = false
  depends_on                             = [module.gke]
}

module "filestore_selection" {
  source      = "../filestore-selection"
  environment = var.environment
}

locals {
  filestore_selection = module.filestore_selection.configuration
  active_filestore_ip = local.filestore_selection.active == "zonal" ? module.filestore_zonal[0].ip_address : module.filestore.ip_address
}

module "filestore_zonal" {
  count         = local.filestore_selection.provision_zonal ? 1 : 0
  source        = "../filestore-zonal"
  project_id    = var.project_id
  instance_name = local.filestore_selection.zonal.instance_name
  network_name  = local.name
  kms_key_name  = module.kms.filestore_key_id
  labels        = local.labels
  depends_on    = [module.networking, module.kms]
}

module "filestore" {
  source               = "../filestore"
  project_id           = var.project_id
  zone                 = local.filestore_zone
  instance_name        = "${local.name}-repos"
  tier                 = var.settings.filestore_tier
  capacity_gb          = var.settings.filestore_capacity_gb
  network_name         = local.name
  network_id           = module.networking.network_id
  gke_node_cidrs       = [var.settings.system_subnet_cidr, var.settings.worker_subnet_cidr]
  create_k8s_resources = false
  k8s_namespace        = "g5"
  kms_key_name         = module.kms.filestore_key_id
  labels               = local.labels
  depends_on           = [module.networking, module.kms]
}

# Daily native backups of the repository share (ontology
# g5.quality.delivery.backups), adopted from the live 2026-11-04 setup by the
# import blocks in each target platform root. Bootstrap owns the custom roles
# and APIs; the central release identity already holds project-wide attachment.
locals {
  filestore_backup_time_utc = { staging = "04:30", prod = "05:00" }
}
module "filestore_backup" {
  source          = "../filestore-backup"
  project_id      = var.project_id
  region          = var.region
  source_instance = local.filestore_selection.selected.instance_name
  source_zone     = local.filestore_selection.selected.zone
  share_name      = local.filestore_selection.selected.share_name
  daily_time_utc  = local.filestore_backup_time_utc[var.environment]
  retention_days  = 30
  depends_on      = [module.filestore, module.filestore_zonal]
}

module "secrets" {
  api_secret_ids                             = []
  source                                     = "../secrets"
  project_id                                 = var.project_id
  region                                     = var.region
  kms_key_id                                 = module.kms.secrets_key_id
  secretmanager_sa_email                     = module.kms.secretmanager_sa_email
  gke_api_sa_email                           = module.iam.api_sa_email
  gke_worker_sa_email                        = module.iam.worker_sa_email
  adopt_existing_secret_ids                  = []
  self_hosted_license_admin_secret_id        = "g5-license-admin-token-${var.environment}"
  enable_app_engine_rotation_consumer_access = false
  enable_release_payload_access              = true
  enable_release_check_fixture               = var.settings.enable_release_check_fixture
  labels                                     = local.labels
  depends_on                                 = [module.kms]
}

resource "google_secret_manager_secret_version" "database" {
  for_each = toset([
    "g5-db-password", "g5-authz-writer-db-password",
    "g5-runtime-system-db-password", "g5-runtime-tenant-db-password",
  ])
  # secret_ids waits for the central account's four scoped accessor bindings.
  secret = module.secrets.secret_ids[each.key]
  secret_data = {
    "g5-db-password"                = module.cloudsql.db_password
    "g5-authz-writer-db-password"   = module.cloudsql.authz_writer_password
    "g5-runtime-system-db-password" = module.cloudsql.runtime_system_password
    "g5-runtime-tenant-db-password" = module.cloudsql.runtime_tenant_password
  }[each.key]
}
