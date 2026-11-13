mock_provider "google" {}

variables {
  project_id  = "g5lean-staging-1ccc"
  environment = "staging"
  region      = "us-central1"
}

run "staging_exact_authority" {
  command = plan

  assert {
    condition = (
      google_storage_bucket_iam_binding.state_legacy_object_readers.bucket == "g5lean-staging-1ccc-lean-tfstate" &&
      google_storage_bucket_iam_binding.state_legacy_object_readers.role == "roles/storage.legacyObjectReader" &&
      google_storage_bucket_iam_binding.state_legacy_object_readers.members == toset(["projectOwner:${var.project_id}"]) &&
      length(google_storage_bucket_iam_binding.state_legacy_object_readers.condition) == 0
    )
    error_message = "Staging state must remove Project Viewer legacy object reads while keeping the explicit planner grants below."
  }

  assert {
    condition = toset(keys(google_project_iam_member.release)) == toset([
      "roles/certificatemanager.editor",
      "roles/cloudkms.admin",
      "roles/cloudscheduler.admin",
      "roles/cloudsql.admin",
      "roles/compute.admin",
      "roles/container.admin",
      "roles/dns.admin",
      "roles/file.editor",
      "roles/iam.serviceAccountAdmin",
      "roles/iam.serviceAccountUser",
      "roles/logging.admin",
      "roles/monitoring.admin",
      "roles/pubsub.admin",
      "roles/resourcemanager.projectIamAdmin",
      "roles/servicenetworking.networksAdmin",
      "roles/serviceusage.serviceUsageAdmin",
      "roles/storage.admin",
      "roles/workflows.editor",
    ])
    error_message = "Only the approved service administration and child-DNS roles may be granted; no project secretAccessor or admin-artifact permissions."
  }

  assert {
    condition = toset(google_project_iam_custom_role.secret_management.permissions) == toset([
      "resourcemanager.projects.get",
      "secretmanager.locations.get", "secretmanager.locations.list",
      "secretmanager.secrets.create", "secretmanager.secrets.delete",
      "secretmanager.secrets.get", "secretmanager.secrets.getIamPolicy",
      "secretmanager.secrets.list", "secretmanager.secrets.setIamPolicy",
      "secretmanager.secrets.update", "secretmanager.versions.add",
      "secretmanager.versions.destroy", "secretmanager.versions.disable",
      "secretmanager.versions.enable", "secretmanager.versions.get",
      "secretmanager.versions.list",
    ])
    error_message = "Secret management must use only the required metadata/write permissions, never payload access or wildcard permissions."
  }

  assert {
    condition = (
      google_project_iam_custom_role.secret_management.project == "g5lean-staging-1ccc" &&
      google_project_iam_member.release_secret_management.project == "g5lean-staging-1ccc" &&
      google_project_iam_member.release_secret_management.role == "projects/g5lean-staging-1ccc/roles/g5LeanSecretManagement" &&
      google_project_iam_member.release_secret_management.member == "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"
    )
    error_message = "The custom secret management role and its grant must be target-local and fixed to the central release identity."
  }

  assert {
    condition = alltrue([
      for role, grant in google_project_iam_member.release :
      grant.project == "g5lean-staging-1ccc" && grant.role == role &&
      grant.member == "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"
    ])
    error_message = "Every staging grant must use the exact approved target and central identity."
  }

  assert {
    condition = (google_storage_bucket.state.project == "g5lean-staging-1ccc" &&
      google_storage_bucket.state.name == "g5lean-staging-1ccc-lean-tfstate" &&
      google_storage_bucket.state.location == "us-central1" &&
      google_storage_bucket.state.uniform_bucket_level_access &&
      google_storage_bucket.state.public_access_prevention == "enforced" &&
      google_storage_bucket.state.versioning[0].enabled &&
    !google_storage_bucket.state.force_destroy)
    error_message = "State must remain protected and local to staging."
  }

  assert {
    condition = (
      google_project_service.required["dns.googleapis.com"].project == "g5lean-staging-1ccc" &&
      google_project_service.required["dns.googleapis.com"].service == "dns.googleapis.com" &&
      !google_project_service.required["dns.googleapis.com"].disable_on_destroy
    )
    error_message = "Staging must enable its own DNS API and retain the service if its Terraform declaration is removed."
  }

  assert {
    condition = alltrue([for api in ["cloudscheduler.googleapis.com", "workflows.googleapis.com", "workflowexecutions.googleapis.com"] :
      google_project_service.required[api].project == "g5lean-staging-1ccc" && !google_project_service.required[api].disable_on_destroy
    ]) && module.filestore_backup_roles.runner_role == "projects/g5lean-staging-1ccc/roles/g5FilestoreBackupRunner"
    error_message = "Bootstrap owns the Scheduler/Workflows APIs and the backup custom roles the platform root binds; no other state may manage them."
  }

  assert {
    condition = (
      toset(keys(google_project_iam_member.planner)) == toset([
        "roles/viewer", "roles/iam.securityReviewer", "roles/secretmanager.viewer",
        ]) && alltrue([for grant in google_project_iam_member.planner :
        grant.project == "g5lean-staging-1ccc" &&
        grant.member == "serviceAccount:g5-lean-release-plan@g5-qa-1ccc.iam.gserviceaccount.com"
      ])
    )
    error_message = "Staging PR plans must retain only the dedicated planner's three read roles, including existing DNS reads; no DNS write grant."
  }

  assert {
    condition = (
      google_storage_bucket_iam_member.planner_state_read.bucket == "g5lean-staging-1ccc-lean-tfstate" &&
      google_storage_bucket_iam_member.planner_state_read.role == "roles/storage.objectViewer" &&
      google_storage_bucket_iam_member.planner_state_read.member == "serviceAccount:g5-lean-release-plan@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_storage_bucket_iam_member.planner_state_locks.bucket == "g5lean-staging-1ccc-lean-tfstate" &&
      google_storage_bucket_iam_member.planner_state_locks.role == "roles/storage.objectAdmin" &&
      google_storage_bucket_iam_member.planner_state_locks.member == "serviceAccount:g5-lean-release-plan@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_storage_bucket_iam_member.planner_state_locks.condition[0].expression == "resource.type == 'storage.googleapis.com/Object' && resource.name in ['projects/_/buckets/g5lean-staging-1ccc-lean-tfstate/objects/platform/default.tflock', 'projects/_/buckets/g5lean-staging-1ccc-lean-tfstate/objects/addons/default.tflock']"
    )
    error_message = "Staging plans must read state but write only the two exact root locks, never state or other workspace objects."
  }
}

run "production_exact_target" {
  command = plan
  assert {
    condition = (
      google_storage_bucket_iam_binding.state_legacy_object_readers.bucket == "g5lean-prod-1ccc-lean-tfstate" &&
      google_storage_bucket_iam_binding.state_legacy_object_readers.role == "roles/storage.legacyObjectReader" &&
      google_storage_bucket_iam_binding.state_legacy_object_readers.members == toset(["projectOwner:${var.project_id}"]) &&
      length(google_storage_bucket_iam_binding.state_legacy_object_readers.condition) == 0
    )
    error_message = "Production state must remove Project Viewer legacy object reads while keeping the explicit planner grants below."
  }
  variables {
    project_id  = "g5lean-prod-1ccc"
    environment = "prod"
  }
  assert {
    condition = length(google_project_iam_member.release) == 18 && alltrue([
      for grant in google_project_iam_member.release :
      grant.project == "g5lean-prod-1ccc" &&
      grant.member == "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"
    ])
    error_message = "Every production grant must remain on the approved target and principal."
  }
  assert {
    condition = (
      google_project_service.required["dns.googleapis.com"].project == "g5lean-prod-1ccc" &&
      google_project_service.required["dns.googleapis.com"].service == "dns.googleapis.com" &&
      !google_project_service.required["dns.googleapis.com"].disable_on_destroy &&
      google_project_iam_member.release["roles/dns.admin"].role == "roles/dns.admin" &&
      google_project_iam_member.release["roles/dns.admin"].member == "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"
    )
    error_message = "Production must retain its own DNS API and grant child-DNS management only to the existing central release account."
  }
  assert {
    condition = (
      google_project_iam_custom_role.secret_management.project == "g5lean-prod-1ccc" &&
      google_project_iam_member.release_secret_management.project == "g5lean-prod-1ccc" &&
      google_project_iam_member.release_secret_management.role == "projects/g5lean-prod-1ccc/roles/g5LeanSecretManagement" &&
      google_project_iam_member.release_secret_management.member == "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"
    )
    error_message = "Production secret management must not cross projects or change the grant recipient."
  }
  assert {
    condition     = output.state_bucket == "g5lean-prod-1ccc-lean-tfstate"
    error_message = "Production must have independent state."
  }

  assert {
    condition = (
      toset(keys(google_project_iam_member.planner)) == toset([
        "roles/viewer", "roles/iam.securityReviewer", "roles/secretmanager.viewer",
        ]) && alltrue([for grant in google_project_iam_member.planner :
        grant.project == "g5lean-prod-1ccc" &&
        grant.member == "serviceAccount:g5-lean-release-plan@g5-qa-1ccc.iam.gserviceaccount.com"
      ])
    )
    error_message = "Production PR plans must retain only the dedicated planner's three read roles, including existing DNS reads; no DNS write grant."
  }

  assert {
    condition = (
      google_storage_bucket_iam_member.planner_state_read.bucket == "g5lean-prod-1ccc-lean-tfstate" &&
      google_storage_bucket_iam_member.planner_state_read.role == "roles/storage.objectViewer" &&
      google_storage_bucket_iam_member.planner_state_read.member == "serviceAccount:g5-lean-release-plan@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_storage_bucket_iam_member.planner_state_locks.bucket == "g5lean-prod-1ccc-lean-tfstate" &&
      google_storage_bucket_iam_member.planner_state_locks.role == "roles/storage.objectAdmin" &&
      google_storage_bucket_iam_member.planner_state_locks.member == "serviceAccount:g5-lean-release-plan@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_storage_bucket_iam_member.planner_state_locks.condition[0].expression == "resource.type == 'storage.googleapis.com/Object' && resource.name in ['projects/_/buckets/g5lean-prod-1ccc-lean-tfstate/objects/platform/default.tflock', 'projects/_/buckets/g5lean-prod-1ccc-lean-tfstate/objects/addons/default.tflock']"
    )
    error_message = "Production plans must read state but write only the two exact root locks, never state or other workspace objects."
  }
}

run "reject_crossed_environment" {
  command = plan
  variables { project_id = "g5lean-prod-1ccc" }
  expect_failures = [var.project_id]
}

run "reject_admin_project" {
  command = plan
  variables { project_id = "g5lean-admin-1ccc" }
  expect_failures = [var.project_id]
}

run "reject_qa_project" {
  command = plan
  variables { project_id = "g5-qa-1ccc" }
  expect_failures = [var.project_id]
}

run "reject_original_production" {
  command = plan
  variables { project_id = "g5-prod-1ccc" }
  expect_failures = [var.project_id]
}

run "reject_unknown_environment" {
  command = plan
  variables { environment = "dev" }
  expect_failures = [var.environment]
}

run "reject_other_region" {
  command = plan
  variables { region = "us-east1" }
  expect_failures = [var.region]
}
