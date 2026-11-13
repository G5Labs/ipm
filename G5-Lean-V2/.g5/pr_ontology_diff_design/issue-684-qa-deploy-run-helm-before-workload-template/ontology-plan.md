# QA deploy: Helm before workload-template publication; gate on the older build's deploy phase

Governing block: [g5.quality.delivery.qa-deploy-order](../../ontology/features/operations.g5.md#g5.quality.delivery.qa-deploy-order) (new).
Related: [g5.scope.deployment.workload_templates](../../ontology/features/operations.g5.md#g5.scope.deployment.workload_templates),
[g5.quality.delivery.qa-forward-only](../../ontology/features/operations.g5.md#g5.quality.delivery.qa-forward-only).
Intent: [issue #684](https://github.com/G5Labs/G5-Lean/issues/684) (dvd-g5, 2026-10-21), including its 2026-10-20 build history.

## Problem

`g5-deploy-qa` serializes Helm by build creation order (`wait-deploy-turn`) and then skips
the deploy when a newer same-environment build exists (`check-main-freshness`, newest-main
predicate). Since the Cloud Hypervisor cutover (`1eb0ac197`, 2026-10-15) `resolve-image-refs`
waits on `publish-workload-templates`, which serially publishes five companion disks for
47–82 minutes, and the queue gate holds a build until every older build has finished entirely,
including that publication and `warm-guest-sccache`. On a busy main the freshness check runs
60–120 minutes after creation, so 16 of 21 builds on 2026-10-20 were stale by the time they
reached Helm and QA received 4 merges.

## Owners

- The deploy pipeline (`infra/ci/cloudbuild-deploy.yaml`) owns the step graph and the
  deploy decision sequence `resolve-image-refs → report-artifacts → wait-deploy-turn →
  check-main-freshness → resolve-worker-image/gke-auth → helm-deploy → verify-deploy →
  smoke-test`. `smoke-test` is the last step of that sequence.
- `scripts/deploy_queue_blockers.py` (new, extracted from the inline `wait-deploy-turn`
  filter) owns the blocker predicate: which older builds still hold the environment's
  deploy turn. The step keeps credential resolution, polling and the timeout.
- `scripts/check_deploy_freshness.py` keeps the freshness verdict unchanged.
- The host template owner ([g5.scope.deployment.workload_templates](../../ontology/features/operations.g5.md#g5.scope.deployment.workload_templates))
  already treats a missing companion as a cache miss and formats locally
  (`backend/g5d-isolation/src/isolation/workload_artifact.rs`, `import`).

## Design

1. **Deploy before the companions.** `resolve-image-refs` waits on the image pushes
   (`push-g5d-g5dev` replaces `publish-workload-templates`). `publish-workload-templates →
   warm-guest-sccache` stays chained after `push-g5d-g5dev` and runs beside the deploy
   sequence. The pushed g5dev image is what both the deploy and the publication consume,
   so neither ordering is weakened. A host that starts a VM on a new base digest before
   its companion is published uses the local formatter, the only path before 2026-10-15;
   identity, capacity and isolation checks are unchanged.
2. **Gate on the older build's deploy phase.** `wait-deploy-turn` lists ongoing builds with
   per-step status (`steps[].id,steps[].status`, available for in-flight builds). An older
   same-environment, same-project build in `build` or `deploy` mode blocks only while its
   `smoke-test` step has no terminal status (SUCCESS, FAILURE, INTERNAL_ERROR, TIMEOUT,
   CANCELLED, EXPIRED). A build without that step, without step data (queued, not yet
   started) or with the step pending/working still blocks, so the predicate fails closed
   toward waiting. Helm runs remain strictly serialized by creation order; migrations and
   anti-rollback ordering are unchanged because only the post-deploy tail leaves the queue.
   A skipped deploy exits `smoke-test` with SUCCESS, so a stale older build releases the
   queue as soon as its (empty) deploy phase passes.
3. **Freshness unchanged.** The newest-main predicate and the freshness head still decide
   whether this build deploys; the change moves the decision earlier and shortens the wait.

## Choices and rejected alternatives

- **Not** making companion publication skip on an existing tag or run in parallel: those
  remove time from the build but not from the deploy's critical path once Helm no longer
  waits on them; they remain optional follow-ups in the issue.
- **Not** gating on `helm-deploy` or `verify-deploy` completion: `smoke-test` is the last step
  that can fail the deploy, and the QA forward-only policy means a failed smoke test is
  reported, not rolled back, so the next build must still see the full phase finish.
- **Extracting the filter to a script** rather than editing it inline: the predicate now
  reads step state and deserves unit tests like `check_deploy_freshness.py`; the inline
  step stays well under Cloud Build's argument limit either way.

## Assumptions and acceptance

- `gcloud builds list --ongoing --format='json(...,steps[].id,steps[].status)'` returns
  step status for WORKING builds: verified by the issue author on a live build and again
  on 2026-10-21 from this workspace with the build-log reader identity (three in-flight
  builds reported QUEUED, WORKING, SUCCESS and FAILURE per step; not-yet-started steps are
  QUEUED, not absent). If a future API change omits steps, the gate degrades to the
  previous whole-build wait.
- Expected effect from the issue's replay of 2026-10-20: 13 of 21 builds would have deployed
  instead of 4; the remaining skips are genuine newer merges within 15–40 minutes.
- Accepted when, after merge, a `g5-deploy-qa` build shows `helm-deploy` starting shortly
  after `push-g5d-g5dev` while `publish-workload-templates` is still running, and a
  following build's `wait-deploy-turn` clears once the older build's `smoke-test` ends.
  Until then the block's metadata records the live run as unexecuted.

## Amendment 2026-10-21: publication must not interrupt the deploy phase (PR #692 review)

Cloud Build cancels a build's other running steps when a step without `allowFailure`
fails. Once `publish-workload-templates` runs beside `helm-deploy`, its failure would have
aborted a Helm upgrade in flight, leaving an unverified partial rollout; before this change
the same failure happened ahead of Helm and simply prevented the deploy. The step now never
fails: a `docker pull`, digest inspection or `publish-workload-template.sh` error stops the
loop, is logged, and the step exits 0 without writing `/workspace/workload-templates.ok`
(written on success, also in deploy-only mode). A new `confirm-workload-templates` step
waits on `publish-workload-templates` and `smoke-test` and fails the build when the marker
is missing, so a publication failure still produces a red build and a delivery alert, but
only after the deploy phase has ended. `allowFailure` was considered and dropped: whether a
step that `waitFor`s a failed `allowFailure` step still launches could not be proved from
this workspace, and the confirmation step must run unconditionally.
The queue gate is unchanged: it releases on `smoke-test`, and the confirmation step is the
only step downstream of it. Rejected alternative: chaining publication after `smoke-test`
needs no marker but delays companion availability by the full queue wait and lengthens
every build by the publication time instead of overlapping it.

Also from the review: the governing block now says the skip condition is the existence of
a newer deploy build of the environment, whatever its status, matching the newest-main
predicate in `check_deploy_freshness.py`; the design never required the newer build to
have deployed first.

The governing block was also split on review: `g5.quality.delivery.qa-deploy-order` keeps
the queue and skip rule; `g5.quality.delivery.qa-companions-off-path` owns companion
scheduling and its failure reporting, since the two change independently.
