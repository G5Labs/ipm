# Administrator-run bootstrap uses local state because it owns the remote-state
# bucket. Secure this state outside Git; it is part of disaster recovery.
terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = { source = "hashicorp/google", version = "~> 5.0" }
  }
}
provider "google" {
  project = var.project_id
  region  = var.region
}

module "bootstrap" {
  source      = "../../terraform/modules/release-bootstrap"
  project_id  = var.project_id
  region      = var.region
  environment = "staging"
}
output "state_bucket" { value = module.bootstrap.state_bucket }
