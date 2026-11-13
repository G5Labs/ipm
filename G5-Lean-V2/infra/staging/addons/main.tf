terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = { source = "hashicorp/google", version = "~> 5.0" }
    helm   = { source = "hashicorp/helm", version = "~> 2.12" }
    null   = { source = "hashicorp/null", version = "~> 3.0" }
  }
  backend "gcs" {}
}

data "terraform_remote_state" "platform" {
  backend = "gcs"
  config = {
    bucket = "${var.project_id}-lean-tfstate"
    prefix = "platform"
  }
}
data "google_client_config" "current" {}
provider "google" {
  project = var.project_id
  region  = var.region
}
provider "helm" {
  kubernetes {
    host                   = "https://${data.terraform_remote_state.platform.outputs.cluster.endpoint}"
    token                  = data.google_client_config.current.access_token
    cluster_ca_certificate = base64decode(data.terraform_remote_state.platform.outputs.cluster.ca)
  }
}

module "addons" {
  source      = "../../terraform/modules/release-addons"
  environment = "staging"
  project_id  = var.project_id
  region      = var.region
  settings    = var.settings
  cluster     = data.terraform_remote_state.platform.outputs.cluster
  application = data.terraform_remote_state.platform.outputs.application
}
output "external_secrets_service_account" { value = module.addons.external_secrets_service_account }
