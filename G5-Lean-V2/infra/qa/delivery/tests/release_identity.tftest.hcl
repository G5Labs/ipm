# Exercise the real delivery root with mock cloud reads. All runs are plan-only;
# the new release account's computed email deliberately remains unknown until apply.
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

run "release_authority_disabled" {
  command = plan

  assert {
    condition = (
      length(google_service_account.release) == 0 &&
      length(google_project_iam_member.release_image_scan_reads) == 0 &&
      length(google_artifact_registry_repository_iam_member.release_artifacts) == 0 &&
      length(google_project_iam_member.release_worker_images) == 0
    )
    error_message = "QA must create no central release account or artifact grants before authority is reviewed."
  }
}

run "first_release_identity_without_targets" {
  command = plan

  variables {
    qa_release_authority_reviewed = true
    promotion_targets             = {}
  }

  override_data {
    target = data.terraform_remote_state.release_isolation[0]
    values = { outputs = { release_identity_isolation_ready = true } }
  }

  assert {
    condition = (
      toset(keys(google_service_account.release)) == toset(["central"]) &&
      google_service_account.release["central"].account_id == "g5-lean-release"
    )
    error_message = "The first activation must plan creation of the central release account."
  }

  assert {
    condition = (
      toset(keys(google_project_iam_member.release_image_scan_reads)) == toset(["central"]) &&
      google_project_iam_member.release_image_scan_reads["central"].project == "g5-qa-1ccc" &&
      google_project_iam_member.release_image_scan_reads["central"].role == "roles/containeranalysis.occurrences.viewer"
    )
    error_message = "Only the central release identity receives project-scoped read access to image scan metadata."
  }

  assert {
    condition = toset(keys(google_artifact_registry_repository_iam_member.release_artifacts)) == toset([
      "g5:serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com",
      "g5-tiers:serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com",
      "g5-worker-images:serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com",
    ])
    error_message = "All three repository grants must be plannable before account creation and retain their existing resource keys."
  }

  assert {
    condition = (
      toset(keys(google_project_iam_member.release_worker_images)) == toset([
        "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"
      ]) &&
      alltrue([for grant in google_artifact_registry_repository_iam_member.release_artifacts :
        grant.project == "g5-qa-1ccc" && grant.role == "roles/artifactregistry.reader"
      ]) &&
      alltrue([for grant in google_project_iam_member.release_worker_images :
        grant.project == "g5-qa-1ccc" && grant.role == "roles/compute.imageUser"
      ])
    )
    error_message = "First activation must preserve the existing image grant key and QA artifact-read roles."
  }

  assert {
    condition = (
      length(google_cloudbuild_trigger.promote) == 0 &&
      length(google_cloudbuild_trigger.terraform_promote) == 0 &&
      length(google_cloud_scheduler_job.release_train) == 0
    )
    error_message = "Identity bootstrap with no targets must not activate promotion or the weekly schedule."
  }
}

run "target_readers_keep_keys_and_add_central_identity" {
  command = plan

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

  assert {
    condition = (
      toset(keys(google_project_iam_member.release_worker_images)) == toset([
        "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com",
        "serviceAccount:g5-lean-staging-api@g5lean-staging-1ccc.iam.gserviceaccount.com",
        "serviceAccount:g5-lean-prod-api@g5lean-prod-1ccc.iam.gserviceaccount.com",
      ]) &&
      length(google_artifact_registry_repository_iam_member.release_artifacts) == 9
    )
    error_message = "Shared readers must retain member-based resource keys and receive one grant per repository without duplicating the central account."
  }

  assert {
    condition = (
      google_project_iam_member.release_worker_images["serviceAccount:g5-lean-staging-api@g5lean-staging-1ccc.iam.gserviceaccount.com"].member == "serviceAccount:g5-lean-staging-api@g5lean-staging-1ccc.iam.gserviceaccount.com" &&
      google_project_iam_member.release_worker_images["serviceAccount:g5-lean-prod-api@g5lean-prod-1ccc.iam.gserviceaccount.com"].member == "serviceAccount:g5-lean-prod-api@g5lean-prod-1ccc.iam.gserviceaccount.com"
    )
    error_message = "Existing target reader grants must keep the same member identities."
  }

  assert {
    condition = alltrue(flatten([
      for target in values(var.promotion_targets) : [
        for member in target.artifact_readers : [
          for repository in ["g5", "g5-tiers", "g5-worker-images"] : (
            google_artifact_registry_repository_iam_member.release_artifacts["${repository}:${member}"].member == member &&
            google_artifact_registry_repository_iam_member.release_artifacts["${repository}:${member}"].project == "g5-qa-1ccc" &&
            google_artifact_registry_repository_iam_member.release_artifacts["${repository}:${member}"].location == "us-central1" &&
            google_artifact_registry_repository_iam_member.release_artifacts["${repository}:${member}"].repository == repository &&
            google_artifact_registry_repository_iam_member.release_artifacts["${repository}:${member}"].role == "roles/artifactregistry.reader"
          )
        ]
      ]
    ]))
    error_message = "Each target reader must receive only read access to each intended QA repository, with its identity preserved."
  }

  assert {
    condition = (
      toset(keys(google_storage_bucket_iam_member.release_worker_artifacts)) == toset(flatten([
        for target in values(var.promotion_targets) : tolist(target.artifact_readers)
      ])) &&
      alltrue([for member, grant in google_storage_bucket_iam_member.release_worker_artifacts :
        grant.member == member &&
        grant.bucket == "g5-qa-1ccc-g5-worker-images" &&
        grant.role == "roles/storage.objectViewer"
      ])
    )
    error_message = "Worker artifact grants must map exactly the target readers to read-only access on the QA worker-images bucket."
  }
}
