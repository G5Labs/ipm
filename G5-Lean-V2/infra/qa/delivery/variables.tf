variable "enable_application_triggers" {
  description = "Create PR CI, deploy, and worker-image triggers using the pinned Cloud Hypervisor platform."
  type        = bool
  default     = false
}

variable "artifact_project" {
  description = "Shared immutable artifact owner. Keep QA until admin provisioning, publication and consumer checks pass; build execution remains in QA."
  type        = string
  default     = "g5-qa-1ccc"
  validation {
    condition     = contains(["g5-qa-1ccc", "g5lean-admin-1ccc"], var.artifact_project)
    error_message = "Only the existing QA or approved Lean admin project may own shared artifacts."
  }
}

# Verified against the existing GCP projects on 2026-10-22. Project numbers
# are immutable. QA plan/apply identities have no target-project read authority,
# so validation must not add a cross-project lookup or trust caller-supplied IDs.
locals {
  release_target_projects = {
    staging = { id = "g5lean-staging-1ccc", number = "237625147788" }
    prod    = { id = "g5lean-prod-1ccc", number = "177811684595" }
  }
}

variable "promotion_targets" {
  description = "Explicit staging/prod bindings. Empty keeps QA isolated. Populate after target bootstrap; secrets are references only."
  type = map(object({
    project_id             = string
    region                 = optional(string, "us-central1")
    cluster                = string
    filestore_instance     = string
    filestore_zone         = string
    filestore_storage      = optional(string, "1Ti")
    release_check_base_url = string
    release_check_secret   = string
    artifact_readers       = set(string)
    terraform_settings     = optional(any, {})
  }))
  default = {}
  validation {
    condition     = length(var.promotion_targets) == 0 || var.qa_release_authority_reviewed
    error_message = "Create the central release identity after QA IAM hardening, then bootstrap target platforms before supplying promotion_targets."
  }
  validation {
    condition = length(var.promotion_targets) == 0 || (
      toset(keys(var.promotion_targets)) == toset(["staging", "prod"]) &&
      alltrue([for target in values(var.promotion_targets) :
        can(regex("^[a-z][a-z0-9-]{4,28}[a-z0-9]$", target.project_id)) &&
        target.project_id != "g5-qa-1ccc" && target.cluster != "" &&
        startswith(target.release_check_base_url, "https://") &&
        target.release_check_secret != "" && length(target.artifact_readers) > 0
      ]) && length(distinct([for target in values(var.promotion_targets) : target.project_id])) == 2
    )
    error_message = "Supply both distinct staging/prod project bindings, clusters, HTTPS checks, check-secret references and explicit artifact readers, or leave promotion_targets empty."
  }
  validation {
    condition = alltrue([for environment, target in var.promotion_targets : try(
      target.project_id == local.release_target_projects[environment].id &&
      alltrue([for member in target.artifact_readers : contains([
        "serviceAccount:g5-lean-${environment}-api@${local.release_target_projects[environment].id}.iam.gserviceaccount.com",
        "serviceAccount:g5-lean-${environment}-host@${local.release_target_projects[environment].id}.iam.gserviceaccount.com",
        "serviceAccount:${local.release_target_projects[environment].number}-compute@developer.gserviceaccount.com",
      ], member)]), false)
    ])
    error_message = "Artifact readers must be the corresponding approved target API, worker-host or default Compute node identity. Central release access is added separately."
  }
  validation {
    condition = alltrue(flatten([for environment, target in var.promotion_targets : [
      for hostname in [try(target.terraform_settings.app_hostname, ""), try(target.terraform_settings.preview_hostname, "")] :
      hostname == "" || try(
        length(hostname) <= 253 &&
        can(regex("^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$", hostname)) &&
        (hostname == lookup({ staging = "lean.g5labs.ai", prod = "app.g5labs.ai" }, environment, "") ||
        endswith(hostname, ".${lookup({ staging = "lean.g5labs.ai", prod = "app.g5labs.ai" }, environment, "")}")),
        false
      )
    ]]))
    error_message = "Configured app and preview hostnames must be lowercase DNS names inside their environment's delegated child."
  }
  validation {
    condition = alltrue([for target in values(var.promotion_targets) : try(
      (!try(target.terraform_settings.external_access_enabled, false) ||
      (try(target.terraform_settings.app_hostname, "") != "" && try(target.terraform_settings.preview_hostname, "") != "")) &&
      (try(target.terraform_settings.app_hostname, "") == "" || try(target.terraform_settings.preview_hostname, "") == "" ||
      try(target.terraform_settings.app_hostname, "") != try(target.terraform_settings.preview_hostname, "")),
      false
    )])
    error_message = "Configured app and preview hostnames must be distinct; public ingress requires both."
  }
}

variable "enable_release_bootstrap" {
  description = "Enable reviewed setup releases without automatic app promotion or external acceptance claims."
  type        = bool
  default     = false
  validation {
    condition     = !var.enable_release_bootstrap || (var.enable_application_triggers && length(var.promotion_targets) == 2)
    error_message = "Bootstrap releases require application triggers and explicit staging/prod bindings."
  }
}

variable "enable_promotion" {
  description = "Activate QA -> approved staging -> automatic prod only after external acceptance and DNS reviews."
  type        = bool
  default     = false
  validation {
    condition     = !var.enable_promotion || (var.enable_application_triggers && length(var.promotion_targets) == 2)
    error_message = "Promotion requires application triggers and both fully configured target environments."
  }
}

variable "enable_release_train" {
  description = "Run G5's weekly release train once both target environments and promotion are enabled."
  type        = bool
  default     = true
}


variable "qa_release_authority_reviewed" {
  description = "After effective QA IAM review, create the central release identity/bucket independently of target setup; does not activate any release."
  type        = bool
  default     = false
}
