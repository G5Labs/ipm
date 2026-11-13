# No cloud credentials or DNS writes. Exercise the certificate/resource graph
# and delegated child records with mocked provider operations.
mock_provider "google" {
  mock_data "google_dns_managed_zone" {
    defaults = { dns_name = "staging.example.com." }
  }
  mock_resource "google_compute_global_address" {
    defaults = { address = "203.0.113.10" }
  }
  mock_resource "google_certificate_manager_dns_authorization" {
    defaults = {
      dns_resource_record = [{
        name = "_acme-challenge.staging.example.com."
        type = "CNAME"
        data = "authorization.example.net."
      }]
    }
  }
}

variables {
  project_id              = "lean-staging-test"
  region                  = "us-central1"
  network_name            = "lean-staging"
  system_subnet_cidr      = "10.10.0.0/20"
  worker_subnet_cidr      = "10.20.0.0/20"
  pod_cidr                = "10.30.0.0/16"
  service_cidr            = "10.40.0.0/20"
  dns_admin_project_id    = "lean-staging-test"
  dns_zone_name           = "lean-staging"
  dns_hostnames           = ["staging.example.com"]
  preview_subdomain_bases = ["preview.staging.example.com"]
}

run "delegated_dns_manages_records_and_keeps_certificates" {
  command = apply

  # Distinct provider responses catch incorrectly paired authorization outputs.
  override_resource {
    target = google_certificate_manager_dns_authorization.preview["staging.example.com"]
    values = {
      dns_resource_record = [{
        name = "_acme-challenge.staging.example.com."
        type = "CNAME"
        data = "app-authorization.example.net."
      }]
    }
  }
  override_resource {
    target = google_certificate_manager_dns_authorization.preview["preview.staging.example.com"]
    values = {
      dns_resource_record = [{
        name = "_acme-challenge.preview.staging.example.com."
        type = "CNAME"
        data = "preview-authorization.example.net."
      }]
    }
  }

  assert {
    condition = (
      length(google_certificate_manager_dns_authorization.preview) == 2 &&
      length(google_certificate_manager_certificate_map_entry.preview) == 2 &&
      length(output.dns_records_to_publish) == 4 &&
      output.dns_records_to_publish["preview:preview.staging.example.com"].name == "*.preview.staging.example.com." &&
      output.dns_records_to_publish["ingress:staging.example.com"].data == "203.0.113.10" &&
      output.dns_records_to_publish["authorization:staging.example.com"].type == "CNAME" &&
      output.dns_records_to_publish["authorization:staging.example.com"].name == "_acme-challenge.staging.example.com." &&
      output.dns_records_to_publish["authorization:staging.example.com"].data == "app-authorization.example.net." &&
      output.dns_records_to_publish["authorization:preview.staging.example.com"].name == "_acme-challenge.preview.staging.example.com." &&
      output.dns_records_to_publish["authorization:preview.staging.example.com"].data == "preview-authorization.example.net."
    )
    error_message = "Delegated DNS must retain TLS resources and expose all managed A/CNAME records for review."
  }
  assert {
    condition = (
      google_dns_record_set.ingress["staging.example.com"].managed_zone == "lean-staging" &&
      google_dns_record_set.ingress["staging.example.com"].project == "lean-staging-test" &&
      google_dns_record_set.ingress["staging.example.com"].name == "staging.example.com." &&
      google_dns_record_set.ingress["staging.example.com"].type == "A" &&
      toset(google_dns_record_set.ingress["staging.example.com"].rrdatas) == toset(["203.0.113.10"]) &&
      google_dns_record_set.preview_wildcard["preview.staging.example.com"].name == "*.preview.staging.example.com." &&
      toset(google_dns_record_set.preview_wildcard["preview.staging.example.com"].rrdatas) == toset(["203.0.113.10"]) &&
      google_dns_record_set.preview_dns_auth_cname["staging.example.com"].name == "_acme-challenge.staging.example.com." &&
      toset(google_dns_record_set.preview_dns_auth_cname["staging.example.com"].rrdatas) == toset(["app-authorization.example.net."]) &&
      google_dns_record_set.preview_dns_auth_cname["preview.staging.example.com"].name == "_acme-challenge.preview.staging.example.com." &&
      toset(google_dns_record_set.preview_dns_auth_cname["preview.staging.example.com"].rrdatas) == toset(["preview-authorization.example.net."]) &&
      alltrue([for record in google_dns_record_set.preview_dns_auth_cname : record.managed_zone == "lean-staging" && record.type == "CNAME" && record.ttl == 300])
    )
    error_message = "The child zone must receive the exact A and provider-issued CNAME values, paired by hostname."
  }
  assert {
    condition = (
      toset(google_certificate_manager_certificate.preview[0].managed[0].domains) == toset([
        "staging.example.com", "*.preview.staging.example.com",
      ]) &&
      toset(keys(google_certificate_manager_certificate_map_entry.preview)) == toset([
        "staging.example.com", "*.preview.staging.example.com",
      ]) &&
      alltrue([
        for hostname, entry in google_certificate_manager_certificate_map_entry.preview :
        entry.hostname == hostname &&
        entry.map == google_certificate_manager_certificate_map.preview[0].name &&
        toset(entry.certificates) == toset([google_certificate_manager_certificate.preview[0].id])
      ])
    )
    error_message = "The certificate and attached map entries must cover both the application host and preview wildcard."
  }
}

run "qa_retains_ip_certificate_and_record_addresses" {
  command = plan
  override_data {
    target = data.google_dns_managed_zone.application[0]
    values = { dns_name = "qa.g5labs.ai." }
  }
  variables {
    project_id              = "g5-qa-1ccc"
    network_name            = "g5-lean-qa"
    static_ip_name          = "g5-lean-qa-ip"
    dns_admin_project_id    = "g5-qa-1ccc"
    dns_zone_name           = "g5-lean-qa"
    dns_hostnames           = ["qa.g5labs.ai"]
    preview_subdomain_bases = ["preview.qa.g5labs.ai"]
  }
  assert {
    condition = (
      toset(keys(google_dns_record_set.ingress)) == toset(["qa.g5labs.ai"]) &&
      toset(keys(google_dns_record_set.preview_wildcard)) == toset(["preview.qa.g5labs.ai"]) &&
      toset(keys(google_dns_record_set.preview_dns_auth_cname)) == toset(["qa.g5labs.ai", "preview.qa.g5labs.ai"]) &&
      google_dns_record_set.ingress["qa.g5labs.ai"].managed_zone == "g5-lean-qa" &&
      google_dns_record_set.ingress["qa.g5labs.ai"].project == "g5-qa-1ccc" &&
      google_compute_global_address.ingress[0].name == "g5-lean-qa-ip" &&
      google_certificate_manager_certificate.preview[0].name == "g5-lean-qa-ip-preview-cert" &&
      google_certificate_manager_certificate_map.preview[0].name == "g5-lean-qa-ip-cert-map" &&
      toset(google_certificate_manager_certificate.preview[0].managed[0].domains) == toset([
        "qa.g5labs.ai", "*.preview.qa.g5labs.ai",
      ]) &&
      google_certificate_manager_dns_authorization.preview["qa.g5labs.ai"].name == "dnsauth-qa-g5labs-ai" &&
      google_certificate_manager_dns_authorization.preview["preview.qa.g5labs.ai"].name == "dnsauth-preview-qa-g5labs-ai" &&
      toset(keys(google_certificate_manager_certificate_map_entry.preview)) == toset([
        "qa.g5labs.ai", "*.preview.qa.g5labs.ai",
      ]) &&
      length(output.dns_records_to_publish) == 4
    )
    error_message = "QA delegation must preserve the four record addresses, zone binding, IP and TLS identities."
  }
}

run "app_only_outputs_a_record_without_preview_certificate" {
  command = plan
  variables { preview_subdomain_bases = [] }
  assert {
    condition = (
      length(google_dns_record_set.ingress) == 1 &&
      length(google_dns_record_set.preview_dns_auth_cname) == 0 &&
      length(output.dns_records_to_publish) == 1 &&
      output.dns_records_to_publish["ingress:staging.example.com"].name == "staging.example.com." &&
      length(google_certificate_manager_certificate.preview) == 0 &&
      output.preview_certificate_map_name == ""
    )
    error_message = "App-only ingress must output its A record without implicitly creating a preview certificate."
  }
}

run "private_bootstrap_reserves_ip_without_public_configuration" {
  command = plan
  variables {
    dns_admin_project_id    = ""
    dns_zone_name           = ""
    dns_hostnames           = []
    preview_subdomain_bases = []
  }
  assert {
    condition = (
      length(google_compute_global_address.ingress) == 1 &&
      length(data.google_dns_managed_zone.application) == 0 &&
      length(google_dns_record_set.ingress) == 0 &&
      length(google_dns_record_set.preview_wildcard) == 0 &&
      length(google_dns_record_set.preview_dns_auth_cname) == 0 &&
      length(google_certificate_manager_certificate.preview) == 0 &&
      length(output.dns_records_to_publish) == 0 &&
      output.preview_certificate_map_name == ""
    )
    error_message = "Private bootstrap must reserve the IP without publishing DNS or requesting certificates."
  }
}

run "no_ingress_ip_means_no_publishable_records" {
  command = plan
  variables {
    create_static_ip = false
  }
  assert {
    condition = (
      length(data.google_dns_managed_zone.application) == 0 &&
      length(google_dns_record_set.ingress) == 0 &&
      length(google_dns_record_set.preview_wildcard) == 0 &&
      length(google_dns_record_set.preview_dns_auth_cname) == 0 &&
      length(output.dns_records_to_publish) == 0
    )
    error_message = "Without an ingress IP, no zone lookup or DNS record may be created."
  }
}

run "reject_missing_zone_binding" {
  command = plan
  variables { dns_zone_name = "" }
  expect_failures = [var.dns_zone_name]
}

run "reject_missing_zone_project" {
  command = plan
  variables { dns_admin_project_id = "" }
  expect_failures = [var.dns_zone_name]
}

run "reject_application_outside_child" {
  command = plan
  variables { dns_hostnames = ["another.example.com"] }
  expect_failures = [data.google_dns_managed_zone.application]
}

run "reject_preview_outside_child" {
  command = plan
  variables { preview_subdomain_bases = ["preview.notstaging.example.com"] }
  expect_failures = [data.google_dns_managed_zone.application]
}

run "reject_authorization_record_outside_child" {
  command = apply
  variables { dns_hostnames = ["new.staging.example.com"] }
  override_resource {
    target = google_certificate_manager_dns_authorization.preview["new.staging.example.com"]
    values = {
      dns_resource_record = [{
        name = "_acme-challenge.another.example.com."
        type = "CNAME"
        data = "authorization.example.net."
      }]
    }
  }
  expect_failures = [google_dns_record_set.preview_dns_auth_cname]
}

run "reject_non_cname_authorization_record" {
  command = apply
  variables { dns_hostnames = ["new-type.staging.example.com"] }
  override_resource {
    target = google_certificate_manager_dns_authorization.preview["new-type.staging.example.com"]
    values = {
      dns_resource_record = [{
        name = "_acme-challenge.new-type.staging.example.com."
        type = "A"
        data = "203.0.113.99"
      }]
    }
  }
  expect_failures = [google_dns_record_set.preview_dns_auth_cname]
}
