mock_provider "google" {}

run "isolated_operator_backend" {
  command = plan
  assert {
    condition = (
      google_storage_bucket_iam_binding.state_legacy_object_readers.bucket == "g5lean-admin-1ccc-lean-tfstate" &&
      google_storage_bucket_iam_binding.state_legacy_object_readers.role == "roles/storage.legacyObjectReader" &&
      google_storage_bucket_iam_binding.state_legacy_object_readers.members == toset(["projectOwner:g5lean-admin-1ccc"]) &&
      length(google_storage_bucket_iam_binding.state_legacy_object_readers.condition) == 0
    )
    error_message = "Admin state must exclude Project Viewers from the legacy object-reader grant; basic Project Viewers must not read state."
  }
  assert {
    condition = (
      google_storage_bucket.state.project == "g5lean-admin-1ccc" &&
      google_storage_bucket.state.name == "g5lean-admin-1ccc-lean-tfstate" &&
      google_storage_bucket.state.location == "us-central1" &&
      google_storage_bucket.state.uniform_bucket_level_access &&
      google_storage_bucket.state.public_access_prevention == "enforced" &&
      google_storage_bucket.state.versioning[0].enabled &&
      !google_storage_bucket.state.force_destroy &&
      length(google_storage_bucket.state.lifecycle_rule) == 0
    )
    error_message = "Admin Terraform state must remain private, versioned and independent of artifact storage."
  }
  assert {
    condition = toset(keys(google_project_service.required)) == toset([
      "artifactregistry.googleapis.com", "cloudresourcemanager.googleapis.com",
      "containeranalysis.googleapis.com", "containerscanning.googleapis.com",
      "compute.googleapis.com", "iam.googleapis.com",
      "serviceusage.googleapis.com", "storage.googleapis.com",
    ])
    error_message = "Admin must not acquire QA build, application runtime, DNS or secret infrastructure."
  }
}

run "reject_qa_target" {
  command = plan
  variables { project_id = "g5-qa-1ccc" }
  expect_failures = [var.project_id]
}

run "reject_other_region" {
  command = plan
  variables { region = "us-east1" }
  expect_failures = [var.region]
}
