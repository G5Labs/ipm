# The trusted QA apply runner attaches only these identities to managed builds,
# Cloud Run, Scheduler and Pub/Sub. Resource references order grants after account
# creation; each attachment explicitly waits for its grant. This is direct-grant
# narrowing, not isolation from the runner's remaining IAM administration.
locals {
  terraform_attached_accounts = merge(
    { for key, account in data.google_service_account.build : key => account.name },
    {
      terraform              = data.google_service_account.terraform.name
      terraform_plan         = data.google_service_account.terraform_plan.name
      slack_notifier         = google_service_account.slack_notifier.name
      slack_notifier_invoker = google_service_account.slack_notifier_invoker.name
    },
    var.qa_release_authority_reviewed ? {
      release_plan = google_service_account.release_plan[0].name
    } : {},
    local.release_train_enabled ? {
      release_train     = google_service_account.release_train[0].name
      release_scheduler = google_service_account.release_scheduler[0].name
      deploy_cutoff     = google_service_account.deploy_cutoff[0].name
    } : {},
  )
}

resource "google_service_account_iam_member" "terraform_attach" {
  for_each           = local.terraform_attached_accounts
  service_account_id = each.value
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${data.google_service_account.terraform.email}"
}

# Central release uses the existing qa_terraform_stage grant in promotion.tf,
# preserving its state address and avoiding two resources owning one binding.
