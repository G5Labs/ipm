# Real trigger plans, no cloud operations. Separate target plans must work even
# while the release queues and weekly schedule remain disabled.
mock_provider "google" {
  mock_data "google_service_account" {
    defaults = {
      name  = "projects/g5-qa-1ccc/serviceAccounts/g5-lean-ci@g5-qa-1ccc.iam.gserviceaccount.com"
      email = "g5-lean-ci@g5-qa-1ccc.iam.gserviceaccount.com"
    }
  }
  mock_data "google_project" {
    defaults = { number = "123456789012" }
  }
}

override_data {
  target = data.terraform_remote_state.foundation
  values = {
    outputs = {
      ci_pool = "projects/g5-qa-1ccc/locations/us-central1/workerPools/g5-ci"
    }
  }
}


variables {
  qa_release_authority_reviewed = true
  promotion_targets = {
    staging = {
      project_id             = "g5lean-staging-1ccc"
      cluster                = "g5-lean-staging"
      filestore_instance     = "g5-lean-staging-repos"
      filestore_zone         = "us-central1-a"
      release_check_base_url = "https://staging.example.test"
      release_check_secret   = "g5-release-check"
      artifact_readers = [
        "serviceAccount:g5-lean-staging-api@g5lean-staging-1ccc.iam.gserviceaccount.com",
      ]
    }
    prod = {
      project_id             = "g5lean-prod-1ccc"
      cluster                = "g5-lean-prod"
      filestore_instance     = "g5-lean-prod-repos"
      filestore_zone         = "us-central1-a"
      release_check_base_url = "https://app.example.test"
      release_check_secret   = "g5-release-check"
      artifact_readers = [
        "serviceAccount:g5-lean-prod-api@g5lean-prod-1ccc.iam.gserviceaccount.com",
      ]
    }
  }
}

override_data {
  target = data.terraform_remote_state.release_isolation[0]
  values = { outputs = { release_identity_isolation_ready = true } }
}

run "target_plans_without_release_activation" {
  command = plan

  assert {
    condition = (
      toset(keys(google_cloudbuild_trigger.terraform_release_plan)) == toset(["staging", "prod"]) &&
      alltrue([for env, trigger in google_cloudbuild_trigger.terraform_release_plan :
        trigger.name == "g5-terraform-plan-${env}" &&
        trigger.project == "g5-qa-1ccc" && trigger.location == "us-central1" &&
        trigger.filename == "infra/ci/cloudbuild-terraform-release.yaml" &&
        !trigger.disabled && !trigger.approval_config[0].approval_required &&
        trigger.repository_event_config[0].pull_request[0].branch == "^main$" &&
        trigger.repository_event_config[0].pull_request[0].comment_control == "COMMENTS_ENABLED_FOR_EXTERNAL_CONTRIBUTORS_ONLY"
      ])
    )
    error_message = "Both target checks must run PR source without release approval, using only the dedicated target planner."
  }

  assert {
    condition = alltrue([for env, trigger in google_cloudbuild_trigger.terraform_release_plan :
      trigger.substitutions["_TF_ACTION"] == "plan" &&
      trigger.substitutions["_TF_BUNDLE_MODE"] == "source" &&
      trigger.substitutions["_TF_WORKSPACE"] == env &&
      trigger.substitutions["_TF_BUNDLE_REF"] == "" &&
      trigger.substitutions["_NEXT_TRIGGER"] == "" &&
      trigger.substitutions["_NEXT_TRIGGER_PROJECT"] == "" &&
      trigger.substitutions["_STATE_BUCKET"] == "${var.promotion_targets[env].project_id}-lean-tfstate" &&
      jsondecode(trigger.substitutions["_TF_CONFIG_JSON"]).project_id == var.promotion_targets[env].project_id &&
      jsondecode(trigger.substitutions["_TF_CONFIG_JSON"]).region == "us-central1" &&
      jsonencode(jsondecode(trigger.substitutions["_TF_CONFIG_JSON"]).settings) == jsonencode(var.promotion_targets[env].terraform_settings) &&
      contains(trigger.included_files, "infra/terraform/modules/**") &&
      contains(trigger.included_files, "infra/${env}/**") &&
      contains(trigger.included_files, "infra/ci/cloudbuild-terraform-release.yaml")
    ])
    error_message = "Plans must use target-specific state and settings, include shared infrastructure changes, and never build/publish/promote bundles."
  }

  assert {
    condition = (
      google_project_iam_member.release_plan_logs[0].role == "roles/logging.logWriter" &&
      google_service_account.release_plan[0].account_id == "g5-lean-release-plan" &&
      google_cloudbuild_trigger.terraform_promote["staging"].disabled &&
      google_cloudbuild_trigger.terraform_promote["prod"].disabled &&
      google_cloudbuild_trigger.terraform_promote["staging"].approval_config[0].approval_required &&
      !google_cloudbuild_trigger.terraform_promote["prod"].approval_config[0].approval_required &&
      google_cloudbuild_trigger.terraform_promote["staging"].substitutions["_NEXT_TRIGGER"] == "g5-terraform-apply-prod" &&
      length(google_cloud_scheduler_job.release_train) == 0 &&
      google_cloudbuild_trigger.terraform_plan.name == "g5-terraform-plan-qa" &&
      google_cloudbuild_trigger.terraform_plan.service_account == data.google_service_account.terraform_plan.name &&
      google_cloudbuild_trigger.terraform_plan.substitutions["_TF_ACTION"] == "plan"
    )
    error_message = "Target PR plans must not activate or change release approvals, the weekly train, or the existing QA planner."
  }
}

run "no_target_checks_before_target_setup" {
  command = plan
  variables { promotion_targets = {} }
  assert {
    condition = (
      length(google_service_account.release_plan) == 1 &&
      length(google_cloudbuild_trigger.terraform_release_plan) == 0
    )
    error_message = "Create the planner for target bootstrap, but no target checks until initialized targets are configured."
  }
}

run "no_target_planner_before_authority_review" {
  command = plan
  variables {
    qa_release_authority_reviewed = false
    promotion_targets             = {}
  }
  assert {
    condition = (
      length(google_service_account.release_plan) == 0 &&
      length(google_project_iam_member.release_plan_logs) == 0 &&
      length(google_cloudbuild_trigger.terraform_release_plan) == 0
    )
    error_message = "Default QA must not create the target planner or its triggers."
  }
}

run "promotion_requires_target_canaries_before_production" {
  command = plan
  variables {
    enable_application_triggers = true
    enable_promotion            = true
  }

  assert {
    condition = toset(keys(google_cloudbuild_trigger.promote)) == toset(["staging", "prod"]) && alltrue([for env, trigger in google_cloudbuild_trigger.promote :
      !trigger.disabled &&
      trigger.name == "g5-promote-${env}" &&
      trigger.git_file_source[0].path == "infra/ci/cloudbuild-deploy.yaml" &&
      trigger.substitutions["_ENVIRONMENT"] == env &&
      trigger.substitutions["_TARGET_PROJECT"] == var.promotion_targets[env].project_id &&
      trigger.substitutions["_RELEASE_CHECK_BASE_URL"] == var.promotion_targets[env].release_check_base_url &&
      trigger.substitutions["_RELEASE_CHECK_SECRET"] == var.promotion_targets[env].release_check_secret &&
      trigger.substitutions["_RELEASE_CHECKS_REQUIRED"] == "true" &&
      trigger.substitutions["_NEXT_TRIGGER"] == (env == "staging" ? "g5-promote-prod" : "") &&
      trigger.approval_config[0].approval_required == (env == "staging")
    ])
    error_message = "Normal promotion must run target-bound canaries in both environments, with only staging approved and only staging chaining production."
  }

  assert {
    condition = (
      google_cloudbuild_trigger.deploy[0].substitutions["_NEXT_TRIGGER"] == "g5-promote-staging" &&
      length(google_cloudbuild_trigger.release_train) == 1 &&
      length(google_cloud_scheduler_job.release_train) == 1
    )
    error_message = "Full promotion must chain QA into staging and activate the existing weekly train."
  }
}

run "bootstrap_cannot_chain_unchecked_staging_to_production" {
  command = plan
  variables {
    enable_application_triggers = true
    enable_release_bootstrap    = true
    enable_promotion            = false
  }

  assert {
    condition = (
      toset(keys(google_cloudbuild_trigger.promote)) == toset(["staging", "prod"]) &&
      alltrue([for env, trigger in google_cloudbuild_trigger.promote :
        !trigger.disabled &&
        trigger.substitutions["_RELEASE_CHECKS_REQUIRED"] == "false" &&
        trigger.substitutions["_NEXT_TRIGGER"] == "" &&
        trigger.approval_config[0].approval_required == (env == "staging")
      ]) &&
      length(google_cloudbuild_trigger.release_train) == 0 &&
      length(google_cloud_scheduler_job.release_train) == 0
    )
    error_message = "Bootstrap may enable setup deployments, but cannot promote unchecked staging into production or activate the weekly train."
  }
}

run "configured_private_names_stay_in_delegated_children" {
  command = plan
  variables {
    promotion_targets = {
      for environment, target in var.promotion_targets : environment => merge(target, {
        terraform_settings = {
          external_access_enabled = false
          app_hostname            = environment == "staging" ? "next.lean.g5labs.ai" : "app.g5labs.ai"
          preview_hostname        = environment == "staging" ? "preview.next.lean.g5labs.ai" : "preview.app.g5labs.ai"
        }
      })
    }
  }
  assert {
    condition = alltrue([for environment, trigger in google_cloudbuild_trigger.promote :
      trigger.substitutions["_EXTERNAL_ACCESS_ENABLED"] == "false" &&
      trigger.substitutions["_APP_HOSTNAME"] == var.promotion_targets[environment].terraform_settings.app_hostname &&
      trigger.substitutions["_PREVIEW_HOSTNAME"] == var.promotion_targets[environment].terraform_settings.preview_hostname
    ])
    error_message = "Private targets must retain their reviewed in-child app/preview configuration in the application trigger."
  }
}

run "configured_public_names_stay_in_delegated_children" {
  command = plan
  variables {
    promotion_targets = {
      for environment, target in var.promotion_targets : environment => merge(target, {
        terraform_settings = {
          external_access_enabled = true
          app_hostname            = environment == "staging" ? "lean.g5labs.ai" : "next.app.g5labs.ai"
          preview_hostname        = environment == "staging" ? "preview.lean.g5labs.ai" : "preview.next.app.g5labs.ai"
        }
      })
    }
  }
}

run "reject_private_app_outside_child" {
  command = plan
  variables {
    promotion_targets = {
      for environment, target in var.promotion_targets : environment => merge(target, {
        terraform_settings = { app_hostname = environment == "staging" ? "evillean.g5labs.ai" : "app.g5labs.ai" }
      })
    }
  }
  expect_failures = [var.promotion_targets]
}

run "reject_private_preview_outside_child" {
  command = plan
  variables {
    promotion_targets = {
      for environment, target in var.promotion_targets : environment => merge(target, {
        terraform_settings = { preview_hostname = environment == "staging" ? "preview.lean.g5labs.ai" : "preview.qa.g5labs.ai" }
      })
    }
  }
  expect_failures = [var.promotion_targets]
}

run "reject_invalid_label_inside_child" {
  command = plan
  variables {
    promotion_targets = {
      for environment, target in var.promotion_targets : environment => merge(target, {
        terraform_settings = { app_hostname = environment == "staging" ? "bad..lean.g5labs.ai" : "app.g5labs.ai" }
      })
    }
  }
  expect_failures = [var.promotion_targets]
}

run "reject_same_app_preview_before_activation" {
  command = plan
  variables {
    promotion_targets = {
      for environment, target in var.promotion_targets : environment => merge(target, {
        terraform_settings = {
          app_hostname     = environment == "staging" ? "lean.g5labs.ai" : "app.g5labs.ai"
          preview_hostname = environment == "staging" ? "lean.g5labs.ai" : "app.g5labs.ai"
        }
      })
    }
  }
  expect_failures = [var.promotion_targets]
}

run "reject_public_ingress_without_hostnames" {
  command = plan
  variables {
    promotion_targets = {
      for environment, target in var.promotion_targets : environment => merge(target, {
        terraform_settings = { external_access_enabled = true }
      })
    }
  }
  expect_failures = [var.promotion_targets]
}

run "reject_public_reader" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      staging = merge(var.promotion_targets.staging, {
        artifact_readers = ["allUsers"]
      })
    })
  }
  expect_failures = [var.promotion_targets]
}

run "reject_authenticated_public_reader" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      staging = merge(var.promotion_targets.staging, {
        artifact_readers = ["allAuthenticatedUsers"]
      })
    })
  }
  expect_failures = [var.promotion_targets]
}

run "reject_ci_reader" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      staging = merge(var.promotion_targets.staging, {
        artifact_readers = ["serviceAccount:g5-lean-ci@g5-qa-1ccc.iam.gserviceaccount.com"]
      })
    })
  }
  expect_failures = [var.promotion_targets]
}

run "reject_wrong_environment_api" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      staging = merge(var.promotion_targets.staging, {
        artifact_readers = ["serviceAccount:g5-lean-prod-api@g5lean-prod-1ccc.iam.gserviceaccount.com"]
      })
    })
  }
  expect_failures = [var.promotion_targets]
}

run "reject_unrelated_compute_reader" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      staging = merge(var.promotion_targets.staging, {
        artifact_readers = ["serviceAccount:999999999999-compute@developer.gserviceaccount.com"]
      })
    })
  }
  expect_failures = [var.promotion_targets]
}

run "allow_observed_compute_reader" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      staging = merge(var.promotion_targets.staging, {
        artifact_readers = ["serviceAccount:237625147788-compute@developer.gserviceaccount.com"]
      })
    })
  }
}

run "reject_unapproved_target_project" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      staging = merge(var.promotion_targets.staging, {
        project_id       = "unrelated-project"
        artifact_readers = ["serviceAccount:237625147788-compute@developer.gserviceaccount.com"]
      })
    })
  }
  expect_failures = [var.promotion_targets]
}

run "reject_production_compute_account_in_staging" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      staging = merge(var.promotion_targets.staging, {
        artifact_readers = ["serviceAccount:177811684595-compute@developer.gserviceaccount.com"]
      })
    })
  }
  expect_failures = [var.promotion_targets]
}

run "allow_production_compute_account_in_production" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      prod = merge(var.promotion_targets.prod, {
        artifact_readers = ["serviceAccount:177811684595-compute@developer.gserviceaccount.com"]
      })
    })
  }
}

run "allow_dedicated_host_reader" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      staging = merge(var.promotion_targets.staging, {
        artifact_readers = ["serviceAccount:g5-lean-staging-host@g5lean-staging-1ccc.iam.gserviceaccount.com"]
      })
    })
  }
  assert {
    condition = (
      contains(keys(google_artifact_registry_repository_iam_member.release_artifacts), "g5:serviceAccount:g5-lean-staging-host@g5lean-staging-1ccc.iam.gserviceaccount.com") &&
      !contains(keys(google_project_iam_member.release_worker_images), "serviceAccount:g5-lean-staging-host@g5lean-staging-1ccc.iam.gserviceaccount.com") &&
      google_storage_bucket_iam_member.release_worker_artifacts["serviceAccount:g5-lean-staging-host@g5lean-staging-1ccc.iam.gserviceaccount.com"].condition[0].expression == "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/g5-qa-1ccc-g5-worker-images/objects/g5-worker-v')"
    )
    error_message = "Host reads workspace images but does not create GCE images/instances; only the API uses the GCE boot image."
  }
}
run "reject_wrong_environment_host" {
  command = plan
  variables {
    promotion_targets = merge(var.promotion_targets, {
      staging = merge(var.promotion_targets.staging, {
        artifact_readers = ["serviceAccount:g5-lean-prod-host@g5lean-prod-1ccc.iam.gserviceaccount.com"]
      })
    })
  }
  expect_failures = [var.promotion_targets]
}

run "committed_map_selects_zonal_storage_for_all_deployments" {
  command = plan
  variables { enable_application_triggers = true }
  assert {
    condition = (
      alltrue([for env, selection in module.filestore_selection :
        selection.configuration.provision_zonal && selection.configuration.active == "zonal"
      ]) &&
      google_cloudbuild_trigger.deploy[0].substitutions["_FILESTORE_INSTANCE"] == "g5-lean-qa-repos-zonal" &&
      google_cloudbuild_trigger.deploy[0].substitutions["_FILESTORE_PVC_NAME"] == "g5-repos-zonal-pvc" &&
      alltrue([for env, trigger in google_cloudbuild_trigger.promote :
        trigger.substitutions["_FILESTORE_INSTANCE"] == "g5-lean-${env}-repos-zonal" &&
        trigger.substitutions["_FILESTORE_PV_NAME"] == "g5-repos-zonal-pv" &&
        trigger.substitutions["_FILESTORE_PVC_NAME"] == "g5-repos-zonal-pvc" &&
        trigger.substitutions["_FILESTORE_ZONE"] == "us-central1-a" &&
        trigger.substitutions["_FILESTORE_STORAGE"] == "1Ti"
      ]) &&
      contains(local.terraform_included_files, "infra/config/**")
    )
    error_message = "The committed map must select zonal storage for every deploy trigger; selection commits must run Terraform checks."
  }
}

run "selected_environments_override_stale_target_inputs" {
  command = plan
  variables {
    enable_application_triggers = true
    promotion_targets = {
      for env, target in var.promotion_targets : env => merge(target, {
        filestore_instance = "obsolete-instance"
        filestore_zone     = "us-central1-b"
        filestore_storage  = "2Ti"
      })
    }
  }
  override_module {
    target  = module.filestore_selection["qa"]
    outputs = { configuration = { "provision_zonal" : true, "active" : "zonal", "legacy" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-qa-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" }, "zonal" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-qa-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" }, "selected" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-qa-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" } } }
  }
  override_module {
    target  = module.filestore_selection["staging"]
    outputs = { configuration = { "provision_zonal" : true, "active" : "zonal", "legacy" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-staging-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" }, "zonal" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-staging-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" }, "selected" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-staging-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" } } }
  }
  # Keep this independent-selection case explicit as the committed map evolves.
  override_module {
    target  = module.filestore_selection["prod"]
    outputs = { configuration = { "provision_zonal" : true, "active" : "legacy", "legacy" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-prod-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" }, "zonal" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-prod-repos-zonal", "pv_name" : "g5-repos-zonal-pv", "pvc_name" : "g5-repos-zonal-pvc" }, "selected" : { "zone" : "us-central1-a", "share_name" : "g5_repos", "capacity_gb" : 1024, "storage" : "1Ti", "instance_name" : "g5-lean-prod-repos", "pv_name" : "g5-repos-pv", "pvc_name" : "g5-repos-pvc" } } }
  }
  assert {
    condition = (
      google_cloudbuild_trigger.deploy[0].substitutions["_FILESTORE_INSTANCE"] == "g5-lean-qa-repos-zonal" &&
      google_cloudbuild_trigger.deploy[0].substitutions["_FILESTORE_PVC_NAME"] == "g5-repos-zonal-pvc" &&
      google_cloudbuild_trigger.promote["staging"].substitutions["_FILESTORE_INSTANCE"] == "g5-lean-staging-repos-zonal" &&
      google_cloudbuild_trigger.promote["staging"].substitutions["_FILESTORE_PV_NAME"] == "g5-repos-zonal-pv" &&
      google_cloudbuild_trigger.promote["staging"].substitutions["_FILESTORE_PVC_NAME"] == "g5-repos-zonal-pvc" &&
      google_cloudbuild_trigger.promote["staging"].substitutions["_FILESTORE_ZONE"] == "us-central1-a" &&
      google_cloudbuild_trigger.promote["staging"].substitutions["_FILESTORE_STORAGE"] == "1Ti" &&
      google_cloudbuild_trigger.promote["prod"].substitutions["_FILESTORE_INSTANCE"] == "g5-lean-prod-repos" &&
      google_cloudbuild_trigger.promote["prod"].substitutions["_FILESTORE_PVC_NAME"] == "g5-repos-pvc"
    )
    error_message = "Only the selected environments switch; stale external storage fields cannot redirect them or production."
  }
}
