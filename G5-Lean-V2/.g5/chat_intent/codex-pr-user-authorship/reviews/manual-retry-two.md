# Independent manual-retry review — reviewer two

Reviewed the frozen uncommitted delta against `4504311be48c72aa19e4ceed49fbc7bd9d679a8b` for PR 268. This is independent read-only discovery; no implementation-session explanation or peer report was consulted. No production, test, design, or ontology files were edited. This report is the initial finding record, saved before sharing.

Inputs: the pinned plan's first section, **Current amendment: manual retry without schema additions**, its ontology diff, the current governing blocks and relevant metadata, the request's **Manual retry without new migrations** section, retained shared-flow/title/automatic-continuation design sections, `ontology-review/SKILL.md`, and the authoring format/design-review guides. The user accepts explicit retry and resubmission of failed title input, but still requires automatic successful-push refresh, useful errors, current admission and preservation of accepted merge pins. Historical durable-pending requirements were treated as superseded.

## Initial findings

### F1 — P1: Included-member manual retry overwrites a completed provider PR's pinned version

**Changed location:** `backend/g5-api-core/src/services/workspace_pull_request_provider_lifecycle.rs:637` (new recorded-PR path), together with the terminal check at 1102 and the existing catch/error writer at 652.

The newly broadened explicit path now calls `create_or_update_one` for an existing provider PR even when `repo_needs_provider_pr` is false. If that PR has merged or closed, `create_or_update_one` returns the new “preserve its merge version” error. The explicit caller catches that error and calls `record_provider_status` with the *current candidate*. Its upsert at 2192–2204 replaces `source_head_sha` and `target_head_sha`, clears observed provider heads when `pull` is absent, and changes the row to `failed`. Thus the refusal itself destroys the version it says it preserves. The previous omitted-title explicit path skipped included members and left these pins alone.

**Counterexample:** A provider member was accepted at A while the G5 PR remains open for import or other members. Its source subsequently advances to B, and its target also contains B, so this member is included while a second member remains outgoing. Click Update PR with no title. The new path gets the recorded PR, observes merged A, then persists failed status with source pin B. `preflight_one_provider_pr_for_merge` checks `row.source_head_sha == pull.head.sha` *before* accepting `pull.merged`; future recovery now refuses A as different from the pinned B. Repeating Update PR does not restore A.

**Obligations:** `g5.governance.pr.follow-push` and the amendment's unchanged accepted-merge protections; `g5.governance.merge`, `g5.governance.merge.effects`, and `g5.architecture.recovery`.

**Suggested repair:** distinguish an observed completed recorded PR from ordinary publication failure and retain its original pins/identity through both evidence and provider phases. Reuse the existing status-only writer for failures on a recorded completed member, or refuse the update before the pin-changing owner runs; avoid admitting a source push as part of reconciliation after completion is known.

**Regression check:** real public Update PR and VM update, two-member fixture, recorded merged/closed member at A, current included member at B. Assert original provider pins and accepted recovery identity are unchanged, no empty/new PR is created, and a later merge-recovery check can still recognize accepted A. Existing `automatic_pr_refresh_checks_identity_included_members_and_terminal_pins` exercises the automatic path only; the amended title test covers an open included PR only.

**Evidence class:** concrete source trace; runtime reproduction not executed in this reviewer.

### F2 — P2: A local evidence refresh clears an unresolved automatic-publication error

**Changed location:** `backend/g5-api-core/src/services/coordinated_repo_publication.rs:1728–1730` and the duplicated list projection at 1768–1770.

`blocking_message` now uses the message only when the latest event of *any kind* is `provider_failed`. The target/source “Refresh PR” operation in `backend/g5-api-routes/src/routes/workspaces.rs:5540–5578` only captures local evidence; `refresh_pull_request` inserts a `refreshed` event without touching provider synchronization. That event makes the new projection forget an outstanding automatic failure. Provider rows may still say `open` at the old source commit, because automatic failures roll back their provider transaction. The three UI consumers now depend on this projection, so they remove the recovery error and may show ready/open although no provider repair happened.

**Counterexample:** create provider PR at A, successfully push B internally while the external branch/provider is unavailable, let automatic refresh fail and record `provider_failed`, then press the target's Refresh action. Local preflight now captures B and emits `refreshed`; `blockingMessage` becomes null while `providerRepos.sourceHeadSha` remains A and the external failure is unchanged. A later background attempt might report failure again, but the local action is not evidence of recovery and should not erase it.

The use of `created_at DESC, id DESC` also does not provide effect order: the baseline uses transaction-start `now()` and random UUID IDs. Multiple provider events in a transaction tie on time, and transactions waiting for the PR lock can have older timestamps than events already committed. This reinforces that “last arbitrary event” is not authoritative completion evidence.

**Obligations:** `g5.governance.pr.follow-push` failure visibility and honest publication; `g5.architecture.recovery`; the amendment's existing outcome/error surfaces. The design refinement saying successful refresh events supersede failure needs to distinguish local evidence refresh from successful publication.

**Suggested repair:** project unresolved automatic failure until a provider/full-publication completion actually supersedes it, using existing event/outcome facts rather than new pending columns. A local preflight event alone must not settle provider failure. Ensure event ordering is meaningful for whichever events determine this projection.

**Regression check:** provider failure → public local-refresh route → API list and both source dialogs/target list still show recovery guidance → successful explicit Update PR clears it. Add a multi-member/provider-event ordering case if chronology remains part of the projection.

**Evidence class:** concrete source trace; runtime reproduction not executed in this reviewer.

### F3 — P2: Automatic discovery misses a new provider member after local refresh advances its evidence

**Changed location:** `backend/g5-api-core/src/services/workspace_pull_request_create.rs:654–661` and deletion of the evidence-change marker in `coordinated_repo_publication.rs::update_pr_evidence`.

After removal of the pending marker, `needs_push_refresh` checks current published source heads against G5 evidence plus source-pin mismatches among *existing provider rows*. It cannot detect a required provider member with no row. The ordinary local-refresh owner may advance G5 evidence without running provider synchronization, making both checks false. Future periodic scans then keep returning `Ok(false)`.

**Counterexample:** an open multi-member G5 PR has a remote-tracked code member initially identical/included in its target and therefore correctly has no provider row. A successful source push introduces its first outgoing commit, already present on the remote branch. Before the automatic worker applies publication, a user refreshes local PR evidence. G5 heads now equal published heads; other existing provider members have unchanged pins; the newly outgoing member has no row to mismatch. Automatic continuation is skipped forever until another head movement or a manual Update PR. There is no failure/error directing the user to retry, so the accepted manual *failure* recovery tradeoff does not cover this successful-push loss.

**Obligations:** `g5.governance.pr.follow-push` successful pushes automatically synchronize provider PRs; `g5.architecture.recovery.progress`; same current-head/provider-identity reconciliation promised by the amendment.

**Suggested repair:** include missing-but-required provider outcomes in discovery, derived from current preflight/candidate eligibility and existing provider rows, without recreating pending persistence or creating empty PRs. Alternatively ensure the local evidence writer cannot erase the only observable distinction from completed publication.

**Regression check:** open PR with no provider row for an initially included remote member → push its first outgoing commit → public local-refresh route → one maintenance run must create/reuse its provider PR and record current pins. Assert an unchanged included member without a row still never creates an empty PR.

**Evidence class:** concrete source trace; runtime reproduction not executed in this reviewer.

## Obligation coverage and inspected evidence

| Obligation | Actual path and discriminating case | Result |
| --- | --- | --- |
| Remove 055/056 and all pending consumers | Migration deletions/checksum diff; repository search for all three snake-case fields, two public camel-case fields, and both migration filenames across backend/frontend/docs | No residual live reference found. Main 001–054 unchanged in the reviewed diff. Fresh database initialization not executed here. |
| `g5.governance.pr.write`, `g5.architecture.pr_flow` | Shared `create_or_update`, typed callers, source preparation, local commit, provider phase; optional title passed only through the same lifecycle owner | No second flow or new persistence introduced. Public/VM execution not rerun here. |
| `g5.governance.publish.readiness` | `prepare_pull_request_source` precedes evidence; source publication precedes local commit; provider failure is partial after commit | No new finding. Note the plan calls this `g5.governance.pr.readiness`; the actual stable ID is `g5.governance.publish.readiness`. |
| `g5.security.admission`, target/owner/incarnation restrictions | `authorized_db`, `source_db_for_target`, `lock_target_access`; repeated admission around preparation and provider lock | Existing guards retained; no weakening found in the delta. Runtime race tests not rerun. |
| `g5.governance.pr.title` | Trim/blank rejection, stored G5 title, PR lock plus comparison with request-scoped title, omitted title None; provider failure row gets explicit `--title` retry text | No new title-order/omission defect found. Included open member works by inspection; completed member is F1. Existing test changes cover failure/resubmission/omission/stale title and open included-member pins. |
| `g5.governance.pr.follow-push` | Shared automatic continuation, published refs, existing PR identity, bounded provider verification, source movement final check | Successful ordinary existing-row path retained. Failure visibility F2; missing-row discovery F3. |
| `g5.governance.approval.version`, merge and merge-effects | No approval writes added; automatic completed-provider check retained; manual included-member broadened path feeds pin-replacing writer; merge preflight compares old provider head against row pin | Approval preservation has no new finding; terminal/manual recovery violation F1. |
| `g5.architecture.recovery` | Same open PR upsert and recorded provider ID; partial sync error contains saved PR identity; retries require current input/authority | Identity reuse and explicit title resubmission supported. Recovery gaps F1–F3. |
| `g5.architecture.recovery.progress` | Maintenance scans all open PRs, one dedicated session lock, per-item timeout plus separate failure-record budget | Bounded progress mechanism unchanged; no new starvation finding. F3 makes one obligation undiscoverable. |
| `g5.design.commit-notify` | Automatic success and failure events published after database commit; unchanged UI query/event consumers | No new premature notification found. Existing timeout test now checks user-visible summary and public retry. |
| Provider authorship and credential policy | Existing-PR reconciliation vs new `CreatePullRequest` credential operation; request-scoped title did not alter token policy | No new finding. Included-member full path now also performs existing publication checks; terminal effects addressed in F1. |
| API/CLI/UI compatibility and error surfaces | Summary fields/type removed, CLI supported-field list removed, docs describe supported outcome fields, three UI components and matching tests migrated to blockingMessage | Live consumers consistent; F2 affects the authoritative projection all three consume. |

Inspected the entire 22-file delta, the owning services around changed methods, local-refresh route and provider merge preflight/status writers, maintenance discovery/leadership, baseline event schema, and affected test cases. The deleted migration test tests removed schema and appropriately disappears. No need for new architecture or a replacement queue was found; the repairs can remain within existing owners and evidence.

## Checks and limits

- `git diff --check 4504311be48c72aa19e4ceed49fbc7bd9d679a8b` passed with no output.
- Pending-field/migration-name search in backend/frontend/docs returned no matches.
- No heavy build, shared database check, provider process, or test suite was run, as requested. Test files were inspected as evidence of intended assertions, not counted as executed results.
- No live GitHub, VM/native callback, process restart, or fresh migration execution was verified. The main agent owns those available checks and consolidation.
- Current ontology meaning matches the recorded amendment. Metadata still says simplification verification is pending; after consolidation it should include the concrete remaining gaps or repaired evidence rather than inheriting historical receipts as proof of this snapshot.
