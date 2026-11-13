variable "project_id" {
  description = "Approved new Lean prod project. Existing G5 environments are never valid targets."
  type        = string
  default     = "g5lean-prod-1ccc"
  validation {
    condition     = var.project_id == "g5lean-prod-1ccc"
    error_message = "This root may only provision g5lean-prod-1ccc."
  }
}
variable "region" {
  type    = string
  default = "us-central1"
}
variable "settings" {
  description = "Non-secret environment settings, validated by the platform module. Shared across roots."
  type        = any
  default     = {}
}
