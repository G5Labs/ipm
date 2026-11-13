variable "project_id" {
  type    = string
  default = "g5lean-admin-1ccc"
  validation {
    condition     = var.project_id == "g5lean-admin-1ccc"
    error_message = "This artifact root may manage only g5lean-admin-1ccc."
  }
}
variable "region" {
  type    = string
  default = "us-central1"
  validation {
    condition     = var.region == "us-central1"
    error_message = "Shared Lean artifacts are provisioned in us-central1."
  }
}

# Supply observed platform identities after the corresponding environment exists.
# The project lookups validate numeric default-Compute identities without guessing
# project numbers or accepting a reader from an unrelated project.
variable "runtime_registry_readers" {
  description = "Explicit serviceAccount: members for GKE nodes, worker hosts and APIs that pull the shared OCI repositories."
  type        = set(string)
  default     = []
  validation {
    condition     = alltrue([for member in var.runtime_registry_readers : contains(local.allowed_runtime_members, member)])
    error_message = "Registry readers must be an approved Lean QA/staging/prod API, worker-host or default Compute node identity."
  }
}
variable "runtime_image_users" {
  description = "Explicit serviceAccount: members that create worker hosts from shared GCE images."
  type        = set(string)
  default     = []
  validation {
    condition     = alltrue([for member in var.runtime_image_users : contains(local.allowed_image_users, member)])
    error_message = "Image users must be an approved Lean QA/staging/prod API identity."
  }
}
variable "runtime_worker_artifact_readers" {
  description = "Explicit serviceAccount: members requiring worker disk/archive metadata reads; leave empty when runtimes only use GCE images."
  type        = set(string)
  default     = []
  validation {
    condition     = alltrue([for member in var.runtime_worker_artifact_readers : contains(local.allowed_runtime_members, member)])
    error_message = "Worker artifact readers must be an approved Lean QA/staging/prod API, worker-host or default Compute node identity."
  }
}
