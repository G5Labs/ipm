# Independent review of committed PR command and push-refresh work

Snapshot: `f2b2d9f578d62771e2ceea893dc1a805aa085864`, relative to parent
`75b522e7`. Discovery was read-only; this report is the only workspace write.
Investigative test/config files are under `/tmp/g5-pr-review-one/`. No peer or earlier
review reports were read before saving this initial report. The saved request, governing
design (including its recorded amendments), actual ontology diff and current governing
blocks/metadata were inputs. Supplied scope named the areas to review, without supplying
specific known-bug hints.

## Initial actionable findings

### F1 — P2: Reconcile displayed mutation snapshots with automatic PR refreshes

Changed lines: `frontend/src/components/workspace/CreatePullRequestDialog.tsx:345-350`
and `frontend/src/components/workspace/PrepareToShipModal.tsx:511-513`.
Root consumer: CreatePullRequestDialog line 176 chooses
`createdPullRequest ?? existingPullRequest`; PrepareToShipModal lines 266-267 chooses
`mergedPullRequest ?? createdResult?.pullRequest ?? existingPullRequest`.
WorkspacePage lines 6397-6400 saves the create response in local state and lines
10990-10991 continues supplying both the stale create result and refreshed query value.

Trigger: create/update a PR in either modal, leave it open, then a later source push
changes persisted `pendingPushRefresh`, `pushRefreshError`, provider state, or local
readiness. The new WebSocket handler correctly invalidates the PR query, but the saved
mutation result always wins over the refreshed `existingPullRequest`. An initial failed
publication followed by successful automatic recovery has the inverse problem: the
modal continues displaying the old failure and requesting another update.

Consequence: the newly added pending/error display and current readiness are stale in
the source-side UI even after authoritative data reaches the component. The user can
see old success while automatic provider publication is failing, or old failure after
recovery. This violates `g5.governance.pr.follow-push`'s visible-failure requirement and
the authoritative-consumer part of `g5.architecture.pr_flow`. The precedence pattern
predates this commit; the new automatic updates and claimed complete UI consumption
make this a changed-consumer integration defect in this commit.

Runtime reproduction: `/tmp/g5-pr-review-one/dialog-refresh.test.tsx` mounts the actual
CreatePullRequestDialog with its saved create result, then rerenders with a newer
persisted pending/error record for the same ID. The discriminating assertion expecting
`create-pr-refresh-pending` fails; DOM still says `PR created` / `ready · local only`.
Command: `node frontend/node_modules/vitest/vitest.mjs run --config
/tmp/g5-pr-review-one/vitest.config.mts` (absolute node script path used from `/tmp`).
Result: one test executed, one failure, on the expected missing error display. An initial
configuration attempt could not load external setup; it executed no test. The corrected
configuration executes source directly and keeps its cache under `/tmp`.

Proposed repair: derive current displayed PR state from the authoritative query when
it supplies the same PR at least as recently as the mutation response; retain mutation
metadata such as `alreadyExisted`/dirty-draft notices separately. Apply this to both
source-side modals, preserving a mutation response until query catch-up and handling
terminal merge results deliberately. Regression checks should drive create failure →
automatic recovery and create success → later push failure while the modal stays mounted;
verify readiness/provider links/messages at the consuming component, not only query
invalidation or a fixture initially containing `pendingPushRefresh`.

### F2 — P2: Guard title acceptance against a concurrent PR close

Changed lines: `backend/g5-api-core/src/services/coordinated_repo_publication.rs:329-333`
and the identical conflict/reuse path at lines 401-405. Both execute
`UPDATE workspace_pull_requests SET title=$2, pending_provider_title=$2 WHERE id=$1`
without checking the current status or locking/rechecking the open row first.

Trigger: a create/update request loads an existing open PR using `load_open_pr_for_pair`
(lines 1259-1282, ordinary SELECT); a concurrent close has locked that PR and has not yet
committed. The title UPDATE waits, then proceeds after close commits. PostgreSQL's
current row still satisfies `id=$1`, so the now-closed PR receives the title and pending
provider title. This is admissible with the current shared source-owner locks; this commit
also changes `lock_pr_for_path_workspace` to lock only `pr` at line 1493, so it does not
serialize closing with the source workspace's shared lock.

Consequence: create/update reports reuse of a closed PR and permanently changes its G5
title, while provider synchronization immediately returns for non-open state
(`workspace_pull_request_provider_lifecycle.rs:560-562`). The explicit title therefore
does not reach the provider and its pending intent is stranded. Existing description and
evidence writers explicitly guard `status='open'` (lines 1245-1248 and 1344-1345), so this
is a newly introduced title transition inconsistency, not the intentionally preserved
closed-PR behavior. Governing obligations: `g5.governance.pr.title`, `g5.governance.pr.write`
and `g5.architecture.pr_flow`'s coherent reuse/title operation.

The later source-publication stage does not roll this back: `workspace_service.rs:8024-8042`
selects only open PRs but returns `Ok(0)` when none is found. The orchestrator commits the
local transaction (`workspace_pull_request_create.rs:310`), the provider stage skips the
closed PR, and the final summary is returned normally. This finding is source/transaction
inspection evidence; a real concurrent API reproduction was not executed here.

Proposed repair: lock and revalidate the reused PR's open status before accepting title,
description and evidence, or use a status-guarded title write with checked row count and
a conflict/retry outcome. Cover both existing-row branches. Regression: hold a real PR
close transaction after changing the row to closed, let a real create/update request
observe the previous open version and block on title acceptance, then commit close.
Assert the closed record/provider title remain unchanged and the API cannot claim a
successful title update. Also retain the existing overlapping-title success case.

## Commitment ledger

All results below are inspection unless explicitly marked runtime. Test source is evidence
of intended coverage, not proof that those tests executed in this independent pass.

| Commitment | Actual path and counterexample/check | Result / gap |
|---|---|---|
| `g5.governance.pr.read` | CLI `pr.rs` → public/incarnation `/pull-requests/info` → shared list or `workspace_pull_request_read.rs`; selectors constrained to attached GitHub repos, comments bounded, no reconciliation call | Preserved on inspected paths; deployed/live GitHub unexecuted |
| `g5.security.read`, `.read.durable`, `.tenant`, `.tenant.rls`, `.tenant.write` | Scoped route DBs, current-incarnation entry, attachment selection, read-only durable lists; explicit other-workspace VM selection rejected by typed admission | No introduced defect found; exhaustive unrelated route matrix outside scope |
| `g5.governance.pr.json` | PR-specific optional field parser before discovery/body loading/network; field projection objects/arrays, node ID, unknown/multi-provider values and comment-demand path | No introduced defect found; inspected compiled-suite source and listed existing binary tests, did not claim stale binaries as commit proof |
| `g5.governance.pr.write`, `g5.architecture.pr_flow` | Public and internal POST adapters call `create_or_update`; source admission → preparation → evidence/reuse → source publication → provider completion | Shared ownership established; F2 breaks concurrent terminal reuse; F1 breaks current-state UI consumption |
| `g5.security.owner`, `.admission`, `.login.service-identities`, runtime lease | Typed caller, owner/VM/epoch checks, `lock_owner_handle`, expected incarnation passed into save/push; transactions released before native callbacks | No introduced bypass found on inspected paths; real active guest/native callback not executed |
| `g5.governance.publish`, `.publish.readiness` | `prepare_pull_request_source` checks stopped dirty receipts; commit then push before evidence; failures preserve earlier commits and prevent later publication | Preserved by inspection; actual guest runtime unexecuted |
| `g5.governance.pr.target` | Shared ordered parent/Base query, readable live same-org choices, source excluded, target resolved once and re-admitted at phases | No introduced deterministic defect found; fine-grained manual target-access races not independently exercised |
| `g5.governance.pr.title` | Explicit trim/blank rejection, pending title column, row serialization, included-member PATCH and omitted-title preservation | F2; pending retry/overlapping intent test source inspected but not rerun |
| `g5.governance.pr.follow-push` | Maintenance durable inventory/event hints → `refresh_after_push` → PublishedEvidenceCapture → provider observation without push; current-head final check and pending/error persistence | Backend behavior supported by inspection; visible source-modal failure/recovery violated by F1 |
| `g5.governance.approval.version`, `.merge`, `.merge.effects` | Refresh does not update approvals; active-merge preflight check; PR serialization and terminal-provider observation; provider/local exact SHA checks inspected | No additional introduced violation established; accepted-merge process-loss/live-provider interleavings unexecuted |
| `g5.architecture.recovery`, `.recovery.progress`, `g5.design.commit-notify` | Full-pass worker continues across per-PR failures; HTTP bounded; periodic scan recovers missed events; update hint after commit | Supported by inspection; new full process restart not run; F1 prevents one consumer from exposing recovered facts |
| `g5.governance.publish.github-author`, `.publish.actor`, `g5.security.credentials.publication` | Credential operation variants and in-scope automatic resolution preserve actor creation grant, installation read/write scope and server-side tokens | Parent authorship feature used as dependency context; no new credential leak/fallback found |
| `g5.design.evolution` | Forward migrations 041/042, pending text/null and boolean/default columns, checksum entries; populated migration test inspected | Additive schema supported; migration was not rerun in this pass |
| `g5.design.minimal-mechanism`, `.change-locality`, `.adapters`, `.enforcement` | CLI/routes thin, domain orchestration shared, existing Git/provider/evidence owners reused, two small durable obligations | No unjustified new mechanism found |
| Worker guidance | Both bundled skills/manifests/instructions and `docs/g5d-pr.md`; explicit save/push effects, field help, provider partial outcomes, no token request | Guidance matches implemented interface and intended constraints |

## Other examined boundaries and limits

- Manual provider synchronization still uses transaction-free credential resolution while
  holding the new PR row lock; with a saturated tenant pool, waiting PR requests can
  cause a pool/row-lock cycle. I did not promote this to a new finding because the parent
  local publication phase already held PR/evidence writes while opening nested credential
  scopes, and the review did not isolate a newly reachable production failure from that
  existing risk. A focused pool-concurrency investigation could clarify it if consolidation
  finds independent evidence. Automatic synchronization correctly uses `resolve_in_scope`.
- The recorded-provider automatic path verifies repository identity and SHA/status rather
  than provider branch names. I did not report a new retarget bug because source inspection
  did not establish a supported mutable local binding transition; external provider base
  edits already challenge the parent's SHA-only merge consumer. This remains outside the
  two actionable findings.
- `git diff --check 75b522e7 f2b2d9f5` reports only an extra EOF blank line in a saved prior
  review artifact. This is not an actionable production finding; no prior report content
  was opened. No production, ontology, shared tests or Git state were changed.
- Existing ontology footers and the design describe broad previous test outcomes. This
  independent review does not re-certify those runs. Reconcile the `follow-push`/`pr_flow`
  UI assessment and title assessment after F1/F2 are dispositioned; valid governing intent
  needs no amendment to repair either defect.

Review complete; implementation untouched. One isolated runtime counterexample and one
source-established concurrency defect are ready for main-agent consolidation. Backend
runtime, live GitHub, deployed worker, and populated migration execution remain unexecuted
in this pass. No duplicate broad suite was requested or run.
