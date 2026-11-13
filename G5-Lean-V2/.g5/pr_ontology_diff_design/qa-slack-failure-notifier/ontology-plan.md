# QA Slack failure notifier

Governing block: [g5.quality.delivery.failure-alerts](../../ontology/features/operations.g5.md#g5.quality.delivery.failure-alerts). Intent: [request](../../chat_intent/qa-slack-failure-notifier/request.md).

## Owner and shape

The QA `delivery` root owns the notifier because it owns the triggers being watched; the watched list is derived from those trigger resources instead of being restated. Delivery follows dev's proven path: Cloud Build status events on the project's `cloud-builds` topic, a push subscription with an OIDC token, the internal-only Cloud Run service running Google's `cloud-build-notifiers/slack` image, and a Slack post through a webhook read from Secret Manager.

## Choices

- **Out-of-band delivery, not a pipeline step.** Cloud Build has no always-run final step, so an inline post cannot report a crashed, timed-out or never-started build, which are the cases that need an alert most.
- **One notifier instance.** Dev's prod-approval and canary instances served promotion and canary triggers that QA does not have.
- **Operator-owned webhook referenced by a data source.** Dev's module owned the secret container plus a placeholder version. Here Terraform only reads the container and grants access, following the platform root's adopted-secret pattern: the URL never enters state, an apply cannot overwrite it, and the read-only planner refreshes it with Secret Manager viewer. The cost is one manual step before the first apply, documented in the README. The image reads the webhook only at startup, so the config pins a secret version: rotation is a reviewed bump of that number, which rolls a new revision, rather than a `latest` read that warm instances would never see.
- **One request per instance.** The image renders each message from a field shared by all requests, so concurrent deliveries on one instance could mix two builds' details; the service runs with request concurrency 1.
- **Copied webhook.** An accepted exception to QA's new-credentials rule (see the request). QA owns its copy and reads nothing from dev at runtime; revoking dev's webhook in Slack silences both.
- **Bootstrap prerequisites applied directly.** `run.googleapis.com` and `roles/run.admin` belong to `bootstrap`, which is applied manually from local state. They were applied with gcloud by a project owner and written into `bootstrap/main.tf`, so the state holder's next apply is a no-op for them.
- **`latest` notifier image.** Google publishes it weekly and deprecates old digests, so a pinned digest would eventually break new revisions; Cloud Run pins the digest per revision.

## Assumptions and acceptance

- Cloud Build publishes to `cloud-builds` once the topic exists, and `TRIGGER_NAME` is set for manual (source-to-build) trigger runs as well as push triggers.
- The planner can refresh the new bucket through the bucket's default `projectViewer` legacy reader bindings (present on existing QA buckets).
- Accepted when a deliberately failed build of a watched trigger posts to the channel after the merge applies; until then the block's status says unexecuted live.
