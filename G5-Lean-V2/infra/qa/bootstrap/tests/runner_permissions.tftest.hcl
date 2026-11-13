mock_provider "google" {}

run "runner_secret_management_excludes_payloads_and_project_act_as" {
  command = plan

  assert {
    condition = toset(google_project_iam_custom_role.terraform_secret_management.permissions) == toset([
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
    error_message = "The runner must manage secret containers, bindings and version lifecycle without direct project-wide payload reads."
  }

  assert {
    condition = (
      google_project_iam_member.terraform_secret_management.role == "projects/g5-qa-1ccc/roles/g5LeanSecretManagement" &&
      !contains(keys(google_project_iam_member.terraform_runner), "roles/secretmanager.admin") &&
      !contains(keys(google_project_iam_member.terraform_runner), "roles/secretmanager.secretAccessor") &&
      !contains(keys(google_project_iam_member.terraform_runner), "roles/iam.serviceAccountUser") &&
      !contains(keys(google_project_iam_member.terraform_runner), "roles/container.developer") &&
      contains(keys(google_project_iam_member.terraform_runner), "roles/container.admin") &&
      contains(keys(google_project_iam_member.terraform_runner), "roles/resourcemanager.projectIamAdmin")
    )
    error_message = "Direct secret/attachment grants must narrow without claiming that the trusted project IAM administrator is sandboxed."
  }
}

run "backup_workflow_deployment_without_role_administration" {
  command = plan
  assert {
    condition = (
      contains(keys(google_project_iam_member.terraform_runner), "roles/workflows.editor") &&
      contains(keys(google_project_iam_member.terraform_runner), "roles/cloudscheduler.admin") &&
      !contains(keys(google_project_iam_member.terraform_runner), "roles/workflows.admin") &&
      !contains(keys(google_project_iam_member.terraform_runner), "roles/iam.roleAdmin") &&
      google_project_service.required["workflows.googleapis.com"].service == "workflows.googleapis.com" &&
      google_project_service.required["workflowexecutions.googleapis.com"].service == "workflowexecutions.googleapis.com" &&
      !contains(keys(google_project_service.required), "cloudscheduler.googleapis.com") &&
      module.filestore_backup_roles.runner_role == "projects/g5-qa-1ccc/roles/g5FilestoreBackupRunner"
    )
    error_message = "The runner deploys the backup workflow and job with editor/admin service roles only; bootstrap owns the custom roles and the Workflows APIs, delivery keeps the Scheduler API."
  }
}
