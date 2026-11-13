mock_provider "google" {}

variables {
  project_id                                 = "g5lean-staging-1ccc"
  region                                     = "us-central1"
  kms_key_id                                 = "projects/g5lean-staging-1ccc/locations/us-central1/keyRings/test/cryptoKeys/secrets"
  secretmanager_sa_email                     = "service-123456789@gcp-sa-secretmanager.iam.gserviceaccount.com"
  gke_api_sa_email                           = "g5-api@g5lean-staging-1ccc.iam.gserviceaccount.com"
  gke_worker_sa_email                        = "g5-worker@g5lean-staging-1ccc.iam.gserviceaccount.com"
  enable_app_engine_rotation_consumer_access = false
}

run "existing_qa_defaults_do_not_add_release_access" {
  command = plan
  variables { project_id = "g5-qa-1ccc" }

  assert {
    condition = (
      length(google_secret_manager_secret_iam_member.release_database_accessor) == 0 &&
      length(google_secret_manager_secret.release_checks) == 0 &&
      length(google_secret_manager_secret_iam_member.release_check_accessor) == 0 &&
      output.release_check_secret_id == null
    )
    error_message = "Existing QA consumers must gain neither release payload grants nor a synthetic-check container by default."
  }
  assert {
    condition = (
      length(google_secret_manager_secret_iam_member.api_accessor) == 0 &&
      toset(keys(google_secret_manager_secret_iam_member.worker_accessor)) == toset(keys(local.worker_secrets))
    )
    error_message = "API payload access must default to none while worker access stays explicitly scoped."
  }
}

run "release_reads_exactly_four_managed_database_secrets" {
  command = plan
  variables { enable_release_payload_access = true }

  assert {
    condition = toset(keys(google_secret_manager_secret_iam_member.release_database_accessor)) == toset([
      "g5-db-password",
      "g5-authz-writer-db-password",
      "g5-runtime-system-db-password",
      "g5-runtime-tenant-db-password",
    ])
    error_message = "Release payload reads must cover exactly the four Terraform-managed database secrets."
  }
  assert {
    condition = alltrue([
      for binding in google_secret_manager_secret_iam_member.release_database_accessor :
      binding.member == "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com" &&
      binding.role == "roles/secretmanager.secretAccessor" &&
      binding.project == "g5lean-staging-1ccc"
    ])
    error_message = "Every scoped database read must target the fixed central release identity and the selected target project."
  }
  assert {
    condition     = length(google_secret_manager_secret.release_checks) == 0
    error_message = "Database refresh access must not implicitly create or enable synthetic checks."
  }
}

run "prod_can_opt_into_the_named_synthetic_fixture" {
  command = plan
  variables {
    project_id                    = "g5lean-prod-1ccc"
    enable_release_payload_access = true
    enable_release_check_fixture  = true
  }

  assert {
    condition = (
      length(google_secret_manager_secret.release_checks) == 1 &&
      google_secret_manager_secret.release_checks[0].secret_id == "g5-release-checks" &&
      google_secret_manager_secret.release_checks[0].project == "g5lean-prod-1ccc" &&
      output.release_check_secret_id == "g5-release-checks" &&
      google_secret_manager_secret_iam_member.release_check_accessor[0].member == "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_secret_manager_secret_iam_member.release_check_accessor[0].role == "roles/secretmanager.secretAccessor"
    )
    error_message = "Opt-in checks must use only the named target-local fixture with the fixed central release reader."
  }
  assert {
    condition = (
      !contains(keys(google_secret_manager_secret_iam_member.api_accessor), "g5-release-checks") &&
      !contains(keys(google_secret_manager_secret_iam_member.worker_accessor), "g5-release-checks") &&
      !contains(keys(output.secret_ids), "g5-release-checks")
    )
    error_message = "Synthetic-check credentials must not be included in the application/worker secret set."
  }
}

run "qa_cannot_opt_into_release_access" {
  command = plan
  variables {
    project_id                    = "g5-qa-1ccc"
    enable_release_payload_access = true
  }
  expect_failures = [var.enable_release_payload_access]
}

run "other_projects_cannot_opt_into_release_access" {
  command = plan
  variables {
    project_id                    = "g5lean-admin-1ccc"
    enable_release_payload_access = true
  }
  expect_failures = [var.enable_release_payload_access]
}

run "other_regions_cannot_opt_into_release_access" {
  command = plan
  variables {
    region                        = "us-east1"
    enable_release_payload_access = true
  }
  expect_failures = [var.enable_release_payload_access]
}

run "fixture_requires_release_configuration" {
  command = plan
  variables { enable_release_check_fixture = true }
  expect_failures = [var.enable_release_check_fixture]
}

run "api_has_no_implicit_reads_and_unused_pat_is_retired" {
  command = plan
  assert {
    condition = (
      length(google_secret_manager_secret_iam_member.api_accessor) == 0 &&
      !contains(keys(output.secret_ids), "github-token") &&
      !contains(keys(google_secret_manager_secret.secrets), "github-token") &&
      toset(keys(google_secret_manager_secret_iam_member.worker_accessor)) == toset(keys(local.worker_secrets))
    )
    error_message = "ESO owns API secret delivery; the unused PAT must not be created or exported."
  }
}

run "direct_api_consumer_must_explicitly_name_its_secret" {
  command = plan
  variables { api_secret_ids = ["g5-jwt-public-key"] }
  assert {
    condition     = toset(keys(google_secret_manager_secret_iam_member.api_accessor)) == toset(["g5-jwt-public-key"])
    error_message = "An explicit direct consumer must not inherit access to other secret containers."
  }
}

run "unknown_api_secret_is_rejected" {
  command = plan
  variables { api_secret_ids = ["typo-secret"] }
  expect_failures = [var.api_secret_ids]
}

run "retired_pat_cannot_be_granted" {
  command = plan
  variables { api_secret_ids = ["github-token"] }
  expect_failures = [var.api_secret_ids]
}
