terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 5.0"
    }
    google-beta = {
      source  = "hashicorp/google-beta"
      version = "~> 7.0"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.25"
    }
  }
  backend "gcs" {
    bucket = "g5-qa-1ccc-lean-tfstate"
    prefix = "platform"
  }
}

locals {
  project_id     = "g5-qa-1ccc"
  region         = "us-central1"
  filestore_zone = "us-central1-a"
  labels         = { application = "g5-lean", environment = "qa" }
}

provider "google" {
  project = local.project_id
  region  = local.region
}
provider "google-beta" {
  project = local.project_id
  region  = local.region
}

module "kms" {
  source       = "../../terraform/modules/kms"
  project_id   = local.project_id
  region       = local.region
  keyring_name = "g5-lean-qa"
  labels       = local.labels
}

module "networking" {
  source                        = "../../terraform/modules/networking"
  project_id                    = local.project_id
  region                        = local.region
  network_name                  = "g5-lean-qa"
  enable_worker_egress_firewall = false
  system_subnet_cidr            = "10.40.0.0/20"
  worker_subnet_cidr            = "10.40.16.0/20"
  pod_cidr                      = "10.44.0.0/14"
  service_cidr                  = "10.48.0.0/20"
  create_static_ip              = true
  static_ip_name                = "g5-lean-qa-ip"
  dns_admin_project_id          = local.project_id
  dns_zone_name                 = "g5-lean-qa"
  dns_hostnames                 = ["qa.g5labs.ai"]
  # Keep the existing record and certificate identities in the QA child zone.
  # Parent NS delegation in Cloudflare is a separately reviewed operator step.
  preview_subdomain_bases = ["preview.qa.g5labs.ai"]
}

module "gke" {
  source                        = "../../terraform/modules/gke"
  project_id                    = local.project_id
  region                        = local.region
  cluster_name                  = "g5-lean-qa"
  network_id                    = module.networking.network_id
  subnetwork_id                 = module.networking.system_subnet_id
  pod_cidr                      = "10.44.0.0/14"
  service_cidr                  = "10.48.0.0/20"
  system_node_count             = 1
  system_machine_type           = "c3d-highcpu-16"
  system_disk_size_gb           = 50
  workload_identity_enabled     = true
  database_encryption_key       = module.kms.gke_key_id
  enable_mesh                   = false
  enable_private_cluster        = true
  master_ipv4_cidr_block        = "172.16.40.0/28"
  master_authorized_cidr_blocks = []
  depends_on                    = [google_service_account_iam_member.terraform_node_act_as]
}

module "cloudsql" {
  source                         = "../../terraform/modules/cloudsql"
  project_id                     = local.project_id
  region                         = local.region
  instance_name                  = "g5-lean-qa-postgres"
  network_id                     = module.networking.network_id
  private_ip_range_id            = module.networking.private_ip_range_id
  database_version               = "POSTGRES_17"
  tier                           = "db-custom-8-32768"
  disk_size_gb                   = 20
  disk_autoresize                = true
  availability_type              = "ZONAL"
  backup_enabled                 = true
  point_in_time_recovery_enabled = true
  database_name                  = "g5_lean_qa"
  db_user                        = "g5"
  database_flags                 = []
  encryption_key_name            = module.kms.cloudsql_key_id
  depends_on                     = [module.networking]
}

module "gcs" {
  source                = "../../terraform/modules/gcs"
  project_id            = local.project_id
  region                = local.region
  bucket_name           = "g5-qa-1ccc-lean-storage"
  cache_max_age_days    = 7
  artifact_max_age_days = 30
  encryption_key_name   = module.kms.gcs_key_id
  labels                = merge(local.labels, { service = "g5" })
}

# Basic Project Viewer membership must not expose customer objects. Own only
# this legacy role; retain bucket metadata access and explicit runtime grants.
resource "google_storage_bucket_iam_binding" "customer_data_legacy_object_readers" {
  bucket = module.gcs.bucket_name
  role   = "roles/storage.legacyObjectReader"
  # Owners retain existing object-owner access; keep a nonempty binding without Viewers.
  members = ["projectOwner:${local.project_id}"]
}

module "iam" {
  source                          = "../../terraform/modules/iam"
  project_id                      = local.project_id
  gke_cluster_name                = module.gke.cluster_name
  gke_location                    = module.gke.cluster_location
  api_sa_name                     = "g5-lean-qa-api"
  worker_sa_name                  = "g5-lean-qa-worker"
  gcs_bucket_name                 = module.gcs.bucket_name
  k8s_namespaces                  = ["g5"]
  enable_api_managed_worker_hosts = true
  # The API may mint registry tokens only for the named puller below, never
  # impersonate the new privileged release/terraform accounts in this project.
  enable_legacy_workspace_identity_admin = false
  depends_on                             = [module.gke]
}

# Self-hosted installer credentials use a pull-only identity owned by QA. The
# API may mint short-lived tokens for it, but cannot grant it wider access.
resource "google_service_account" "registry_puller" {
  project      = local.project_id
  account_id   = "g5-registry-puller"
  display_name = "G5 Lean QA registry puller"
}
resource "google_artifact_registry_repository_iam_member" "registry_puller" {
  project    = local.project_id
  location   = local.region
  repository = "g5"
  role       = "roles/artifactregistry.reader"
  member     = "serviceAccount:${google_service_account.registry_puller.email}"
}
resource "google_service_account_iam_member" "api_registry_token_creator" {
  service_account_id = google_service_account.registry_puller.name
  role               = "roles/iam.serviceAccountTokenCreator"
  member             = "serviceAccount:${module.iam.api_sa_email}"
}

module "filestore_selection" {
  source      = "../../terraform/modules/filestore-selection"
  environment = "qa"
}

locals {
  filestore_selection = module.filestore_selection.configuration
  active_filestore_ip = local.filestore_selection.active == "zonal" ? module.filestore_zonal[0].ip_address : module.filestore.ip_address
}

module "filestore_zonal" {
  count         = local.filestore_selection.provision_zonal ? 1 : 0
  source        = "../../terraform/modules/filestore-zonal"
  project_id    = local.project_id
  instance_name = local.filestore_selection.zonal.instance_name
  network_name  = "g5-lean-qa"
  kms_key_name  = module.kms.filestore_key_id
  labels        = local.labels
  # Key grants and VPC must be ready before the CMEK instance is created.
  depends_on = [module.networking, module.kms]
}

module "filestore" {
  source               = "../../terraform/modules/filestore"
  project_id           = local.project_id
  zone                 = local.filestore_zone
  instance_name        = "g5-lean-qa-repos"
  tier                 = "BASIC_HDD"
  capacity_gb          = 1024
  network_name         = "g5-lean-qa"
  network_id           = module.networking.network_id
  gke_node_cidrs       = ["10.40.0.0/20", "10.40.16.0/20"]
  create_k8s_resources = false
  k8s_namespace        = "g5"
  kms_key_name         = module.kms.filestore_key_id
  labels               = local.labels
}

# Daily native backups of the repository share (ontology
# g5.quality.delivery.backups). Bootstrap owns the custom roles and Workflows
# APIs; delivery owns the Scheduler API. The runner attaches both identities
# explicitly, as for every other managed workload. Live resources are adopted
# by filestore-backup-imports.tf.
module "filestore_backup" {
  source          = "../../terraform/modules/filestore-backup"
  project_id      = local.project_id
  region          = local.region
  source_instance = local.filestore_selection.selected.instance_name
  source_zone     = local.filestore_selection.selected.zone
  share_name      = local.filestore_selection.selected.share_name
  daily_time_utc  = "04:00"
  retention_days  = 30
  depends_on      = [module.filestore, module.filestore_zonal]
  attach_members  = ["serviceAccount:${data.google_service_account.terraform.email}"]
}

module "secrets" {
  api_secret_ids                             = []
  source                                     = "../../terraform/modules/secrets"
  project_id                                 = local.project_id
  region                                     = local.region
  kms_key_id                                 = module.kms.secrets_key_id
  secretmanager_sa_email                     = module.kms.secretmanager_sa_email
  gke_api_sa_email                           = module.iam.api_sa_email
  gke_worker_sa_email                        = module.iam.worker_sa_email
  adopt_existing_secret_ids                  = []
  self_hosted_license_admin_secret_id        = "g5-license-admin-token-qa"
  enable_app_engine_rotation_consumer_access = false
  labels                                     = local.labels
  depends_on                                 = [module.kms]
}

resource "google_secret_manager_secret_version" "db_password" {
  secret      = module.secrets.secret_ids["g5-db-password"]
  secret_data = module.cloudsql.db_password
  depends_on  = [google_secret_manager_secret_iam_member.terraform_runner_accessor]
}
resource "google_secret_manager_secret_version" "authz_writer_db_password" {
  secret      = module.secrets.secret_ids["g5-authz-writer-db-password"]
  secret_data = module.cloudsql.authz_writer_password
  depends_on  = [google_secret_manager_secret_iam_member.terraform_runner_accessor]
}
resource "google_secret_manager_secret_version" "runtime_system_db_password" {
  secret      = module.secrets.secret_ids["g5-runtime-system-db-password"]
  secret_data = module.cloudsql.runtime_system_password
  depends_on  = [google_secret_manager_secret_iam_member.terraform_runner_accessor]
}
resource "google_secret_manager_secret_version" "runtime_tenant_db_password" {
  secret      = module.secrets.secret_ids["g5-runtime-tenant-db-password"]
  secret_data = module.cloudsql.runtime_tenant_password
  depends_on  = [google_secret_manager_secret_iam_member.terraform_runner_accessor]
}

locals {
  terraform_database_secrets = toset([
    "g5-db-password",
    "g5-authz-writer-db-password",
    "g5-runtime-system-db-password",
    "g5-runtime-tenant-db-password",
  ])
}

data "google_service_account" "terraform" {
  project    = local.project_id
  account_id = "g5-lean-terraform"
}

# Install these before removing bootstrap's broad secret roles: existing secret
# versions read payloads during plan refresh. The version dependencies above
# also cover a fresh install's immediate provider read after creation.
resource "google_secret_manager_secret_iam_member" "terraform_runner_accessor" {
  for_each   = local.terraform_database_secrets
  project    = local.project_id
  secret_id  = each.key
  role       = "roles/secretmanager.secretAccessor"
  member     = "serviceAccount:${data.google_service_account.terraform.email}"
  depends_on = [module.secrets]
}

# The read-only planner (bootstrap: g5-lean-terraform-plan) refreshes the
# secret versions above, and the provider reads a version's payload on every
# refresh. Grant payload access on exactly these secrets rather than
# roles/secretmanager.secretAccessor on the project, which would also expose
# the API keys and OAuth/JWT material seeded out of band.
data "google_service_account" "terraform_plan" {
  project    = local.project_id
  account_id = "g5-lean-terraform-plan"
}

resource "google_secret_manager_secret_iam_member" "terraform_plan_accessor" {
  for_each   = local.terraform_database_secrets
  project    = local.project_id
  secret_id  = each.key
  role       = "roles/secretmanager.secretAccessor"
  member     = "serviceAccount:${data.google_service_account.terraform_plan.email}"
  depends_on = [module.secrets]
}

data "google_project" "qa" { project_id = local.project_id }
# Both temporary and system GKE node pools use the default Compute identity.
# Creating/updating them needs actAs on this account, not every project account.
resource "google_service_account_iam_member" "terraform_node_act_as" {
  service_account_id = "projects/${local.project_id}/serviceAccounts/${data.google_project.qa.number}-compute@developer.gserviceaccount.com"
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${data.google_service_account.terraform.email}"
}
resource "google_project_iam_member" "compute_registry_reader" {
  project = local.project_id
  role    = "roles/artifactregistry.reader"
  member  = "serviceAccount:${data.google_project.qa.number}-compute@developer.gserviceaccount.com"
}
# Worker hosts use their dedicated identity; GKE retains the default node SA. The host cache
# door reads the organization's fleet sccache prefix in the deploy-artifacts
# bucket with that identity and serves it to guests read-only (ontology
# g5.runtime.cache.fleet); the prefix is scoped per host by the API, and
# guests never hold this credential. Object read only: the deploy pipeline
# is the sole writer.
resource "google_storage_bucket_iam_member" "host_guest_sccache_reader" {
  bucket = "${local.project_id}-deploy-artifacts"
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${module.iam.host_sa_email}"
  # Only the guest sccache prefix (uniform bucket-level access makes the
  # condition apply): whether a guest can reach the metadata server through
  # passt is unverified, so the host identity must not be able to read the
  # bucket's other deploy artifacts even if its token leaked.
  condition {
    title       = "guest-sccache-prefix-only"
    description = "The worker host identity reads only guest-sccache/ (ontology g5.runtime.cache.fleet)."
    expression  = "resource.name.startsWith(\"projects/_/buckets/${local.project_id}-deploy-artifacts/objects/guest-sccache/\")"
  }
}

# Existing VMs retain the default Compute identity until rotation. Preserve this
# original binding address and grant; retire it only in a post-rotation change.
resource "google_storage_bucket_iam_member" "compute_guest_sccache_reader" {
  bucket = "${local.project_id}-deploy-artifacts"
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${data.google_project.qa.number}-compute@developer.gserviceaccount.com"
  # Only the guest sccache prefix (uniform bucket-level access makes the
  # condition apply): whether a guest can reach the metadata server through
  # passt is unverified, so the host identity must not be able to read the
  # bucket's other deploy artifacts even if its token leaked.
  condition {
    title       = "guest-sccache-prefix-only"
    description = "The worker host identity reads only guest-sccache/ (ontology g5.runtime.cache.fleet)."
    expression  = "resource.name.startsWith(\"projects/_/buckets/${local.project_id}-deploy-artifacts/objects/guest-sccache/\")"
  }
}

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
    project_id         = local.project_id
    region             = local.region
    external_url       = "https://qa.g5labs.ai"
    static_ip          = module.networking.ingress_static_ip
    static_ip_name     = module.networking.ingress_static_ip_name
    database_ip        = module.cloudsql.private_ip
    database_name      = "g5_lean_qa"
    database_user      = "g5"
    filestore_ip       = local.active_filestore_ip
    filestore_instance = local.filestore_selection.selected.instance_name
    filestore_zone     = local.filestore_selection.selected.zone
    filestore_storage  = local.filestore_selection.selected.storage
    filestore_pv_name  = local.filestore_selection.selected.pv_name
    filestore_pvc_name = local.filestore_selection.selected.pvc_name
    api_sa_email       = module.iam.api_sa_email
    worker_sa_email    = module.iam.worker_sa_email
    host_sa_email      = module.iam.host_sa_email
    node_sa_email      = "${data.google_project.qa.number}-compute@developer.gserviceaccount.com"
    artifact_readers = toset([
      "serviceAccount:${module.iam.api_sa_email}",
      "serviceAccount:${module.iam.host_sa_email}",
      "serviceAccount:${data.google_project.qa.number}-compute@developer.gserviceaccount.com",
    ])
  }
  sensitive = true
}

output "filestore_migration" {
  description = "Selected and candidate identities for the controlled migration; provisioning does not activate a candidate."
  value = merge(local.filestore_selection, {
    candidate_ip = try(module.filestore_zonal[0].ip_address, null)
  })
}

output "dns_records_to_publish" {
  description = "Terraform-managed application, preview and certificate-validation records in the QA child zone."
  value       = module.networking.dns_records_to_publish
}

output "release_identity_isolation_ready" {
  description = "Release identities may be introduced only after applying the API impersonation hardening."
  value       = module.iam.release_identity_isolation_ready
}

# The host pulls workspace images from g5 and cache tiers from g5-tiers.
# The worker-image OCI metadata repository remains available for verification.
resource "google_artifact_registry_repository_iam_member" "host_registry_reader" {
  for_each   = toset(["g5", "g5-tiers", "g5-worker-images"])
  project    = local.project_id
  location   = local.region
  repository = each.key
  role       = "roles/artifactregistry.reader"
  member     = "serviceAccount:${module.iam.host_sa_email}"
}
resource "google_storage_bucket_iam_member" "host_worker_artifact_reader" {
  bucket = "${local.project_id}-g5-worker-images"
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${module.iam.host_sa_email}"
  condition {
    title      = "worker_version_objects_only"
    expression = "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/${local.project_id}-g5-worker-images/objects/g5-worker-v')"
  }
}
