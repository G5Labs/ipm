# Target/title/node-ID follow-up — independent review two

2026-09-30. Independent, full-scope, read-only discovery against the frozen checkout
represented by `/tmp/g5-pr-followup-snapshot.tgz`. No implementation history, author's
review explanation, or peer report was read before saving this report. No production,
ontology or test files were changed; no further agents were delegated.

Inputs: `request.md`; the pinned `ontology-plan.md` first section and final target/title/
GitHub-ID amendment; current governance and Git/governance architecture bodies and
metadata; relevant security and system-design principles; ontology-review and its design
guide. The first-section recorded ontology diff matches the follow-up's target/title,
shared-owner and node-ID commitments. Pending footer assessments accurately indicate that
this follow-up has not yet completed review.

## Initial findings (before repair)

### F1 — P2: failed title PATCH discards the known provider PR identity

Path: `backend/g5-api-core/src/services/workspace_pull_request_provider_lifecycle.rs:1048–1052`,
then `:534–545` and `:1867–1878`.

When an existing GitHub PR is found, the new title PATCH can fail (the new integration
test already exercises HTTP 422). The `?` drops the successfully discovered `pull` and
the outer failure recorder receives `pull: None`. Its upsert overwrites
`provider_pr_number`, node ID, URL and observed provider SHAs with NULL even when the
provider row was previously populated. Thus a rejected rename removes the user's link
to the existing PR and leaves subsequent provider merge admission reporting "Provider
PR number is missing" (`:640`, `:1200`). It also loses the identity when this request
first discovers an existing provider PR, so merely COALESCE-ing old columns is not a
complete repair.

Governing: `g5.governance.pr.title`, `g5.governance.pr.write`, and
`g5.architecture.recovery` require explicit partial failure while retaining discoverable
completed publication. This defect is the newly introduced rename error path through an
existing destructive status writer, rather than a claim that all historical callers are
in scope.

Repair: on title-update failure, record failure together with the already discovered
provider identity, preserving observed facts and the actionable error. Do not report the
title as synchronized. Regression: create/adopt provider PR 22, reject its title PATCH,
then assert response and durable provider row still contain number/node ID/URL and error;
also cover first discovery of a pre-existing remote PR when no provider row existed yet.
Current evidence: source-path counterexample; existing 422 test asserts failed status
and local title only, so it does not discriminate this loss.

### F2 — P2: target choices no longer follow workspace-list invalidation

Path: `frontend/src/hooks/useWorkspace.ts:429–443`; consumers in
`frontend/src/pages/WorkspacePage.tsx:6298–6319` and `:6352–6365`.

The UI now caches its choices under `workspace-pull-request-targets`, but no mutation
or WebSocket invalidation references that key. Workspace visibility/deletion changes
and reconnect refreshes continue invalidating `workspaces` (e.g. hook `:224–265`,
`frontend/src/lib/ws.ts:748–755`, `:1305–1328`). Previously those refreshed the list from
which PR choices were derived. With the page mounted and window focused, a newly eligible
Base, removed parent, or revoked private destination leaves the choice/default stale
indefinitely: staleTime does not schedule a fetch, and opening the dialog does not refetch.
The UI can keep no destination while CLI creation succeeds, or submit the obsolete
explicit default while the CLI selects the current default. Backend authority checks
still protect effect admission; this is a UI/default consistency regression.

Governing: `g5.governance.pr.target` and `g5.architecture.pr_flow` (same owner, current
eligible choices consumed by UI).

Repair: make target queries participate in relevant workspace/access invalidation, or
explicitly refresh when opening create/prepare-to-ship and on relevant events; preserve
explicit user selection only while still eligible. Regression: mount the actual target
hook/consumer, change an eligible workspace through a real mutation or notification,
and verify its target request and displayed default update without focus/navigation.
Current evidence: source inspection plus an isolated QueryClient runtime probe confirmed
invalidating `["workspaces"]` sets that list invalidated while the new target key remains
`isInvalidated: false` with the old parent. Existing hook test covers initial ordering only.

### F3 — P2: explicit title skips existing provider PRs for now-unchanged members

Path: `backend/g5-api-core/src/services/workspace_pull_request_provider_lifecycle.rs:479–487`,
`repo_needs_provider_pr` at `:2157`.

The reused provider creation gate discards `Identical`/`SourceAlreadyMerged` members (and
equal heads) before considering the new explicit title. An open multi-repository G5 PR
can already have a provider PR for that member, then its captured changes become included
in the target while other members still require the G5 PR. A subsequent explicit title
updates the G5 row and changed members but silently leaves that recorded provider PR's
title unchanged. The contract and both worker guides promise an explicit title for each
provider PR, including existing PRs. The gate is appropriate for deciding whether to
create a new empty provider PR, but is insufficient for metadata updates to an existing
one.

Repair: separate existing-provider title synchronization eligibility from new-PR
creation eligibility inside the same provider owner; retain the no-empty-PR rule and
source/target authority. Regression: establish an existing provider row for a member,
advance its target to include its source while another member still differs, then title
update and assert the existing provider is patched without creating a new provider PR
for an unchanged member. Current evidence: source counterexample, not executed against
the API or live GitHub; validate the modeled member transition in consolidation.

## Commitment ledger

| Commitment | Actual path and discriminating case | Result / evidence |
|---|---|---|
| `pr.target` | `workspace_pull_request_create::targets`: same-org/live/source-excluding SQL, parent priority, name/ID order, per-candidate `user_can_access_workspace`; omitted target resolved once before preparation | Backend inspected; saved 32-test API run includes ordering/private/deleted/foreign/exclusion/no-default cases. UI invalidation violates ongoing agreement (F2). |
| `pr.title` | Trim/blank refusal before source preparation; optional title carried into prepared create and both reuse branches, committed G5 row; POST and PATCH provider payloads | Happy-path, omission and failure inspected; saved API test run covers persisted G5 title and PATCH request. F1/F3 remain. |
| `pr.json` | REST `node_id` -> API `id`; common CLI field catalog and ordinary selected-value projection | Inspected correct; missing/nonstring upstream ID becomes null, never number. Saved CLI run covers numeric and URL selectors. Independently ran all five `pr_json` unit/parser tests: pass. |
| `pr.write`, `architecture.pr_flow`, adapters/minimal mechanism | User/VM routes enter `create_or_update`; CLI `update` is create alias; UI consumes narrow target projection | Preserved by inspection. No separate save/push/provider sequence added to adapters. Title flag added to shared request, UI need not gain title input. |
| `security.owner`, `admission`, service identities, read, tenant | Typed caller source DB; target fixed once; authorized DB rechecks before preparation/application/provider phases; scoped targets consult existing access owner | Follow-up preserves checks. Saved API suite includes user/VM refusals and access loss during save. Source restrictions and explicit target policy unchanged. No new authority defect found. |
| `publish.readiness`, `publish`, publication credentials/authorship | Save/push before evidence; current-incarnation fencing retained; credentials remain server-side; PATCH uses existing write-PR policy, new POST uses acting user's grant | Inspected reuse policy, no credential expansion found. Saved API suite covers source/provider failure and reuse; actual native guest and live GitHub are unexecuted. |
| recovery / authority / current facts | G5 commit precedes provider phase; incomplete outer failures carry PR ID and phase; per-repository status remains partial | Existing reporting retained, but known provider identity erased by new PATCH failure (F1). |
| CLI/docs compatibility | Both profiles and `docs/g5d-pr.md` agree on optional destination/title, node ID and partial publication | Checked. Read selection, JSON discovery/invalid input remain before transport; no unrelated flags changed by this follow-up. |

## Checks and limits

- Independent execution: existing compiled `target/debug/deps/g5d_cli-923c3e826392c280
  pr_json --nocapture`: 5 passed, 0 failed. No rebuild or shared test database used.
- Independent QueryClient probe: target old-parent remains cached and uninvalidated
  after workspace-list invalidation; list itself becomes invalidated. This validates key
  behavior, not a mounted UI regression test.
- Inspected prior logs: `/tmp/g5-pr-followup-api-recheck.log` reports 32 API tests passed;
  `cli.log` reports 12 compiled PR tests passed; `ui.log` reports 69 tests / 4 files passed;
  `check.log` reports successful cargo check. TypeScript log is empty, so its success
  requires the invoking command's exit receipt rather than the empty file alone.
- `/tmp/g5-pr-followup-units.log` reports 75 pass / 2 fail, with host image-preparation
  tests failing to bind wiremock sockets under sandboxing. This is an environment-limited
  run, not a green full-unit-suite claim. Focused PR JSON units independently passed.
- No new API execution: discovery avoids shared mutable fixtures and broad rebuilds.
  F1 and F3 require the proposed runtime regressions; UI transition coverage is missing.
  Live GitHub, provider post-PATCH title observation, and deployed guest execution remain
  unverified. The existing PATCH mock returns the same fixed PR object and records the
  expected request; it proves transmitted title rather than authoritative GitHub state.

Implementation review is complete for this assigned follow-up; the three findings are
unrepaired proposals for main-agent consolidation. No governing-meaning amendment is
needed to fix them. After consolidation/recheck, update pending target/title, JSON and
shared-flow footer assessments with actual evidence and retain unexecuted limits.
