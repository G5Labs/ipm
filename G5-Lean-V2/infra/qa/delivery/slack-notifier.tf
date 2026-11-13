# Cloud Build -> Slack notifier (ontology g5.quality.delivery.failure-alerts,
# g5.quality.delivery.approval-alerts).
#
# Cloud Build publishes every build status change to this project's
# `cloud-builds` Pub/Sub topic. A Cloud Run service running Google's
# `cloud-build-notifiers/slack` image receives them through a push
# subscription and posts watched failures and approved staging starts to Slack.
# Delivery is outside the build, so a step that crashes,
# times out or never starts still reports, which an inline curl step cannot.
#
# This reuses the retired dev notifier's failure and approval notifications
# through one service and subscription.

locals {
  slack_notifier_name = "g5-cloudbuild-slack-notifier"
  # Google publishes this image weekly and deprecates older digests; Cloud Run
  # resolves the tag to a digest when it creates a revision.
  slack_notifier_image = "us-east1-docker.pkg.dev/gcb-release/cloud-build-notifiers/slack:latest"

  # The image reads the webhook once, at startup, so a running revision keeps
  # posting to whatever URL it started with. Pin the version: rotating is
  # adding a version and bumping this, which changes the config object and so
  # rolls a revision that reads the new URL. Only the number is in state.
  slack_webhook_version = "1"

  # Every trigger in this root that is not a pull-request check. Their builds
  # have no PR surface, so this notifier is their only push-style alarm; PR
  # triggers report through PR checks. scripts/test_qa_slack_notifier.py fails
  # when a non-PR trigger is missing here.
  slack_notifier_trigger_names = concat(
    google_cloudbuild_trigger.deploy[*].name,
    google_cloudbuild_trigger.worker_image[*].name,
    [for trigger in google_cloudbuild_trigger.promote : trigger.name],
    [for trigger in google_cloudbuild_trigger.terraform_promote : trigger.name],
    google_cloudbuild_trigger.release_train[*].name,
    google_cloudbuild_trigger.deploy_cutoff[*].name,
    [
      google_cloudbuild_trigger.terraform_apply.name,
      google_cloudbuild_trigger.bootstrap_builder.name,
    ],
  )

  # Only staging owns an approval gate; production follows it automatically.
  # WORKING reports both human and release-train approvals once the job starts.
  slack_approval_trigger_names = concat(
    [for environment, trigger in google_cloudbuild_trigger.promote : trigger.name if environment == "staging"],
    [for environment, trigger in google_cloudbuild_trigger.terraform_promote : trigger.name if environment == "staging"],
  )
}

data "google_project" "current" {
  project_id = local.project_id
}

# The webhook is operator-owned, like the platform root's adopted secrets:
# Terraform reads the container and grants access but never holds the URL,
# so it never enters state and an apply cannot overwrite it. Create the
# secret and add the URL before the first apply of this file (infra/qa/README.md).
data "google_secret_manager_secret" "slack_webhook" {
  project   = local.project_id
  secret_id = "g5-cloudbuild-slack-webhook"
}

# The image reads its filter and template once, at startup, from GCS.
resource "google_storage_bucket" "slack_notifier_config" {
  project                     = local.project_id
  name                        = "${local.project_id}-cloudbuild-slack-notifier"
  location                    = local.region
  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"
  # Holds only the rendered config below; nothing here outlives the notifier.
  force_destroy = true
}

resource "google_storage_bucket_object" "slack_notifier_config" {
  bucket = google_storage_bucket.slack_notifier_config.name
  name   = "notifier-config.yaml"
  content = templatefile("${path.module}/slack-notifier-config.yaml.tftpl", {
    WEBHOOK_SECRET_RESOURCE    = "projects/${data.google_project.current.number}/secrets/${data.google_secret_manager_secret.slack_webhook.secret_id}/versions/${local.slack_webhook_version}"
    TRIGGER_NAMES_CEL          = join(", ", [for name in local.slack_notifier_trigger_names : "'${name}'"])
    APPROVAL_TRIGGER_NAMES_CEL = join(", ", [for name in local.slack_approval_trigger_names : "'${name}'"])
  })
}

resource "google_service_account" "slack_notifier" {
  project      = local.project_id
  account_id   = "g5-cb-slack-notifier"
  display_name = "Cloud Build to Slack notifier (Cloud Run)"
}

resource "google_storage_bucket_iam_member" "slack_notifier_config_reader" {
  bucket = google_storage_bucket.slack_notifier_config.name
  role   = "roles/storage.objectViewer"
  member = "serviceAccount:${google_service_account.slack_notifier.email}"
}

resource "google_secret_manager_secret_iam_member" "slack_notifier_webhook_reader" {
  project   = local.project_id
  secret_id = data.google_secret_manager_secret.slack_webhook.secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.slack_notifier.email}"
}

resource "google_cloud_run_v2_service" "slack_notifier" {
  project  = local.project_id
  name     = local.slack_notifier_name
  location = local.region
  ingress  = "INGRESS_TRAFFIC_INTERNAL_ONLY"

  template {
    service_account = google_service_account.slack_notifier.email

    # The image never re-reads its config, so a changed filter or trigger list
    # must roll a new revision or the warm one keeps serving the old filter.
    # Hash the planned content: the provider's md5hash can still describe the
    # previous object during this plan, leaving a warm revision unchanged.
    annotations = {
      "g5/config-md5" = md5(google_storage_bucket_object.slack_notifier_config.content)
    }

    # The image keeps the build being rendered in a field shared by all
    # requests, so two concurrent failures on one instance could post each
    # other's details. Serve one request at a time.
    max_instance_request_concurrency = 1

    scaling {
      min_instance_count = 0
      max_instance_count = 2
    }

    containers {
      image = local.slack_notifier_image
      env {
        name  = "CONFIG_PATH"
        value = "gs://${google_storage_bucket.slack_notifier_config.name}/${google_storage_bucket_object.slack_notifier_config.name}"
      }
      env {
        name  = "PROJECT_ID"
        value = local.project_id
      }
      resources {
        limits = {
          cpu    = "1"
          memory = "512Mi"
        }
      }
    }
  }

  depends_on = [
    google_service_account_iam_member.terraform_attach,
    google_storage_bucket_iam_member.slack_notifier_config_reader,
    google_secret_manager_secret_iam_member.slack_notifier_webhook_reader,
  ]
}

# Cloud Build publishes to `cloud-builds` only when the topic exists; nothing
# else in QA consumes it.
resource "google_pubsub_topic" "cloud_builds" {
  project = local.project_id
  name    = "cloud-builds"
}

# Identity Pub/Sub uses to mint the OIDC token for each push to Cloud Run.
resource "google_service_account" "slack_notifier_invoker" {
  project      = local.project_id
  account_id   = "g5-cb-slack-pubsub-inv"
  display_name = "Pub/Sub invoker for the Cloud Build to Slack notifier"
}

resource "google_cloud_run_v2_service_iam_member" "slack_notifier_invoker" {
  project  = local.project_id
  location = google_cloud_run_v2_service.slack_notifier.location
  name     = google_cloud_run_v2_service.slack_notifier.name
  role     = "roles/run.invoker"
  member   = "serviceAccount:${google_service_account.slack_notifier_invoker.email}"
}

# Without this the push carries no token and Cloud Run answers 401.
resource "google_service_account_iam_member" "slack_notifier_invoker_token_creator" {
  service_account_id = google_service_account.slack_notifier_invoker.name
  role               = "roles/iam.serviceAccountTokenCreator"
  member             = "serviceAccount:service-${data.google_project.current.number}@gcp-sa-pubsub.iam.gserviceaccount.com"
}

resource "google_pubsub_subscription" "slack_notifier" {
  project              = local.project_id
  name                 = local.slack_notifier_name
  topic                = google_pubsub_topic.cloud_builds.id
  ack_deadline_seconds = 30

  push_config {
    push_endpoint = google_cloud_run_v2_service.slack_notifier.uri
    oidc_token {
      service_account_email = google_service_account.slack_notifier_invoker.email
    }
  }

  expiration_policy {
    ttl = ""
  }

  retry_policy {
    minimum_backoff = "10s"
    maximum_backoff = "600s"
  }

  depends_on = [
    google_service_account_iam_member.terraform_attach,
    google_cloud_run_v2_service_iam_member.slack_notifier_invoker,
    google_service_account_iam_member.slack_notifier_invoker_token_creator,
  ]
}

output "slack_notifier_trigger_names" { value = local.slack_notifier_trigger_names }
