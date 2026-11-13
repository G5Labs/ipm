# Recover a skipped QA worker bake

[Issue #1130](https://github.com/G5Labs/G5-Lean/issues/1130) records a worker-input
push skipped while its trigger was paused. Resuming a trigger does not replay
that push, and later pushes can miss its file filter. The deploy must recover
without accepting an image built before those inputs changed.

The [worker-image coverage commitment](../../ontology/features/operations.g5.md#g5.quality.delivery.qa-worker-image-coverage)
keeps coverage admission in the existing deploy gate. Cloud Build owns bake
execution and status; the image publisher owns the resulting READY image.
After the existing bounded list rechecks find no in-range bake, an enabled
trigger may be run once for the promoted SHA. Recheck its enabled state before
dispatch, log the returned build ID, then describe that build directly so list
eventual consistency cannot cause repeated launches. Reuse the gate's existing
wait deadline, READY grace, coverage checks and publisher provenance checks.
A failed dispatch, failed bake, unreadable status or exhausted bound stops the
deploy. An existing failed bake is not automatically retried. Concurrent deploys
may each launch a bake; no lock or separate recovery lifecycle is needed.

QA delivery Terraform grants the trusted deploy account attachment only to the
image account. Its existing build-editor grant must also be present when QA
application triggers are enabled without promotion targets. This follows
[explicit service-account attachment](../../ontology/features/operations.g5.md#g5.quality.delivery.terraform-authority)
and preserves [artifact ownership](../../ontology/features/operations.g5.md#g5.quality.delivery.artifact-authority).
Google requires [build creation and attachment authority](https://docs.cloud.google.com/build/docs/cloud-build-service-account)
for this operation. No PR identity receives the image-account grant.

The [QA deploy order](../../ontology/features/operations.g5.md#g5.quality.delivery.qa-deploy-order),
[forward recovery](../../ontology/features/operations.g5.md#g5.quality.delivery.qa-forward-only),
[deploy-only references](../../ontology/features/operations.g5.md#g5.quality.delivery.qa-deploy-only-refs)
and [production promotion](../../ontology/features/operations.g5.md#g5.quality.delivery.production)
remain unchanged. Trigger pause/resume and live incident recovery are outside
this implementation. Terraform must install the grant before live recovery can
work; effective live IAM has not been verified from this VM.

Acceptance uses the real gate with fake Cloud Build/image responses: a missing
bake launches once at the promoted SHA, survives list invisibility, waits and
pins a covering image; dispatch failure, disabled trigger, bake failure,
cancellation, wait expiry and READY-grace expiry refuse deployment. A bake
appearing during rechecks prevents dispatch. Mocked delivery plans check the
named grants with and without promotion configured.

Local verification passed: 148 Python tests across the gate, resolver,
worker/app artifacts, pipeline ordering and QA attachment contracts (one optional
ORAS integration test skipped); the worker-image Helm guard and freshness
orchestration suite; Terraform formatting/validation and all 34 provider-mocked
delivery tests. No live build, deployment or IAM apply was performed.
