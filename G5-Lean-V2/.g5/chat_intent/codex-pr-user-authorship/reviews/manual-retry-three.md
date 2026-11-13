# Manual retry review — independent reviewer three

## Initial discovery snapshot

Read-only independent review of the entire uncommitted implementation delta against
`4504311be48c72aa19e4ceed49fbc7bd9d679a8b` (confirmed HEAD), for PR 268. The only
write in this review is this report. No peer reports or author review explanation were
read. No further delegation or shared database/build checks were performed.

Inputs: the first `Ontology changes and governance` section of `ontology-plan.md`,
especially `Current amendment: manual retry without schema additions`; the saved
request including its last section; the actual ontology diff; the review skill and
authoring design-review guide; current affected governance, architecture, recovery,
notification, authority and merge commitments plus their metadata. Earlier design
sections were treated as historical where explicitly superseded.

The current amendment accurately records the three ontology changes. The requested
tradeoff is acceptable: failed explicit title intent may be resubmitted and manual
Update PR may repair synchronization. The findings below concern retained guarantees,
not a demand to restore pending columns.

## Findings saved before sharing

### R3-F1 — P2: Local preflight refresh hides an unresolved automatic publication failure

Changed location: `backend/g5-api-core/src/services/coordinated_repo_publication.rs:1728–1730`
and the matching list projection at `1768–1770`.

The new projection shows an automatic failure only when the latest event of **any**
type is `provider_failed`. A local preflight refresh produces `refreshed` without
attempting provider synchronization. Consequently, the existing list Refresh action
clears the sole new automatic-failure presentation while the provider is still at its
old evidence cut.

Concrete counterexample:

1. An open PR/provider row is pinned to source A. A successful source push advances
   to B, but automatic refresh fails because B has not reached the remote branch (or
   provider access fails). Its transaction rolls back and `record_refresh_failure`
   records the actionable `provider_failed` event; provider status remains `open`.
2. Use the existing PR list Refresh button, which calls
   `POST /workspaces/:id/pull-requests/:pullRequestId/refresh`.
3. `refresh_workspace_pull_request` at
   `backend/g5-api-routes/src/routes/workspaces.rs:5541–5576` only prepares/applies local
   evidence. `apply_prepared_workspace_pull_request_refresh` calls
   `refresh_pull_request(..., "refreshed")` at
   `coordinated_repo_publication.rs:1063–1070`.
4. The new summary projection returns no automatic error (and typically a null
   `blockingMessage`), even though no provider recovery occurred. All three changed
   UI consumers now depend on this message; the list shows the old provider as open
   and local readiness without the Update PR recovery instruction.

Previously, local refresh did not clear `push_refresh_error`/`pending_push_refresh`.
This is therefore a regression in the new projection, not existing local-refresh
behavior. Background scanning may eventually report the failure again when provider
pins differ, but that does not make the intervening successful-looking response true;
with no recorded provider row the local head update can also remove the head mismatch
used by discovery.

Governing obligations: `g5.governance.pr.follow-push` requires incomplete publication
to remain honestly reported and directs the owner to retry; `g5.architecture.recovery`
requires recovery to follow the actual outcome. A local preflight is not provider
completion.

Suggested fix: derive unresolved automatic failure from synchronization-relevant
events/outcomes and current evidence, so a local-only refresh cannot settle it. Do
not add a new pending column. Ensure only a verified full synchronization (or a
terminal outcome) supersedes this failure.

Suggested regression: extend the real automatic remote-publication failure test to
call the actual `/refresh` route before fixing the remote, assert the list and single
PR summary still expose the actionable failure while provider pins remain A, then
call the real Update PR route after fixing the cause and assert the message clears.
This test must not merely inject a final `blockingMessage` into a UI fixture.

Evidence status: confirmed by source control-flow and SQL projection inspection;
the counterexample was not executed in a database by this reviewer.

### R3-F2 — P2: Manual included-member retry overwrites a completed provider PR's pinned evidence

Changed locations: `backend/g5-api-core/src/services/workspace_pull_request_provider_lifecycle.rs:581–587`,
`629–664`, and `1101–1108`.

The newly shared path sends an already-included member's recorded provider PR through
`create_or_update_one` with `after_push=false`. Its new completion guard returns an
error saying to preserve the merge version. However, the caller catches that error
and writes it through `record_provider_status`, whose upsert replaces
`source_head_sha`, `target_head_sha`, `provider_head_sha` and `provider_base_sha`
(`2170–2200`). That writes the current candidate pins and null provider evidence over
the old accepted version, despite detecting that the provider PR completed.

Concrete counterexample:

1. Recorded provider PR #21 has source A and was merged remotely, while the G5 PR
   remains open because another member is outstanding or merge import is unfinished.
2. The member source later advances to B, already included in its target, while the
   other member still differs. Invoke ordinary Update PR (with or without a title).
3. The included member now enters the new recorded-PR path. GET #21 returns its
   completed A version. The guard returns `Provider PR completed during refresh;
   preserve its merge version`.
4. The generic explicit-error handler commits a `failed` provider row pinned to B,
   with its previously observed provider head/base cleared. A later merge preflight
   compares that B pin with #21's A head and fails (`preflight_one_provider_pr_for_merge`,
   approximately `1528–1548`), while the accepted A evidence has already been lost.

Before this delta an omitted-title update skipped that included member; a title-only
update used `update_existing_provider_status`, which preserved the pinned source and
target. Automatic refresh still returns the error and rolls back, so its existing
terminal-pin tests do not cover the newly added manual path.

Governing obligations: the amendment explicitly retains terminal/obsolete operation
protection; `g5.governance.pr.follow-push`, `g5.governance.merge` and
`g5.governance.merge.effects` retain accepted merge evidence and recovery without
repinning an unresolved operation.

Suggested fix: do not use the candidate-repinning upsert for failure/refusal of an
existing recorded PR. Preserve that row's original pins and observed provider proof
when completion/identity validation refuses refresh, and retain an actionable error
through the existing outcome surface. Check before destructive provider/record effects
where possible.

Suggested regression: extend
`automatic_pr_refresh_checks_identity_included_members_and_terminal_pins` or the
explicit included-member test with a real Update PR request after remote completion.
Assert that the same provider identity and original source/target pins and provider
proof survive, that no new empty PR is created, and that later merge recovery is not
made impossible by a rewritten pin. Cover both a previously recorded `merged` status
and a completion first discovered through GET.

Evidence status: confirmed by source control-flow and SQL writer inspection; this
reviewer did not execute the database/provider counterexample.

## Obligation ledger and evidence without findings

| Commitment / acceptance boundary | Actual paths and counterexample/check | Assessment |
| --- | --- | --- |
| No migrations 055/056 or replacement pending columns | Deleted SQL/checksum entries and migration-specific test; repository search for all three SQL column names, JSON names and migration filenames across backend/frontend/docs | No remaining production/test/doc consumers found. All 54 retained migration checksums pass. Fresh database setup not executed here. |
| `g5.architecture.pr_flow`, `g5.governance.pr.write` | UI and CLI retain shared `create_or_update`; provider title is now an explicit request argument; no new queue or second publication owner | No architecture violation found. The included-member change is justified by the manual-retry outcome, with R3-F2 on its error writer. |
| `g5.governance.publish.readiness` | `create_or_update` still runs runtime save/push preparation before evidence capture and commits local PR before provider phase | No changed bypass found; no live runtime execution performed. The plan calls this `g5.governance.pr.readiness`, but its actual stable ID is `g5.governance.publish.readiness`; correct that reference during final reconciliation. |
| Current owner / incarnation / target admission | `PullRequestCaller::authorized_db`, typed runtime handle, target re-admission/lock and continuation owner checks remain in place | No authority expansion found in this delta. Existing current-owner/target tests were inspected but not run. |
| Successful-push continuation / bounded progress | `refresh_after_push_with_timeout`, current published-head and provider-pin comparison, maintenance scan and separate failure-record deadline | No new compute/save trigger or starvation path found. Timeout and held-lock tests remain, now asserting outcome messages. Failure persistence is affected by R3-F1. |
| `g5.governance.pr.title` omission/resubmission/concurrency | Trim and reject blank input; store accepted G5 title; compare request title under PR lock; `None` never patches existing provider title | Correct by inspection. Modified explicit-title test covers failure guidance, omitted retry, explicit resubmission, stale request rejection, concurrent requests and included-member success. Runtime execution not performed. |
| Same provider identity / no empty PR | Recorded included member is reconciled, a member with no diff and no recorded provider number is skipped | Success path implements requested behavior; completion/error variant violates retained pin protection in R3-F2. |
| `g5.governance.approval.version` and merge evidence | No approval writes added; automatic refresh retains PR lock and pre-effect provider completion checks | No change granting approval found. Existing merge preflight is a material downstream consumer for R3-F2. |
| Useful existing API/CLI/UI failure surfaces | Outer partial error includes saved PR ID/recovery; provider row errors get Update PR or explicit-title resubmission guidance; CLI text/JSON and three UI consumers expose fields | No dangling removed fields found. UI fixture tests cover display and live replacement; actual continued error visibility is affected by R3-F1. |
| `g5.design.commit-notify` | Failure-event transaction commits before `publish_pr_updated`; successful automatic refresh also commits before publication | Preserved by inspection. No runtime event ordering test was executed here. |
| Ontology accuracy/minimal implementation | Actual diff matches saved current amendment; paired footer entries honestly remain pending | No unjustified new persistence/abstraction. Final evidence/metadata reconciliation remains the main agent's responsibility. |

## Checks and limits

- `git diff --check`: passed.
- `shasum -a 384 -c checksums.sha384` in the migrations directory: all 001–054 passed.
- Removed-field/migration-name search in backend/frontend/docs: no matches.
- Inspected all changed production, test, documentation and ontology files, plus the
  affected local-refresh route, maintenance, provider merge reader/writers and UI/CLI
  consumers. Tests in the delta provide useful intended-path coverage; both findings
  require owner-transition regressions beyond those current cases.
- No heavy/shared DB/build checks, fresh DB creation, native runtime exercise, live
  GitHub access, browser execution or process-restart recovery was performed by this
  reviewer. No passing runtime guarantee is claimed from source inspection.

Initial recommendation: repair the two concrete recovery defects before delivery;
then run the main agent's selected API/provider and UI checks and update scoped
ontology evidence. No production, tests, design or ontology edits were made here.
