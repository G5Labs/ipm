# Workspace inspection and model bindings

> Historical investigation: model counts, TLC outcomes and reproduction commands below
> describe [commit `38046b28`](https://github.com/G5Labs/G5-Lean/tree/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology). On October 15, 2026, the user requested
> removal of all TLA campaign artifacts from this branch. Source findings and unresolved
> gaps remain; the archived model evidence is not a current executable test suite.

Application baseline `20e393c0`; source read directly in this checkout. Models are
finite abstractions, not extracted programs or runtime reproductions. No application
code or tests were changed. Two users, two incarnations and two repositories exercise
independent authority and vector dimensions; arbitrary cardinalities are not proved.

## Direct inspection

Paths below are relative to `backend/`.

| Source | Directly read ranges | Behavioral evidence |
| --- | --- | --- |
| `g5-api-core/src/services/identity/org_service.rs` | 235–472, 810–977, 1033–1155 | Organization/docs/Base metadata in transaction; active owner eligibility on role/removal; explicit invite accept locks. |
| `g5-api/src/db/migrations/003_p4_1_runtime_schema.sql` | 115–180 | Deferred `(org, active owner)` FK targets eligible memberships; commit protects concurrent transfer/removal. |
| `g5-api-core/src/services/identity/workspace_mutation_policy.rs` | 150–270, 304–399 | Readonly/freezing origins, owner/pending guards; review workspace exclusions. |
| `g5-api-core/src/services/identity/workspace_incarnation.rs` | 1–120 | Current org/owner/VM/epoch/lease lookup; reread after lock acquisition. |
| `g5-api-core/src/services/workspace/workspace_service.rs` | 1980–2040, 2990–3245 | Explicit owner-moved phase; deletion refuses live children and unfinished operations, requires stopped VM and removes refs before tombstone transaction. |
| `g5-api-routes/src/routes/workspaces.rs` | 2080–2205, 3295–3340, 4250–4285 | Owner updates; delete stops target; review merge refusal. |
| `g5-api-core/src/services/workspace/workspace_vm_service.rs` | 1321–1680 | Idempotent transfer admission; mark pending, stop old VM, revalidate owner/target membership, no active VM, then owner change. |
| `g5-api-core/src/services/workspace/workspace_runtime.rs` | 1–130, 266–317, 390–625 | Owner handle; exact runtime authorization and lock held through regular RPC. Lifecycle open/close/flush deliberately release fence before RPC and use callback revalidation. |
| `g5-api-core/src/services/workspace/workspace_file_save.rs` | 1–410 | Observed incarnation and file-hash compare; same-content conflict retry; exact selected-file checkpoint, push exact saved head; applied/published outcomes separate. |
| `g5-api-core/src/services/workspace/workspace_review_service.rs` | 1–180 | Review is durable fork with findings/suggestions; owner applies suggestions to live files. |
| `g5-api-core/src/services/workspace/workspace_fork_service.rs` | 1–170, 583–850, 1006–1465, 1487–1657, 1690–2070 | Admission digest and whole source vector; live flush before refs; replay serialization; create-only proof refs; publish after all proofs; cleanup matches receipt. |
| `g5-scm/src/repo.rs` | 340–415, 616–695 | Atomic branch+private proof creation; equal branch alone is not ownership; cleanup refuses changed/foreign refs. |

Other directly read cross-domain snippets: `hosts/workspace_vm_release.rs` 1–160,
350–405 and `g5d-isolation/src/isolation/mod.rs` 1110–1205; these inform runtime
questions but are not counted as complete lifecycle/isolation inspection.

## Model bindings

[Executed cases](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/workspace/results.json) retain
input SHA256, JVM, named expected counterexamples and state counts; adjacent logs
contain complete traces. `python3 .g5/TLA_verification/ben-ontology/workspace/run.py`
reruns them with the pinned jar documented in the campaign README.

- **WorkspaceAuthority**: `BeginTransfer/Stop/CommitTransfer/Resume` bind to
  `workspace_vm_service.rs`1321–1680. `Capture/Apply` bind to runtime handle creation
  and admission390–625. `Apply` is atomic at the held DB fence; it does not assert
  remote rollback after an indeterminate reply. Independent owner, tenant, epoch,
  transfer-pending and stop-before-owner-change controls each violate `SafeAuthority`.
  `NoNewOwnerCompletion` intentionally fails on a valid transfer/resume/new-owner call.
  Foreign-tenant and nonowner records abstract untrusted request input; they are not
  claims that the normal handle constructor emits such requests. Physical VM kill,
  external credentials, wall-clock lease drift and lifecycle callback exceptions are
  outside this model and are covered/qualified in the runtime investigation.
- **ForkManifest**: admission freezes the complete vector; member creation and proof
  verification bind to fork583–850,1487–1657,1748–2070 and scm340–415,616–695.
  Parent edits can happen between repo materializations. `CrashRetry` represents
  reentering with the same persisted operation, not a durable-disk simulation.
  `ExactPublishedCut`, `StableMaterialization`, `ForeignRefsPreserved` have independent
  partial-publication, recapture and unsafe-cleanup controls. Completion witness
  requires crash/retry and successful publication. `EventuallyPublished` passes under weak fairness of member creation and publication, with abort/conflict excluded and finite parent changes; it does not promise recovery from permanent external failure. Live capture is abstracted at its
  completed immutable manifest boundary; flush construction has a separate Git model.
- **WorkspaceProducts**: authorized same-owner stopped child, no running operations;
  intended delete cascades, as-built refuses. Review merge likewise intended versus
  explicit route refusal. `DeleteSupported` and `ReviewMergeSupported` fail in the
  as-built comparator; `NoOrphans` passes for both and fails in the orphan control.
  This checks acceptance/refusal correspondence, not Git merge correctness or an
  implementation of cascade. Valid intended completion is demonstrated, not shipped.

## Dispositions

- **WORKSPACE-CASCADE**: Ben `g5.workspace.delete` requires cascade. Service3018
  rejects all children; tombstone3190 repeats no-child condition. This is a code gap
  against the supplied contract. Additional confirmation and undo retention remain
  distinct clauses; the model does not claim those implemented.
- **WORKSPACE-REVIEW-MERGE**: Ben `g5.workspace.review` permits optional merge;
  mutation policy and route4250 reject it. Preserve intent; do not turn the guard
  into an inferred prohibition. Durable review storage also qualifies “ephemeral”:
  lifecycle/durability interpretation needs a visible question, not data deletion.
- No claim of owner-membership race: the eligible-membership foreign key protects the
  commit boundary in addition to service preflight checks.
- No stage-gating defect: source says progression *may* be gated; code currently
  permits explicit owner movement. Approval governance has its own narrower owner.
- No “dirty fork unimplemented” claim: live mode is implemented and owner-restricted;
  durable selection can use a current complete flush or the current branch vector.
  The old source comment alone cannot establish current implementation absence.

## Step 5, iteration 1: independent reconstruction

Route assembly and service/worker discovery identified documents, anchored discussion,
intent traceability, browser draft recovery, self-hosted distribution, platform reports,
shared rate limits and telemetry as consequential surfaces missing from the initial
human statements. These are new agent contracts, not additional human requirements.
An implementation obeying only “Project Docs folder” could omit retained view references,
line discussion, recoverable indexes or drafts and still satisfy that sentence. The
actual product source gives those behaviors distinct owners and acceptance paths.

Additional direct reads, repository-relative:

| Source | Direct read ranges | Evidence / disposition |
| --- | --- | --- |
| `backend/g5-api-routes/src/routes/mod.rs` | 1–200 | Independent public capability inventory; module names alone not behavioral coverage. |
| `backend/g5-api-routes/src/routes/docs.rs` | 1–160 | Retained views/project docs and explicit retired routes; canonical repository handles. |
| `backend/g5-api-routes/src/routes/ontology.rs` | 1–165 | Ontology tree/node operations and exact source metadata. |
| `backend/g5-api-routes/src/routes/files.rs` | 1–120 | Compatibility aliases delegate to owner-aware file facade; no new write authority. |
| `backend/g5-api-core/src/services/workspace/doc_service.rs` | 4168–4265, 4811–5044, 5132–5298 | Typed file identity, view/source selection, live owner overlay and committed fallback labels; projections not canonical bytes. |
| `backend/g5-api-core/src/services/workspace/doc_reference_service.rs` | 1–230, 260–390 | Complete captured view validation and reference resolution; legacy UUID enrichment not content authority. |
| `backend/g5-api-core/src/services/workspace/projection_convergence.rs` | 1–340 | Polling plus hints, immutable source OID, transaction-bound data/checkpoint, missing-mirror distinction, current docs/ontology consumers. |
| `backend/g5-api-core/src/services/workspace/comment_service.rs` | 1–190, 423–525, 627–805, 895–961 | Anchored thread identity, owner/author/read-actor policy and commit callbacks. |
| `backend/g5-api-routes/src/routes/grounding.rs` | 1–190, 408–780, 1227–1502 | Captured authorized read sets; original chat context versus extracted source; stale block semantics; old review-write route delegates to retired mutation. |
| `backend/g5-api-routes/src/routes/intent_log.rs` | 1–70 | Read-only cut projection with malformed-source diagnostics. |
| `frontend/src/hooks/useFiles.ts` | 343–441 | Submitted-content cache update, current-VM check and conflicting-read invalidation. |
| `frontend/src/hooks/useDocs.ts` | 177–246 | Saved doc cache scoped to observed VM; source implementation evidence only. |
| `frontend/src/components/editor/CodeEditor.tsx` | 780–935, 1038–1159 | Editing-session generation, saved-byte baseline and preserved later typing. |
| `frontend/src/components/files/fileDrafts.ts` | 1–195 | Person/workspace/repo/path key, matching draft-ID update, principal clearing. |
| `backend/g5-api-routes/src/routes/self_hosted.rs` | 1–88 | Separate configured admin and license-token authority; no tenant grant. |
| `backend/g5-api-core/src/services/identity/self_hosted_license_service.rs` | 1–210, 239–425, 503–550 | License key hash, active/expiry/product/version admission and fixed registry. |
| `backend/g5-api-routes/src/routes/platform_issues.rs` | 1–165, 185–437 | Human and incarnation-attributed admission, shared throttle, content bounds, PNG validation and bounded decode concurrency. |
| `backend/g5-api-core/src/services/identity/platform_issue_intake.rs` | 35–110, 150–560 | Durable intake, claim/reclaim, marker reconciliation, screenshot fallback, external-send fence and terminal tombstone. |
| `backend/g5-api-core/src/services/identity/platform_issue_outbox.rs` | 1–58 | Sequential batch worker per instance, no global leader in this owner. |
| `backend/g5-api-core/src/maintenance/mod.rs` | 248–276 | Report worker spawn without leadership. |
| `backend/g5-api-core/src/services/identity/platform_issue_github.rs` | 55–180 | Fixed destination/client grant; marker and create delegation. |
| `backend/g5-scm/src/providers/transport.rs` | 40–95 | 30-second request bound, no automatic write retries, same-origin credentials. |
| `backend/g5-api-core/src/middleware/rate_limit.rs` | 255–455 | Lock all keys before one database clock sample; rollback pair on refusal; trim skips held rows. |
| `backend/g5-api-routes/src/routes/telemetry.rs` | 1–90, 100–280 | Session-derived identity, client provenance, whole-batch validation before logging. |
| `backend/g5-api-core/src/services/identity/spend_service.rs` | 1–100, 288–356, 564–671 | Retained accounting/limit helper; repository search finds only tests calling check_spend_limit/record_usage. No production hard spending cap is inferred from stale header comments. |
| `backend/g5-api-routes/src/routes/compute.rs` | 1–120 | Member-visible running inventory filters by workspace read access; no independent authority granted by UI layout. |
| `docs/research/projection-convergence.md` | 1–60 | Historical source for recoverable projections. Its old search consumer is retired in current implementation; do not reconstruct it as current product intent. |
| `docs/features.md` | 1–10 | No authorized broad retained-feature removal list; dormant/retired surfaces require individual evidence. |

History inspected for discovery: latest 85 first-parent commit subjects, and path
history for file drafts, comments, projection convergence and report intake. Subject
lists are navigation evidence only. The merged `16c87e13` Redis-removal change confirms
shared PostgreSQL rate limits; the current implementation is the binding, not a claim
that a commit subject supplied human intent. Current source explicitly retires search
projection and node-thread creation, preserves old threads/views, and refuses the old
Project Doc ledger draft-write mechanism with “attached workspace file API” conflict.
No dormant route is promoted to a working product promise from its old header alone.

## Additional executed obligations

Eleven new models add 60 checks to the workspace campaign (79 retained executions total).
Every control requires its exact named invariant failure; source-preservation and artifact
validation independently check that logs match the current inputs.

| Model | Code-bound guarantee and meaningful completion | Explicit limit |
| --- | --- | --- |
| SaveAcknowledgment | Only submitted bytes become the save baseline; later edits and a replacement draft survive. Separate submitted-byte/session/overwrite controls and two witnesses. | One pending save, two edit generations; same-key overlapping independent saves are not universally ordered by this model. Backend incarnation and expected-hash admission is a separate owner. |
| DocumentReference | Committed document and referenced content use the captured vector; a lagging projection cannot provide canonical content. Independent recapture/projection controls and successful read. | Two repos/two Git revisions; live OS snapshot atomicity, YAML parsing, duplicate selectors and malformed input handling are inspected/qualified, not proved. |
| ProjectionConvergence | Data and source checkpoint match, unavailable mirror cannot trigger deletion; finite source changes converge under fair successful polling without relying on hints. | Projection OID labels input, not a permanent claim of newest Git head. External writes can move immediately after validation. No cloud storage durability proof. |
| CommentAuthority | Create/resolve/delete owner, edit author, reply reader. One control for each action and legal reader reply. | Identity/anchor representation and commit-notify are separate inspected/passed owners. Does not prove the complete RLS schema or reanchoring algorithm. |
| LicenseAdmission | Active/unexpired license and product/release entitlement at token admission; independent controls and valid issuance. | Does not promise physical image isolation, secret cryptography, provider IAM or revocation of an already minted token. |
| ReportDelivery | Durable create fence, marker adoption and same-submission competing workers. Intended fresh-claim fence passes, actual unconditional fence can duplicate external creation. | Two generations/one report, one expired claim; marker visibility can lag. Intended comparator is not implemented. See gap below. |
| SharedQuota | Each successful paired admission charges both dimensions; rejected pair charges neither; independent user/global limits. | SQL lock/transaction boundary is an inspected premise. One finite window; clock rollover and distributed PostgreSQL correctness not proved. |
| TelemetryAdmission | Only browser session identity, explicit client provenance, complete valid batch before emission; each guard challenged independently. | Two events; validator predicates abstract actual field parsing and payload bounds. Log delivery/storage is not reliable message delivery. |

### PLATFORM-REPORT-CLAIM

**Code-bound concurrency defect in recovered report-delivery intent.** Claim selection
uses `FOR UPDATE SKIP LOCKED`, but commits before network work; processing rows become
reclaimable after ten minutes. Each claimant holds a copied create-fence value. If an
old worker is delayed before its create fence, a new claimant can also read no fence
and no visible provider marker. `mark_create_started` uses unconditional ID-only
`COALESCE` update, checks neither current attempt nor affected rows, and both workers
can then POST. A completed/deleted row also cannot stop the older copied worker there.
The worker is spawned per maintenance instance without global leadership. Sequential
batches and process scheduling mean a 30-second per-HTTP-call timeout is not an
ownership fence. The Git auditor independently read the same claim, fence, loop and
spawn owners and found no protecting gate.

ReportDelivery-asbuilt violates `AtMostOneExternalIssue`; the intended fresh atomic
claim/create-fence comparator allows marker recovery and passes. This is not a runtime
reproduction and does not assert a specific observed production duplicate. Proposed
reproduction: pause claimant A before its fence, age/reclaim row, let B snapshot the
same unfenced row and empty marker, resume both, and count actual provider POSTs. Also
exercise normal send, ambiguous accepted response with delayed marker, and stale A after
B completes/deletes the outbox row. Preserve truthful accepted-versus-created statuses.


## Workbench and guided outcomes

The route and implementation reads below fill a separately discovered UI capability,
not new wording attributed to Ben. Workbench actions/personas load from one committed
revision. Empty named phases replace builtin action lists, matching IDs inherit only
omitted behavior, and any invalid file produces a whole fallback plus visible error.
Chat admission checks that revision and the clicked phase (IDs can repeat across phases).
Guided work uses actual files/PR outcomes or latest explicit marks. Starting chat does
not complete a step; navigation cannot retarget an in-flight mark. Home attention reads
authorized owners' questions and addressed approvals; it does not create a second
state owner. Saved explainer templates are catalogued from committed files; editing
writes live configuration and is not itself a promise of Git publication. Its old
route header says commit, but the actual service returns the existing branch head.

| Model | Binding and executed checks | Limits |
| --- | --- | --- |
| WorkbenchConfig / 6 | Committed config loader and chat revision/phase gate; invalid whole fallback, intentional empty phase, revision and phase controls, admitted action witness. | One overridden phase, two revisions; YAML parser and every action template are directly inspected/structural, not proved. |
| JourneyEvidence / 4 | Captured workspace mutation versus navigation and explicit evidence versus activity; independent controls and completion witness. | One pending mark/two workspaces; latest-event ordering and all document/PR evaluators are inspected, not fully modeled. |

Direct source ranges added:

- `backend/g5-api-routes/src/routes/workbench.rs`: 1–65.
- `backend/g5-api-core/src/services/agents/workbench_config.rs`: 1–230, 437–520.
- `backend/g5-api-core/src/services/agents/workbench_builtins/actions.yaml`: 1–170.
- `frontend/src/hooks/useJourney.ts`: 1–130.
- `frontend/src/lib/journeySteps.ts`: 1–295.
- `frontend/src/pages/WorkspacePage.tsx`: 6550–6705.
- `backend/g5-api-routes/src/routes/home.rs`: 1–115.
- `backend/g5-api-core/src/services/workspace/home_service.rs`: 158–350.
- `backend/g5-api-routes/src/routes/explainer_templates.rs`: 1–150.
- `backend/g5-api-core/src/services/workspace/workspace_service.rs`: 2074–2168, 8386–8605.
- `backend/g5-api-routes/src/routes/chat.rs`: 2535–2635.

No application tests ran. Model authoring errors were corrected and affected cases rerun; parse/type errors were not accepted as negative controls.


## Security and programmatic authority

`ApiKeyAdmission` adds eight executed cases: valid admission, independent revoked,
expired, organization, token-ceiling, current-permission and membership controls,
and legitimate-use witness. The model separates credential authentication from later
operation admission and permits an already authenticated request to proceed after key
revocation, while requiring current target authority. Two organizations and Boolean
scope/membership states expose each dimension; this is not proof of every public route
registration or the PostgreSQL RLS schema. Scoped transactions acquire current workspace
owner/shared authority and reject pending transfer; WorkspaceAuthority covers that
logical boundary. System transactions still require explicit typed owner checks.

Recoverable config/task/SCM secrets use the inspected AES-GCM manager with random nonce
and authenticated decode; unavailable encryption configuration fails storage. API keys
instead use one-way hashes and a once-returned plaintext. Configuration views derive
only bounded hints from decrypted values, and explicit execution channels own plaintext.
Cryptographic strength, deployment key management, full tenant table inventory and
secret leakage across all consumers remain security qualification, not TLC conclusions.
No secret values were fetched or printed in this audit.

The dispatcher redactor scans configured values across byte/structured-event boundaries,
retains unresolved prefixes, merges overlapping spans, and sanitizes keys as well as
values. Structured-output timeout masks substantial prefixes and retains continuation
chains; I/O errors do not masquerade as EOF. Settlement is sanitized once and that exact
result feeds journal, status and replay; redaction need not be idempotent when a secret
itself overlaps the mask text. These directly inspected algorithms support the existing
redaction block; no new intent is needed merely to restate it. Arbitrary Unicode,
all secret combinations and every native private-request channel are not formally
verified here. The module-lifecycle auditor contributes retained-output ordering.

Additional direct ranges:

- `backend/g5-api-core/src/services/identity/api_key_service.rs`: 1–325.
- `backend/g5-api-routes/src/routes/api_keys.rs`: 44–114.
- `backend/g5-api-core/src/middleware/auth_middleware.rs`: 620–700, 876–916.
- `backend/g5-api-core/src/db/scoped.rs`: 1–195, 419–560.
- `backend/g5-authz/src/scoped_db.rs`: 1–160.
- `backend/g5-api-core/src/auth/scm.rs`: 220–321.
- `backend/g5-api-core/src/services/agents/config_service.rs`: 230–275, 285–358, 1048–1145.
- `backend/g5d-dispatch/src/modules/redact.rs`: 1–230, 250–430.
- `backend/g5d-foundation/src/secret_redaction.rs`: 1–100.
- `backend/g5d-dispatch/src/modules/mod.rs`: 780–909.
- `backend/g5d-dispatch/src/modules/runner.rs`: 810–900, 940–1075.


## Iteration 2 — independent review and selected Save

The fresh-context [workspace/Git review](../reviews/workspace-git.md) found a
stopped-read contradiction, an overbroad use of “Save” for VM flush receipts, and a
missing equal-content foreign-ownership state in ForkManifest. The original 285 bodies
are unchanged. The new saved-read D now preserves Ben's public-read authority and the
flush D names its actual VM receipt scope; the dirty stopped read is GIT-GAP-5 in the
Git investigation. A new feature-owned selected Save D fills the missing contract.

ForkManifest now admits a foreign branch at the exact captured OID with no operation
proof. Its independent proof-only control preserves content equality but removes the
proof predicate and fails ForeignRefsPreserved. The safe case protects that branch;
existing exact-cut, recapture, cleanup, retry/completion and fair-operation checks
remain valid. Foreign collision is excluded from the fairness premise that promises
completion, not silently assumed recoverable as this operation's branch.

**WORKSPACE-SAVE-BLOB — staged content can differ from the checked Save bytes.**
`workspace_file_save.rs:268–321` promises only the written selected blob and separates
Applied from Published. Native `checkpoint.rs:143–166` first runs `git add` into its
private index, then separately hashes the live file with `--no-filters`. The expected
hash is compared with that later live observation, while `write-tree/commit-tree` at
249–255 use the earlier staged blob. For an ordinary no-filter file: Save writes A;
a native writer changes it to B; staging captures B; the native writer restores A;
the live hash matches A; the private tree commits B. The saved response still names A.
The ordinary index lock is acquired later and does not exclude shell/editor writes.
The fresh reviewer independently corroborated this schedule. This is source-bound
concurrency evidence, not an executed runtime reproduction or a proposed code repair.

SelectedSave models two native edits, separate Stage and Check, a competing branch
head, selected-only publication and preserved unrelated index state. The as-built
variant fails ExactSavedBlob on the A→B→A schedule. A hypothetical same-capture check
passes, with separate path-scope, index-preservation and ref-CAS controls, conflict and
published-completion witnesses. This comparator is not implemented and does not
prescribe a second storage layer. Git filters are abstracted to identity for this
counterexample; that ordinary supported case is sufficient. No all-writer file CAS,
Git library implementation, clean-filter correctness or crash durability proof follows.
The model begins after the live write; file admission is FileMutationCAS and editor
acknowledgment is SaveAcknowledgment. Selected publication re-resolves current owner
handles between commands, so a single precheck is not treated as a retained VM lock.

Proposed runtime reproduction: pause after private-index staging of B, restore A before
raw hash verification, finish Save, then compare reported hash with actual committed
blob and preserve an unrelated staged edit. Also exercise changed-head refusal, equal
retry after failed push, VM replacement between publication calls and ordinary success.
No runtime tests were added/run under uplift.

New directly inspected ranges:

- `backend/g5-api-core/src/services/workspace/workspace_file_save.rs`: 1–433.
- `backend/g5-scm/src/native/member/checkpoint.rs`: 109–273, 296–398.

Focused review also challenged publication ownership, not only cleanup: ForkManifest
now asserts PublishedRefsOwned and separately bypasses only the Publish proof check.
An equal-content foreign branch reaches that control's counterexample; exact content
alone cannot establish operation ownership. SelectedSave's retry witness now requires
a recorded failed push before successful publication, distinct from ordinary completion.
All affected configurations were refreshed after these changes.


## Full review at cbc71212

The [independent full review](../reviews/workspace-git-full.md) replaces broad assurance
readings of the earlier summaries with explicit per-model contracts, direct source links,
sensitivity gaps and repair comparisons. All 15 original workspace modules were read
and rerun. The campaign now has **19 modules / 112 current configurations**, all freshly
executed in this review with exact expected outcomes, input hashes, command and UTC time.
The source manifest's 62 file fingerprints match the current source; this check does not
count as rereading every historical range.

`ProjectionConvergence` was materially corrected. At
[projection_convergence.rs255–281](../../../../backend/g5-api-core/src/services/workspace/projection_convergence.rs#L255-L281),
clean paths may commit while protected paths prevent checkpoint advancement. The new
two-path model admits this partial progress and checks `ConsistentCheckpoint` as a last
**complete** source version, not equality with every current path. Protected-advance and
torn controls fail; partial-progress and completed-progress witnesses are reachable.
Finite fair convergence assumes protection eventually releases and the mirror stays
available. No arbitrary Git-history ordering or all-writer refinement is claimed.

`DirtyFork` fills the omitted capture/inheritance concern: current code offers Live,
Durable and Branch sources; Live requires source ownership, while a reader may use an
eligible durable receipt. The model binds restore to frozen bytes, checks owner/stale-cut/
recapture controls, and reaches both dirty inheritance after later editing and nonowner
Live refusal. Ben's c2 unimplemented comment is historical relative to this implementation;
`ForkManifest` still does not prove native dirty-byte capture.

`CascadeRepair`, `ReviewMergeReuse` and `ReportClaim` compare reuse of existing owners
under late forks/restarts, split source/target authority, and reclaimed report workers.
Their concrete designs and implementation conditions are in the full review. A one-time
report create-fence CAS alone prevents repeated sends but does not protect later row
mutations by stale workers; every transition needs the current attempt and affected-row
evidence. Cascading uses a complete final locked tree recheck and durable logical removal
before resumable physical cleanup; repeatedly calling the existing per-leaf operation
does not itself establish that protocol. No application implementation or runtime test
was performed. Parse/action-completeness errors during construction were corrected and
all affected cases rerun; no such error was accepted as a negative control.

Cascade cleanup has a concrete existing reuse guarantee: the
[branch-name ownership query](../../../../backend/g5-api-core/src/services/workspace/workspace_fork_service.rs#L2793-L2815)
includes workspace mappings without filtering tombstones. Retaining mappings after
logical removal reserves the old names until ref cleanup succeeds. Cleanup must reacquire
the repo locks and skip work once those mappings are absent; remove mappings only after
ref cleanup. This avoids a new cleanup ledger and avoids replay against a newly reused
branch name. Shared locking by all competing writers remains an implementation condition.
