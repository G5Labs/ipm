# Plan only with mocked provider operations; no IAM or API changes.
mock_provider "google" {}

run "dns_delegation_retains_dns_and_certificate_management" {
  command = plan
  assert {
    condition = (
      contains(keys(google_project_service.required), "dns.googleapis.com") &&
      contains(keys(google_project_iam_member.terraform_runner), "roles/dns.admin") &&
      contains(keys(google_project_service.required), "certificatemanager.googleapis.com") &&
      contains(keys(google_project_iam_member.terraform_runner), "roles/certificatemanager.editor") &&
      alltrue([for api in google_project_service.required : !api.disable_on_destroy])
    )
    error_message = "Delegated DNS must retain DNS and certificate authority while preserving safe API removal semantics."
  }
}
