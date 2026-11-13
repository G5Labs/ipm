# Plan only: provider mocks do not suppress the module's kubectl provisioners.
mock_provider "google" {}
mock_provider "helm" {}
mock_provider "null" {}

variables {
  project_id                          = "g5-qa-1ccc"
  region                              = "us-central1"
  cluster_name                        = "g5-lean-qa"
  cluster_location                    = "us-central1"
  database_private_ip                 = "10.1.2.3"
  self_hosted_license_admin_secret_id = "g5-license-admin-token-qa"
  sync_worker_secret                  = false
}

run "api_projection_grants_exact_named_secrets" {
  command = plan
  assert {
    condition = toset(keys(google_secret_manager_secret_iam_member.eso_secret_accessor)) == toset([
      "g5-jwt-secret", "g5-jwt-public-key", "g5-scm-encryption-key",
      "g5-db-password", "g5-authz-writer-db-password", "g5-runtime-system-db-password", "g5-runtime-tenant-db-password",
      "g5-github-oauth-client-id", "g5-github-oauth-client-secret", "g5-license-admin-token-qa",
      "g5-anthropic-api-key", "g5-openai-api-key", "g5-openrouter-api-key",
      "g5-platform-issue-reporting-enabled", "g5-platform-issue-github-repo", "g5-platform-issue-github-app-id",
      "g5-platform-issue-github-app-installation-id", "g5-platform-issue-github-app-private-key", "g5-platform-issue-github-api-base-url",
      "g5-github-app-id", "g5-github-app-slug", "g5-github-app-private-key", "g5-github-app-oauth-client-id",
      "g5-github-app-oauth-client-secret", "g5-github-app-oauth-callback-url", "g5-github-app-connection-state-secret",
      "g5-email-transport", "g5-email-from-domain", "g5-email-resend-api-key",
    ])
    error_message = "API projection may read only its 29 current inputs, excluding PATs, worker pools, release fixtures and legacy license tokens."
  }
  assert {
    condition = toset(keys(google_secret_manager_secret_iam_member.eso_secret_accessor)) == toset(flatten([
      for manifest in [
        local.api_external_secret_yamls["g5"],
        local.platform_issue_reporting_external_secret_yamls["g5"],
        local.github_app_external_secret_yamls["g5"],
        local.email_external_secret_yamls["g5"],
      ] : [for ref in yamldecode(manifest).spec.data : ref.remoteRef.key]
    ]))
    error_message = "Every rendered API reference must have a scoped grant, with no unused grants."
  }
  assert {
    condition = alltrue([for key, grant in google_secret_manager_secret_iam_member.eso_secret_accessor :
      grant.project == "g5-qa-1ccc" && grant.secret_id == key && grant.role == "roles/secretmanager.secretAccessor"
    ])
    error_message = "Projection reads must be individual secrets in the selected project."
  }
}

run "minimal_worker_pool_reads_only_its_own_token" {
  command = plan
  variables {
    sync_api_secret                     = false
    sync_worker_secret                  = true
    worker_secret_minimal               = true
    worker_registration_token_secret_id = "g5-tungsten-worker-registration-token"
  }
  assert {
    condition = (
      toset(keys(google_secret_manager_secret_iam_member.eso_secret_accessor)) == toset(["g5-tungsten-worker-registration-token"]) &&
      toset([for ref in yamldecode(local.worker_external_secret_yamls["g5"]).spec.data : ref.remoteRef.key]) == toset(["g5-tungsten-worker-registration-token"]) &&
      length(null_resource.api_external_secret) == 0 &&
      length(null_resource.platform_issue_reporting_external_secret) == 0 &&
      length(null_resource.github_app_external_secret) == 0 &&
      length(null_resource.email_external_secret) == 0
    )
    error_message = "An isolated worker pool must gain no default-pool, API, database, license or provider-secret access."
  }
}

run "extended_worker_projection_keeps_only_its_five_inputs" {
  command = plan
  variables {
    sync_api_secret    = false
    sync_worker_secret = true
  }
  assert {
    condition = toset(keys(google_secret_manager_secret_iam_member.eso_secret_accessor)) == toset([
      "g5-worker-registration-token", "g5-org-id", "g5-anthropic-api-key", "g5-openai-api-key", "g5-openrouter-api-key",
    ])
    error_message = "The legacy worker projection reads only its explicit five inputs."
  }
}

run "combined_projections_deduplicate_shared_secrets_and_namespaces" {
  command = plan
  variables {
    sync_worker_secret                  = true
    additional_namespaces               = ["g5", "g5-migration", "g5-migration"]
    self_hosted_license_admin_secret_id = "g5-license-admin-token-prod"
  }
  assert {
    condition = (
      length(google_secret_manager_secret_iam_member.eso_secret_accessor) == 31 &&
      contains(keys(google_secret_manager_secret_iam_member.eso_secret_accessor), "g5-license-admin-token-prod") &&
      !contains(keys(google_secret_manager_secret_iam_member.eso_secret_accessor), "g5-license-admin-token-qa") &&
      length(null_resource.api_external_secret) == 2 &&
      length(null_resource.worker_external_secret) == 2 &&
      length(null_resource.github_app_external_secret) == 1
    )
    error_message = "Additional namespaces must not expand secret authority; the configured license input must replace the previous environment's token."
  }
}

run "disabled_projections_grant_no_secret_access" {
  command = plan
  variables {
    sync_api_secret    = false
    sync_worker_secret = false
  }
  assert {
    condition     = length(google_secret_manager_secret_iam_member.eso_secret_accessor) == 0
    error_message = "A controller with no enabled projection must receive no payload access."
  }
}
