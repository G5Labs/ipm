terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = { source = "hashicorp/google", version = "~> 5.0" }
    helm   = { source = "hashicorp/helm", version = "~> 2.12" }
    null   = { source = "hashicorp/null", version = "~> 3.0" }
  }
}

variable "project_id" {
  type = string
  validation {
    condition = var.project_id == lookup({
      staging = "g5lean-staging-1ccc"
      prod    = "g5lean-prod-1ccc"
    }, var.environment, "")
    error_message = "Addons must use the approved project for their release environment."
  }
}
variable "environment" {
  type = string
  validation {
    condition     = contains(["staging", "prod"], var.environment)
    error_message = "Only staging and prod are release-addon environments."
  }
}
variable "region" {
  type = string
  validation {
    condition     = var.region == "us-central1"
    error_message = "The approved target region is us-central1."
  }
}
variable "cluster" {
  type = object({ name = string, location = string, endpoint = string, ca = string })
  validation {
    condition     = var.cluster.name == "g5-lean-${var.environment}" && var.cluster.location == var.region
    error_message = "Platform state must identify this environment's target cluster."
  }
}
variable "application" {
  type = object({
    project_id              = string
    region                  = string
    external_url            = string
    external_access_enabled = bool
    preview_hostname        = optional(string, "")
    database_ip             = string
    database_name           = string
    database_user           = string
  })
  validation {
    condition = (
      var.application.project_id == var.project_id && var.application.region == var.region &&
      var.application.database_name == "g5_lean_${var.environment}"
    )
    error_message = "Application state must belong to this target."
  }
}
variable "settings" {
  description = "The same non-secret settings object used by platform; apply platform before changing addon configuration."
  type        = any
  default     = {}
  validation {
    condition = (
      var.application.external_access_enabled == try(var.settings.external_access_enabled, false) &&
      var.application.preview_hostname == try(var.settings.preview_hostname, "") &&
      var.application.external_url == (try(var.settings.app_hostname, "") != "" ? "https://${var.settings.app_hostname}" : "https://unconfigured.invalid")
    )
    error_message = "Addon hostname and access settings must match the applied platform settings."
  }
  validation {
    condition = alltrue([for hostname in [try(var.settings.app_hostname, ""), try(var.settings.preview_hostname, "")] :
      hostname == "" || try(
        length(hostname) <= 253 &&
        can(regex("^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$", hostname)) &&
        (hostname == lookup({ staging = "lean.g5labs.ai", prod = "app.g5labs.ai" }, var.environment, "") ||
        endswith(hostname, ".${lookup({ staging = "lean.g5labs.ai", prod = "app.g5labs.ai" }, var.environment, "")}")),
        false
      )
    ])
    error_message = "Configured app and preview hostnames must be lowercase DNS names inside this environment's delegated child."
  }
  validation {
    condition = try(
      (!try(var.settings.external_access_enabled, false) ||
      (try(var.settings.app_hostname, "") != "" && try(var.settings.preview_hostname, "") != "")) &&
      (try(var.settings.app_hostname, "") == "" || try(var.settings.preview_hostname, "") == "" ||
      try(var.settings.app_hostname, "") != try(var.settings.preview_hostname, "")),
      false
    )
    error_message = "Configured app and preview hostnames must be distinct; public ingress requires both."
  }
}

# Reuse the deployed ESO flow with target-local credentials/configuration. The QA
# PR planner and its Helm-secret reader Role are deliberately not copied here.
# ESO is the trusted secret-sync controller in this isolated environment; its
# module grants it only the enabled ExternalSecret remote references. Application/worker
# identities and the release fixture keep their separately scoped grants.
module "external_secrets" {
  source                              = "../external-secrets"
  project_id                          = var.project_id
  region                              = var.region
  namespace                           = "g5"
  additional_namespaces               = []
  cluster_name                        = var.cluster.name
  cluster_location                    = var.cluster.location
  sync_api_secret                     = true
  sync_worker_secret                  = false
  external_url                        = var.application.external_url
  worker_api_url                      = var.application.external_url
  database_private_ip                 = var.application.database_ip
  database_name                       = var.application.database_name
  database_user                       = var.application.database_user
  self_hosted_license_admin_secret_id = "g5-license-admin-token-${var.environment}"
  labels                              = { application = "g5-lean", environment = var.environment }
}

output "external_secrets_service_account" {
  value = module.external_secrets.eso_sa_email
}
