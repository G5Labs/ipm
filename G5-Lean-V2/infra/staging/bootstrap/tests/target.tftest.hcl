mock_provider "google" {}

run "approved_target" {
  command = plan
  assert {
    condition     = output.state_bucket == "g5lean-staging-1ccc-lean-tfstate"
    error_message = "State must remain in this environment's approved project."
  }
}

run "reject_other_environment" {
  command = plan
  variables { project_id = "g5lean-prod-1ccc" }
  expect_failures = [var.project_id]
}

run "reject_original_production" {
  command = plan
  variables { project_id = "g5-prod-1ccc" }
  expect_failures = [var.project_id]
}
