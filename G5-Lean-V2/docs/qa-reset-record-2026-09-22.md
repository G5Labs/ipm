# QA reset and deployment record: 2026-09-22 (K2)

This is the infrastructure half of gap G7 in
[mvp-workspace-checkpoint-status-and-handoff.md](mvp-workspace-checkpoint-status-and-handoff.md)
(workstream K2), executed with [qa-database-reset.md](qa-database-reset.md)
against the disposable `g5-qa-1ccc` environment. It records exact artifacts
and state transitions as observed, in UTC. Product-level acceptance
observations (Start, Save, transfer, fork, native requests, preview,
terminal) are not claimed here; they belong to the owning workstreams and the
full G7 observation after K1, Z2 and R2 land.

## 1. Cutover inputs

| Field | Value |
|---|---|
| Cutover commit (`QA_CUTOVER_SHA`) | `96f6b9335a9b3b989c01857bd5e8ca260d3cec34` (merge of checkpoint PR #117) |
| Cloud SQL instance / database | `g5-lean-qa-postgres` (POSTGRES_17, `db-custom-8-32768`, private IP `10.217.0.3`) / `g5_lean_qa` |
| Database owner / encoding / collation after recreate | `cloudsqlsuperuser` / `UTF8` / `en_US.UTF8` |
| Pre-reset backup | on-demand backup `1790110146725`, `SUCCESSFUL`, window `20:49:06Z` to `20:50:38Z`, description "pre-reset 2026-09-22 before g5_lean_qa recreate" |
| Automated backups | daily at `03:00Z` (most recent before the reset: `1790046000000`) |
| Writers quiesced | `g5-api` and `g5-api-indexer` per runbook step 2; no worker VMs existed at 20:50Z (the first host was provisioned at 22:21Z). The scale-down itself was not re-verified for this record. |
| Bootstrap administrator | `kaihui.zheng@g5labs.ai` |

## 2. State transitions

| Time (UTC) | Transition | Evidence |
|---|---|---|
| 19:26 | First deploy of `96f6b93` (build `34f334f2-6362-43d2-ba81-f72443bbc113`) failed in migration 008: the migrator role `g5` has no `BYPASSRLS` on Cloud SQL, so data statements on `FORCE ROW LEVEL SECURITY` tables saw zero rows while the following constraint scanned the real heap. | Cloud Logging `g5-db-migrate`; fix landed as PR #122 (migrator visibility guard, CI migrator `NOBYPASSRLS`). |
| 20:16 | Helm revision 7 recorded `failed`; revision 8 superseded it at 20:17, before the reset. | Helm release secrets `sh.helm.release.v1.g5.v7`/`v8`. |
| 20:31 | Retry deploy `de0fb542-8969-46c2-ae24-3f439c48091d` cancelled in favour of the reset. | `gcloud builds list`. |
| 20:49 to 20:50 | On-demand backup `1790110146725` taken and `SUCCESSFUL`. | `gcloud sql backups describe`. |
| 20:50 | `g5_lean_qa` dropped and recreated with the recorded owner and encoding. Instance, roles, networking, secrets untouched. | `gcloud sql databases describe`: `UTF8` / `en_US.UTF8`. |
| 20:52 to 21:28 | Deploy build `74022387-5fad-4cbf-82ce-c49ffb815fe4` for `96f6b93` succeeded end to end: image builds, migration verification, `helm-deploy` (`REVISION: 9`), `verify-deploy` (both ExternalSecrets `Ready=True`), smoke test, microVM and preview canaries. | Build steps all `SUCCESS`; log line `REVISION: 9`. |
| 21:27:55 to 21:27:56 | Migrations `001` to `016` applied on the fresh schema, all `success = true`. | `_sqlx_migrations` (table in section 3). |
| 21:30:09 to 21:30:17 | Production reseeder (`scripts/seed-prod-org.sh`, `USE_K8S_PSQL=1`) created org `g5` (`4fb72c5e-e1e3-4b16-8f40-3f2dcfe6975d`), base workspace `Base` (`ba6d0104-0ca8-4301-a767-ea125f5bd2a2`), the admin invitation (accepted) and worker registration token `c0a23f4e-73b8-4ab0-801f-545ca368b717`. | `orgs`, `workspaces`, `org_invitations`, `worker_tokens`. |
| 22:12 to 22:23 | Base workspace repaired by hand for the P4.1 schema (`workspace_repos` row, unborn docs branch, docs mirror on Filestore, org default image); the repairs were then automated in PR #125. Workspaces `kaihuiz-test` and `zcl-test` forked from `Base`. | `workspaces`; memory of the 8d88baab session; PR #125. |
| 22:21 to 22:31 | Two GCE worker hosts provisioned for org `g5` (`58b3e119…` 1 slot, `dbca08fb…` 5 slots) from worker image `g5-worker-v20260922-1927-96f6b93`; both crash-looped on the missing runtime module catalog until the two catalog files were copied live and `g5d-host` restarted; both then registered `ready`. | Host journal: `workspace runtime catalog is absent: /var/g5d-data/runtime/modules/catalog.json`; fix in PR #124 (packer stages the catalog). |
| 22:39 and 22:40 | First workspace VMs booted on QA: `7017c9a1…` (workspace `kaihuiz-test`) and `a7a67f83…` (workspace `zcl-test`) reported `booted` then `ready` about 1.5 s apart, and the API marked both `running`. Both exited later (23:25, 23:36) and the rows are `stopped` with no `last_error`. | `worker_vm_events`, `workspace_vms`. |
| 22:48 to 23:27 | Deploy of `1195def` (PR #124, build `408d01e6-ff2a-4216-a6e4-52370fcdf19b`) succeeded; Helm revision 10 is `deployed` since 22:50:49. Packer build `840c02ce-afcf-4be7-8462-268e69c7a9a5` produced `g5-worker-v20260922-2250-1195def` at 23:38. | Helm release secret `v10`; `gcloud builds list`. |
| 23:26 and 23:36 | Two more hosts for org `g5` (`ca5944d9…`, `c76a1504…`) were provisioned before the fixed image was pinned; they boot from `g5-worker-v20260922-2203-6ca751d`, which predates PR #124, and crash-loop on the same missing catalog (51 restarts on `ca5944d9…` by 23:52). They stay `provisioning` and never heartbeat. | IAP `journalctl -u g5d-host`; `gcloud compute disks describe … sourceImage`. |
| 23:43 | Host `a8a8ae44…` (org `zcl`) provisioned from `g5-worker-v20260922-2250-1195def` registered `ready` without intervention. | `worker_hosts`, disk `sourceImage`. |
| 23:26 onward | Deploys of `ad32add` (PR #123) and `319268f` (PR #125) were still `WORKING` when this record was written. | `gcloud builds list`. |

## 3. Verification queries (read-only, 2026-09-22 23:45Z to 23:55Z)

| Check | Result |
|---|---|
| `_sqlx_migrations` count / max version | 16 / 16, every row `success = true`, all installed within one second at 21:27:55Z |
| Migration set | 001 lean baseline, 002 remove workspace search, 003 p4 1 runtime schema, 004 mvp host registry runtime, 005 agent session inbox cutover, 006 workflow job spec, 007 remove crdt and add comment anchors, 008 lean mvp execution cutover, 009 remove legacy chat input triggers, 010 allow pending stop supersession, 011 allow force delete stop supersession, 012 allow force delete pending nudge, 013 serialize active fork names, 014 rotate active attempt reconciliation, 015 allow cancelled before start attempt reason, 016 allow queued input task binding |
| Organizations | 2: `g5` (seeded, 21:30:09Z) and `zcl` (created through the product, 22:02:56Z) |
| Org `g5` workspaces | 3: `Base` (base), `kaihuiz-test`, `zcl-test`; none deleted |
| Org `g5` users / invitations | 3 users; invitations: `kaihui.zheng@g5labs.ai` admin `accepted`, `hariharan.rahul@g5labs.ai` admin `pending` (expires 2026-09-29) |
| Worker registration tokens (org `g5`) | 1 row, `c0a23f4e…`, created 21:30:17Z, `revoked_at` NULL, `expires_at` NULL |
| Worker hosts | 6 rows: 3 `ready` (`58b3e119…`, `dbca08fb…`, `a8a8ae44…`, heartbeats at 23:48Z), 2 `provisioning` (see section 2), 1 `terminated` (`48680c1f…`) |
| Workspace VMs | 3 rows, all `stopped`, no `last_error`; `worker_vm_events`: 2 `booted`, 2 `ready`, 2 `exited` |
| Deployed images (Helm v10) | `g5-api` and `g5-api-indexer`: `us-central1-docker.pkg.dev/g5-qa-1ccc/g5/api@sha256:139ae427a1134c3b97f16841f8e855e0f01b59ed5f3cfaf30c243b5a3eb2d684`; `g5-edge`: `…/g5/edge@sha256:45084e924d2ea0656bb1943fe03af9537e70cdb164afdb41ac01b59856b5f75a`; all deployments 1/1 ready, pods started 22:51Z |
| Promoted workspace base images at the reset deploy | `g5d-g5dev@sha256:ce254ae5e79d36e60b6b844434753c3d35869fb82c177e8ddbb4cb8480358506`, `g5d-tungsten@sha256:9e11b08f…`, `g5d-pairsoft-preview@sha256:af8d4a15…`, `g5d-pnc-bankworkflow@sha256:8ca51a33…`, `g5d-pnc-cash-receipt@sha256:618e0eda…` (from the `helm-deploy` step of build `74022387`) |
| Pinned worker boot image at the reset deploy | `projects/g5-qa-1ccc/global/images/g5-worker-v20260922-1927-96f6b93` (`resolve-worker-image` step) |
| Triggers | `g5-deploy-qa`, `g5-packer-worker-main-qa`, `g5-ci-rust`, `g5-ci-e2e` all enabled at the time of writing |
| Admin database role | `g5`: `rolbypassrls = false`, `rolsuper = false`; `authz_lookup_definer` carries `BYPASSRLS` but is not `SET`-able by `g5` |

## 4. Acceptance criteria from the handoff (K2)

| Criterion | Status |
|---|---|
| Backup and reset record | Met: backup `1790110146725`, recreate with recorded owner/encoding. |
| Migrations 001 to 016 on the rebuilt schema | Met, on deploy build `74022387` at Helm revision 9. |
| Two-pass reseed | Partially met: the first pass created the org, base workspace, invitation and token; the second pass (PR #125's idempotent re-run) was executed with `ROTATE_WORKER_TOKEN=0` so that the already registered hosts stayed valid. Consequently there is **one active bootstrap token and no revoked prior token**; the "prior token revoked" observation was intentionally not produced. Repeat the seed without `ROTATE_WORKER_TOKEN=0` only when re-registering hosts is acceptable. |
| Exact image digests | Met (section 3). |
| Host ready | Met for three hosts; two hosts remain `provisioning` because they boot from a worker image that predates PR #124 (section 2). Deleting and re-provisioning them after a deploy that pins `g5-worker-v20260922-2250-1195def` or later is the remaining action. |
| Deployment build ID | Met: `74022387-5fad-4cbf-82ce-c49ffb815fe4` (reset), `408d01e6-ff2a-4216-a6e4-52370fcdf19b` (current Helm v10). |
| Rollback and recovery limits | Recorded in section 5. |

## 5. Rollback and recovery limits

- Migrations `001` to `016` are forward-only; reverting application code does
  not reverse applied schema. The only pre-reset state is backup
  `1790110146725`, whose `_sqlx_migrations` ledger carries the pre-baseline
  checksums that the current migrator refuses. Restoring it therefore also
  requires the application and worker versions that were deployed before
  Helm revision 9, and discards everything written since 20:50Z (the seeded
  org, both test workspaces, the `zcl` org, host registrations).
- Fixing forward is the supported path; a Helm rollback to revision 8 or
  earlier against the rebuilt schema is not (see the runbook's failure
  section).
- Worker hosts carry no durable state that the database does not also
  hold: a host can be deleted and re-provisioned at any time, and its
  retained roots are restored from Git receipts.

## 6. Defects and corrections surfaced by the run

| Finding | Owner / fix |
|---|---|
| Migrator role on Cloud SQL lacks `BYPASSRLS`; CI's migrator had it and masked the failure. | PR #122 (merged): row-visibility guard in `g5-db-migrate`, CI migrator provisioned `NOBYPASSRLS`, rehearsal script. |
| Seeded base workspace was unusable by the product (no `workspace_repos` row, `materialized` zero-commit branch, no docs mirror, no org default image). | PR #125 (merged): seeder produces the P4.1 state; runbook updated. |
| Worker image never staged the runtime module catalog the host supervisor requires; every host crash-looped. | PR #124 (merged): packer stages `modules/catalog.json` and `.sha256`, image gate asserts them. Hosts provisioned between a packer build and the deploy that pins it still boot from the older image (section 2). |
| Chat follow-up turns rejected after a frozen agent dispatch (`invalid_persisted_agent_launch`). | PR #128 (merged). |
| Verification queries on tenant tables return zero rows for the admin user unless an authorization context is set. | Runbook correction in this PR. |
| `kubectl run --rm -i` attach to the query pod times out although the pod schedules within seconds. | Runbook correction in this PR (exec into a long-lived pod). |

## 7. Still unverified

- Deployed host loss, reconnect, replay and drain behaviour on the QA hosts
  (K1 evidence exists in CI; the deployed observation is the next K2 step
  after PR #129 and a deploy that includes it).
- The user-visible S1 to S6 acceptance observations listed in the handoff's
  G7 entry.
- The two `provisioning` hosts remain until they are re-provisioned on the
  fixed worker image.
