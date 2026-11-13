# Plan only: ESO has kubectl provisioners that provider mocks do not suppress.
mock_provider "google" {
  mock_resource "google_service_account" {
    defaults = { email = "g5-lean-staging-eso@g5lean-staging-1ccc.iam.gserviceaccount.com" }
  }
}
mock_provider "helm" {}
mock_provider "null" {}

variables {
  project_id  = "g5lean-staging-1ccc"
  region      = "us-central1"
  environment = "staging"
  cluster = {
    name = "g5-lean-staging", location = "us-central1", endpoint = "10.0.0.1", ca = "bW9jaw=="
  }
  application = {
    project_id    = "g5lean-staging-1ccc", region = "us-central1",
    external_url  = "https://unconfigured.invalid", external_access_enabled = false,
    database_ip   = "10.60.0.2",
    database_name = "g5_lean_staging", database_user = "g5"
  }
}

run "staging_private_bootstrap" {
  command = plan
}
run "prod_origin_without_public_activation" {
  command = plan
  variables {
    project_id  = "g5lean-prod-1ccc"
    environment = "prod"
    cluster = {
      name = "g5-lean-prod", location = "us-central1", endpoint = "10.0.0.2", ca = "bW9jaw=="
    }
    application = {
      project_id    = "g5lean-prod-1ccc", region = "us-central1",
      external_url  = "https://app.g5labs.ai", external_access_enabled = false,
      database_ip   = "10.60.0.3",
      database_name = "g5_lean_prod", database_user = "g5"
    }
    settings = { app_hostname = "app.g5labs.ai" }
  }
}
run "reject_other_project_state" {
  command = plan
  variables {
    application = {
      project_id    = "g5lean-prod-1ccc", region = "us-central1",
      external_url  = "https://unconfigured.invalid", external_access_enabled = false,
      database_ip   = "10.60.0.2",
      database_name = "g5_lean_prod", database_user = "g5"
    }
  }
  expect_failures = [var.application]
}
run "reject_qa_cluster" {
  command = plan
  variables {
    cluster = { name = "g5-lean-qa", location = "us-central1", endpoint = "10.0.0.1", ca = "bW9jaw==" }
  }
  expect_failures = [var.cluster]
}
run "accepts_platform_state_and_settings_from_before_the_allowlist_retirement" {
  # Applied platform state and reviewed inputs may still carry the retired
  # login allowlist; it is ignored rather than compared.
  command = plan
  variables {
    application = merge(var.application, { auth_allowed_emails = "@example.com" })
    settings    = { auth_allowed_emails = "@example.com,person@example.org" }
  }
}
run "reject_public_settings_not_applied_to_platform" {
  command = plan
  variables { settings = { external_access_enabled = true } }
  expect_failures = [var.settings]
}
run "reject_other_region" {
  command = plan
  variables { region = "us-east1" }
  expect_failures = [var.region]
}

run "public_settings_match_applied_platform" {
  command = plan
  variables {
    settings = {
      external_access_enabled = true,
      app_hostname            = "lean.g5labs.ai", preview_hostname = "preview.lean.g5labs.ai"
    }
    application = {
      project_id       = "g5lean-staging-1ccc", region = "us-central1",
      external_url     = "https://lean.g5labs.ai", external_access_enabled = true,
      preview_hostname = "preview.lean.g5labs.ai",
      database_ip      = "10.60.0.2", database_name = "g5_lean_staging", database_user = "g5"
    }
  }
}
run "reject_preview_settings_not_applied_to_platform" {
  command = plan
  variables { settings = { preview_hostname = "changed.g5labs.ai" } }
  expect_failures = [var.settings]
}

run "renamed_private_hosts_match_applied_platform" {
  command = plan
  variables {
    settings = {
      app_hostname     = "next.lean.g5labs.ai"
      preview_hostname = "preview.next.lean.g5labs.ai"
    }
    application = merge(var.application, {
      external_url     = "https://next.lean.g5labs.ai"
      preview_hostname = "preview.next.lean.g5labs.ai"
    })
  }
}

run "reject_matching_legacy_state_outside_child" {
  command = plan
  variables {
    settings    = { app_hostname = "next-stage.g5labs.ai" }
    application = merge(var.application, { external_url = "https://next-stage.g5labs.ai" })
  }
  expect_failures = [var.settings]
}

run "reject_matching_lookalike_child" {
  command = plan
  variables {
    settings    = { app_hostname = "evillean.g5labs.ai" }
    application = merge(var.application, { external_url = "https://evillean.g5labs.ai" })
  }
  expect_failures = [var.settings]
}

run "reject_matching_preview_outside_child" {
  command = plan
  variables {
    settings    = { preview_hostname = "preview.app.g5labs.ai" }
    application = merge(var.application, { preview_hostname = "preview.app.g5labs.ai" })
  }
  expect_failures = [var.settings]
}

run "reject_matching_same_app_preview_before_activation" {
  command = plan
  variables {
    settings = {
      app_hostname     = "lean.g5labs.ai"
      preview_hostname = "lean.g5labs.ai"
    }
    application = merge(var.application, {
      external_url     = "https://lean.g5labs.ai"
      preview_hostname = "lean.g5labs.ai"
    })
  }
  expect_failures = [var.settings]
}
