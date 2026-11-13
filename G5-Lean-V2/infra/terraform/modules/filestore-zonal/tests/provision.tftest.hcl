mock_provider "google-beta" {}

variables {
  project_id    = "g5-qa-1ccc"
  instance_name = "g5-lean-qa-repos-zonal"
  network_name  = "g5-lean-qa"
  kms_key_name  = "projects/g5-qa-1ccc/locations/us-central1/keyRings/g5-lean-qa/cryptoKeys/filestore"
  labels        = { environment = "qa" }
}

run "separate_fixed_performance_candidate" {
  command = plan
  assert {
    condition = (
      google_filestore_instance.repos.name == "g5-lean-qa-repos-zonal" &&
      google_filestore_instance.repos.project == "g5-qa-1ccc" &&
      google_filestore_instance.repos.location == "us-central1-a" &&
      google_filestore_instance.repos.tier == "ZONAL" &&
      google_filestore_instance.repos.protocol == "NFS_V3" &&
      google_filestore_instance.repos.file_shares[0].name == "g5_repos" &&
      google_filestore_instance.repos.file_shares[0].capacity_gb == 1024 &&
      google_filestore_instance.repos.performance_config[0].fixed_iops[0].max_iops == 4000 &&
      length(google_filestore_instance.repos.performance_config[0].iops_per_tb) == 0
    )
    error_message = "The new instance must be 1 TiB ZONAL/NFSv3 with fixed 4000 IOPS, not capacity-scaled performance."
  }
  assert {
    condition = (
      google_filestore_instance.repos.kms_key_name == var.kms_key_name &&
      google_filestore_instance.repos.deletion_protection_enabled &&
      google_filestore_instance.repos.networks[0].network == "g5-lean-qa" &&
      google_filestore_instance.repos.networks[0].connect_mode == "DIRECT_PEERING" &&
      google_filestore_instance.repos.networks[0].modes == tolist(["MODE_IPV4"])
    )
    error_message = "The separately protected candidate must use the existing VPC and regional CMEK."
  }
}

run "reject_missing_cmek" {
  command = plan
  variables { kms_key_name = "" }
  expect_failures = [var.kms_key_name]
}

run "reject_wrong_region_cmek" {
  command = plan
  variables { kms_key_name = "projects/g5-qa-1ccc/locations/europe-west1/keyRings/test/cryptoKeys/filestore" }
  expect_failures = [var.kms_key_name]
}
