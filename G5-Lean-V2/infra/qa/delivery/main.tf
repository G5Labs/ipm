terraform {
  required_version = ">= 1.9, < 2.0"
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 5.0"
    }
  }
  backend "gcs" {
    bucket = "g5-qa-1ccc-lean-tfstate"
    prefix = "delivery"
  }
}

locals {
  project_id      = "g5-qa-1ccc"
  region          = "us-central1"
  connection_name = "G5labs-G5-QA"
  repository_name = "G5Labs-G5-Lean"
  repository      = "projects/${local.project_id}/locations/${local.region}/connections/${local.connection_name}/repositories/${local.repository_name}"
}

module "filestore_selection" {
  for_each    = toset(["qa", "staging", "prod"])
  source      = "../../terraform/modules/filestore-selection"
  environment = each.key
}

provider "google" {
  project = local.project_id
  region  = local.region
}

data "terraform_remote_state" "foundation" {
  backend = "gcs"
  config = {
    bucket = "g5-qa-1ccc-lean-tfstate"
    prefix = "foundation"
  }
}
data "google_service_account" "build" {
  for_each   = toset(["ci", "deploy", "images"])
  project    = local.project_id
  account_id = "g5-lean-${each.value}"
}
data "google_service_account" "terraform" {
  project    = local.project_id
  account_id = "g5-lean-terraform"
}
data "google_service_account" "terraform_plan" {
  project    = local.project_id
  account_id = "g5-lean-terraform-plan"
}

resource "google_cloudbuild_trigger" "ci" {
  for_each = var.enable_application_triggers ? {
    rust = "infra/ci/cloudbuild-ci-rust.yaml"
    e2e  = "infra/ci/cloudbuild-ci-e2e.yaml"
  } : {}
  project     = local.project_id
  location    = local.region
  name        = "g5-ci-${each.key}"
  description = "G5 Lean QA ${each.key} pull-request checks"
  repository_event_config {
    repository = local.repository
    pull_request {
      branch          = "^.*$"
      comment_control = "COMMENTS_ENABLED_FOR_EXTERNAL_CONTRIBUTORS_ONLY"
    }
  }
  filename        = each.value
  service_account = data.google_service_account.build["ci"].name
  substitutions = {
    _POOL                     = data.terraform_remote_state.foundation.outputs.ci_pool
    _TIER_REPOSITORY          = "g5-tiers"
    _G5_CI_SCCACHE_GCS_BUCKET = "g5-qa-1ccc-build-artifacts"
    _GITHUB_OWNER             = "G5Labs"
    _GITHUB_REPO              = "G5-Lean"
    _GITHUB_CONNECTION        = local.connection_name
  }
  include_build_logs = "INCLUDE_BUILD_LOGS_WITH_STATUS"
  depends_on         = [google_service_account_iam_member.terraform_attach]
}

# The deployment freshness checker (scripts/check-main-freshness.sh and
# scripts/check_deploy_freshness.py) filters on this exact trigger name as the
# build-stream identity; rename both together. Project scoping keeps it isolated.
#
# Like the original G5 delivery module, the deploy and worker-image triggers
# are never disabled from Terraform: every push to main deploys. A temporary
# pause (docs/qa-database-reset.md) is done on the trigger itself with gcloud,
# and the next apply of this root re-enables it.
resource "google_cloudbuild_trigger" "deploy" {
  count       = var.enable_application_triggers ? 1 : 0
  project     = local.project_id
  location    = local.region
  name        = "g5-deploy-qa"
  description = "Build and deploy G5 Lean to QA on pushes to main"
  repository_event_config {
    repository = local.repository
    push { branch = "^main$" }
  }
  filename        = "infra/ci/cloudbuild-deploy.yaml"
  service_account = data.google_service_account.build["deploy"].name
  substitutions = {
    _AR_REGION            = local.region
    _ARTIFACT_PROJECT     = var.artifact_project
    _AR_PROJECT           = var.artifact_project
    _AR_REPOSITORY        = "g5"
    _TARGET_PROJECT       = local.project_id
    _GKE_CLUSTER          = "g5-lean-qa"
    _GKE_ZONE             = local.region
    _HELM_RELEASE         = "g5"
    _HELM_NAMESPACE       = "g5"
    _ENVIRONMENT          = "qa"
    _PROMOTION_MODE       = "build"
    _NEXT_TRIGGER         = local.release_queue_enabled ? "g5-promote-staging" : ""
    _RELEASE_BUCKET       = local.release_queue_enabled ? local.release_bucket : ""
    _STAGING_PROJECT      = try(local.release_targets["staging"].project_id, "")
    _PROD_PROJECT         = try(local.release_targets["prod"].project_id, "")
    _STAGING_CLUSTER      = try(local.release_targets["staging"].cluster, "")
    _PROD_CLUSTER         = try(local.release_targets["prod"].cluster, "")
    _FILESTORE_INSTANCE   = module.filestore_selection["qa"].configuration.selected.instance_name
    _FILESTORE_ZONE       = module.filestore_selection["qa"].configuration.selected.zone
    _FILESTORE_PV_NAME    = module.filestore_selection["qa"].configuration.selected.pv_name
    _FILESTORE_PVC_NAME   = module.filestore_selection["qa"].configuration.selected.pvc_name
    _FILESTORE_STORAGE    = module.filestore_selection["qa"].configuration.selected.storage
    _WORKER_IMAGE_PROJECT = var.artifact_project
    _WORKER_IMAGE_FAMILY  = "g5-worker"
    # Worker-image gate waits for a covering bake, starting one for this SHA
    # if a worker-input push was skipped while the trigger was paused.
    _WORKER_IMAGE_TRIGGER   = "g5-packer-worker-main-qa"
    _WORKER_IMAGE_WAIT_MINS = "90"
    # GCS-backed sccache for the two Rust image compiles (cloudbuild_pipelines
    # D33). The deploy-artifacts bucket is writable only by the deploy service
    # account, so PR CI (which writes build-artifacts) cannot seed objects a
    # deploy would link in, and nothing outside g5-qa-1ccc can reach it. Keys
    # live under the Dockerfiles' default prefix sccache/deploy.
    _SCCACHE_GCS_BUCKET = "g5-qa-1ccc-deploy-artifacts"
    # Fleet sccache for workspace guests (ontology g5.runtime.cache.fleet):
    # the deploy's warm-guest-sccache step compiles the test runner's package
    # sets inside the g5dev image and writes them under
    # guest-sccache/orgs/<org> of the same bucket; worker hosts of that org
    # read the prefix through their cache door. The org is G5's own (the
    # G5-on-G5 Base workspaces); other orgs get no fleet objects until a
    # producer exists for their repositories.
    _GUEST_SCCACHE_ORG_ID = "58008163-1b68-4c60-a9b0-1665a8f331d9"
    _GUEST_SCCACHE_PREFIX = "guest-sccache"
    # The trusted tier repository the publish-base-tiers step pushes to (only the deploy identity writes it).
    _TIER_REPOSITORY              = "g5-tiers"
    _GITHUB_OWNER                 = "G5Labs"
    _GITHUB_REPO                  = "G5-Lean"
    _GITHUB_CONNECTION            = local.connection_name
    _GITHUB_CONNECTION_REPOSITORY = local.repository_name
  }
  include_build_logs = "INCLUDE_BUILD_LOGS_WITH_STATUS"
  depends_on = [
    google_service_account_iam_member.terraform_attach,
    google_project_iam_member.qa_deploy_chain,
    google_service_account_iam_member.qa_deploy_worker_image,
  ]
}

resource "google_cloudbuild_trigger" "worker_image" {
  count       = var.enable_application_triggers ? 1 : 0
  project     = local.project_id
  location    = local.region
  name        = "g5-packer-worker-main-qa"
  description = "Build and sign G5 Lean worker images after relevant main changes"
  repository_event_config {
    repository = local.repository
    push { branch = "^main$" }
  }
  filename        = "infra/ci/cloudbuild-packer-worker.yaml"
  service_account = data.google_service_account.build["images"].name
  substitutions = {
    _RUN_FULL_BUILD             = "true"
    _IMAGE_FAMILY               = "g5-worker"
    _ARTIFACT_PROJECT           = var.artifact_project
    _AR_PROJECT                 = var.artifact_project
    _GCE_IMAGE_PROJECT          = var.artifact_project
    _TRIGGER_DEV_WORKER_ROLLOUT = "false"
  }
  included_files = [
    "infra/packer/g5-worker/**", "infra/ci/cloudbuild-packer-worker.yaml",
    "scripts/image-test-gate/**", "scripts/verify-worker-image.sh",
    "scripts/prepare-workload-template-input.py",
    "scripts/release_artifacts.py", "scripts/worker_artifacts.py",
    "infra/guest/**", "scripts/build-cloud-hypervisor-runtime.sh",
    "scripts/install-cloud-hypervisor-platform.sh", "scripts/build-runtime-disk.sh",
    "scripts/stage-guest-bootstrap.py", "scripts/test-cloud-hypervisor*",
    "scripts/test_cloud_hypervisor*", "scripts/prepare-cloud-hypervisor-test.py",
    "backend/g5d/**", "backend/g5d-*/**",
    "backend/g5-contracts/**", "backend/g5-auth/**", "backend/g5-internal-api/**", "backend/g5-scm/**", "backend/g5-semantic/**", "backend/g5-storage/**",
    "Cargo.toml", "Cargo.lock",
  ]
  include_build_logs = "INCLUDE_BUILD_LOGS_WITH_STATUS"
  depends_on         = [google_service_account_iam_member.terraform_attach]
}

resource "google_cloudbuild_trigger" "bootstrap_builder" {
  project     = local.project_id
  location    = local.region
  name        = "g5-bootstrap-ci-builder"
  description = "Manual QA CI builder bootstrap"
  source_to_build {
    repository = local.repository
    ref        = "refs/heads/main"
    repo_type  = "GITHUB"
  }
  git_file_source {
    path       = "infra/ci/cloudbuild-bootstrap-builder.yaml"
    repository = local.repository
    revision   = "refs/heads/main"
    repo_type  = "GITHUB"
  }
  service_account = data.google_service_account.build["images"].name
  substitutions = {
    _POOL          = data.terraform_remote_state.foundation.outputs.ci_pool
    _AR_REGION     = local.region
    _AR_REPOSITORY = "g5-builders"
    _IMAGE         = "g5-ci-builder-bootstrap"
  }
  depends_on = [google_service_account_iam_member.terraform_attach]
}

# Terraform plan on pull requests and apply on main, the QA counterpart of
# dev's g5-terraform-plan / g5-terraform-apply-dev (module cloudbuild). Both
# run infra/ci/cloudbuild-terraform-qa.yaml over the remote-state roots in
# dependency order as the bootstrap-created runner identity. Like dev's
# apply, it runs without approval: merging to main authorizes it, and the
# pipeline refuses destructive plans and superseded builds on its own. Values
# a laptop apply reads from the untracked terraform.tfvars ride along as
# substitutions.
locals {
  # The platform and addons roots source modules from infra/terraform/modules,
  # so a module-only change must plan and apply here too.
  terraform_included_files = [
    "infra/qa/**",
    "infra/config/**",
    "infra/terraform/modules/**",
    "infra/ci/cloudbuild-terraform-qa.yaml",
    "infra/ci/cloudbuild-terraform-release.yaml",
    "scripts/terraform_release.py",
    "scripts/terraform-qa-freshness.sh",
    "scripts/release_artifacts.py",
    "infra/docker/Dockerfile.terraform-bundle", "scripts/ensure-base-images.sh",
    "infra/staging/**", "infra/prod/**",
  ]
  terraform_substitutions = {
    _TF_ROOTS                       = "foundation,delivery,platform,addons"
    _TF_ROOT_DIR                    = "infra/qa"
    _QA_ENABLE_APPLICATION_TRIGGERS = var.enable_application_triggers ? "true" : "false"
    _QA_ENABLE_RELEASE_BOOTSTRAP    = var.enable_release_bootstrap ? "true" : "false"
    _QA_ENABLE_PROMOTION            = var.enable_promotion ? "true" : "false"
    _QA_ENABLE_RELEASE_TRAIN        = var.enable_release_train ? "true" : "false"
    _QA_RELEASE_AUTHORITY_REVIEWED  = var.qa_release_authority_reviewed ? "true" : "false"
    _QA_PROMOTION_TARGETS_JSON      = jsonencode(var.promotion_targets)
    _ARTIFACT_PROJECT               = var.artifact_project
    _AR_REPOSITORY                  = "g5"
    _REGION                         = local.region
    _RELEASE_BUCKET                 = local.release_queue_enabled ? local.release_bucket : ""
    _TF_NEXT_TRIGGER                = local.release_queue_enabled ? "g5-terraform-apply-staging" : ""
  }
}

# The plan trigger runs the PR head's pipeline and Terraform, so it uses the
# read-only planner identity; only the apply trigger, which runs main's
# reviewed code, runs as the privileged runner (infra/qa/bootstrap/main.tf).
resource "google_cloudbuild_trigger" "terraform_plan" {
  project     = local.project_id
  location    = local.region
  name        = "g5-terraform-plan-qa"
  description = "Terraform fmt, validate and plan of the QA roots on pull requests"
  repository_event_config {
    repository = local.repository
    pull_request {
      branch          = "^main$"
      comment_control = "COMMENTS_ENABLED_FOR_EXTERNAL_CONTRIBUTORS_ONLY"
    }
  }
  filename           = "infra/ci/cloudbuild-terraform-qa.yaml"
  service_account    = data.google_service_account.terraform_plan.name
  substitutions      = merge(local.terraform_substitutions, { _TF_ACTION = "plan" })
  included_files     = local.terraform_included_files
  include_build_logs = "INCLUDE_BUILD_LOGS_WITH_STATUS"
  depends_on         = [google_service_account_iam_member.terraform_attach]
}

resource "google_cloudbuild_trigger" "terraform_apply" {
  project     = local.project_id
  location    = local.region
  name        = "g5-terraform-apply-qa"
  description = "Terraform apply of the QA roots on pushes to main"
  repository_event_config {
    repository = local.repository
    push { branch = "^main$" }
  }
  filename        = "infra/ci/cloudbuild-terraform-qa.yaml"
  service_account = data.google_service_account.terraform.name
  substitutions   = merge(local.terraform_substitutions, { _TF_ACTION = "apply" })
  # Every reviewed SHA gets matching infrastructure evidence once promotion is active.
  included_files     = local.release_queue_enabled ? [] : local.terraform_included_files
  include_build_logs = "INCLUDE_BUILD_LOGS_WITH_STATUS"
  approval_config {
    approval_required = false
  }
  depends_on = [google_service_account_iam_member.terraform_attach]
}

output "connection_resource" { value = "projects/${local.project_id}/locations/${local.region}/connections/${local.connection_name}" }
output "repository_resource" { value = local.repository }
output "ci_triggers" { value = { for name, trigger in google_cloudbuild_trigger.ci : name => trigger.trigger_id } }
output "deploy_trigger" { value = try(google_cloudbuild_trigger.deploy[0].trigger_id, null) }
output "worker_image_trigger" { value = try(google_cloudbuild_trigger.worker_image[0].trigger_id, null) }
output "bootstrap_builder_trigger" { value = google_cloudbuild_trigger.bootstrap_builder.trigger_id }
output "terraform_plan_trigger" { value = google_cloudbuild_trigger.terraform_plan.trigger_id }
output "terraform_apply_trigger" { value = google_cloudbuild_trigger.terraform_apply.trigger_id }
