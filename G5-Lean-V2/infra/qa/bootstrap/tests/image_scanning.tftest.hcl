# Provider-mocked plan only: no API activation or cloud calls.
mock_provider "google" {}

run "automatic_scanning_in_artifact_owner" {
  command = plan
  assert {
    condition = alltrue([for api in ["containeranalysis.googleapis.com", "containerscanning.googleapis.com"] :
      google_project_service.required[api].project == "g5-qa-1ccc" &&
      !google_project_service.required[api].disable_on_destroy
    ]) && !contains(keys(google_project_service.required), "ondemandscanning.googleapis.com")
    error_message = "Enable automatic scanning in the QA artifact owner without on-demand scan administration or disabling APIs on removal."
  }
}
