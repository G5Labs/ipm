mock_provider "google" {}
mock_provider "google-beta" {}

run "worker_publisher_keeps_scoped_artifact_and_signing_access" {
  command = plan
  assert {
    condition = (
      google_project_iam_member.image_publisher.project == "g5-qa-1ccc" &&
      google_project_iam_member.image_publisher.role == "projects/g5-qa-1ccc/roles/g5LeanWorkerImagePublisher" &&
      google_artifact_registry_repository_iam_member.trusted_writer["images"].role == "roles/artifactregistry.writer" &&
      !contains(keys(google_storage_bucket_iam_member.artifact_writer), "images") &&
      google_storage_bucket_iam_member.images_object_lister.bucket == "g5-qa-1ccc-g5-worker-images" &&
      google_storage_bucket_iam_member.images_object_lister.role == "projects/g5-qa-1ccc/roles/g5LeanArtifactObjectLister" &&
      length(google_storage_bucket_iam_member.images_object_lister.condition) == 0 &&
      toset(keys(google_storage_bucket_iam_member.images_versions)) == toset(["roles/storage.objectCreator", "roles/storage.objectViewer"]) &&
      alltrue([for grant in google_storage_bucket_iam_member.images_versions :
        grant.condition[0].expression == "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/g5-qa-1ccc-g5-worker-images/objects/g5-worker-v')"
      ]) &&
      google_storage_bucket_iam_member.images_mutable.role == "roles/storage.objectAdmin" &&
      google_storage_bucket_iam_member.images_mutable.condition[0].expression == "resource.type == 'storage.googleapis.com/Object' && (resource.name == 'projects/_/buckets/g5-qa-1ccc-g5-worker-images/objects/latest.json' || resource.name.startsWith('projects/_/buckets/g5-qa-1ccc-g5-worker-images/objects/iso/'))" &&
      google_storage_bucket_iam_member.images_bucket_viewer.role == "roles/storage.bucketViewer" &&
      google_storage_bucket_iam_member.images_build_artifacts_writer.role == "roles/storage.objectCreator" &&
      google_storage_bucket_iam_member.images_build_artifacts_reader.role == "roles/storage.objectViewer" &&
      google_kms_crypto_key_iam_member.image_signer.role == "roles/cloudkms.signerVerifier" &&
      google_kms_crypto_key_iam_member.image_signer_viewer.role == "roles/cloudkms.viewer"
    )
    error_message = "Worker builds retain their repository, bucket and key-scoped publication access while Compute uses only the bootstrap publisher role."
  }
}

run "builder_publication_isolated_and_artifact_buckets_private" {
  command = plan
  assert {
    condition = (
      google_artifact_registry_repository.docker["g5-builders"].repository_id == "g5-builders" &&
      google_artifact_registry_repository_iam_member.images_builder.repository == "g5-builders" &&
      google_artifact_registry_repository_iam_member.images_builder.role == "roles/artifactregistry.writer"
    )
    error_message = "The worker builder must publish outside application artifacts and PR caches."
  }
  assert {
    condition     = alltrue([for bucket in google_storage_bucket.artifacts : bucket.uniform_bucket_level_access && bucket.public_access_prevention == "enforced"])
    error_message = "All QA artifact buckets must prevent public and object-ACL access."
  }
}
