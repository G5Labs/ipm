terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 5.0"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.12"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.25"
    }
    null = {
      source  = "hashicorp/null"
      version = "~> 3.0"
    }
  }
  backend "gcs" {
    bucket = "g5-qa-1ccc-lean-tfstate"
    prefix = "addons"
  }
}

locals {
  project_id = "g5-qa-1ccc"
  region     = "us-central1"
  platform   = data.terraform_remote_state.platform.outputs.application
  cluster    = data.terraform_remote_state.platform.outputs.cluster
  labels     = { application = "g5-lean", environment = "qa" }
}

data "terraform_remote_state" "platform" {
  backend = "gcs"
  config = {
    bucket = "g5-qa-1ccc-lean-tfstate"
    prefix = "platform"
  }
}
data "google_client_config" "current" {}

provider "google" {
  project = local.project_id
  region  = local.region
}
provider "helm" {
  kubernetes {
    host                   = "https://${local.cluster.endpoint}"
    token                  = data.google_client_config.current.access_token
    cluster_ca_certificate = base64decode(local.cluster.ca)
  }
}
provider "kubernetes" {
  host                   = "https://${local.cluster.endpoint}"
  token                  = data.google_client_config.current.access_token
  cluster_ca_certificate = base64decode(local.cluster.ca)
}

module "external_secrets" {
  source = "../../terraform/modules/external-secrets"
  providers = {
    google = google
    helm   = helm
    null   = null
  }
  project_id                          = local.project_id
  region                              = local.region
  namespace                           = "g5"
  additional_namespaces               = []
  cluster_name                        = local.cluster.name
  cluster_location                    = local.cluster.location
  sync_api_secret                     = true
  sync_worker_secret                  = false
  external_url                        = local.platform.external_url
  worker_api_url                      = local.platform.external_url
  database_private_ip                 = local.platform.database_ip
  database_name                       = local.platform.database_name
  database_user                       = local.platform.database_user
  self_hosted_license_admin_secret_id = "g5-license-admin-token-qa"
  labels                              = local.labels
}

# Helm keeps release state in Secrets of the release namespace, and the
# provider lists and reads them on every refresh. The read-only planner
# (bootstrap: g5-lean-terraform-plan) holds roles/viewer, whose GKE mapping
# excludes secrets, so it needs an in-cluster grant. A namespaced Role keeps
# that to the ESO controller's namespace rather than container.secrets.* in
# Cloud IAM, which would also expose the application secrets ESO syncs into
# the g5 namespace. GKE authenticates the service account as a User named by
# its email. The namespace is created by the release, hence depends_on.
data "google_service_account" "terraform_plan" {
  project    = local.project_id
  account_id = "g5-lean-terraform-plan"
}

resource "kubernetes_role" "terraform_plan_helm_reader" {
  metadata {
    name      = "terraform-plan-helm-reader"
    namespace = "external-secrets"
    labels    = local.labels
  }
  rule {
    api_groups = [""]
    resources  = ["secrets"]
    verbs      = ["get", "list"]
  }
  depends_on = [module.external_secrets]
}

resource "kubernetes_role_binding" "terraform_plan_helm_reader" {
  metadata {
    name      = "terraform-plan-helm-reader"
    namespace = "external-secrets"
    labels    = local.labels
  }
  role_ref {
    api_group = "rbac.authorization.k8s.io"
    kind      = "Role"
    name      = kubernetes_role.terraform_plan_helm_reader.metadata[0].name
  }
  subject {
    api_group = "rbac.authorization.k8s.io"
    kind      = "User"
    name      = data.google_service_account.terraform_plan.email
  }
}

output "external_secrets_service_account" {
  value = module.external_secrets.eso_sa_email
}
