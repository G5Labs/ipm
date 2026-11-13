# Independent commit review three

Initial read-only discovery, saved before sharing findings or reading peer/prior review
reports. Snapshot: `f2b2d9f578d62771e2ceea893dc1a805aa085864` relative to parent
`75b522e7` in `/Users/bvandiver/G5/qa/G5-Lean`. Reviewed 2026-09-30. Source remained
unchanged during discovery; investigative code and mutable resources are confined to
`/private/tmp/g5-pr-commit-three-ui`. No delegation and no production/ontology repair.

Inputs: repository `AGENTS.md`, ontology-review skill and design-review guide, the pinned
`ontology-plan.md` beginning with Ontology changes and governance, saved `request.md`,
actual commit diff, affected governance/architecture/security/principle commitments and
their metadata, implementation and material callers. The plan and metadata contain prior
assessment claims; those are supplied claims, not independently executed evidence. No
peer or prior review report was read. No known-defect hints were supplied.

## Finding F1 — P2: live automatic refresh is masked by the retained create response

**Changed behavior:** the newly added pending/error display in
`frontend/src/components/workspace/CreatePullRequestDialog.tsx:345-350` and
`frontend/src/components/workspace/PrepareToShipModal.tsx:280-281,511-515` depends on a
snapshot that stops following the refreshed query after the user creates or updates a PR
in that open dialog.

**Exact cause and consumers:**

- `CreatePullRequestDialog.tsx:176` unconditionally prefers `createdPullRequest` over
  `existingPullRequest`. `WorkspacePage.tsx:6397-6401` retains the mutation response;
  `WorkspacePage.tsx:10990-10991` passes both the newly fetched existing PR and that old
  response. It only clears the latter on dialog reopening/cancel or destination changes.
- `PrepareToShipModal.tsx:266-267` likewise prefers `createdResult.pullRequest` over the
  live `existingPullRequest`; the real create handler stores that snapshot at line 302.
  Its pending banner, provider failure list, readiness and provider links derive from
  `activePullRequest`. `WorkspacePage.tsx:11058` supplies the refreshed existing PR, but
  it cannot win the precedence while the modal remains open.
- `frontend/src/lib/ws.ts:976-980` invalidates the PR queries after the new server event.
  Consequently the data can be fresh in React Query and reach the component while the
  user still sees the obsolete response. The query invalidation itself is not the bug.

**Trigger and consequence:** create/update a PR in either modal, leave it open, then
push a newer source revision (or let an initially partial provider publication recover).
After the automatic refresh event/refetch, a new failure is invisible if the retained
response was successful; conversely a recovered PR still displays its old pending/error
state if the response was partial. The user is directed to unnecessary manual recovery or
sees apparent publication success despite the current pending failure. This violates
`g5.governance.pr.follow-push`'s visible failure/recovery outcome and the authoritative
consumer obligation of `g5.design.authority`. It is a new automatic-refresh interaction
with pre-existing response-precedence code, not a claim that the precedence lines were
first written in this commit. No unsafe merge was demonstrated; backend merge validation
is a distinct protection.

**Proposed repair:** render the current record for the same PR identity after a query
refresh, keeping the mutation response only as the immediate fallback until newer query
data arrives. Prefer one cache-backed current entity over competing persistent snapshots.
Do not blindly prefer an older pre-mutation `existingPullRequest`, and do not rely solely
on PR `updatedAt` for ordering: failure recording in
`workspace_pull_request_create.rs:414-419` does not update that column. Preserve the
one-time create outcome notice separately if needed.

**Regression evidence:** an isolated Vitest suite imports the real components and uses
the real create hook/MSW HTTP boundary for the Prepare-to-Ship case:

`/private/tmp/g5-pr-commit-three-ui/refresh.test.tsx`

1. Create dialog with retained successful response, then rerender with the same PR's
   newly pending query record: pending banner expected, absent (fails).
2. Create dialog with retained pending response and current completed query record:
   pending banner expected absent, old error remains (fails).
3. Click the real Prepare-to-Ship create button, await the successful mutation result,
   then rerender with a newly pending same-identity query record: pending banner
   expected, absent (fails).
4. Control: same pending query record without a retained create response renders its
   pending message (passes).

Command: `frontend/node_modules/.bin/vitest run --config /private/tmp/g5-pr-commit-three-ui/vite.config.ts`.
Final receipt: 4 tests, 3 failed / 1 passed, no unhandled errors; log at
`/private/tmp/g5-pr-commit-three-ui/result.log`. This is mounted consumer evidence, not
a live WebSocket/deployed-browser test. The harness setup initially needed canonical
`/private/tmp` rather than `/tmp`; a fixture resolver error was corrected before the
final receipt. Neither setup issue is a product finding.

## Commitment ledger

Each row separates source inspection from executed evidence. “No finding” is scoped to
the inspected paths; it is not a blanket production guarantee.

| Commitment | Actual path | Counterexample/check | Result / evidence limit |
|---|---|---|---|
| `g5.governance.pr.read`; `g5.security.read`, `.read.durable`, `.tenant` | `pr.rs::request_path/select_output`; public/internal `/pull-requests/info`; `workspace_pull_request_read.rs` | Workspace UUID vs number/URL; attached repository requirement; ambiguity; current VM adapter; read without reconciliation | Inspected route registration, current-incarnation admission helper use, attached-repository SQL, no-redirect bounded reader and stored list owner. No finding. Live provider/deployed guest not executed. |
| `g5.governance.pr.json` | `pr_json.rs::Selection`, `pr.rs::execute`, `lib.rs::scope_json_flag`, `process.rs` | Bare fields discovery and invalid/empty fields before context/network/mutation; view object/list array; multi-repo null; node ID | Inspected parser, projection, error transport and compiled-test sources. Fields are validated before API construction; GitHub `node_id` is exposed, G5 UUID preserved. No finding. Did not rerun the existing broad CLI suite. |
| `g5.governance.pr.write`; `g5.architecture.pr_flow` | `workspace_pull_request_create.rs::create_or_update`; thin UI/VM routes; CLI Create alias Update | Same preparation/evidence/publication sequence; accidental alternate writer; typed actor vs invented human token | Inspected full orchestration and material calls. No duplicated public/VM sequence found; title-less retries and local/provider transaction boundary are shared. Worker guidance matches this boundary. |
| `g5.security.owner`, `.admission`, `.login.service-identities`, `.tenant.write` | `PullRequestCaller::authorized_db/source_db`; `WorkspaceRuntime::prepare_pull_request_source`; incarnation route matcher | Wrong workspace, replaced VM/epoch, stopped VM, lost owner or access during preparation | Inspected expected-incarnation propagation, runtime lock admission and repeated source/target checks. Continuation separately uses current owner and GitOperation policy. No new defect established. Active guest/native callback not executed. |
| `g5.governance.pr.target` | `targets`, create target resolution, `useWorkspacePullRequestTargets`, `WorkspacePage` | Unreadable/deleted parent, multiple Bases, tied names, no eligible destination, later silent retarget | SQL orders readable parent then same-org live Base by name/ID; choice resolves before preparation and stays pinned. UI consumes server list. No finding; source inspection only. |
| `g5.governance.pr.title` | `coordinated_repo_publication` prepared create/apply; provider lifecycle PR row lock; migration 041 | Blank title, omission, partial failure, latest pending title, existing unchanged member | Inspected durable pending intent and serialized provider consumption; existing included members use title-only synchronization. No finding established. Live PATCH and concurrent titles not rerun. |
| `g5.governance.publish.readiness`, `.publish`; `g5.architecture.policy` | explicit `prepare_pull_request_source` then evidence/publication; automatic PublishedEvidenceCapture | Saving dirty work during automatic continuation; using dirty retained snapshot; failure publishing older head | Explicit path retains save/push and stopped-dirty refusal; automatic path reads published snapshots and excludes VM preparation. Inspected owner calls and preflight modes; no finding. |
| `g5.governance.publish.github-author`, `.publish.actor`; `g5.security.credentials.publication` | `RepositoryCredentials::resolve/resolve_in_scope`; `create_or_update_one` | Bot fallback for new PR; broad credential target; use of wrong actor after transfer | Earlier authorship work treated as context. Refactored in-scope credential path retains membership, URL, binding and capability checks; new PR creation still requests CreatePullRequest and acting user grant. Reads/Git retain separate operations. No new defect established. Live GitHub identity not executed. |
| `g5.governance.pr.follow-push`; `g5.architecture.recovery`, `.recovery.progress` | maintenance worker, `refresh_after_push`, `needs_push_refresh`, evidence/provider owners | Lost events; failure/retry; obsolete hint; source advance; manual external-push policy; closed PR | Inspected open-PR durable inventory, pending and provider-pin mismatch detection, periodic scan, current-head coalescing, bounded provider reads, rechecks and pending error recording. Automatic mode observes external refs instead of pushing. Backend no new defect established; UI violation F1. Real process restart not executed. |
| `g5.governance.approval.version`, `.merge` | published preflight, PR serialization, `ensure_automatic_refresh_allowed`, provider merge/import, local merge application | Approval silently advanced; accepted/unknown provider merge repinned; source moved before merge | No automatic approval writer found. Inspected closed/merged provider observation, PR lock, active merge preflight refusal and downstream exact-cut checks. No new defect established. Existing broader P6 merge gaps are outside this commit. |
| `g5.design.authority`, `.commit-notify` | commit then WorkspacePullRequestUpdated; WS query invalidation; all three PR displays | Correct DB/cached record but stale visible consumer; notification before commit | Server publishes after commit; list consumes query records directly. Dialog snapshots mask current records: F1, 3 failing mounted checks with a passing control. |
| `g5.design.evolution`, `.durability` | migrations 041/042, checksums, summary fields, migration test | Older PR preservation and pending-field defaults | Additive nullable title/error and false-default pending flag; old source/provider mismatch remains discoverable. Populated migration test source inspected (including restricted role and replay). Not independently rerun. |
| `g5.design.minimal-mechanism`, `.change-locality`, `.adapters`, `.enforcement` | shared service, narrow credential API, PR read adapter, CLI transport/presentation, worker profiles | Duplicated orchestration, widened credential semantics, unrelated new mechanism | New mechanisms have concrete title/recovery/read responsibilities; operation-specific policy is retained. Main design is coherent. F1 shows presentation state still needs a single current-record authority. |

## Ontology and design assessment

The commit adds `g5.governance.pr.read/write/follow-push/target/title/json` and
`g5.architecture.pr_flow`, and amends readiness and affected metadata. The plan's saved
diff compares against the branch's earlier `main` base, so it also includes authorship
and settings additions already present in parent `75b522e7`; those were treated as
governing context rather than newly changed features. The actual commit ontology is
consistent with the stated automatic/manual operation split and contains enough meaning
to require the F1 repair without a policy amendment.

After repair, update the scoped metadata evidence for `g5.governance.pr.follow-push` and
`g5.architecture.pr_flow` with the mounted post-create refresh case. Existing claims that
all three PR consumers are covered do not establish this case. No ontology text change
is proposed to justify retaining stale UI state.

## Completion and limits

Independent full-scope code/ontology/architecture inspection complete for this snapshot.
One actionable cause found (F1), affecting two consumers. No repairs applied. No other
new actionable defect established; absence of findings is not proof of exhaustive race
coverage. Existing security/P6/initial-publication debt recorded in governing metadata
was not promoted into this commit's obligations.

Executed only the focused isolated UI reproduction, because it tested an unresolved
consumer boundary. Existing API/CLI/migration tests were inspected, not redundantly
rerun. Live GitHub, deployed worker/native-agent flows, process restart and full browser
WebSocket recovery remain unexecuted in this review. The main reviewer should consolidate
and repair F1, then recheck current-state rendering after both success and failure without
requiring users to close or reopen either dialog.
