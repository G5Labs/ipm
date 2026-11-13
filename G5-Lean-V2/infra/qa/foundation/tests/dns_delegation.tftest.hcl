# Plan only with mocked providers; preserve the existing QA zone/state identity.
mock_provider "google" {}
mock_provider "google-beta" {}

run "qa_zone_and_parent_delegation_keep_original_identity" {
  command = plan
  assert {
    condition = (
      google_dns_managed_zone.qa.project == "g5-qa-1ccc" &&
      google_dns_managed_zone.qa.name == "g5-lean-qa" &&
      google_dns_managed_zone.qa.dns_name == "qa.g5labs.ai." &&
      google_dns_managed_zone.qa.visibility == "public" &&
      output.dns_delegation.name == "qa.g5labs.ai." &&
      output.dns_delegation.type == "NS" &&
      output.dns_delegation.ttl == 300
    )
    error_message = "QA must retain its existing foundation zone and expose the parent NS delegation without moving state."
  }
}
