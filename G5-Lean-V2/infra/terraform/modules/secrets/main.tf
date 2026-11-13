# =============================================================================
# Secret Manager Module — Application secrets with rotation + K8s sync
# =============================================================================
# Stores sensitive application credentials in GCP Secret Manager instead of
# plain K8s Secrets. Secrets are encrypted with CMEK and support automatic
# rotation via Cloud Scheduler + Cloud Function (or manual rotation).
#
# Secrets managed:
#   - g5-jwt-secret          — ES256 key pair for JWT signing
#   - g5-worker-registration-token
#                             — Org-scoped worker bootstrap token
#   - g5-scm-encryption-key  — AES-256-GCM key for SCM token encryption
#   - g5-db-password         — Cloud SQL admin/migration user password
#   - g5-authz-writer-db-password
#                             — Cloud SQL authz writer password
#   - g5-runtime-system-db-password
#                             — Restricted runtime system-pool password
#   - g5-runtime-tenant-db-password
#                             — Restricted runtime tenant-pool password
#   - g5-github-oauth        — GitHub OAuth client ID + secret
#   - g5-llm-keys            — LLM API keys (Anthropic, OpenAI, OpenRouter)
#   - g5-github-app-*        — G5 GitHub App credentials for external issues
#                              (optional; inert blank defaults until seeded)
#
# K8s integration:
#   Use External Secrets Operator (ESO) or the GCP Secret Manager CSI driver
#   to sync secrets into K8s. The bootstrap script creates both GCP secrets
#   and K8s secrets, with GCP Secret Manager as the source of truth.
# =============================================================================

variable "project_id" { type = string }
variable "region" { type = string }
variable "kms_key_id" {
  description = "KMS crypto key ID for envelope encryption of secrets"
  type        = string
}
variable "labels" {
  type    = map(string)
  default = {}
}

variable "secretmanager_sa_email" {
  description = "Email of the Secret Manager service identity (for Pub/Sub publisher grant)"
  type        = string
}

variable "adopt_existing_secret_ids" {
  description = "Secret IDs that already exist in Secret Manager and should be adopted instead of created"
  type        = set(string)
  default     = []
}

variable "self_hosted_license_admin_secret_id" {
  description = "Environment-specific Secret Manager secret ID containing the raw admin bearer token for self-hosted license provisioning."
  type        = string
  default     = "g5-license-admin-token"
}

variable "enable_app_engine_rotation_consumer_access" {
  description = "Grant the legacy PROJECT@appspot.gserviceaccount.com rotation consumer access. Disable when the project has no App Engine application."
  type        = bool
  default     = true
}

variable "enable_release_payload_access" {
  description = "Allow the fixed central Lean release identity to refresh only the four Terraform-managed database secret versions in the approved staging/prod projects."
  type        = bool
  default     = false

  validation {
    condition = !var.enable_release_payload_access || (
      contains(["g5lean-staging-1ccc", "g5lean-prod-1ccc"], var.project_id) && var.region == "us-central1"
    )
    error_message = "Central release payload access is limited to g5lean-staging-1ccc and g5lean-prod-1ccc in us-central1."
  }
}

variable "enable_release_check_fixture" {
  description = "Create the named g5-release-checks synthetic-check secret container and central-release read access; its payload is provisioned separately."
  type        = bool
  default     = false

  validation {
    condition     = !var.enable_release_check_fixture || var.enable_release_payload_access
    error_message = "The release-check fixture requires the approved central release payload access configuration."
  }
}

# -----------------------------------------------------------------------------
# Secrets — each with CMEK, replication, and rotation metadata
# -----------------------------------------------------------------------------
locals {
  # github-token is intentionally retired. The plan deletes its container;
  # that also destroys versions not managed here. Keep the deletion guard.
  # See docs/production-pipeline-iam-review.md, "Ordered rollout after review".
  secrets = merge({
    "g5-jwt-secret" = {
      description = "ES256 private key for JWT signing (g5-api)"
    }
    "g5-jwt-public-key" = {
      description = "ES256 public key for JWT verification (shared)"
    }
    "g5-worker-registration-token" = {
      description = "Worker registration token used to exchange into short-lived session JWTs"
    }
    "g5-license-admin-token" = {
      description = "Legacy admin bearer token used by the manual self-hosted license provisioning job"
    }
    "g5-scm-encryption-key" = {
      description = "AES-256-GCM key for encrypting SCM tokens at rest (64 hex chars)"
    }
    "g5-db-password" = {
      description = "Cloud SQL application user (g5) password"
    }
    "g5-authz-writer-db-password" = {
      description = "Cloud SQL authz_writer password"
    }
    "g5-runtime-system-db-password" = {
      description = "Cloud SQL runtime system user (g5_runtime_system) password"
    }
    "g5-runtime-tenant-db-password" = {
      description = "Cloud SQL runtime tenant user (g5_runtime_tenant) password"
    }
    "g5-github-oauth-client-id" = {
      description = "GitHub OAuth App client ID"
    }
    "g5-github-oauth-client-secret" = {
      description = "GitHub OAuth App client secret"
    }
    "g5-anthropic-api-key" = {
      description = "Anthropic API key for Claude"
    }
    "g5-openai-api-key" = {
      description = "OpenAI API key"
    }
    "g5-openrouter-api-key" = {
      description = "OpenRouter API key"
    }
    "g5-platform-issue-reporting-enabled" = {
      description = "Platform issue reporting feature gate for g5-api"
    }
    "g5-platform-issue-github-repo" = {
      description = "Target GitHub repository for platform issue reporting"
    }
    "g5-platform-issue-github-app-id" = {
      description = "GitHub App id for platform issue reporting"
    }
    "g5-platform-issue-github-app-installation-id" = {
      description = "GitHub App installation id for platform issue reporting"
    }
    "g5-platform-issue-github-app-private-key" = {
      description = "GitHub App private key for platform issue reporting"
    }
    "g5-platform-issue-github-api-base-url" = {
      description = "GitHub API base URL for platform issue reporting"
    }
    "g5-org-id" = {
      description = "Default org UUID for worker registration (seeded by the environment bootstrap script)"
    }
    "g5-github-app-id" = {
      description = "G5 GitHub App id (external issues and GitHub App bindings)"
    }
    "g5-github-app-slug" = {
      description = "G5 GitHub App slug"
    }
    "g5-github-app-private-key" = {
      description = "G5 GitHub App private key (PEM)"
    }
    "g5-github-app-oauth-client-id" = {
      description = "G5 GitHub App OAuth client id"
    }
    "g5-github-app-oauth-client-secret" = {
      description = "G5 GitHub App OAuth client secret"
    }
    "g5-github-app-oauth-callback-url" = {
      description = "G5 GitHub App OAuth callback URL (<external URL>/api/v1/github-app/oauth/callback)"
    }
    "g5-github-app-connection-state-secret" = {
      description = "Secret (at least 32 bytes) signing G5 GitHub App connection state and the PKCE cookie"
    }
    "g5-email-transport" = {
      description = "Outbound email transport for g5-api: disabled, capture or resend"
    }
    "g5-email-from-domain" = {
      description = "Outbound email sending domain verified at the provider, e.g. mail.g5labs.ai"
    }
    "g5-email-resend-api-key" = {
      description = "Resend API key for outbound email (sending access only)"
    }
    "g5-tungsten-worker-registration-token" = {
      description = "Tungsten worker pool registration token — must be minted against the Tungsten tenant org so the Tungsten worker cluster registers org-scoped to that tenant. Seeded operationally; the container is created empty by Terraform."
    }
    }, {
    (var.self_hosted_license_admin_secret_id) = {
      description = "Environment-specific admin bearer token used by the manual self-hosted license provisioning job"
    }
  })
  adopted_secret_ids = toset([
    for secret_id in var.adopt_existing_secret_ids : secret_id
    if contains(keys(local.secrets), secret_id)
  ])
  managed_secrets = {
    for secret_id, config in local.secrets : secret_id => config
    if !contains(local.adopted_secret_ids, secret_id)
  }
}

resource "google_secret_manager_secret" "secrets" {
  for_each = local.managed_secrets

  project   = var.project_id
  secret_id = each.key

  labels = merge(var.labels, {
    managed_by = "terraform"
  })

  replication {
    user_managed {
      replicas {
        location = var.region

        customer_managed_encryption {
          kms_key_name = var.kms_key_id
        }
      }
    }
  }

  # Pub/Sub topic for rotation notifications — required alongside rotation block
  topics {
    name = google_pubsub_topic.secret_rotation.id
  }

  # Rotation reminder — triggers a Pub/Sub notification. The Cloud Function
  # in the secret-rotation module performs the actual rotation.
  rotation {
    rotation_period    = "7776000s" # 90 days
    next_rotation_time = timeadd(timestamp(), "7776000s")
  }

  lifecycle {
    # rotation.next_rotation_time changes on every plan; ignore after creation
    ignore_changes = [rotation[0].next_rotation_time]
  }

  # SM service agent must have pubsub.publisher on the topic before secrets
  # reference it; the IAM binding is created in the same module.
  depends_on = [google_pubsub_topic_iam_member.sm_publisher]
}

data "google_secret_manager_secret" "adopted" {
  for_each = local.adopted_secret_ids

  project   = var.project_id
  secret_id = each.key
}

# -----------------------------------------------------------------------------
# Platform issue reporting — inert "disabled" default bootstrap
# -----------------------------------------------------------------------------
# The six g5-platform-issue-* containers hold an optional, operator-seeded
# delivery configuration; real values arrive out-of-band and never enter
# Terraform state. But ESO renders its target Secret all-or-nothing, so a
# versionless container makes the g5-platform-issue-reporting-secrets
# ExternalSecret unsyncable ("Secret does not exist") and the fail-closed
# verify-deploy gate red on the first deploy.
#
# Bootstrap one inert default version per managed container so ESO can render
# from day one with the feature OFF: the gate value is literal "false" and the
# five delivery fields are a single blank space. g5-api's config normalizes
# blank values to unset (normalized_optional) and its is_deliverable() check is
# fail-closed on enabled=false, so the pod runs with platform issue reporting
# disabled until an operator overlays real values (as newer versions; latest
# wins) and flips the gate to "true".
#
# This is deliberately NOT a Terraform-owned google_secret_manager_secret_version
# resource: that made Terraform believe it owned a version it could later
# destroy or push back over a live operator value. Instead a terraform_data
# resource runs a metadata-only helper (scripts/bootstrap_platform_issue_default.py)
# that lists version STATE only, adds the inert default ONLY when the container
# has no ENABLED version (so operator values always win, and re-applies no-op),
# never reads/logs/stores secret bytes, and fails closed if it cannot list.
# Adopted (pre-existing) containers are skipped — they already carry
# operator-managed versions. See ontology terraform_secrets [D1]/[D11]/[I3].
#
# The prior google_secret_manager_secret_version.platform_issue_reporting_default
# instances are forgotten from state WITHOUT destroying their remote versions
# via the `removed { lifecycle { destroy = false } }` block below.
locals {
  platform_issue_reporting_default_versions = {
    "g5-platform-issue-reporting-enabled"          = "false"
    "g5-platform-issue-github-repo"                = " "
    "g5-platform-issue-github-app-id"              = " "
    "g5-platform-issue-github-app-installation-id" = " "
    "g5-platform-issue-github-app-private-key"     = " "
    "g5-platform-issue-github-api-base-url"        = " "
  }
  platform_issue_reporting_default_seed = {
    for secret_id, value in local.platform_issue_reporting_default_versions :
    secret_id => value
    if contains(keys(local.managed_secrets), secret_id)
  }
}

# Non-destructive state migration: forget the six previously-managed
# google_secret_manager_secret_version.platform_issue_reporting_default
# instances WITHOUT destroying their remote Secret Manager versions. Those
# already-ENABLED inert versions satisfy the bootstrap probe below, so the first
# apply after this change is a metadata-only no-op. `destroy = false` is the
# Terraform (>= 1.7) syntax for forgetting a resource without destroying it.
# Remove this block once every environment's state no longer tracks the old
# resource.
removed {
  from = google_secret_manager_secret_version.platform_issue_reporting_default

  lifecycle {
    destroy = false
  }
}

# -----------------------------------------------------------------------------
# G5 GitHub App — inert blank default bootstrap
# -----------------------------------------------------------------------------
# The seven g5-github-app-* containers configure the optional G5 GitHub App
# (external issues: organization bindings, issue reads, comments). Like the
# platform-issue containers above, real values arrive out-of-band and never
# enter Terraform state, and the isolated g5-github-app-secrets ExternalSecret
# renders all-or-nothing. Each container is bootstrapped with a single-space
# version by the same metadata-only helper so that target syncs before real
# values exist: g5-api treats a blank value as unset, and with every value
# unset the App is simply not configured. An operator configures it by adding
# all seven real values as newer versions (latest wins); a partial set is
# reported by g5-api as missing settings. Whether an organization uses
# external issues is its own setting, stored in the database, not here.
locals {
  github_app_default_seed = {
    for secret_id in [
      "g5-github-app-id",
      "g5-github-app-slug",
      "g5-github-app-private-key",
      "g5-github-app-oauth-client-id",
      "g5-github-app-oauth-client-secret",
      "g5-github-app-oauth-callback-url",
      "g5-github-app-connection-state-secret",
    ] : secret_id => " "
    if contains(keys(local.managed_secrets), secret_id)
  }
}

# -----------------------------------------------------------------------------
# Outbound email — inert "disabled" default bootstrap
# -----------------------------------------------------------------------------
# The three g5-email-* containers configure outbound email (invitations; see
# .g5/ontology/software_architecture/email.g5.md and docs/email-delivery.md).
# Like the GitHub App containers, real values arrive out-of-band and never
# enter Terraform state, and the isolated g5-email-secrets ExternalSecret
# renders all-or-nothing. The transport is bootstrapped as literal "disabled"
# and the sending domain and key as a single space, so the pod starts with
# email off (g5-api refuses to start with an enabled transport that lacks its
# sending domain or key). An operator enables email by adding the real domain
# and key as newer versions, then a "resend" transport version (latest wins).
# The display name is G5_EMAIL_FROM_NAME (default "G5"), not a secret.
locals {
  email_default_seed = {
    for secret_id, value in {
      "g5-email-transport"      = "disabled"
      "g5-email-from-domain"    = " "
      "g5-email-resend-api-key" = " "
    } : secret_id => value
    if contains(keys(local.managed_secrets), secret_id)
  }
}

resource "terraform_data" "email_default_bootstrap" {
  for_each = local.email_default_seed

  triggers_replace = each.key

  provisioner "local-exec" {
    command = "command -v python3 >/dev/null 2>&1 || { echo 'ERROR: python3 not found in the apply image; cannot bootstrap ${google_secret_manager_secret.secrets[each.key].secret_id} (fail closed)' >&2; exit 1; }; python3 ${path.module}/scripts/bootstrap_platform_issue_default.py --project ${var.project_id} --secret-id ${google_secret_manager_secret.secrets[each.key].secret_id}"

    environment = {
      BOOTSTRAP_INERT_VALUE = each.value
    }
  }
}

resource "terraform_data" "github_app_default_bootstrap" {
  for_each = local.github_app_default_seed

  triggers_replace = each.key

  provisioner "local-exec" {
    command = "command -v python3 >/dev/null 2>&1 || { echo 'ERROR: python3 not found in the apply image; cannot bootstrap ${google_secret_manager_secret.secrets[each.key].secret_id} (fail closed)' >&2; exit 1; }; python3 ${path.module}/scripts/bootstrap_platform_issue_default.py --project ${var.project_id} --secret-id ${google_secret_manager_secret.secrets[each.key].secret_id}"

    environment = {
      BOOTSTRAP_INERT_VALUE = each.value
    }
  }
}

# Idempotent, non-destructive bootstrap: run the metadata-only helper once per
# managed container (create-only; triggers_replace is the stable secret id). The
# helper adds the inert disabled default ONLY when no ENABLED version exists,
# never reads/logs/stores payloads, and fails closed if it cannot list versions.
# terraform_data (a built-in resource) is used so this module needs no extra
# provider declaration. Referencing google_secret_manager_secret.secrets[each.key]
# makes each bootstrap depend on its container existing first; the ESO module's
# depends_on = [module.secrets] then ensures all six run before the isolated
# all-or-nothing ExternalSecret is applied.
#
# The inert default value is passed through the provisioner ENVIRONMENT, never on
# the command line. It is only ever "false" or a single space (never a real
# credential), but keeping values off argv avoids process-table exposure and
# keeps the helper safe if reused for a genuine secret.
resource "terraform_data" "platform_issue_reporting_default_bootstrap" {
  for_each = local.platform_issue_reporting_default_seed

  triggers_replace = each.key

  provisioner "local-exec" {
    # Fail closed with a clear diagnostic if the apply image has no python3
    # interpreter, rather than surfacing an opaque `local-exec` exec error. The
    # apply runner (gcr.io/cloud-builders/gcloud) ships python3, but this keeps
    # the metadata-only bootstrap contract (fail closed, write nothing) intact
    # if that ever changes — without coupling the step to a specific image.
    command = "command -v python3 >/dev/null 2>&1 || { echo 'ERROR: python3 not found in the apply image; cannot bootstrap ${google_secret_manager_secret.secrets[each.key].secret_id} (fail closed)' >&2; exit 1; }; python3 ${path.module}/scripts/bootstrap_platform_issue_default.py --project ${var.project_id} --secret-id ${google_secret_manager_secret.secrets[each.key].secret_id}"

    environment = {
      BOOTSTRAP_INERT_VALUE = each.value
    }
  }
}

locals {
  secret_refs = merge(
    {
      for secret_id, secret in google_secret_manager_secret.secrets : secret_id => {
        id        = secret.id
        project   = secret.project
        secret_id = secret.secret_id
      }
    },
    {
      for secret_id, secret in data.google_secret_manager_secret.adopted : secret_id => {
        id        = secret.id
        project   = secret.project
        secret_id = secret.secret_id
      }
    },
  )
}

# -----------------------------------------------------------------------------
# Pub/Sub topic for rotation notifications
# -----------------------------------------------------------------------------
resource "google_pubsub_topic" "secret_rotation" {
  name    = "g5-secret-rotation"
  project = var.project_id

  labels = var.labels
}

# Grant the Secret Manager service identity permission to publish
# rotation notifications to the Pub/Sub topic
resource "google_pubsub_topic_iam_member" "sm_publisher" {
  project = var.project_id
  topic   = google_pubsub_topic.secret_rotation.name
  role    = "roles/pubsub.publisher"
  member  = "serviceAccount:${var.secretmanager_sa_email}"
}

# Configure each secret to publish rotation events
resource "google_secret_manager_secret_iam_member" "rotation_publisher" {
  for_each = var.enable_app_engine_rotation_consumer_access ? local.secrets : {}

  project   = local.secret_refs[each.key].project
  secret_id = local.secret_refs[each.key].secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${var.project_id}@appspot.gserviceaccount.com"
}

# -----------------------------------------------------------------------------
# IAM — Grant GKE workload identity SA access to read secrets
# -----------------------------------------------------------------------------
variable "gke_api_sa_email" {
  description = "Email of the GKE Workload Identity SA for g5-api (e.g. g5-api@project.iam.gserviceaccount.com)"
  type        = string
  default     = ""
}

variable "gke_worker_sa_email" {
  description = "Email of the GKE Workload Identity SA for g5d workers"
  type        = string
  default     = ""
}

variable "api_secret_ids" {
  description = "Explicit Secret Manager reads for a direct API consumer. Lean APIs use ESO-projected Kubernetes secrets and require none."
  type        = set(string)
  default     = []
  validation {
    condition     = alltrue([for key in var.api_secret_ids : contains(keys(local.secrets), key)])
    error_message = "API secret access must name existing secret map keys."
  }
}

# New secret containers grant no API access by default; ESO owns projection.
resource "google_secret_manager_secret_iam_member" "api_accessor" {
  for_each = var.api_secret_ids

  project   = local.secret_refs[each.key].project
  secret_id = local.secret_refs[each.key].secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${var.gke_api_sa_email}"
}

# Workers can only read worker-specific secrets.
#
# `g5-tungsten-worker-registration-token` is intentionally NOT listed here.
# Worker pods never read GCP Secret Manager directly — they consume the
# `g5d-worker-secrets` Kubernetes secret materialized by External Secrets
# Operator, whose service account reads only its enabled manifest references
# (see modules/external-secrets). Adding
# the Tungsten token to this map would instead grant the *standard* worker
# GSA (`gke_worker_sa_email`) read access to the Tungsten tenant's token,
# widening the cross-pool blast radius for no benefit. If a future change
# needs direct GSM access from the Tungsten worker GSA, add a dedicated IAM
# member keyed on that distinct SA rather than extending this map.
locals {
  worker_secrets = {
    "g5-worker-registration-token" = true
    "g5-anthropic-api-key"         = true
    "g5-openai-api-key"            = true
    "g5-openrouter-api-key"        = true
    "g5-org-id"                    = true
  }
}

resource "google_secret_manager_secret_iam_member" "worker_accessor" {
  for_each = local.worker_secrets

  project   = local.secret_refs[each.key].project
  secret_id = local.secret_refs[each.key].secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${var.gke_worker_sa_email}"
}

# The central release identity manages target infrastructure but does not have
# project-wide secretAccessor. The provider reads these four generated password
# versions on create/refresh; OAuth, model and signing credentials remain outside
# this payload-read grant. No arbitrary principal or additional secret list is
# accepted here.
locals {
  release_member = "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"
  release_database_secret_ids = toset([
    "g5-db-password",
    "g5-authz-writer-db-password",
    "g5-runtime-system-db-password",
    "g5-runtime-tenant-db-password",
  ])
}

resource "google_secret_manager_secret_iam_member" "release_database_accessor" {
  for_each = var.enable_release_payload_access ? local.release_database_secret_ids : toset([])

  project   = local.secret_refs[each.key].project
  secret_id = local.secret_refs[each.key].secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = local.release_member
}

# Kept separate from local.secrets: application and worker identities must not
# inherit read access to the synthetic release-check credentials. No version is
# Terraform-managed, so a real check payload never enters this module's state.
resource "google_secret_manager_secret" "release_checks" {
  count = var.enable_release_check_fixture ? 1 : 0

  project   = var.project_id
  secret_id = "g5-release-checks"
  labels    = merge(var.labels, { managed_by = "terraform" })

  replication {
    user_managed {
      replicas {
        location = var.region
        customer_managed_encryption {
          kms_key_name = var.kms_key_id
        }
      }
    }
  }
}

resource "google_secret_manager_secret_iam_member" "release_check_accessor" {
  count = var.enable_release_check_fixture ? 1 : 0

  project   = google_secret_manager_secret.release_checks[0].project
  secret_id = google_secret_manager_secret.release_checks[0].secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = local.release_member
}

# -----------------------------------------------------------------------------
# Outputs
# -----------------------------------------------------------------------------
output "secret_ids" {
  description = "Map of secret name to full resource ID"
  value       = { for secret_id, secret in local.secret_refs : secret_id => secret.id }

  # Consumers create the generated DB versions through this output. Ensure the
  # release identity has its per-secret read grant before the provider creates
  # and immediately refreshes a version on a fresh environment. A live first
  # apply still needs to verify IAM propagation; dependency ordering is not a
  # claim that access changes propagate synchronously.
  depends_on = [google_secret_manager_secret_iam_member.release_database_accessor]
}

output "release_check_secret_id" {
  description = "Optional synthetic-check container name; payload must be configured out of band before checks are enabled."
  value       = var.enable_release_check_fixture ? google_secret_manager_secret.release_checks[0].secret_id : null
  depends_on  = [google_secret_manager_secret_iam_member.release_check_accessor]
}

output "rotation_topic_id" {
  value = google_pubsub_topic.secret_rotation.id
}
