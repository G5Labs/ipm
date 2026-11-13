terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google      = { source = "hashicorp/google", version = "~> 5.0" }
    google-beta = { source = "hashicorp/google-beta", version = "~> 7.0" }
    random      = { source = "hashicorp/random", version = "~> 3.0" }
    kubernetes  = { source = "hashicorp/kubernetes", version = "~> 2.25" }
  }
  backend "gcs" {}
}
provider "google" {
  project = var.project_id
  region  = var.region
}

provider "google-beta" {
  project = var.project_id
  region  = var.region
}

module "environment" {
  source      = "../../terraform/modules/release-platform"
  project_id  = var.project_id
  region      = var.region
  environment = "prod"
  settings    = var.settings
}
output "cluster" {
  value     = module.environment.cluster
  sensitive = true
}
output "application" {
  value     = module.environment.application
  sensitive = true
}
output "dns_records_to_publish" { value = module.environment.dns_records_to_publish }
output "dns_delegation" { value = module.environment.dns_delegation }

output "filestore_migration" { value = module.environment.filestore_migration }
