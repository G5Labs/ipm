# Mock-provider plan only; no cloud resources are created. Google IAM evaluation and real Terraform
# lock acquisition/release still require the live preactivation check.
mock_provider "google" {}

run "planner_can_read_state_and_only_mutate_locks" {
  command = plan

  assert {
    condition = (
      google_storage_bucket_iam_binding.state_legacy_object_readers.bucket == "g5-qa-1ccc-lean-tfstate" &&
      google_storage_bucket_iam_binding.state_legacy_object_readers.role == "roles/storage.legacyObjectReader" &&
      google_storage_bucket_iam_binding.state_legacy_object_readers.members == toset(["projectOwner:g5-qa-1ccc"]) &&
      length(google_storage_bucket_iam_binding.state_legacy_object_readers.condition) == 0
    )
    error_message = "QA state must remove Project Viewer legacy object reads without replacing explicit planner or apply grants."
  }

  assert {
    condition     = google_storage_bucket.state.uniform_bucket_level_access && google_storage_bucket.state.public_access_prevention == "enforced"
    error_message = "QA state must prevent public access and use uniform bucket-level access."
  }
  assert {
    condition = (
      google_storage_bucket_iam_member.terraform_plan_state_read.role == "roles/storage.objectViewer" &&
      google_storage_bucket_iam_member.terraform_plan_state_read.bucket == google_storage_bucket.state.name
    )
    error_message = "The PR planner needs bucket-scoped read/list access to the existing QA state."
  }
  assert {
    condition = (
      google_storage_bucket_iam_member.terraform_plan_state.role == "roles/storage.objectAdmin" &&
      google_storage_bucket_iam_member.terraform_plan_state.bucket == google_storage_bucket.state.name &&
      google_storage_bucket_iam_member.terraform_plan_state.condition[0].expression == "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/g5-qa-1ccc-lean-tfstate/objects/') && resource.name.endsWith('.tflock')"
    )
    error_message = "The former unrestricted planner grant must be replaced by exact-bucket lock-only writes."
  }
  assert {
    condition = (
      google_storage_bucket_iam_member.terraform_state.role == "roles/storage.objectAdmin" &&
      length(google_storage_bucket_iam_member.terraform_state.condition) == 0
    )
    error_message = "The trusted apply identity must retain state-writing authority."
  }
}
