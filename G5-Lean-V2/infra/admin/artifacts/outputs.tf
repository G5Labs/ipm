output "artifact_project" { value = var.project_id }
output "artifact_region" { value = var.region }
output "repositories" {
  value = { for name, repository in google_artifact_registry_repository.docker : name => "${var.region}-docker.pkg.dev/${var.project_id}/${name}" }
}
output "release_bucket" { value = google_storage_bucket.artifacts["lean-releases"].name }
output "worker_images_bucket" { value = google_storage_bucket.artifacts["g5-worker-images"].name }
output "worker_image_project" { value = var.project_id }
