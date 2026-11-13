# Plan only: the operator creates this role before automated foundation apply.
mock_provider "google" {}

run "worker_role_only_publishes_images" {
  command = plan
  assert {
    condition = (
      google_project_iam_custom_role.worker_image_publisher.project == "g5-qa-1ccc" &&
      google_project_iam_custom_role.worker_image_publisher.role_id == "g5LeanWorkerImagePublisher" &&
      toset(google_project_iam_custom_role.worker_image_publisher.permissions) == toset([
        "compute.images.create", "compute.images.get", "compute.images.list",
        "compute.images.setLabels", "compute.globalOperations.get",
      ])
    )
    error_message = "QEMU publication needs only image creation, inspection, labels and operation polling; no VM, disk, service-account, deletion or IAM authority."
  }
  assert {
    condition     = !contains(keys(google_project_iam_member.terraform_runner), "roles/iam.roleAdmin")
    error_message = "The operator-owned publisher role must not add custom-role administration to automated Terraform."
  }
}

run "artifact_destination_discovery_role_cannot_read_or_mutate_payloads" {
  command = plan
  assert {
    condition = (
      google_project_iam_custom_role.artifact_object_lister.project == "g5-qa-1ccc" &&
      google_project_iam_custom_role.artifact_object_lister.role_id == "g5LeanArtifactObjectLister" &&
      toset(google_project_iam_custom_role.artifact_object_lister.permissions) == toset(["storage.objects.list"])
    )
    error_message = "Upload destination discovery needs only bucket-level listing, never payload reads/writes, deletion or bucket administration."
  }
}
