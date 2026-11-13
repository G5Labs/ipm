variable "project_id" {
  type = string
  validation {
    condition = var.project_id == lookup({
      staging = "g5lean-staging-1ccc"
      prod    = "g5lean-prod-1ccc"
    }, var.environment, "")
    error_message = "The environment must match its reviewed Lean target project."
  }
}
variable "region" {
  type = string
  validation {
    condition     = var.region == "us-central1"
    error_message = "The reviewed Lean targets use us-central1."
  }
}
variable "environment" {
  type = string
  validation {
    condition     = contains(["staging", "prod"], var.environment)
    error_message = "This module provisions staging or prod only."
  }
}

variable "settings" {
  description = "Reviewed environment configuration. Hostnames prepare child DNS/TLS; external access separately enables application ingress."
  type = object({
    external_access_enabled       = optional(bool, false)
    app_hostname                  = optional(string, "")
    preview_hostname              = optional(string, "")
    enable_release_check_fixture  = optional(bool, false)
    system_subnet_cidr            = optional(string, "10.60.0.0/20")
    worker_subnet_cidr            = optional(string, "10.60.16.0/20")
    pod_cidr                      = optional(string, "10.64.0.0/14")
    service_cidr                  = optional(string, "10.68.0.0/20")
    master_ipv4_cidr_block        = optional(string, "172.16.60.0/28")
    master_authorized_cidr_blocks = optional(list(object({ cidr_block = string, display_name = string })), [])
    system_node_count             = optional(number, 1)
    system_machine_type           = optional(string, "c3d-highcpu-16")
    system_disk_size_gb           = optional(number, 50)
    database_tier                 = optional(string, "db-custom-8-32768")
    database_disk_size_gb         = optional(number, 20)
    database_availability_type    = optional(string, "ZONAL")
    filestore_zone                = optional(string, "")
    filestore_tier                = optional(string, "BASIC_HDD")
    filestore_capacity_gb         = optional(number, 1024)
  })
  default = {}
  validation {
    condition = !var.settings.external_access_enabled || (
      var.settings.app_hostname != "" && var.settings.preview_hostname != "" &&
      var.settings.app_hostname != var.settings.preview_hostname
    )
    error_message = "External access requires explicit application and preview hostnames."
  }
  validation {
    condition = alltrue([for hostname in [var.settings.app_hostname, var.settings.preview_hostname] :
      hostname == "" || (
        length(hostname) <= 253 &&
        can(regex("^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$", hostname)) &&
        (hostname == local.dns_domain || endswith(hostname, ".${local.dns_domain}"))
      )
    ])
    error_message = "Configured application and preview hostnames must be within this environment's delegated child zone."
  }
  validation {
    condition = (
      var.settings.app_hostname == "" || var.settings.preview_hostname == "" ||
      var.settings.app_hostname != var.settings.preview_hostname
    )
    error_message = "Configured application and preview hostnames must be distinct, including during private DNS preparation."
  }
  validation {
    condition     = contains(["ZONAL", "REGIONAL"], var.settings.database_availability_type)
    error_message = "Database availability must be ZONAL or REGIONAL."
  }
}
