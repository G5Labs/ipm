# =============================================================================
# External Secrets Operator (ESO) Module
# =============================================================================
# Installs ESO via Helm and configures a ClusterSecretStore backed by GCP
# Secret Manager. Creates ExternalSecret resources that automatically sync
# GCP secrets to K8s Secrets, replacing the static placeholder secrets.
#
# Prerequisites:
#   - GKE cluster with Workload Identity enabled
#   - GCP Secret Manager secrets provisioned (module "secrets")
#   - IAM: ESO reads only the enabled ExternalSecret remote references
#
# What this module creates:
#   1. ESO Helm release (external-secrets namespace)
#   2. K8s ServiceAccount for ESO with Workload Identity annotation
#   3. Per-secret GCP IAM bindings for the enabled projections
#   4. ClusterSecretStore pointing to GCP Secret Manager
#   5. ExternalSecret for g5-api-secrets (syncs all API secrets)
#   6. ExternalSecret for g5d-worker-secrets (syncs worker secrets)
# =============================================================================

terraform {
  required_providers {
    google = {
      source = "hashicorp/google"
    }
    helm = {
      source = "hashicorp/helm"
      # Standalone module tests must use the same set-block schema as callers.
      version = "~> 2.12"
    }
    null = {
      source = "hashicorp/null"
    }
  }
}

variable "project_id" { type = string }
variable "region" { type = string }
variable "namespace" {
  description = "K8s namespace for application secrets (target of ExternalSecret)"
  type        = string
  default     = "g5"
}

variable "additional_namespaces" {
  description = "Additional Kubernetes namespaces to keep synced during migrations"
  type        = list(string)
  default     = []
}

variable "eso_namespace" {
  description = "K8s namespace for the ESO controller"
  type        = string
  default     = "external-secrets"
}

variable "eso_chart_version" {
  description = "External Secrets Operator Helm chart version"
  type        = string
  default     = "0.12.1"
}

variable "cluster_name" { type = string }
variable "cluster_location" { type = string }

variable "refresh_interval" {
  description = "How often ESO polls GCP Secret Manager for changes"
  type        = string
  default     = "1h"
}

variable "sync_api_secret" {
  description = "Whether to sync g5-api-secrets into the target cluster"
  type        = bool
  default     = true
}

variable "sync_worker_secret" {
  description = "Whether to sync g5d-worker-secrets into the target cluster"
  type        = bool
  default     = true
}

variable "worker_registration_token_secret_id" {
  description = "GCP Secret Manager secret ID supplying the worker registration token synced into g5d-worker-secrets as G5D_REGISTRATION_TOKEN. Override for an org-scoped pool (e.g. the Tungsten worker cluster) so its workers register under the right tenant org."
  type        = string
  default     = "g5-worker-registration-token"
}

variable "self_hosted_license_admin_secret_id" {
  description = "GCP Secret Manager secret ID containing the raw admin bearer token. The API secret receives only its SHA-256 digest."
  type        = string
  default     = "g5-license-admin-token"
}

variable "worker_secret_minimal" {
  description = "When true, g5d-worker-secrets carries only G5D_API_URL + G5D_REGISTRATION_TOKEN. Used by org-scoped pools (e.g. Tungsten): the registration token alone pins the org, and provider API keys reach tasks via Runtime Config (TaskAssignment.secrets), never baked into the worker pod. When false, the secret also carries G5D_ORG_ID and the shared LLM keys (legacy default-pool behavior)."
  type        = bool
  default     = false
}

variable "external_url" {
  description = "Public-facing URL for G5_EXTERNAL_URL (e.g. https://dev.g5labs.ai)"
  type        = string
  default     = ""
}

variable "worker_api_url" {
  description = "Base URL workers should use for g5-api (e.g. https://dev.g5labs.ai)"
  type        = string
  default     = "http://g5-api:7500"
}

variable "database_private_ip" {
  description = "Cloud SQL private IP for constructing DATABASE_URL"
  type        = string
}

variable "database_name" {
  description = "Database name"
  type        = string
  default     = "g5"
}

variable "database_user" {
  description = "Database user"
  type        = string
  default     = "g5"
}

variable "labels" {
  type    = map(string)
  default = {}
}

# -----------------------------------------------------------------------------
# GCP Service Account for ESO controller (Workload Identity)
# -----------------------------------------------------------------------------
resource "google_service_account" "eso" {
  account_id   = "${var.cluster_name}-eso"
  project      = var.project_id
  display_name = "External Secrets Operator"
  description  = "Service account for ESO to read GCP Secret Manager secrets"
}

# The manifest reference lists are the allowlist: adding a container elsewhere
# never grants this controller access. Keep keys independent of rendered YAML,
# whose database address can be unknown during the first platform plan.
resource "google_secret_manager_secret_iam_member" "eso_secret_accessor" {
  for_each = local.enabled_secret_ids

  project   = var.project_id
  secret_id = each.key
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.eso.email}"
}

# Workload Identity binding: K8s SA "external-secrets" in ESO namespace
resource "google_service_account_iam_member" "eso_workload_identity" {
  service_account_id = google_service_account.eso.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "serviceAccount:${var.project_id}.svc.id.goog[${var.eso_namespace}/external-secrets]"
}

# -----------------------------------------------------------------------------
# ESO Helm Release
# -----------------------------------------------------------------------------
resource "helm_release" "external_secrets" {
  name             = "external-secrets"
  repository       = "https://charts.external-secrets.io"
  chart            = "external-secrets"
  version          = var.eso_chart_version
  namespace        = var.eso_namespace
  create_namespace = true
  wait             = true
  timeout          = 300

  # Annotate the ESO controller's K8s SA for Workload Identity
  set {
    name  = "serviceAccount.annotations.iam\\.gke\\.io/gcp-service-account"
    value = google_service_account.eso.email
  }

  set {
    name  = "serviceAccount.name"
    value = "external-secrets"
  }

  # Install CRDs
  set {
    name  = "installCRDs"
    value = "true"
  }
}

# -----------------------------------------------------------------------------
# ClusterSecretStore — GCP Secret Manager backend
# Uses null_resource + kubectl apply to avoid plan-time CRD validation.
# The CRDs only exist after the helm_release above installs ESO.
# -----------------------------------------------------------------------------
resource "null_resource" "cluster_secret_store" {
  triggers = {
    project_id       = var.project_id
    cluster_name     = var.cluster_name
    cluster_location = var.cluster_location
    manifest         = md5(local.cluster_secret_store_yaml)
  }

  provisioner "local-exec" {
    command = "gcloud container clusters get-credentials ${var.cluster_name} --location ${var.cluster_location} --project ${var.project_id} && echo '${base64encode(local.cluster_secret_store_yaml)}' | base64 -d | kubectl apply -f -"
  }

  provisioner "local-exec" {
    when    = destroy
    command = "gcloud container clusters get-credentials ${self.triggers.cluster_name} --location ${self.triggers.cluster_location} --project ${self.triggers.project_id} >/dev/null 2>&1 && kubectl delete clustersecretstore gcp-secret-manager --ignore-not-found || true"
  }

  depends_on = [helm_release.external_secrets, google_secret_manager_secret_iam_member.eso_secret_accessor]
}

resource "null_resource" "application_namespace" {
  for_each = {
    for namespace in local.target_namespaces : namespace => namespace
    if namespace != "default"
  }

  triggers = {
    namespace        = each.value
    project_id       = var.project_id
    cluster_name     = var.cluster_name
    cluster_location = var.cluster_location
  }

  provisioner "local-exec" {
    command = "gcloud container clusters get-credentials ${var.cluster_name} --location ${var.cluster_location} --project ${var.project_id} && kubectl create namespace ${self.triggers.namespace} --dry-run=client -o yaml | kubectl apply -f -"
  }

  depends_on = [helm_release.external_secrets]
}

# No destroy-time delete on any ExternalSecret null_resource: replacement is
# destroy-before-create and Terraform is non-transactional, so a destroy-time
# `kubectl delete externalsecret` removes the live projection (and, via the
# target's creationPolicy: Owner, the rendered Kubernetes Secret) before any
# create runs — a mid-apply failure then strands the cluster secret-less
# (2026-08-10 dev outage, issue #1768). The create-time kubectl apply updates
# the same-named resource in place on replacement; retiring a sync target is an
# explicit operator procedure (terraform_secrets [D12]/[I4]).
resource "null_resource" "api_external_secret" {
  for_each = var.sync_api_secret ? toset(local.target_namespaces) : toset([])

  triggers = {
    manifest         = md5(local.api_external_secret_yamls[each.value])
    namespace        = each.value
    project_id       = var.project_id
    cluster_name     = var.cluster_name
    cluster_location = var.cluster_location
  }

  provisioner "local-exec" {
    command = "gcloud container clusters get-credentials ${var.cluster_name} --location ${var.cluster_location} --project ${var.project_id} && echo '${base64encode(local.api_external_secret_yamls[each.value])}' | base64 -d | kubectl apply -f -"
  }

  depends_on = [
    null_resource.cluster_secret_store,
    null_resource.application_namespace,
  ]
}

# Platform issue reporting is deliberately isolated from g5-api-secrets. A
# versionless optional feature credential must not prevent ESO from rendering
# the API's required database/auth/service configuration.
#
# This target syncs from the very first deploy: modules/secrets seeds an inert
# "disabled" default version (enabled=false, others blank) into each of the six
# g5-platform-issue-* containers, so ESO can render immediately and the
# fail-closed deploy verification gate stays green. Operators overlay real
# values (and flip the gate to true) as newer versions later. See
# terraform_secrets [D1]/[D11]/[I3].
resource "null_resource" "platform_issue_reporting_external_secret" {
  for_each = var.sync_api_secret ? toset([var.namespace]) : toset([])

  triggers = {
    manifest         = md5(local.platform_issue_reporting_external_secret_yamls[each.value])
    namespace        = each.value
    project_id       = var.project_id
    cluster_name     = var.cluster_name
    cluster_location = var.cluster_location
  }

  provisioner "local-exec" {
    command = "gcloud container clusters get-credentials ${var.cluster_name} --location ${var.cluster_location} --project ${var.project_id} && echo '${base64encode(local.platform_issue_reporting_external_secret_yamls[each.value])}' | base64 -d | kubectl apply -f -"
  }

  depends_on = [
    null_resource.cluster_secret_store,
    null_resource.application_namespace,
  ]
}

# The G5 GitHub App target, applied like the platform-issue one: create-time
# apply only, no destroy-time delete (terraform_secrets [D12]).
resource "null_resource" "github_app_external_secret" {
  for_each = var.sync_api_secret ? toset([var.namespace]) : toset([])

  triggers = {
    manifest         = md5(local.github_app_external_secret_yamls[each.value])
    namespace        = each.value
    project_id       = var.project_id
    cluster_name     = var.cluster_name
    cluster_location = var.cluster_location
  }

  provisioner "local-exec" {
    command = "gcloud container clusters get-credentials ${var.cluster_name} --location ${var.cluster_location} --project ${var.project_id} && echo '${base64encode(local.github_app_external_secret_yamls[each.value])}' | base64 -d | kubectl apply -f -"
  }

  depends_on = [
    null_resource.cluster_secret_store,
    null_resource.application_namespace,
  ]
}

# The outbound email target, applied like the GitHub App one: create-time
# apply only, no destroy-time delete (terraform_secrets [D12]).
resource "null_resource" "email_external_secret" {
  for_each = var.sync_api_secret ? toset([var.namespace]) : toset([])

  triggers = {
    manifest         = md5(local.email_external_secret_yamls[each.value])
    namespace        = each.value
    project_id       = var.project_id
    cluster_name     = var.cluster_name
    cluster_location = var.cluster_location
  }

  provisioner "local-exec" {
    command = "gcloud container clusters get-credentials ${var.cluster_name} --location ${var.cluster_location} --project ${var.project_id} && echo '${base64encode(local.email_external_secret_yamls[each.value])}' | base64 -d | kubectl apply -f -"
  }

  depends_on = [
    null_resource.cluster_secret_store,
    null_resource.application_namespace,
  ]
}

resource "null_resource" "worker_external_secret" {
  for_each = var.sync_worker_secret ? toset(local.target_namespaces) : toset([])

  triggers = {
    manifest         = md5(local.worker_external_secret_yamls[each.value])
    namespace        = each.value
    project_id       = var.project_id
    cluster_name     = var.cluster_name
    cluster_location = var.cluster_location
  }

  provisioner "local-exec" {
    command = "gcloud container clusters get-credentials ${var.cluster_name} --location ${var.cluster_location} --project ${var.project_id} && echo '${base64encode(local.worker_external_secret_yamls[each.value])}' | base64 -d | kubectl apply -f -"
  }

  depends_on = [
    null_resource.cluster_secret_store,
    null_resource.application_namespace,
  ]
}

# -----------------------------------------------------------------------------
# YAML locals — rendered once so triggers can track changes via md5()
# -----------------------------------------------------------------------------
locals {
  target_namespaces = distinct(concat([var.namespace], var.additional_namespaces))

  cluster_secret_store_yaml = yamlencode({
    apiVersion = "external-secrets.io/v1beta1"
    kind       = "ClusterSecretStore"
    metadata = {
      name = "gcp-secret-manager"
    }
    spec = {
      provider = {
        gcpsm = {
          projectID = var.project_id
        }
      }
    }
  })

  # Each native reference list supplies both its manifest and its IAM grants.
  api_secret_remote_refs = [
    { secretKey = "g5_jwt_secret", remoteRef = { key = "g5-jwt-secret" } },
    { secretKey = "g5_jwt_public_key", remoteRef = { key = "g5-jwt-public-key" } },
    { secretKey = "g5_scm_encryption_key", remoteRef = { key = "g5-scm-encryption-key" } },
    { secretKey = "g5_db_password", remoteRef = { key = "g5-db-password" } },
    { secretKey = "g5_authz_writer_db_password", remoteRef = { key = "g5-authz-writer-db-password" } },
    { secretKey = "g5_runtime_system_db_password", remoteRef = { key = "g5-runtime-system-db-password" } },
    { secretKey = "g5_runtime_tenant_db_password", remoteRef = { key = "g5-runtime-tenant-db-password" } },
    { secretKey = "g5_github_client_id", remoteRef = { key = "g5-github-oauth-client-id" } },
    { secretKey = "g5_github_client_secret", remoteRef = { key = "g5-github-oauth-client-secret" } },
    { secretKey = "g5_license_admin_token", remoteRef = { key = var.self_hosted_license_admin_secret_id } },
    { secretKey = "g5_anthropic_api_key", remoteRef = { key = "g5-anthropic-api-key" } },
    { secretKey = "g5_openai_api_key", remoteRef = { key = "g5-openai-api-key" } },
    { secretKey = "g5_openrouter_api_key", remoteRef = { key = "g5-openrouter-api-key" } },
  ]
  platform_issue_reporting_secret_remote_refs = [
    { secretKey = "g5_platform_issue_reporting_enabled", remoteRef = { key = "g5-platform-issue-reporting-enabled" } },
    { secretKey = "g5_platform_issue_github_repo", remoteRef = { key = "g5-platform-issue-github-repo" } },
    { secretKey = "g5_platform_issue_github_app_id", remoteRef = { key = "g5-platform-issue-github-app-id" } },
    { secretKey = "g5_platform_issue_github_app_installation_id", remoteRef = { key = "g5-platform-issue-github-app-installation-id" } },
    { secretKey = "g5_platform_issue_github_app_private_key", remoteRef = { key = "g5-platform-issue-github-app-private-key" } },
    { secretKey = "g5_platform_issue_github_api_base_url", remoteRef = { key = "g5-platform-issue-github-api-base-url" } },
  ]
  github_app_secret_remote_refs = [
    { secretKey = "g5_github_app_id", remoteRef = { key = "g5-github-app-id" } },
    { secretKey = "g5_github_app_slug", remoteRef = { key = "g5-github-app-slug" } },
    { secretKey = "g5_github_app_private_key", remoteRef = { key = "g5-github-app-private-key" } },
    { secretKey = "g5_github_app_oauth_client_id", remoteRef = { key = "g5-github-app-oauth-client-id" } },
    { secretKey = "g5_github_app_oauth_client_secret", remoteRef = { key = "g5-github-app-oauth-client-secret" } },
    { secretKey = "g5_github_app_oauth_callback_url", remoteRef = { key = "g5-github-app-oauth-callback-url" } },
    { secretKey = "g5_github_app_connection_state_secret", remoteRef = { key = "g5-github-app-connection-state-secret" } },
  ]
  email_secret_remote_refs = [
    { secretKey = "g5_email_transport", remoteRef = { key = "g5-email-transport" } },
    { secretKey = "g5_email_from_domain", remoteRef = { key = "g5-email-from-domain" } },
    { secretKey = "g5_email_resend_api_key", remoteRef = { key = "g5-email-resend-api-key" } },
  ]
  enabled_secret_ids = toset(concat(
    var.sync_api_secret ? [for ref in concat(
      local.api_secret_remote_refs,
      local.platform_issue_reporting_secret_remote_refs,
      local.github_app_secret_remote_refs,
      local.email_secret_remote_refs,
    ) : ref.remoteRef.key] : [],
    var.sync_worker_secret ? [for ref in local.worker_secret_remote_refs : ref.remoteRef.key] : [],
  ))

  api_external_secret_yamls = {
    for namespace in local.target_namespaces : namespace => yamlencode({
      apiVersion = "external-secrets.io/v1beta1"
      kind       = "ExternalSecret"
      metadata = {
        name      = "g5-api-secrets"
        namespace = namespace
      }
      spec = {
        refreshInterval = var.refresh_interval
        secretStoreRef = {
          name = "gcp-secret-manager"
          kind = "ClusterSecretStore"
        }
        target = {
          name           = "g5-api-secrets"
          creationPolicy = "Owner"
          template = {
            type          = "Opaque"
            engineVersion = "v2"
            data = {
              G5_DATABASE_URL               = "postgres://g5_runtime_system@${var.database_private_ip}:5432/${var.database_name}?password={{ .g5_runtime_system_db_password }}&sslmode=require"
              G5_TENANT_DATABASE_URL        = "postgres://g5_runtime_tenant@${var.database_private_ip}:5432/${var.database_name}?password={{ .g5_runtime_tenant_db_password }}&sslmode=require"
              G5_AUTHZ_DATABASE_URL         = "postgres://authz_writer@${var.database_private_ip}:5432/${var.database_name}?password={{ .g5_authz_writer_db_password }}&sslmode=require"
              G5_ADMIN_DATABASE_URL         = "postgres://${var.database_user}@${var.database_private_ip}:5432/${var.database_name}?password={{ .g5_db_password }}&sslmode=require"
              G5_AUTHZ_WRITER_DB_PASSWORD   = "{{ .g5_authz_writer_db_password }}"
              G5_RUNTIME_SYSTEM_DB_PASSWORD = "{{ .g5_runtime_system_db_password }}"
              G5_RUNTIME_TENANT_DB_PASSWORD = "{{ .g5_runtime_tenant_db_password }}"
              G5_REQUIRE_RUNTIME_DB_SPLIT   = "true"
              G5_EXTERNAL_URL               = var.external_url
              G5_JWT_SECRET                 = "{{ .g5_jwt_secret }}"
              G5_JWT_PRIVATE_KEY            = "{{ .g5_jwt_secret }}"
              G5_JWT_PUBLIC_KEY             = "{{ .g5_jwt_public_key }}"
              G5_SCM_ENCRYPTION_KEY         = "{{ .g5_scm_encryption_key }}"
              G5_GITHUB_CLIENT_ID           = "{{ .g5_github_client_id }}"
              G5_GITHUB_CLIENT_SECRET       = "{{ .g5_github_client_secret }}"
              G5_LICENSE_ADMIN_TOKEN_SHA256 = "{{ .g5_license_admin_token | sha256sum }}"
              ANTHROPIC_API_KEY             = "{{ .g5_anthropic_api_key }}"
              OPENAI_API_KEY                = "{{ .g5_openai_api_key }}"
              OPENROUTER_API_KEY            = "{{ .g5_openrouter_api_key }}"
            }
          }
        }
        data = local.api_secret_remote_refs
      }
    })
  }

  platform_issue_reporting_external_secret_yamls = {
    for namespace in [var.namespace] : namespace => yamlencode({
      apiVersion = "external-secrets.io/v1beta1"
      kind       = "ExternalSecret"
      metadata = {
        name      = "g5-platform-issue-reporting-secrets"
        namespace = namespace
      }
      spec = {
        refreshInterval = var.refresh_interval
        secretStoreRef = {
          name = "gcp-secret-manager"
          kind = "ClusterSecretStore"
        }
        target = {
          name           = "g5-platform-issue-reporting-secrets"
          creationPolicy = "Owner"
          template = {
            type          = "Opaque"
            engineVersion = "v2"
            data = {
              G5_PLATFORM_ISSUE_REPORTING_ENABLED          = "{{ .g5_platform_issue_reporting_enabled }}"
              G5_PLATFORM_ISSUE_GITHUB_REPO                = "{{ .g5_platform_issue_github_repo }}"
              G5_PLATFORM_ISSUE_GITHUB_APP_ID              = "{{ .g5_platform_issue_github_app_id }}"
              G5_PLATFORM_ISSUE_GITHUB_APP_INSTALLATION_ID = "{{ .g5_platform_issue_github_app_installation_id }}"
              G5_PLATFORM_ISSUE_GITHUB_APP_PRIVATE_KEY     = "{{ .g5_platform_issue_github_app_private_key }}"
              G5_PLATFORM_ISSUE_GITHUB_API_BASE_URL        = "{{ .g5_platform_issue_github_api_base_url }}"
            }
          }
        }
        data = local.platform_issue_reporting_secret_remote_refs
      }
    })
  }

  # The optional G5 GitHub App credentials (external issues) are isolated like
  # platform issue reporting: the seven containers are bootstrapped with inert
  # blank versions (modules/secrets), so this target syncs from the first
  # deploy and g5-api sees the App as not configured until an operator adds
  # real values.
  github_app_external_secret_yamls = {
    for namespace in [var.namespace] : namespace => yamlencode({
      apiVersion = "external-secrets.io/v1beta1"
      kind       = "ExternalSecret"
      metadata = {
        name      = "g5-github-app-secrets"
        namespace = namespace
      }
      spec = {
        refreshInterval = var.refresh_interval
        secretStoreRef = {
          name = "gcp-secret-manager"
          kind = "ClusterSecretStore"
        }
        target = {
          name           = "g5-github-app-secrets"
          creationPolicy = "Owner"
          template = {
            type          = "Opaque"
            engineVersion = "v2"
            data = {
              G5_GITHUB_APP_ID                      = "{{ .g5_github_app_id }}"
              G5_GITHUB_APP_SLUG                    = "{{ .g5_github_app_slug }}"
              G5_GITHUB_APP_PRIVATE_KEY             = "{{ .g5_github_app_private_key }}"
              G5_GITHUB_APP_OAUTH_CLIENT_ID         = "{{ .g5_github_app_oauth_client_id }}"
              G5_GITHUB_APP_OAUTH_CLIENT_SECRET     = "{{ .g5_github_app_oauth_client_secret }}"
              G5_GITHUB_APP_OAUTH_CALLBACK_URL      = "{{ .g5_github_app_oauth_callback_url }}"
              G5_GITHUB_APP_CONNECTION_STATE_SECRET = "{{ .g5_github_app_connection_state_secret }}"
            }
          }
        }
        data = local.github_app_secret_remote_refs
      }
    })
  }

  # Outbound email (invitations) is isolated like the GitHub App: the three
  # g5-email-* containers are bootstrapped with an inert "disabled" transport
  # and blank sending domain/key (modules/secrets), so this target syncs from the
  # first deploy and g5-api runs with email off until an operator adds real
  # values (docs/email-delivery.md).
  email_external_secret_yamls = {
    for namespace in [var.namespace] : namespace => yamlencode({
      apiVersion = "external-secrets.io/v1beta1"
      kind       = "ExternalSecret"
      metadata = {
        name      = "g5-email-secrets"
        namespace = namespace
      }
      spec = {
        refreshInterval = var.refresh_interval
        secretStoreRef = {
          name = "gcp-secret-manager"
          kind = "ClusterSecretStore"
        }
        target = {
          name           = "g5-email-secrets"
          creationPolicy = "Owner"
          template = {
            type          = "Opaque"
            engineVersion = "v2"
            data = {
              G5_EMAIL_TRANSPORT      = "{{ .g5_email_transport }}"
              G5_EMAIL_FROM_DOMAIN    = "{{ .g5_email_from_domain }}"
              G5_EMAIL_RESEND_API_KEY = "{{ .g5_email_resend_api_key }}"
            }
          }
        }
        data = local.email_secret_remote_refs
      }
    })
  }

  # Always-present worker secret fields: API URL + registration token.
  worker_secret_template_data_base = {
    G5D_API_URL            = var.worker_api_url
    G5D_REGISTRATION_TOKEN = "{{ .g5_worker_registration_token }}"
  }
  worker_secret_remote_refs_base = [
    { secretKey = "g5_worker_registration_token", remoteRef = { key = var.worker_registration_token_secret_id } },
  ]
  # Legacy default-pool fields: shared org id + LLM keys. Omitted for
  # minimal (org-scoped) pools — the registration token alone pins the org,
  # and provider keys reach tasks via Runtime Config, not the worker pod.
  worker_secret_template_data_extended = {
    G5D_ORG_ID         = "{{ .g5_org_id }}"
    ANTHROPIC_API_KEY  = "{{ .g5_anthropic_api_key }}"
    OPENAI_API_KEY     = "{{ .g5_openai_api_key }}"
    OPENROUTER_API_KEY = "{{ .g5_openrouter_api_key }}"
  }
  worker_secret_remote_refs_extended = [
    { secretKey = "g5_org_id", remoteRef = { key = "g5-org-id" } },
    { secretKey = "g5_anthropic_api_key", remoteRef = { key = "g5-anthropic-api-key" } },
    { secretKey = "g5_openai_api_key", remoteRef = { key = "g5-openai-api-key" } },
    { secretKey = "g5_openrouter_api_key", remoteRef = { key = "g5-openrouter-api-key" } },
  ]
  worker_secret_template_data = var.worker_secret_minimal ? local.worker_secret_template_data_base : merge(
    local.worker_secret_template_data_base,
    local.worker_secret_template_data_extended,
  )
  worker_secret_remote_refs = var.worker_secret_minimal ? local.worker_secret_remote_refs_base : concat(
    local.worker_secret_remote_refs_base,
    local.worker_secret_remote_refs_extended,
  )

  worker_external_secret_yamls = {
    for namespace in local.target_namespaces : namespace => yamlencode({
      apiVersion = "external-secrets.io/v1beta1"
      kind       = "ExternalSecret"
      metadata = {
        name      = "g5d-worker-secrets"
        namespace = namespace
      }
      spec = {
        refreshInterval = var.refresh_interval
        secretStoreRef = {
          name = "gcp-secret-manager"
          kind = "ClusterSecretStore"
        }
        target = {
          name           = "g5d-worker-secrets"
          creationPolicy = "Owner"
          template = {
            type          = "Opaque"
            engineVersion = "v2"
            data          = local.worker_secret_template_data
          }
        }
        data = local.worker_secret_remote_refs
      }
    })
  }
}

# -----------------------------------------------------------------------------
# Outputs
# -----------------------------------------------------------------------------
output "eso_sa_email" {
  description = "GCP service account email for ESO"
  value       = google_service_account.eso.email
}
