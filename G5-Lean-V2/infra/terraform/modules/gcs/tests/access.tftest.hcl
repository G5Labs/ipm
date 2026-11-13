mock_provider "google" {}
variables {
  project_id            = "g5-qa-1ccc"
  region                = "us-central1"
  bucket_name           = "g5-qa-1ccc-lean-storage"
  cache_max_age_days    = 7
  artifact_max_age_days = 90
}
run "customer_storage_prevents_public_and_object_acl_access" {
  command = plan
  assert {
    condition     = google_storage_bucket.main.uniform_bucket_level_access && google_storage_bucket.main.public_access_prevention == "enforced"
    error_message = "QA customer storage must enforce its private boundary without relying on inherited policy."
  }
}
