# Release coordination remains in QA. Like G5's dev release line, one trusted
# identity runs application and Terraform promotions with direct target authority.
# PR CI/planner and the release-train approver remain separate identities.
locals {
  release_queue_enabled = var.enable_release_bootstrap || var.enable_promotion
  release_targets       = var.promotion_targets
  release_bucket        = "${var.artifact_project}-lean-releases"
  # Identity setup precedes target platform creation and reader discovery.
  release_identities = var.qa_release_authority_reviewed ? { central = "g5-lean-release" } : {}
  artifact_readers   = toset(flatten([for target in values(local.release_targets) : tolist(target.artifact_readers)]))
  # Keep IAM instance keys known before the release account is created. Values
  # retain the resource reference so grants wait for account creation.
  release_artifact_members = merge(
    { for member in local.artifact_readers : member => member },
    { for key, account_id in local.release_identities :
      "serviceAccount:${account_id}@${local.project_id}.iam.gserviceaccount.com" => "serviceAccount:${google_service_account.release[key].email}"
    },
  )
}

data "terraform_remote_state" "release_isolation" {
  count   = var.qa_release_authority_reviewed ? 1 : 0
  backend = "gcs"
  config = {
    bucket = "${local.project_id}-lean-tfstate"
    prefix = "platform"
  }
}

resource "google_service_account" "release" {
  for_each     = local.release_identities
  project      = local.project_id
  account_id   = each.value
  display_name = "G5 Lean central release"
  lifecycle {
    precondition {
      condition = var.qa_release_authority_reviewed && try(
        data.terraform_remote_state.release_isolation[0].outputs.release_identity_isolation_ready, false
      )
      error_message = "Apply QA bootstrap planner hardening and QA platform identity isolation first, verify effective IAM, then set qa_release_authority_reviewed=true."
    }
  }
}
resource "google_project_iam_member" "release_logs" {
  for_each = google_service_account.release
  project  = local.project_id
  role     = "roles/logging.logWriter"
  member   = "serviceAccount:${each.value.email}"
}
resource "google_project_iam_member" "release_build_reads" {
  for_each = google_service_account.release
  project  = local.project_id
  role     = "roles/cloudbuild.builds.viewer"
  member   = "serviceAccount:${each.value.email}"
}
# Artifact Analysis occurrence reads are project-scoped. The reporter cannot
# start scans, write findings or change repository settings. Keep QA reads while
# existing releases reference QA; admin grants belong to infra/admin/artifacts.
resource "google_project_iam_member" "release_image_scan_reads" {
  for_each = google_service_account.release
  project  = local.project_id
  role     = "roles/containeranalysis.occurrences.viewer"
  member   = "serviceAccount:${each.value.email}"
}
resource "google_project_iam_member" "release_pool" {
  for_each = google_service_account.release
  project  = local.project_id
  role     = "roles/cloudbuild.workerPoolUser"
  member   = "serviceAccount:${each.value.email}"
}
# The central release identity chains staging into prod but cannot approve queues.
resource "google_project_iam_member" "release_chain" {
  for_each = google_service_account.release
  project  = local.project_id
  role     = "roles/cloudbuild.builds.editor"
  member   = "serviceAccount:${each.value.email}"
}
resource "google_service_account_iam_member" "release_chain_act_as" {
  for_each           = google_service_account.release
  service_account_id = each.value.name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${each.value.email}"
}
resource "google_project_iam_member" "qa_deploy_chain" {
  # QA also starts missing worker bakes when no promotion targets are configured.
  count   = var.enable_application_triggers || length(local.release_targets) == 2 ? 1 : 0
  project = local.project_id
  role    = "roles/cloudbuild.builds.editor"
  member  = "serviceAccount:${data.google_service_account.build["deploy"].email}"
}
resource "google_service_account_iam_member" "qa_deploy_worker_image" {
  count              = var.enable_application_triggers ? 1 : 0
  service_account_id = data.google_service_account.build["images"].name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${data.google_service_account.build["deploy"].email}"
}
resource "google_service_account_iam_member" "qa_deploy_stage" {
  count              = length(local.release_targets) == 2 ? 1 : 0
  service_account_id = google_service_account.release["central"].name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${data.google_service_account.build["deploy"].email}"
}
resource "google_service_account_iam_member" "qa_terraform_stage" {
  count              = var.qa_release_authority_reviewed ? 1 : 0
  service_account_id = google_service_account.release["central"].name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${data.google_service_account.terraform.email}"
}
resource "google_project_iam_member" "release_source_reads" {
  for_each = google_service_account.release
  project  = local.project_id
  role     = "roles/cloudbuild.readTokenAccessor"
  member   = "serviceAccount:${each.value.email}"
}

# Immutable app manifests are separate from build-artifacts, which arbitrary
# PR code can write. Terraform bundles use G5's existing OCI registry pattern.
resource "google_storage_bucket" "releases" {
  count   = var.qa_release_authority_reviewed ? 1 : 0
  project = local.project_id
  # Preserve this QA resource/state address through the admin transition.
  # infra/admin owns the separate admin bucket and all its bindings.
  name                        = "${local.project_id}-lean-releases"
  location                    = local.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  force_destroy               = false
  versioning { enabled = true }
  lifecycle { prevent_destroy = true }
}
resource "google_storage_bucket_iam_member" "release_writers" {
  for_each = var.qa_release_authority_reviewed ? {
    deploy = data.google_service_account.build["deploy"].email
  } : {}
  bucket = google_storage_bucket.releases[0].name
  role   = "roles/storage.objectCreator"
  member = "serviceAccount:${each.value}"
}
resource "google_storage_bucket_iam_member" "release_readers" {
  for_each = merge(
    { for key, sa in google_service_account.release : key => sa.email },
    var.qa_release_authority_reviewed ? {
      deploy = data.google_service_account.build["deploy"].email
    } : {},
  )
  bucket = google_storage_bucket.releases[0].name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${each.value}"
}

# Retain QA reads while existing consumers still reference QA artifacts.
# Admin read/write bindings belong to infra/admin, never this QA apply root.
# Input members come from the target platform outputs, not guessed project numbers.
resource "google_artifact_registry_repository_iam_member" "release_artifacts" {
  for_each = {
    for pair in setproduct(toset(["g5", "g5-tiers", "g5-worker-images"]), keys(local.release_artifact_members)) : "${pair[0]}:${pair[1]}" => pair
  }
  project    = local.project_id
  location   = local.region
  repository = each.value[0]
  role       = "roles/artifactregistry.reader"
  member     = local.release_artifact_members[each.value[1]]
}
resource "google_project_iam_member" "release_worker_images" {
  for_each = { for key, member in local.release_artifact_members : key => member if contains(concat(
    ["serviceAccount:g5-lean-release@${local.project_id}.iam.gserviceaccount.com"],
    [for environment, project in local.release_target_projects :
      "serviceAccount:g5-lean-${environment}-api@${project.id}.iam.gserviceaccount.com"
    ],
  ), key) }
  project = local.project_id
  role    = "roles/compute.imageUser"
  member  = each.value
}
resource "google_storage_bucket_iam_member" "release_worker_artifacts" {
  for_each = local.artifact_readers
  bucket   = "${local.project_id}-g5-worker-images"
  role     = "roles/storage.objectViewer"
  member   = each.value
  # New host consumers need immutable versions, not builder ISO/pointer state.
  dynamic "condition" {
    for_each = strcontains(each.key, "-host@") ? [true] : []
    content {
      title      = "worker_version_objects_only"
      expression = "resource.type == 'storage.googleapis.com/Object' && resource.name.startsWith('projects/_/buckets/${local.project_id}-g5-worker-images/objects/g5-worker-v')"
    }
  }
}

resource "google_cloudbuild_trigger" "promote" {
  for_each    = local.release_targets
  project     = local.project_id
  location    = local.region
  name        = "g5-promote-${each.key}"
  description = "Deploy the validated QA artifacts to ${each.key}; staging approval is the production release decision"
  disabled    = !local.release_queue_enabled
  source_to_build {
    repository = local.repository
    ref        = "refs/heads/main"
    repo_type  = "GITHUB"
  }
  git_file_source {
    path       = "infra/ci/cloudbuild-deploy.yaml"
    repository = local.repository
    repo_type  = "GITHUB"
  }
  service_account = google_service_account.release["central"].name
  approval_config { approval_required = each.key == "staging" }
  substitutions = {
    _AR_REGION                    = local.region
    _ARTIFACT_PROJECT             = var.artifact_project
    _AR_PROJECT                   = var.artifact_project
    _AR_REPOSITORY                = "g5"
    _TARGET_PROJECT               = each.value.project_id
    _GKE_CLUSTER                  = each.value.cluster
    _GKE_ZONE                     = each.value.region
    _HELM_RELEASE                 = "g5"
    _HELM_NAMESPACE               = "g5"
    _ENVIRONMENT                  = each.key
    _EXTERNAL_ACCESS_ENABLED      = tostring(try(each.value.terraform_settings.external_access_enabled, false))
    _APP_HOSTNAME                 = try(each.value.terraform_settings.app_hostname, "")
    _PREVIEW_HOSTNAME             = try(each.value.terraform_settings.preview_hostname, "")
    _INGRESS_STATIC_IP_NAME       = "g5-lean-${each.key}-ip"
    _CERTIFICATE_MAP_NAME         = "g5-lean-${each.key}-ip-cert-map"
    _PROMOTION_MODE               = "deploy"
    _FILESTORE_INSTANCE           = module.filestore_selection[each.key].configuration.selected.instance_name
    _FILESTORE_ZONE               = module.filestore_selection[each.key].configuration.selected.zone
    _FILESTORE_PV_NAME            = module.filestore_selection[each.key].configuration.selected.pv_name
    _FILESTORE_PVC_NAME           = module.filestore_selection[each.key].configuration.selected.pvc_name
    _FILESTORE_STORAGE            = module.filestore_selection[each.key].configuration.selected.storage
    _WORKER_IMAGE_PROJECT         = var.artifact_project
    _WORKER_IMAGE_FAMILY          = "g5-worker"
    _WORKER_IMAGE_TRIGGER         = "g5-packer-worker-main-qa"
    _STAGING_PROJECT              = local.release_targets["staging"].project_id
    _PROD_PROJECT                 = local.release_targets["prod"].project_id
    _STAGING_CLUSTER              = local.release_targets["staging"].cluster
    _PROD_CLUSTER                 = local.release_targets["prod"].cluster
    _NEXT_TRIGGER                 = each.key == "staging" && var.enable_promotion ? "g5-promote-prod" : ""
    _RELEASE_BUCKET               = local.release_bucket
    _RELEASE_CHECK_BASE_URL       = each.value.release_check_base_url
    _RELEASE_CHECK_SECRET         = each.value.release_check_secret
    _RELEASE_CHECKS_REQUIRED      = var.enable_promotion ? "true" : "false"
    _TIER_REPOSITORY              = "g5-tiers"
    _GITHUB_OWNER                 = "G5Labs"
    _GITHUB_REPO                  = "G5-Lean"
    _GITHUB_CONNECTION            = local.connection_name
    _GITHUB_CONNECTION_REPOSITORY = local.repository_name
  }
  depends_on = [google_service_account_iam_member.qa_terraform_stage]
}

resource "google_cloudbuild_trigger" "terraform_promote" {
  for_each    = local.release_targets
  project     = local.project_id
  location    = local.region
  name        = "g5-terraform-apply-${each.key}"
  description = "Apply G5's promoted OCI Terraform bundle to the ${each.key} roots"
  disabled    = !local.release_queue_enabled
  source_to_build {
    repository = local.repository
    ref        = "refs/heads/main"
    repo_type  = "GITHUB"
  }
  git_file_source {
    path       = "infra/ci/cloudbuild-terraform-release.yaml"
    repository = local.repository
    revision   = "refs/heads/main"
    repo_type  = "GITHUB"
  }
  service_account = google_service_account.release["central"].name
  approval_config { approval_required = each.key == "staging" }
  substitutions = {
    _TF_ACTION            = "apply"
    _TF_WORKSPACE         = each.key
    _TF_BUNDLE_MODE       = "use"
    _TF_BUNDLE_REF        = ""
    _AR_PROJECT           = var.artifact_project
    _AR_REPOSITORY        = "g5"
    _REGION               = each.value.region
    _STATE_BUCKET         = "${each.value.project_id}-lean-tfstate"
    _INFRA_DIR            = "infra"
    _NEXT_TRIGGER         = each.key == "staging" ? "g5-terraform-apply-prod" : ""
    _NEXT_TRIGGER_PROJECT = each.key == "staging" ? local.project_id : ""
    _TF_CONFIG_JSON       = jsonencode({ project_id = each.value.project_id, region = each.value.region, settings = each.value.terraform_settings })
  }
  depends_on = [google_service_account_iam_member.qa_terraform_stage]
}

output "release_identities" { value = { for key, sa in google_service_account.release : key => sa.email } }
output "release_bucket" { value = var.qa_release_authority_reviewed ? local.release_bucket : null }
output "artifact_project" { value = var.artifact_project }
output "promotion_triggers" { value = { for env, trigger in google_cloudbuild_trigger.promote : env => trigger.trigger_id } }
