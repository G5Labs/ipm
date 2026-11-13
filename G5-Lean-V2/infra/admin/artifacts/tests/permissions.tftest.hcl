mock_provider "google" {
  mock_data "google_project" {
    defaults = { number = "123456789" }
  }
}

run "fixed_artifact_resources_and_publishers" {
  command = plan
  assert {
    condition = (
      toset(keys(google_artifact_registry_repository.docker)) == toset(["g5", "g5-tiers", "g5-worker-images"]) &&
      toset(keys(google_storage_bucket.artifacts)) == toset(["lean-releases", "g5-worker-images"]) &&
      alltrue([for repository in google_artifact_registry_repository.docker : repository.project == "g5lean-admin-1ccc" && repository.location == "us-central1" && length(repository.cleanup_policies) == 0]) &&
      alltrue([for bucket in google_storage_bucket.artifacts : bucket.project == "g5lean-admin-1ccc" && bucket.versioning[0].enabled && bucket.uniform_bucket_level_access && bucket.public_access_prevention == "enforced" && !bucket.force_destroy && length(bucket.lifecycle_rule) == 0])
    )
    error_message = "Only protected shared artifact resources belong here; no admin state, QA caches or cleanup rules."
  }
  assert {
    condition = (
      google_artifact_registry_repository_iam_member.publisher["g5"].member == "serviceAccount:g5-lean-deploy@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_artifact_registry_repository_iam_member.publisher["g5-tiers"].member == "serviceAccount:g5-lean-deploy@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_artifact_registry_repository_iam_member.publisher["g5-worker-images"].member == "serviceAccount:g5-lean-images@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_artifact_registry_repository_iam_member.terraform_publisher.member == "serviceAccount:g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_artifact_registry_repository_iam_member.terraform_publisher.repository == "g5" &&
      google_artifact_registry_repository_iam_member.terraform_publisher.role == "roles/artifactregistry.writer" &&
      alltrue([for grant in google_artifact_registry_repository_iam_member.publisher : grant.role == "roles/artifactregistry.writer"])
    )
    error_message = "Only the fixed trusted QA producer may publish each repository."
  }
  assert {
    condition = (
      length(google_storage_bucket_iam_member.publisher) == 4 &&
      google_storage_bucket_iam_member.publisher["app:roles/storage.objectCreator"].member == "serviceAccount:g5-lean-deploy@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_storage_bucket_iam_member.publisher["worker:roles/storage.objectCreator"].member == "serviceAccount:g5-lean-images@g5-qa-1ccc.iam.gserviceaccount.com" &&
      alltrue([for grant in google_storage_bucket_iam_member.publisher : contains(["roles/storage.objectCreator", "roles/storage.objectViewer"], grant.role)]) &&
      google_storage_bucket_iam_member.publisher["app:roles/storage.objectCreator"].condition[0].expression == "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/g5lean-admin-1ccc-lean-releases/objects/app-releases/')" &&
      google_storage_bucket_iam_member.publisher["worker:roles/storage.objectCreator"].condition[0].expression == "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/g5lean-admin-1ccc-g5-worker-images/objects/g5-worker-v')" &&
      alltrue([for producer in ["app", "worker"] : google_storage_bucket_iam_member.publisher["${producer}:roles/storage.objectViewer"].condition[0].expression == google_storage_bucket_iam_member.publisher["${producer}:roles/storage.objectCreator"].condition[0].expression])
    )
    error_message = "Each producer must have create-only and exact-prefix reads, without overwrite/delete, mutable pointers or another producer's records."
  }
}

run "publishers_can_discover_destinations_without_broader_payload_access" {
  command = plan
  assert {
    condition = (
      google_project_iam_custom_role.artifact_object_lister.project == "g5lean-admin-1ccc" &&
      google_project_iam_custom_role.artifact_object_lister.role_id == "g5LeanArtifactObjectLister" &&
      toset(google_project_iam_custom_role.artifact_object_lister.permissions) == toset(["storage.objects.list"]) &&
      toset(keys(google_storage_bucket_iam_member.publisher_object_lister)) == toset(["app", "worker"]) &&
      google_storage_bucket_iam_member.publisher_object_lister["app"].bucket == "g5lean-admin-1ccc-lean-releases" &&
      google_storage_bucket_iam_member.publisher_object_lister["app"].member == "serviceAccount:g5-lean-deploy@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_storage_bucket_iam_member.publisher_object_lister["worker"].bucket == "g5lean-admin-1ccc-g5-worker-images" &&
      google_storage_bucket_iam_member.publisher_object_lister["worker"].member == "serviceAccount:g5-lean-images@g5-qa-1ccc.iam.gserviceaccount.com" &&
      alltrue([for grant in google_storage_bucket_iam_member.publisher_object_lister :
        grant.role == "projects/g5lean-admin-1ccc/roles/g5LeanArtifactObjectLister" && length(grant.condition) == 0
      ]) &&
      alltrue([for grant in google_storage_bucket_iam_member.publisher : length(grant.condition) == 1])
    )
    error_message = "Only each fixed publisher's artifact bucket may grant destination listing; payload permissions must remain prefix-scoped."
  }
}

run "central_release_is_only_a_consumer" {
  command = plan
  assert {
    condition = (
      google_project_iam_member.release_image_scan_reads.project == "g5lean-admin-1ccc" &&
      google_project_iam_member.release_image_scan_reads.role == "roles/containeranalysis.occurrences.viewer" &&
      google_project_iam_member.release_image_scan_reads.member == "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com" &&
      length(google_artifact_registry_repository_iam_member.reader) == 3 &&
      alltrue([for grant in google_artifact_registry_repository_iam_member.reader : grant.role == "roles/artifactregistry.reader" && grant.member == "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"]) &&
      length(google_storage_bucket_iam_member.release_reader) == 2 &&
      alltrue([for grant in google_storage_bucket_iam_member.release_reader : grant.role == "roles/storage.objectViewer" && grant.member == "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com" && grant.bucket != "g5lean-admin-1ccc-lean-tfstate"]) &&
      alltrue([for grant in google_storage_bucket_iam_member.publisher : grant.member != "serviceAccount:g5-lean-release@g5-qa-1ccc.iam.gserviceaccount.com"]) &&
      alltrue([for grant in google_project_iam_member.image_reader : grant.role == "roles/compute.imageUser"]) &&
      length(google_storage_bucket_iam_member.runtime_worker_reader) == 0
    )
    error_message = "Central release may read artifacts/images only; no admin state or publishing grants, and runtime readers default empty."
  }
}

run "worker_publication_has_no_vm_or_administration_authority" {
  command = plan
  assert {
    condition = (
      toset(google_project_iam_custom_role.worker_image_publisher.permissions) == toset([
        "compute.images.create", "compute.images.get", "compute.images.list",
        "compute.images.setLabels", "compute.globalOperations.get",
      ]) &&
      google_project_iam_member.worker_image_publisher.project == "g5lean-admin-1ccc" &&
      google_project_iam_member.worker_image_publisher.member == "serviceAccount:g5-lean-images@g5-qa-1ccc.iam.gserviceaccount.com" &&
      google_project_iam_member.worker_image_publisher.role == "projects/g5lean-admin-1ccc/roles/g5LeanWorkerImagePublisher" &&
      google_storage_bucket_iam_member.worker_bucket_metadata.bucket == "g5lean-admin-1ccc-g5-worker-images" &&
      google_storage_bucket_iam_member.worker_bucket_metadata.role == "roles/storage.bucketViewer"
    )
    error_message = "Worker publishing needs only image creation/inspection/labels and operation polling, not VM, disk, service-account or IAM administration."
  }
}

run "explicit_runtime_members_get_only_selected_reads" {
  command = plan
  variables {
    runtime_registry_readers        = ["serviceAccount:123456789-compute@developer.gserviceaccount.com"]
    runtime_image_users             = ["serviceAccount:g5-lean-staging-api@g5lean-staging-1ccc.iam.gserviceaccount.com"]
    runtime_worker_artifact_readers = ["serviceAccount:g5-lean-prod-api@g5lean-prod-1ccc.iam.gserviceaccount.com"]
  }
  assert {
    condition = (
      length(google_artifact_registry_repository_iam_member.reader) == 6 &&
      length(google_project_iam_member.image_reader) == 3 &&
      length(google_storage_bucket_iam_member.runtime_worker_reader) == 1 &&
      google_storage_bucket_iam_member.runtime_worker_reader["serviceAccount:g5-lean-prod-api@g5lean-prod-1ccc.iam.gserviceaccount.com"].role == "roles/storage.objectViewer" &&
      length(google_artifact_registry_repository_iam_member.publisher) == 3 &&
      length(google_storage_bucket_iam_member.publisher) == 4
    )
    error_message = "Runtime selection may add only its requested read/image-use grants, never writers."
  }
}

run "reject_public_readers" {
  command = plan
  variables { runtime_registry_readers = ["allUsers"] }
  expect_failures = [var.runtime_registry_readers]
}

run "reject_pr_ci_as_runtime" {
  command = plan
  variables { runtime_image_users = ["serviceAccount:g5-lean-ci@g5-qa-1ccc.iam.gserviceaccount.com"] }
  expect_failures = [var.runtime_image_users]
}

run "reject_unrelated_compute_identity" {
  command = plan
  variables { runtime_worker_artifact_readers = ["serviceAccount:999999999-compute@developer.gserviceaccount.com"] }
  expect_failures = [var.runtime_worker_artifact_readers]
}

run "reject_wrong_project" {
  command = plan
  variables { project_id = "g5-qa-1ccc" }
  expect_failures = [var.project_id]
}

run "reject_wrong_region" {
  command = plan
  variables { region = "us-east1" }
  expect_failures = [var.region]
}

run "dedicated_host_registry_reader" {
  command = plan
  variables { runtime_registry_readers = ["serviceAccount:g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com"] }
  assert {
    condition = (
      length(google_artifact_registry_repository_iam_member.reader) == 6 &&
      google_artifact_registry_repository_iam_member.reader["g5:serviceAccount:g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com"].role == "roles/artifactregistry.reader"
    )
    error_message = "Dedicated hosts need workspace image pulls, alongside preserved node readers."
  }
}
run "reject_host_as_gce_image_user" {
  command = plan
  variables { runtime_image_users = ["serviceAccount:g5-lean-staging-host@g5lean-staging-1ccc.iam.gserviceaccount.com"] }
  expect_failures = [var.runtime_image_users]
}
