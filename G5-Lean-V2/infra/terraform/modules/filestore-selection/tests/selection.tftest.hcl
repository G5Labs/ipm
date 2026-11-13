run "qa_provisions_and_selects_zonal_storage" {
  command = plan
  variables { environment = "qa" }
  assert {
    condition = (
      output.configuration.provision_zonal &&
      output.configuration.active == "zonal" &&
      output.configuration.selected == output.configuration.zonal &&
      output.configuration.legacy.instance_name == "g5-lean-qa-repos" &&
      output.configuration.zonal.instance_name == "g5-lean-qa-repos-zonal" &&
      output.configuration.zonal.pv_name != output.configuration.legacy.pv_name &&
      output.configuration.zonal.pvc_name != output.configuration.legacy.pvc_name
    )
    error_message = "QA must provision and select its separate zonal storage."
  }
}

run "staging_provisions_and_selects_zonal_storage" {
  command = plan
  variables { environment = "staging" }
  assert {
    condition = (
      output.configuration.provision_zonal &&
      output.configuration.active == "zonal" &&
      output.configuration.selected == output.configuration.zonal &&
      output.configuration.legacy.instance_name == "g5-lean-staging-repos" &&
      output.configuration.zonal.instance_name == "g5-lean-staging-repos-zonal" &&
      output.configuration.zonal.zone == "us-central1-a" &&
      output.configuration.zonal.capacity_gb == 1024 && output.configuration.zonal.storage == "1Ti"
    )
    error_message = "Staging must provision and select the reviewed zonal storage profile."
  }
}

run "production_provisions_and_selects_zonal_storage" {
  command = plan
  variables { environment = "prod" }
  assert {
    condition = (
      output.configuration.provision_zonal &&
      output.configuration.active == "zonal" &&
      output.configuration.selected == output.configuration.zonal &&
      output.configuration.legacy.instance_name == "g5-lean-prod-repos" &&
      output.configuration.zonal.instance_name == "g5-lean-prod-repos-zonal"
    )
    error_message = "Production must provision and select its separate zonal storage."
  }
}

run "reject_other_environment" {
  command = plan
  variables { environment = "dev" }
  expect_failures = [var.environment]
}
