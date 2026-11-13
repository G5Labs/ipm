# V9 design review — responses and amendments

Current disposition revision: September 18, 2026, after the explicit MVP scope reduction. The original [review](lean-backend-design-v9-review.md) of `33154ac` is preserved unchanged, including all 125 finding headings below. See the [final scope and review reconciliation](v9-final-scope-and-review-reconciliation.md) and [data-model response](lean-backend-design-v9-datamodel-review-response.md) for the selected current contracts.

The earlier evaluation counted 71 agreements and 54 partial agreements. Those historical counts are not a current closure score: result-product findings are now deferred with that feature, and retained findings have more specific remedies. Every item below records its current disposition. The later approved fork simplification removes the separate reservation table; unfinished `workspace_forks` rows retain durable member/name claims while Git proof refs remain separate application evidence. **A selected design remedy is documentation and planned implementation work, not an implemented, runtime-validated, deployed or observed fix.** Deferred items have no MVP implementation or acceptance promise.

The retained changes cover Git object isolation, preview origin and guest egress; complete multi-repository flush/restore; owner start/transfer authority; shared task/session/native-request behavior; and a dependency-safe delivery plan. The dedicated review/explainer request/result/reply product and generic provider-initiated platform writes are outside MVP. Ordinary forks, conversations, outputs/artifacts, optional jobs and continuation dependencies remain.

Current documents: [design](lean-backend-design-v9.md), [implementation plan](lean-implementation-plan-v9.md), [tracker](lean-implementation-status-v9.md), [Workspace VM API](workspace-vm-api-v9.md), [sharing](workspace-sharing-v9.md), [Worker Host API](worker-host-api-v9.md), [Agent API](agent-api-surface-v9.md). Earlier versions are preserved; this revision does not open a PR or authorize deployment.

## Finding-by-finding response

### N1 — [P1] Designated producer workspace_requests.task_id has no writer or designation rule

**Deferred with the feature.** The original producer-designation gap was valid in the proposed returned-results workflow.

**Selected design remedy:** No designated producer, candidate slot, result-missing error or publication barrier remains. Ordinary agent task completion uses its normal attempt/output contract. See [scope reconciliation](v9-final-scope-and-review-reconciliation.md) and design §5.9.

**Planned implementation and validation:** No MVP implementation or test gate for this removed product. The former P4.14 is removed from the active plan; future product scope requires a new decision.

### N2 — [P1] Flush pins only HEAD; unpushed commits on the workspace branch are lost when HEAD is on another branch or detached

**Agree.** The old §5.4 pins only HEAD and §5.5 rewrites the mapped branch to it. An isolated real-Git probe confirmed an unpushed mapped-branch commit was absent from the bare origin after HEAD-only capture, and present with separate branch proof.

**Selected design remedy:** Receipt retains local_branch_oid, HEAD/ref/sequencer and separate /snapshot and /branch leaves; restore preserves each independently and does not replay sequencer state. Fork normalizes its own HEAD and parents the selected-tree snapshot on the mapped tip. The review’s suggested /branch beneath the old snapshot leaf would cause a Git ref-prefix collision, so both leaves move below the operation prefix. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.3a/P4.3b/P4.4b. Actual probe passed for the proposed retention mechanism; planned dispatcher tests cover foreign/detached HEAD, conflict rebase, warm/cold restore and fork.

### N3 — [P1] Guest processes spoof G5D_EXEC_ID through the workspace-token proxy (S3)

**Partially agree; provider platform writes deferred.** Plain G5D_EXEC_ID cannot authenticate a repository process, and neither a same-UID socket nor an unproven provider bridge solves that boundary.

**Selected design remedy:** Provider/MCP/shell platform mutations fail closed; generic provider task/PR writes are post-MVP. The trusted platform-shipped workspace.bootstrap supervisor alone submits the typed initial job graph through the exact POST /api/internal/jobs/:job_id/tasks route, private host broker and invoke-minted capability bound to module/hash, task/attempt, owner/job/workspace, lifecycle generation and incarnation. No authority enters repository descendants. Agent API §6.6 specifies atomic graph admission/idempotency; host §6 specifies UID/FD/proc and egress isolation.

**Planned implementation and validation:** P4.2b/P4.4b/P4.6/P4.8/P4.12: legitimate bounded bootstrap graph succeeds; copied IDs, sibling/provider processes, stale capability and another route/job fail. No pinned provider-write bridge is an MVP gate or claimed proven. Native Claude questions use their separate CLI protocol.

### N4 — [P1] Every control-plane deploy is a fleet-wide lease-expiry event unless the lease TTL exceeds the deploy downtime; the TTL is unspecified and the deploy is Recreate

**Partially agree.** Recreate can outlast a current lease; server expiry and nominal TTL are not evidence of a host's remaining monotonic authority. Expired leases cannot be revived.

**Selected design remedy:** P4.5 selects a new external deploy guard around helm-deploy, holding the placement advisory lock through Recreate and recovery. It samples every placed incarnation using vm.status(refresh_lease=true), subtracts RTT/age, and checks the host sequence/fence plus conservative remaining bound. Draft TTL300s and start/outage/recovery15+120+60s are assumptions to measure, not availability guarantees. Missing/stale evidence refuses deployment.

**Planned implementation and validation:** P4.4b/P4.5/P4.13 implement host evidence, placement guard and a new real API/scripted-host restart test; P4.6–P4.8 add live agent/native-wait variants. P8.7 reuses the MVP guard. Test below-bound continuity, above-bound loss, missing incarnation, stale replies and guard disconnect; the proposed script/test do not exist yet.

### N5 — [P1] Preview relay still serves untrusted code on the platform origin (S2)

**Agree.** The retained preview has same-origin frontend and backend fallback paths, and cross-site iframe cookies/DNS/TLS also need positive-path proof.

**Selected design remedy:** Require a separate registrable preview domain, remove platform-origin guest serving for every HTTP method/WS and every sameOrigin frontend branch, and fail PREVIEW_ORIGIN_UNCONFIGURED. Use scoped host-only tickets, supported partitioned embedded storage and explicit isolated new-tab fallback. P5.1 must provision real QA DNS/TLS and a two-domain test-CA CI fixture; domain ownership is an external deployment input.

**Planned implementation and validation:** P5.1 plus Platform: actual route/middleware and hostile iframe/new-tab tests prove normal isolated HTTP/POST/WS works, platform paths never relay guest bytes, and preview code gains no platform authority. Test cookie fallback, CORS/CSRF/postMessage effects and missing config. P9.1 reuses the boundary; no deployed preview proof is claimed.

### N6 — [P1] Repo-level upload-pack exposes other workspaces' branches and flush snapshots (S1)

**Partially agree.** Repo-level authorization and hideRefs do not prevent hidden-object access; the retained whole-bare cache/materializer can re-expose objects after network filtering.

**Selected design remedy:** Select GitObjectViewManifestV1: current authorized roots and scope/access generations produce a self-contained non-thin verified object-closure pack, with no global alternates/refs/reflogs or reused broad pack. Receive-pack validates hidden parents/thin bases. Host cache keys include scope and content digests, revalidates authority and materializes only that closure.

**Planned implementation and validation:** P4.3a/P4.4b/P4.5/P4.9a/P4.12: real fetch/receive-pack and cold/warm/fork/transfer materialization tests deny known sibling OIDs, including warming a private sibling first. Missing/stale artifacts fail closed; replacing the scoped materializer with the legacy bare copy must fail the test.

### N7 — [P1] g5-ci-e2e lane depends on per-task workers that P4.9a/P4.9b delete while lane edits are reserved to P8.6

**Agree.** Deleting host-up/worker registration before every retained CI producer and readiness query is migrated would break required CI.

**Selected design remedy:** Stage all four Cloud Build chains: seed-ci-org/start-workers/wait-workers, seed-value-loop-org/start-value-loop-worker/wait-value-loop-workers, seed-rust-e2e-org/start-rust-e2e-worker/wait-rust-e2e-workers, and seed-pa-org/start-pa-worker/wait-pa-workers. P4.4a separates v9 enrollment from bounded legacy credentials; P4.5 makes workspace fixtures use checked forks and explicit owner Start; P4.9a/b switch every start/wait before deletion.

**Planned implementation and validation:** Those owning PRs update seed scripts, waitFor dependencies, failure markers and required lane receipts together; extend existing cloudbuild ordering/docker-contract checks. P8.6 audits retained coverage, not its first implementation. Both current-head CI lanes remain required.

### N8 — [P1] No host-enforced guest egress policy; passt exposes metadata and host listeners

**Agree.** The retained passt loopback door is not host-enforced guest egress isolation; the earlier prose did not identify an enforcement point or non-skipping test.

**Selected design remedy:** Select exclusive per-incarnation passt UID plus host nftables OUTPUT rules installed before sockets/boot, with exact own-proxy/DNS exceptions, platform deny precedence, validated org allow destinations, no IPv6, and cleanup before UID/port reuse. Missing rules/pin/capabilities refuse boot; no process/macOS production fallback.

**Planned implementation and validation:** P4.4b adds new workspace_vm_egress.rs and strict workspace-vm-egress Cloud Build step; P4.12 repeats with the real broker. Real libkrun/passt tests cover allowed paths, metadata/host/sibling/unapproved/redirect/DNS-alias/IPv6 denial, install failure and stale flow reuse. Skip-capable isolation fixtures do not satisfy this gate.

### N9 — [P1] Open v8 security findings have no owning MVP PR or test

**Agree.** Owner-only UI does not establish Git object confidentiality, preview-origin isolation or guest egress.

**Selected design remedy:** Each retained security boundary has an introducing MVP owner: Git closure/export and cache P4.3a/P4.4b, preview origin/QA/CI positive path P5.1, kernel egress P4.4b/P4.12. Plan acceptance lists actual boundary observations; P8.8 audits those receipts.

**Planned implementation and validation:** Required tests must pass on the implementation candidate before enabling these paths. This documentation selects mechanisms and test owners; it does not close the runtime security findings.

### N10 — [P2] S4 still contradictory: the host companion and P4.12 build a host-keyed git credential injector that permits external pushes, while the design reserves publication for the API and defers the external grant

**Agree.** The companion said external pushes were supported by host-keyed credential injection, contradicting API-only publication and deferred dependency grants.

**Selected design remedy:** MVP proxy grants are internal Git only, exact origin/repository/operation. Later external dependency grants are upload-pack only; external publication stays in the control plane. Ambiguous paths and redirects fail without credential forwarding. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.12, P4.5. Fake upstream observes no credential for external receive-pack, off-repo paths, encoded escapes or redirected destinations.

### N11 — [P2] Storage cost drivers are unbounded: flush snapshots have no size cap and no object garbage collection story, session files and task_events have no retention, host disk sizing is undefined

**Partially agree.** repo_gc currently triggers by loose-object count; snapshot ref deletion alone does not reclaim packed objects. Existing docs lack aggregate/session/event retention bounds.

**Selected design remedy:** Added snapshot bounds, packed-object maintenance with grace and pins, session total/age policy and settled-event/usage retention. Host owner adds disk budget/eviction. Do not mandate monthly partitioning without workload evidence or adopt silent file exclusion; those are implementation choices, not necessary invariants. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a/P4.4b/P4.11/P7.2. Size cap, pinned GC, inactive session retention and active/unacknowledged-data exemption tests are implementation gates.

### N12 — [P2] Receipt completion verifies the dispatcher-reported vector, not the workspace's required member set or local_head reachability

**Agree.** Original completion step verifies every reported ref, while the invariant requires every member and reconstructible local history.

**Selected design remedy:** Completion checks exact duplicate-free required repo set, both immutable commit refs, HEAD reachability, and clean snapshot equality before updating last_flush_id. Membership is frozen and checked under the same lock. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a. Missing/duplicate member, fabricated HEAD, clean mismatch and valid multi-repo restore boundary tests.

### N13 — [P2] Harvest failure inside the stop sequence has no policy and no fault-injection test

**Partially agree.** The stop sequence required harvest/journal durability but only defined Git-flush failure handling.

**Selected design remedy:** Defined bounded harvest retry, normal abort versus deadline loss, marked fallback for invalid provider-session bundles, and atomic Agents stop commit plus workspace release after confirmed exit. Disagree that every deterministic rejection authorizes release: stale incarnation is fenced, and required task/event journal loss is not equivalent to losing a reconstructible provider bundle. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.5/P4.6/P4.11. Transient harvest error, over-cap bundle, stale epoch, journal failure, database commit failure and lost-exit replay tests.

### N14 — [P2] No rule for who may add or remove read invites or change visibility; on main this authority is org-admin OR member-admin role OR creator

**Partially agree.** can_manage_workspace_members currently accepts org/workspace admin or created_by, which conflicts with owner authority after transfer.

**Selected design remedy:** Specified invite/visibility as owner-only and removed creator/membership-role authority from the planned cutover. Disagree with adding a new non-owner admin write exception: the user’s read-only rule requires admins to take ownership first. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5/P4.10. Owner/former creator/reader/admin-without-transfer matrix covers list, add/remove invite and visibility routes.

### N15 — [P2] RLS unspecified for new tables lacking org_id

**Agree.** Baseline tenant tables use org_id plus ENABLE/FORCE RLS and tenant_isolation; new sketches omitted this for joins and operation rows.

**Selected design remedy:** Established the baseline RLS/grants rule for every new tenant row, including scoped configs, and same-org workspace/member/task/operation constraints. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1 and each later table-owning PR. Fresh/upgrade cross-tenant reads/writes and same-org reference rejection for every introduced table.

### N16 — [P2] Pruning ignores receipts pinned by pending forks, request manifests and reader selection; row-retention semantics are ambiguous

**Agree on retained pins.** Pruning needs to distinguish active ordinary fork/read/restore recovery from settled history; dedicated review-request manifests are now deferred.

**Selected design remedy:** Ordinary source selection establishes pins under the prune lock. Current restore, pending fork/copy, active reader and unresolved operation sources survive pruning; retained receipt rows identify pruned content and explicit selection returns FLUSH_PRUNED. No request/result-specific pin is added.

**Planned implementation and validation:** P4.1/P4.3a/P4.3b/P4.5: concurrent prune versus ordinary fork/read/restore, explicit pruned selection and pending copy recovery.

### N17 — [P2] workspace_requests.pr_id has no MVP referent before P6.1

**Deferred with the feature.** The proposed request-to-PR binding preceded the target PR schema.

**Selected design remedy:** No dedicated review request or PR binding is introduced. Existing PR behavior and the separately scheduled governance migration retain their own contracts. See [scope reconciliation](v9-final-scope-and-review-reconciliation.md) and design §5.9.

**Planned implementation and validation:** No MVP implementation or test gate for this removed product. The former P4.14 is removed from the active plan; future product scope requires a new decision.

### N18 — [P2] Request status transitions are unspecified: nothing sets running or failed, reviewer decline and requester cancel are undefined, and a request can be running before a child exists

**Deferred with the feature.** The proposed request lifecycle lacked complete writers and cancellation semantics.

**Selected design remedy:** The review-request state machine, automatic review child and accept/withdraw controls are removed from MVP; ordinary checked forks and owner-only task cancellation remain. See [scope reconciliation](v9-final-scope-and-review-reconciliation.md) and design §5.9.

**Planned implementation and validation:** No MVP implementation or test gate for this removed product. The former P4.14 is removed from the active plan; future product scope requires a new decision.

### N19 — [P2] Result payload schema, version field, size bound and digest canonicalization are unspecified, and the workspace_results column set differs across design, agent-api and P4.1

**Deferred with the feature.** The proposed returned-result schema and column lists were inconsistent.

**Selected design remedy:** No dedicated result payload/row/renderer is selected for MVP. Ordinary transcript, task output and existing artifacts remain under their normal storage and rendering rules. See [scope reconciliation](v9-final-scope-and-review-reconciliation.md) and design §5.9.

**Planned implementation and validation:** No MVP implementation or test gate for this removed product. The former P4.14 is removed from the active plan; future product scope requires a new decision.

### N20 — [P2] Result payloads, candidate rows and replies are 'bounded' without any bound, and candidates per attempt are uncapped

**Deferred with the feature.** The original proposed result/candidate/reply boundaries lacked concrete limits.

**Selected design remedy:** The result product is removed, including its payload, candidate, reply and candidate-count settings. Neither the superseded twenty-candidate design nor the later single-attempt candidate slot is active. See [scope reconciliation](v9-final-scope-and-review-reconciliation.md) and design §5.9.

**Planned implementation and validation:** No MVP implementation or test gate for this removed product. The former P4.14 is removed from the active plan; future product scope requires a new decision.

### N21 — [P2] Capacity queue has no data model or persisted representation

**Partially agree.** queued_for_capacity was UI prose without durable queue representation.

**Selected design remedy:** Queue is the existing workspace_vms row’s typed start_intent, with TTL/FIFO-within-org/cancel/rank and a two-second/event-driven reconciler. No separate start-authorizations table is necessary for one current intent per workspace. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.5/P4.10/P4.13. API restart, expiry/cancel, quota and slot-release ordering tests.

### N22 — [P2] Compound 'Start workspace and …' has no API surface, persisted record or lifetime; capability tables are looser than the prose

**Partially agree.** Compound Start intent had no concrete creator, persistence or consumption contract.

**Selected design remedy:** Resume/restart owns the intent; optional compound launch creates task/input and consent atomically, consumes once at readiness, binds owner/generation and has bounded boot failure/cancel behavior. Terminal opens after readiness. Reuses start_intent, not a new table or implicit per-feature wake flag. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.5/P4.6/P4.10. Compound task visible while queued, cancelled launch never dispatches, readiness after transfer/expiry cannot consume stale intent.

### N23 — [P2] The child receipt created by fork-copy has no valid shape under the workspace_flushes schema

**Partially agree.** Original child receipt requires VM/epoch despite a stopped fork with no incarnation.

**Selected design remedy:** Added origin=flush|fork_copy, source_flush_id and null VM/epoch only for copy. Fresh child receipt normalizes its HEAD and snapshot parent to the child branch before publication. Disagree with blindly copying the source local_head/snapshot fields: a foreign HEAD would recreate the N2 association bug. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.3b/P4.5. Fork-copy schema checks, foreign-HEAD fork tree/mapped-tip assertions, partial copy recovery.

### N24 — [P2] Plan's code grounding omits the actual read-access authority, which still grants the creator invite_only read access after transfer

**Agree.** Read authority in db/scoped.rs check_user_workspace_access uses created_by for invite_only; list predicates repeat creator authority.

**Selected design remedy:** Named actual scoped/list producers and require owner_user_id replacement, with no residual access for an uninvited former creator. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5. Invite-only transfer: former creator denied by detail/list/Git/durable metadata routes while new owner reads.

### N25 — [P2] 'Recorded continuation authorization' for drain/rollout has no data-model home, and the `owner_resume_required` set-by list omits deadline-bound teardowns

**Partially agree.** Recorded rollout consent had no state owner; hold-setting prose omitted deadline reasons.

**Selected design remedy:** Owner/generation/expiry-bound start_intent persists consent; planned restart/rollout may preauthorize one successor generation, later stop/transfer/loss invalidates it. All release reasons require owner resume unless that successor remains valid. Avoided an unnecessary separate authorization table. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.5/P4.13/P8.7. Queued work cannot wake; planned successor versus later stop/transfer/loss and API restart races.

### N26 — [P2] Renewed workspace token is never installed into the host proxy's grant table

**Agree.** The old lease message only forwarded the renewed token to the dispatcher and never replaced workspace-token proxy grants.

**Selected design remedy:** Specified token expiry at server lease grant and atomic installation into every host workspace-token grant before dispatcher delivery, ignoring obsolete/expired renewal replies. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4b, P4.5, P4.12. After multiple short-TTL renewals, new Git/platform requests use the latest token while expired tokens are rejected.

### N27 — [P2] Workspace token has no audience or type claim, and the platform API route set it may reach is undefined

**Agree.** Runtime identity types and admitted route sets must be explicit; an incarnation token must not become a user or generic provider credential.

**Selected design remedy:** Host §6 selects disjoint typed validators and a principal/route matrix. Dispatcher authority is limited to lifecycle/receipt/harvest/event/session operations; private bootstrap graph delegation adds only its exact bound route. Provider/repository descendants get model/internal Git only. No linked-result exception remains.

**Planned implementation and validation:** P4.4a/P4.5/P4.6/P4.12: reject wrong audience/use, expired/other-workspace identities and user/admin/transfer/workspace-create calls; legitimate scoped supervisor operations succeed. Copied env IDs never authorize writes.

### N28 — [P2] Incarnation may create flush refs without a started receipt row; orphan refs are unbounded and never pruned

**Agree.** Receive-pack scope previously allowed current incarnation to mint arbitrary flush IDs with no operation row.

**Selected design remedy:** Require receipt start/frozen member set before guest ref creation and reject FLUSH_REF_UNKNOWN. Sweep failed/incomplete/unclaimed refs while protecting pending server fork-copy manifest refs. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a. Unknown ID/other member/old incarnation denied; failed attempt eventually pruned; active fork-copy retained.

### N29 — [P2] Retry of a failed multi-repo flush is undefined against create-only immutable refs and non-deterministic commit-tree

**Agree.** Create-only refs and nondeterministic commit-tree recomputation lacked a retry rule.

**Selected design remedy:** One capture keeps immutable ID/OIDs/vector for transport retries; recomputation after failure gets a new ID. Same-OID pushes and identical completion are idempotent; changed manifests conflict. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a. Second-repo push failure/retry and lost completion response recover one valid receipt without rewriting immutable refs.

### N30 — [P2] Admission is four overlapping barriers with no state machine: owner_resume_required, human mutation admission, transfer admission and drain continuation authority

**Agree.** Several independently described barriers could authorize or reject differently around stop/transfer.

**Selected design remedy:** Specified ordered authorization/lifecycle/readiness/association checks and one mutation lease spanning in-flight Save and receive-pack; Agents/dispatcher barriers implement that decision and cannot authorize wake. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5/P4.6/P2.1. Admitted Save/push versus transfer CAS, stale VM, pending transfer and stopped-owner matrices.

### N31 — [P2] The SaaS has no consistent backup until the self-hosted phase: the barrier §16.4 requires is scheduled only for P9.3, which excludes the SaaS

**Partially agree.** The only prior consistent backup barrier implementation was self-hosted P9.3 although SaaS also stores coupled DB/Git state.

**Selected design remedy:** Moved the common barrier to P4.3a; SaaS scheduling/paired manifest/restore rehearsal is P8.7 production readiness, with initial measurable RPO/RTO targets. P9.3 only packages self-hosted use. No extra PR is needed to express the dependency. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a/P8.7/P9.3. Every writer/GC honors barrier, timeout recovery, paired restore refusal for missing half and rehearsal evidence.

### N32 — [P2] No size policy for snapshot pushes; an oversized non-ignored file makes a workspace impossible to stop gracefully

**Partially agree.** The review proposed explicitly recorded exclusions with visible UI, not silent loss. That alternative is not selected because it changes a complete durable workspace snapshot into an intentionally partial capture; a separate product decision would need restore expectations for excluded tracked/nonignored files.

**Selected design remedy:** Keep explicit file/tree/pack caps and matching transport limits; over-cap capture fails FLUSH_TOO_LARGE without a new complete receipt. Deterministic failure persists last_error.flush_block, deduplicates owner notification by fingerprint, exposes operator age/category and stops periodic idle retries even if files change. The owner repairs the cause and explicitly retries the intended lifecycle action; no Flush UI is added. Transient failures alone use cooldown; deadline teardown records loss.

**Planned implementation and validation:** P4.3a/P4.5: exact cap/one-over and real route-limit tests, no partial success, restart/repeated idle ticks produce no retry storm, owner repair+explicit retry clears the block only after complete capture, and deadline loss is truthful.

### N33 — [P2] Restore step 3 is not idempotent on a dirty warm root whose HEAD differs from the receipt; mid-rebase state and the no-receipt warm path are unspecified

**Agree.** checkout -B without forced cleanup is not idempotent on a dirty retained root with later commits or sequencer state.

**Selected design remedy:** Force materialization for receipt and null-receipt restore, explicitly clearing lost incarnation’s operation metadata; mapped branch remains separate from foreign/detached HEAD. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a/P4.4b. Warm later-commit+dirty, interrupted rebase and null-receipt roots equal cold tree; snapshot-present untracked file survives.

### N34 — [P2] Retained roots and scratch disks outlive workspace delete and transfer

**Partially agree.** Delete described Git/DB removal while retained roots and scratch persisted and no host purge command existed. Do not delete retained vm.stop results or event journals during purge: those are required to reconcile ambiguous release. Logical purge also does not certify physical media erasure.

**Selected design remedy:** Added generation-specific idempotent roots.purge and durable delete/transfer purge intents. Transfer removes old ignored/secret-bearing cache before owner commit; retries reconcile unreachable hosts. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4b, P4.5, P4.13. Delete/transfer and lost purge replies converge; active/newer-generation roots are not purged; cold new-owner restore contains no old ignored secrets.

### N35 — [P2] Provider namespace preflight for forks names no credential

**Agree.** The retained git_auth_resolver explicitly selects only the acting user connection; v9 reader fork preflight specified no provider identity.

**Selected design remedy:** Assigned server-side namespace preflight and protected-ref mirroring to the recorded repository provider read connection; publication retains acting-user authority. Missing/revoked read connection is a visible unverifiable state, never absent-branch success. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P1.5, P4.5. A reader without a personal SCM connection can fork using authorized repo read identity; provider 401/5xx/revocation prevents completion and existing names conflict.

### N36 — [P2] Task-less invocations (Save, PR create) have no execution identity

**Agree.** Changes/diagnostic calls from Save/PR routes have no task row although old exec_id required task_id.

**Selected design remedy:** Added tagged operation.<uuid>.1.<vm_uuid> identity, facade-minted and stable on retry, with full journal/disposition/cancel/audit behavior and shared bounded limits. No synthetic task/queue row. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.2b/P4.6. Independent operation IDs, identical retry, stop cancellation and zero synthetic task rows.

### N37 — [P2] Stuck operational states have no operator surface or runbook

**Partially agree.** Fork cleanup and transfer recovery had durable stuck states without operator actions; retained root purge was absent.

**Selected design remedy:** Keep existing aborting status plus recovery_required error, expose proof-based admin reconcile and deliver stuck-state runbook with lifecycle PR; ineligible recovered transfer aborts without changing owner or auto-boot. Host owner adds purge intent/command. `pending`/`aborting` workspace_forks rows retain the frozen member/name claim; only verified completion or verified abort cleanup with late-writer fencing releases it. Neither timeout nor TTL releases a stuck claim. A new reservation table or generic recovery framework/status is unnecessary. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3b/P4.5/P4.4b/P8.7. Partial fork cleanup never deletes preexisting/advanced refs or releases a name while late writers can still act; invalid target after release aborts; purge/runbook drills.

### N38 — [P2] Preview activity has no unit and no measuring point

**Agree.** Preview HTTP streams are per TCP connection, but the design never defined whether long-lived stream bytes count as activity.

**Selected design remedy:** Defined activity at the API relay: admitted owner HTTP request or new stream/WS upgrade once, coalesced every10s; keepalive/bytes on an open connection do not refresh it. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.6, P5.1. A ping-only WebSocket reaches idle stop; an owner request/upgrade advances activity; reader denial leaves it unchanged.

### N39 — [P2] The `credential` teardown reason is decided by the host, which the protocol forbids

**Agree.** The stop-reason row named the host as credential teardown decider, contradicting the host reporting-only invariant.

**Selected design remedy:** WorkerHostService now initiates durable credential drain on refused renewal or30-minute expiry margin, targets completion5 minutes before expiry, closes expired/revoked tunnels and applies recorded-loss policy when out of time. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.13, P8.7. Changed identity/refused renewal drains running VMs through the control plane; old credential cannot reconnect after expiry/revocation.

### N40 — [P2] `delivery_unknown` has no owner-visible resolution after ungraceful loss

**Partially agree.** Agent API §8.1 previously allowed delivery_unknown→delivered|released but no owner action; local-only lost journal cannot prove handoff. Existing plan already forbids blind redelivery. Use a clearly labeled redelivery action, not an ambiguous Continue that silently replays side effects.

**Selected design remedy:** Keep unknown input excluded from fallback with waiting/hold_reason=delivery_unknown. Explicit possible-duplicate redelivery reuses id/revision with generation+1; Cancel preserves uncertainty audit and withdraws task input. Ordinary Continue cannot silently resend. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.6; P4.7; P4.10. Kill after handoff before acceptance/harvest; unknown hold blocks next message; owner redelivery vs Cancel; stale generation acceptance cannot settle new reservation.

### N41 — [P2] Self-review is asserted by the acceptance matrix and P4.14 but never defined

**Deferred with the feature.** The proposed self-review request/child flow lacked a complete definition.

**Selected design remedy:** There is no special self-review flow. An owner can ask an ordinary agent to review code or create an ordinary checked fork; neither creates a source-versioned returned-result contract. See [scope reconciliation](v9-final-scope-and-review-reconciliation.md) and design §5.9.

**Planned implementation and validation:** No MVP implementation or test gate for this removed product. The former P4.14 is removed from the active plan; future product scope requires a new decision.

### N42 — [P2] VmSpec env allowlist is a prose placeholder and contradicts host-added proxy keys

**Agree.** VmSpec.env used vague provider paths and a host-allocated proxy URL while stating the host adds nothing.

**Selected design remedy:** Published exact spec environment keys and separate host-added URL/secret/acquired-token provenance. The host allocates the proxy port; caller-supplied private proxy keys fail validation. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4a, P4.4b, P4.12. Control-plane spec validates on host; unknown/credential/private proxy keys fail and the booted dispatcher receives only host-generated private values.

### N43 — [P2] No PR builds the CI workspace-VM runtime image three prompts need to boot

**Agree.** stage-local-worker-runtime.sh stages legacy per-task runtime layout; no v9 PR owned catalog/dispatch/fake-CLI bundle production.

**Selected design remedy:** Assigned catalog export to P4.2b and complete /opt/g5 runtime staging/CI image production to P4.4b; catalog/hash verified at ready. Linux isolation CI must fail when KVM/enforcement is absent. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.2b, P4.4b. Real VM reaches ready with pinned catalog/provider fixtures; missing catalog or guest-image replacement fails boot; process fallback never counts as passing isolation.

### N44 — [P2] Cache and startup "measurement" has no metric, fixture, harness or record location

**Agree.** The design required measurements but had no common metric, fixture, runs or evidence location.

**Selected design remedy:** Added three-run cold/warm/child recipe with committed fixture, pinned environment, ready wall time and Git restore bytes; raw values/medians and artifact identities are recorded before deletion and compared at P4.9a. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4b, P4.9a. Same recipe before/after cache removal; explain changed results. No invented performance SLA.

### N45 — [P2] Host command `deadline` has no clock contract, so the lease bound depends on NTP

**Partially agree.** Command deadline clocks were unspecified while the initial lease proof depended on their agreement. The suggested absolute deadline/re-arm approach still complicates late-start authority. Acquisition uses the same proven duration bound as renewal and does not need synchronized clocks.

**Selected design remedy:** Replaced wall-clock safety dependency with mandatory host-originated lease.acquire before boot. Both initial and renewed grants use host departure monotonic time; local budget_ms limits processing only. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4a, P4.4b, P4.5. Delayed commands, cancelled starts, reordered replies and ±clock skew cannot boot without current grant or outlive it.

### N46 — [P2] Task secrets reach every module process but transcript redaction is by key name, not value

**Partially agree.** claude_code.rs:1240-1275 filters free-text values by key name and min length; DATABASE_URL lacks listed token/secret/key/password substrings. redact_overlapping_values at1057 exists; session-file routes internal.rs:9246+ require worker/session auth. Six-character threshold would still leak configured short secrets; value filtering does not prevent intentional transformed exfiltration.

**Selected design remedy:** Require all configured nonempty secret values regardless of key or length, across split/overlapping free text and structured C2/log/diagnostic data; preserve streaming redactor. Raw session files stay byte-exact/private, not redacted. No host key export solely for filtering. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.2b; P4.11; P4.12. Neutral DATABASE_URL and short secret split over frames redacted from transcript/events/logs; raw JSONL restores byte-identically via private authorized routes; reader bundle access denied.

### N47 — [P2] `workspace.status.get` is a dispatcher RPC but must answer for stopped workspaces and readers

**Agree.** workspace.status.get was simultaneously declared guest RPC and a stopped/reader control-plane read.

**Selected design remedy:** Removed it from dispatcher method set; GET /workspaces/:id/vm composes only durable/last-reported state for every caller, including running owner. runtime.inspect remains live owner-only. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.2a/P4.5. Guest method_not_found and status route zero-VM-call tests for stopped/reader/running-owner.

### N48 — [P2] Save/upload field contract is missing: hash, create semantics, encoding, size, confinement

**Partially agree.** Named hashes, binary upload, cap and confinement lacked a producer/consumer field contract.

**Selected design remedy:** Defined repository-format blob hash, explicit null create/matching update, UTF-8/base64 and 32 MiB decoded cap, confined descriptor-relative traversal, and collision/missing errors. Improved on the suggested canonicalize-then-open check because parent symlink races remain; conflicts return bounded metadata, not unbounded content. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.2a/P2.1/P2.2. Binary/hash/create/stale/missing/cap+1 and race-safe .git/absolute/parent/symlink escape tests through real dispatcher/API.

### N49 — [P2] Reader stream revocation on access removal is unspecified (S6 partially open)

**Partially agree.** ws/channels.rs checks subscribe admission; v9 described owner stream closure but not durable reader delivery revocation. A five-second asynchronous close alone cannot promise zero leakage. Delivery must be serialized/revalidated against the committed generation; closure is cleanup, not a grace period.

**Selected design remedy:** Extended access generations and delivery authorization to live relays and durable reader channels, with ACL/generation commit ordering, stale-ticket rejection and bounded post-commit closure. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P2.1, P4.7, P4.10, P5.1. Revoke an invite/visibility while subscribed, then publish chat/task/ref events; no newly unauthorized payload is delivered and reconnect is rejected.

### N50 — [P2] Preview replacement, loss and readiness have undefined branches

**Partially agree.** Preview profile DTO preview_profiles.rs:70 provides port; prior design had replacement waiting plus no-auto-restart without binding queued replacement loss semantics. Do not overwrite an already committed cancellation as vm_lost, and do not sweep later independently authorized starts.

**Selected design remedy:** Explicit old-incarnation preview-intent settlement and separate serving readiness. Already terminal cancellation stays cancelled; unsent old replacement fails vm_lost. Readiness uses declared port/probe; not-serving relay503 without task completion. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.6; P5.1. A→B replacement then loss leaves no launch on Resume; preserve cancelled A; later independent new start unaffected; delayed/never listening and portclose fixtures.

### N51 — [P2] P4.1 omits several target `tasks` columns and adds one the design lacks

**Agree.** 001_lean_baseline.sql:4856+ has task_type, job_id NOT NULL, no target workspace/spec/outcome/session columns and legacy max_duration_secs. Design §7.1 and P4.1 lists disagreed. Finding independently confirmed; proposed contract change accepted.

**Selected design remedy:** Canonical target column list and limits in spec.limits; staged kind beside legacy task_type; workspace backfill, nullable job, shared session guard and safe historical mapping specified. Root apply script adds all migration/prompt steps; legacy unmappable/retired rows are nondispatchable. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1; P4.6; P4.9b. Seed each legacy kind, unresolved job/workspace and jobless new task; assert exact mapping/constraints, preserved RLS, SQLx query compilation for target fields.

### N52 — [P2] Hold, continuation and dependency-failure fields have no values or owner

**Partially agree.** TaskService needs explicit values/writers, but duplicate booleans and configurable indefinite dependency holds are unnecessary.

**Selected design remedy:** Use continuation_reason interrupted|vm_lost|suspended with task/current-ended-attempt state; clear on new admission or terminalization. No continuation_pending or publication hold remains. Agent API §2.2.1 defines owner/idle/transfer/start-required/response-timeout/VM-loss/delivery-unknown holds and clears; live native waits have a live attempt and no hold. Fixed fail_dependents applies to job and jobless DAGs; capacity reasons are derived.

**Planned implementation and validation:** P4.1/P4.6/P4.8: reachable state/hold/cause tuples, one live attempt, held prerequisite no cascade, final failure cascade and same-task continuation dependency preservation.

### N53 — [P2] Permission policy shape is undefined; `permission_mode` cannot express auto-deny versus auto-allow

**Partially agree.** runner/codex.rs:27-61 accepts only never plus explicit sandbox; claude.rs forwards raw permission_mode and disables AskUserQuestion. Historical CLI contract §3/§4 records auto-deny versus bypass distinction. Codex never denies escalation, not all tool use; tool availability must not confuse Claude auto-approval allow flags. Do not claim unrecorded native combinations already verified.

**Selected design remedy:** Typed AgentPermissionPolicyV1 and exact-pin admission/mapping table; question flag, prompt/auto_allow/auto_deny, explicit sandbox/tool list/limits and immutable snapshot. Reject unsupported combos before spawn, remove ambiguous input permission_mode. Native support remains verification gate. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1; P4.6; P4.11. Every admitted/rejected combination including AskUserQuestion suppressors; snapshot/restart policy immutability; no origin-based bypass; Codex never+each sandbox tested.

### N54 — [P2] Usage has two producers and `UsageV1`/`QuotaSnapshotV1` are never defined

**Partially agree.** runner/events.rs:15-27 defines TurnUsage. chat_service.rs:10664 names spend_service authority; internal.rs:8806 currently calls record_usage. llm_proxy.rs:929+ captures allowlisted Anthropic quota data. Attempt tuple alone cannot deduplicate multiple upstream requests; arbitrary response-header map is unsafe. Use host-minted per-request ID and allowlist.

**Selected design remedy:** Define UsageV1/QuotaSnapshotV1; sole billable writer becomes proxy request-ID metering with durable replay/trusted attempt attribution. Adapter outcomes/events/message usage display-only. Quota headers allowlisted, unavailable cost unknown. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.6; P4.11; P4.12. N requests in one attempt followed by VM loss still N billing rows; display replay adds none; duplicate proxy request dedups; unknown quota headers removed.

### N55 — [P2] Docs and ontology projections are single-version rows, but §9.1 requires reader views pinned to a receipt OID and §9.4 projects snapshot refs; no PR re-keys the projections

**Agree.** projection_convergence enumerates refs/heads and requires branch refs; docs/ontology rows are not versioned for simultaneous branch/snapshot views.

**Selected design remedy:** Flush views render historical Git, not projection rows. Committed views require exact applied_oid in the same read snapshot or Git fallback. Remove flush convergence claim; P4.5 rewrites mapping join. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a/P2.1/P4.5/P7.2. Head H1/flush S1/head H2 reader docs/tree agree with selected OID through repeated convergence; no flush projection rows.

### N56 — [P2] Comment anchors `(path, line, line hash)` have no re-anchoring, orphaning or source-vector rule, and the retained `comment_index` shape is not reconciled

**Partially agree.** Existing comment service maps commit_sha by Git hunks; the proposed file/line/hash triple lacks content identity and ambiguity behavior.

**Selected design remedy:** Anchor actual byte hash and bounded context plus base commit; use confirmed hunk/unique contextual match, otherwise orphan/ambiguous marker. Disagree that HEAD alone identifies an uncommitted buffer or nearest duplicate is safe. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P2.2. Insert/delete/duplicate-hash/dirty-buffer/older-receipt/cross-branch comment render tests.

### N57 — [P2] `vm.event` journal ingestion has no table in the data model or the P4.1 migration

**Agree.** The wire contract promised durable `(vm_id,seq)` ingestion but no model/PR owned the storage.

**Selected design remedy:** Added worker_vm_events with org RLS, atomic insert/transition, contiguous durable acknowledgements, stale-incarnation history and explicit retention/watermark rules. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1, P4.4a. API crash before ack and host restart/replay yield one transition; stale events cannot mutate replacement VM.

### N58 — [P2] Ownership backfill requires an operator mapping with no mechanism; unattended deploy blocker

**Partially agree.** Unattended one-transaction NOT NULL migration could not accept operator resolutions created inside the same failed transaction.

**Selected design remedy:** Stage nullable owner field with fail-closed new paths and preflight; reviewed JSON mapping command writes validated explicit resolutions before P4.5 strict activation. Do not add a permanent one-off owner-resolution table. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.5. Eligible creator, null/departed/viewer creator, explicit resolution and unresolved strict-cutover failure across fresh/upgrade/no-op.

### N59 — [P2] S5 partially addressed: the transfer cutover guard keys on "active" legacy execution, and the frozen provider-account resolver is outside P4.5's steps and tests

**Agree.** Retained claim_task selects pending work and config_service calls the frozen-account resolver; blocking only active executions permits old-owner queued work after transfer.

**Selected design remedy:** P4.5 blocker covers all nonterminal/claimed/held/recovering/sticky legacy work and guards claim UPDATE/frozen resolver with current owner, resume and transfer admission before transfer activates. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5/P4.7/P4.8. Queued capacity task versus transfer, paused/recovering/sticky variants, claim-before-CAS race and no old-owner account use.

### N60 — [P2] Owner leaving the org and tenant offboarding are undefined

**Partially agree; erasure policy explicitly deferred.** Removing an owner can violate ownership and retain spend authority. Replacing author IDs is not a complete user-erasure policy for organization-owned content, Git history, audit and backups.

**Selected design remedy:** P4.5 blocks removal/demotion with OWNER_OF_WORKSPACES until ordinary transfer/delete completes, and tenant deletion must drain/purge resources before row removal. Broader personal-data erasure policy and its product/legal retention decisions are explicitly post-MVP in design §17 and P8.8; do not claim an erase-user action or anonymization implementation.

**Planned implementation and validation:** P4.5 tests owner removal/transfer and ordinary delete/purge recovery. P8.8 audits the retained boundaries and records the erasure-policy gap; it does not certify erased Git/history/backups or complete a deferred erasure workflow.

### N61 — [P2] No policy for CI lanes red outside the PR or for known flakes

**Partially agree.** The claim that there is no inherited-red policy is contradicted by AGENTS.md and existing plan §2.1/§2.9: repair or keep open. The CI edit prohibition and missing rerun guidance are real gaps. A CI-owner-only rerun rule would add an unnecessary bottleneck.

**Selected design remedy:** Plan §2.11/§2.14 permits scoped CI maintenance and evidence-based infrastructure reruns, records failures/reasons/build ids and forbids weakening retained coverage. No fixed retry count or green-run waiver is introduced. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N62 — [P2] P4.5 bundles fork, lifecycle, mapping cutover and UI into one XL PR

**Partially agree.** P4.5 is broad and deserves bounded review stages, but breadth alone does not prove the proposed three-way split is better. The meeting specifically asked to reassess fragmentation; splitting an authorization/mapping cutover can also leave unsafe intermediate states.

**Selected design remedy:** Plan §2.2 explicitly retains P4.5 as XL with separate schema/fork/lifecycle/transfer review stages and a gate for any later regrouping. No P4.5a/b/c identifiers are invented without a coherent replacement cutover/dependency review. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N63 — [P2] "Works in QA" acceptance is unobservable; no deployment owner or step

**Agree.** Implementation/CI evidence does not establish QA deployment or observation; each has its own owner and authorization.

**Selected design remedy:** Plan §7.2 and tracker evidence distinguish candidate commit/digests, builds, authorized deployment, QA configuration and actual smoke/browser observations. Platform owns lease-guarded deployment and preview DNS/TLS; capability owners own acceptance receipts. Dedicated returned-result QA is removed.

**Planned implementation and validation:** Owning MVP PRs produce local/exact-head CI receipts; the separately authorized candidate deployment records host/VM/fork/shared-agent/preview/terminal behavior. Missing external QA domain/configuration is a visible prerequisite, not an observed pass.

### N64 — [P2] P4.3a gates user receive-pack on a running incarnation before any PR can produce one, breaking laptop push on main until P4.5

**Agree.** P4.3a has no production running-incarnation producer in its dependency closure. Activating that admission predicate there would block every retained laptop push until P4.5.

**Selected design remedy:** P4.3a owns identity and ref-scope checks; P4.5 activates running/open-admission checks with the real lifecycle. Both prompts specify the staged boundary tests, preserving the final v9 restriction without a temporary feature flag. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N65 — [P2] Grant and secret resolution channel is missing from the ctl protocol that claims to be complete

**Agree.** The host had to resolve secrets at boot/update/401 without a declared ctl message or route.

**Selected design remedy:** Added grants.resolve as the single typed ctl path, scoped to current placement/owner/spec grant generation, audited without values and never journaled as secret content. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4a, P4.4b, P4.12. Wrong host, stale VM, undeclared ref or old generation cannot resolve a secret; valid rotation refreshes only its own grant.

### N66 — [P2] Task status vocabulary is staged inconsistently: P4.1 keeps the legacy CHECK, P4.6 writes `queued|succeeded`, P7.1 removes the wrong values

**Agree.** Baseline tasks_status_check at4907 rejects queued/succeeded; contracts/types/enums.rs:80+ lacks Queued/Succeeded/Waiting. Existing plan kept statuses then new service wrote target values. Finding independently confirmed; proposed contract change accepted.

**Selected design remedy:** Widen CHECK/shared enum in P4.1; P4.6 writes target six verbatim. P7.1 explicit historical backfill includes completed→succeeded before narrowing; ambiguous legacy active rows cannot auto-dispatch. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1; P4.6; P7.1. All legacy rows survive staged schema, all target inserts accepted, v9 writes subset target values, final zero legacy rows/check restriction.

### N67 — [P2] Legacy branch-mapping dual-write window (P4.1 to P4.5) has no reconciliation marker and repeats the unattended-abort pattern

**Partially agree.** The review correctly identified the unambiguous common-name migration case; a blanket rejection of adopting existing generated names was too broad.

**Selected design remedy:** Keep legacy writers authoritative during staging. An approved bulk preflight may preserve each existing common Git-valid branch and set workspace.name to it with the display-name change explicitly approved; retain slug and create/rename/overwrite no refs. Record source row/ref versions, eligible owner policy and reconciliation markers. Apply with CAS; divergent/missing/conflicting mappings or unknown owners remain blocked and require new preflight/resolution. Cutover/import must respect unfinished fork-row member/name claims under the shared repository lock protocol; the removed reservation table is not an authority or migration prerequisite.

**Planned implementation and validation:** P4.1/P4.5: positive bulk migration for existing common names, idempotent resume, interval writes/drift, divergent docs branch, base workspace and orphan owner. Helm cannot flip authority/tighten checks before all live mappings reconcile.

### N68 — [P2] P4.11 must call control-plane surfaces whose server side lands in P4.5/P4.6, which depend on P4.11; the graph hides the cycle

**Partially agree.** Agent input had prompt_ref with no route/table. internal.rs:9237 PutChatSessionFileQuery lacks vm/epoch; retained routes use worker auth. P4.5 owns replacement fences while P4.11 originally depended only P4.2b. Reject extra prompt-artifact table/API and env-ID authentication. Inline prompt plus real server prerequisite removes cycle.

**Selected design remedy:** Choose review’s simpler inline bounded prompt{text,sha256}; no new artifact service. Agent API §6.5 specifies retained session routes plus execution/incarnation/generation fences. P4.11 dependency adds P4.5; exact client fixtures run against real router, not stand-in server drift. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5; P4.6; P4.11. Hash mismatch before spawn, 512KiB prompt cap, stale harvest409, same contract fixtures through real P4.5 router and module client.

### N69 — [P2] New MVP runtime crates run in no CI lane until P8.6 (wave 13)

**Partially agree.** The Cloud Build Rust lane has explicit archive/test/coverage lists that omit proposed new crates; those must be maintained immediately. The broader assertion that g5-authz/g5-blob appear in no lane list is inaccurate: current Justfile unit/doc/coverage lists include them.

**Selected design remedy:** Plan §2.11 plus P4.2a/P4.11 require applicable CI and local lists, archive/execution/coverage evidence and introducing-PR ownership. P8.6 cannot defer coverage. A crate need not appear in every specialized integration list when it has no such target. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N70 — [P2] API-side lifecycle and dispatch tests need a host-protocol peer that no prompt provides

**Agree.** The API integration command can run without KVM but the lifecycle tests require a host protocol peer. The original no-stand-in language did not supply that harness.

**Selected design remedy:** Plan §2.2/P4.4a assigns the shared scripted ctl peer and real tunnel/journal path; P4.5 composes it with real dispatcher. KVM substrate tests remain separate required evidence. A real facade call log is test-injected; no production audit table is required. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N71 — [P2] No metrics, SLOs, alerts or traces are named anywhere; operability rests on heartbeat fields and audit documents

**Partially agree.** The draft named alerts and failure states but lacked metric producers/conditions. Accepted observability requirements without inventing product SLO targets or imposing a specific telemetry stack before measuring baselines.

**Selected design remedy:** Added internal metrics, bounded labels, lifecycle/lease/slot/proxy counters, feature-owned failure metrics, operator alerts and QA SLI measurement responsibilities. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5, P4.13, P8.7 and each feature owner. Fault-path tests observe named metrics; rollout rehearsal records attach/flush/outage evidence.

### N72 — [P3] P4.12 acceptance requires a provider turn inside a workspace VM but depends only on P4.4b

**Partially agree.** P4.12 can test its proxy through a test-only guest entry before agent.turn and lifecycle composition exist; the original full-turn wording was unachievable from its dependencies. External Git push credentials must not be added merely to satisfy a test.

**Selected design remedy:** P4.12 now tests provider/internal-Git HTTP via the real host proxy and fake upstream. P4.7/P4.8 own full provider-turn composition. The fixture never becomes a production arbitrary-process API. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N73 — [P3] Child deletion tombstone and result FK behavior undefined

**Deferred with the feature.** The proposed child deletion/result provenance relationship needed an explicit retention contract.

**Selected design remedy:** No result-specific child tombstone, producer/candidate FK or parent result visibility rule remains. Ordinary workspace deletion, history retention, receipt pins and audit requirements continue independently. See [scope reconciliation](v9-final-scope-and-review-reconciliation.md) and design §5.9.

**Planned implementation and validation:** No MVP implementation or test gate for this removed product. The former P4.14 is removed from the active plan; future product scope requires a new decision.

### N74 — [P3] DDL in §5.1/§7.1 and the P4.1 prompt disagree on columns: submissions `captured_at` vs `created_at`, `workspace_results` shape, transfers without `idempotency_key`, `tasks.wall_clock_limit_secs`, settings in `workspace_settings` vs `configs`, job kind `agent_review` vs `approval_review`

**Agree on retained schema reconciliation; result portion deferred.** The older response preserved superseded result/candidate timestamps and overstated their current relevance.

**Selected design remedy:** Remove the dedicated result schema, candidate/captured_at and related inventory from MVP. Retained schemas align transfer scoped idempotency/digest, task spec.limits, authoritative attempts/current caches, shared physical chat fields, normalized dependencies and typed workspace_settings versus registered org/user configs. Job profile naming/cutover remains explicit. The approved fork simplification removes workspace_fork_reservations from the target inventory: pending/aborting workspace_forks rows freeze claim members/name before I/O, while source/progress fields fill under that operation later.

**Planned implementation and validation:** P4.1/P4.5/P4.6/P4.8: schema/model/API roundtrip and legacy cutover fixtures for retained fields; no result-schema migration or test is required. See the current data-model response.

### N75 — [P3] `exec_id` is stored as a column and used in UNIQUE keys but its text encoding is defined nowhere

**Partially agree.** Prior docs tuple execution identity used as scalar journal path/key while no scalar definition existed. Workspace review also identifies taskless changes/diagnostics requiring separate identity. Prefix tag needed to avoid collisions with taskless operations; no need for future generated-column framework.

**Selected design remedy:** API-minted task.<taskUUID>.<attempt>.<vmUUID>, operation.<operationUUID>.1.<vmUUID>; lowercase canonical UUID/positive decimal attempt, opaque clients, triple/kind validation. Execution IDs never authorize writes. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1; P4.2b; P4.6. Golden serialization and journal retry fixtures, mismatched triple rejected, operation scalar refused by agent-turn/provider-request routes, no path traversal.

### N76 — [P3] `workspaces.status` removal is required by the design but assigned to no PR

**Agree.** Target removes workspaces.status but prior plan names no complete producer/consumer/drop owner.

**Selected design remedy:** P4.5 removes model/DTO/update/filter writers, P4.10 replaces frontend type/chips/mapping, P7.2 drops old column/CHECK/type after audit. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5/P4.10/P7.2. Retained API client field rejection/compatibility and UI lifecycle status fixtures, final schema absence.

### N77 — [P3] Base workspace ownership has undefined administration semantics

**Agree.** Base workspaces keep an owner but writable administration versus forbidden lifecycle was ambiguous.

**Selected design remedy:** Defined connecting-user owner, org default visibility, ordinary owner invite/visibility and admin transfer; no VM, WORKSPACE_IS_BASE for lifecycle, delete blocked while repo connected. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5/P4.10. Owner/admin/viewer base matrix and zero VM calls on rejected lifecycle.

### N78 — [P3] Org role vocabulary is inconsistent: 'org admin' vs org role owner|admin, and viewer's ability to fork/own/receive transfers is undefined

**Agree.** Baseline org role vocabulary includes owner/admin/member/viewer while draft loosely says member/admin.

**Selected design remedy:** Canonical admin=owner|admin, ownership eligibility=active owner|admin|member; viewer cannot fork, receive transfer or be backfilled owner. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.5/P6.4. Viewer fork/transfer/backfill refusals and both admin roles exercising intended recovery/read authority.

### N79 — [P3] Org-admin self-transfer needs no owner consent and triggers no notification; the audit row is the only trace

**Partially agree.** Admin self-transfer is already authorized but lacked owner notification.

**Selected design remedy:** Kept explicit admin recovery authority without adding consent; final owner/audit transaction notifies previous/new owner, deduplicated. Did not inject a new notice into every chat session when a lifecycle notification already carries the event. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5. Running/stopped admin self-transfer emits one transactionally deduplicated notification per distinct recipient; aborted transfer no success notice.

### N80 — [P3] Vocabulary drift: 'response deadline' vs 'runner wait', three idle-activity definitions, 'healthy' undefined, 'C3' used before definition, `timeout`/`timed_out`, handle state `ready` vs `running`, delivery diagram `released`, receive-pack STOPPING vs NOT_RUNNING

**Agree.** Vocabulary and status admission order vary across companions and plan.

**Selected design remedy:** Defined running+open+fresh private health, status RPC ownership, STOPPING before NOT_RUNNING, canonical activity list and response deadline terminology; agent owner aligns outcome/handle/delivery names. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5/P4.6/P4.10/P4.11. Contract matrices cover owner readiness, stopping precedence, typed outcome mapping and consistent UI status.

### N81 — [P3] stop_reason vocabulary differs between the actions table, the teardown table, the flush table, the decisions register and the P4.5 prompt

**Agree.** The earlier stop-reason lists were inconsistent. Dedicated result-completion reasons are now removed.

**Selected design remedy:** One stop enum contains user,restart,transfer,idle,drain,rollout,credential,capacity; every graceful stop flush reason is shutdown, live fork remains fork. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.5. All reason policy rows exercised; no invented flush reason and explicit remains rejected.

### N82 — [P3] No rate limits or bounds on the new tenant-driven amplifiers: forks trigger provider namespace checks per member repo, lifecycle actions consume host boot capacity, inbox/replies/result payloads have no stated caps

**Partially agree.** Retained fork/start/inbox amplification needs bounded admission and payload policies.

**Selected design remedy:** Assign bounded per-org/per-user fork/launch rates, pending operations and inbox/native-response limits with 429 Retry-After/413 to owning PRs. Do not treat a stale cached provider namespace as authoritative proof a requested name is free; preflight still verifies current provider state. Configurable draft defaults now cap forks at 60/org/hour and four pending/requester, start requests at six/workspace/minute, and queued ordinary input at 100/session. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5/P4.6. Burst/replay/cap-boundary tests and provider collision freshness before ref creation.

### N83 — [P3] Lease timer arming after renewal is contradictory in the host note

**Agree.** The lease message said arm from receipt while its invariant said departure; initial deadline argument depended on clocks.

**Selected design remedy:** Unified all lease acquisition/renewal language around departure-time monotonic bound, sequence fencing and no expired revival; removed provisional boot clock argument. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4a, P4.4b, P4.5. Delayed/reordered replies and partitions cannot extend host expiry beyond API grant; stale sequence never reopens proxy.

### N84 — [P3] Proxy re-resolves a grant on every upstream 401 with no rate limit, and a guest can force 401s by sending its own Authorization header

**Agree.** The old contract forwarded arbitrary client Authorization and retried resolution once per upstream request.

**Selected design remedy:** Credential-injected routes strip client authority; only proxy-attached 401s may refresh, single-flight per grant, minimum60s with backoff to15m and5s timeout. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.12. 401 storm causes at most one resolution per window; suppressed/timeout paths surface the original error and increment bounded counters.

### N85 — [P3] `runtime.ready` payload: P4.2a omits `module_catalog_version`, P4.2b does not add it, but P4.5 records it on `ready`

**Agree.** runtime.ready consumer required a catalog version but producer/host forwarding plans omitted it.

**Selected design remedy:** The canonical ready payload and host event now include VM/epoch/catalog/dispatcher versions; P4.2b exports the catalog and host/API compare hashes before running. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.2a, P4.2b, P4.4a, P4.5. Ready/replayed-ready preserves versions; missing/mismatched catalog never marks VM running.

### N86 — [P3] `agent.turn` manifest declares no limits; the per-VM bounded-execution slot count that dispatch depends on is undefined, as is its interplay with `jobs.max_concurrency`

**Partially agree.** Agent manifest previously no limits; dispatch required bounded slots with no source/count. Legacy workspace_settings max_concurrent_tasks exists (baseline5445+), not mapped to per-incarnation budget. Review incorrectly exempts bootstrap; command is bounded. Capacity belongs workspace incarnation, not agent.turn catalog alone.

**Selected design remedy:** Declared module caps plus Workspaces bounded_execution_slots default2; API reserves at invoke intent, dispatcher enforces; agents AND commands counted, preview exempt; job/module fanout intersection and derived queued_for_slot. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.2b; P4.5; P4.6; P4.8. Two slots/four tasks admit exactly2 plus preview; commands consume slots; ambiguous invoke retains slot; missing module limits rejected.

### N87 — [P3] Per-org single-tenant host capacity model and its consequences unstated

**Partially agree.** The host was already declared single-org, but region/quota configuration and provisioning assumptions were absent. Did not adopt automatic cross-region fallback: it can violate placement/data-location expectations. Autoscaling/cross-org pooling remains deferred, with capacity measurements informing later design.

**Selected design remedy:** Specified org configs worker_host_region (deployment-region default) and max_running_vms(default10), active-state quota counting, static operator-provisioned pools and initially zero hosts; placement locks and requires org+region match. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1, P4.13, P8.8. Cross-org/wrong-region hosts are excluded, stopping rows count, quota default/config and concurrent placement are tested.

### N88 — [P3] Status vocabularies disagree across design table, companion schema and chat rows; attempt outcome to chat_messages.status mapping is undefined

**Partially agree.** Baseline chat status CHECK already fixed pending/streaming/complete/error/stopped; module status timed_out disagreed with reason timeout in main. Existing legacy projection does not define new attempt cases. Do not redefine reason as status: cause and outcome are different dimensions.

**Selected design remedy:** Keep reason cause separate from outcome.status; canonical timed_out spelling; explicit atomic assistant projection complete/error/stopped including vm_lost override, held task label from logical state. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1; P4.6; P4.7. One attempt per status/cause through real TaskService checks all three rows; crash transaction rollback/replay; interrupted/suspended attempts project stopped assistant output while the same logical task remains held.

### N89 — [P3] Design DDL uses logical column names that differ from retained baseline columns without a rename directive

**Partially agree.** The baseline physical fields differ from the schema shorthand. Keeping duplicate provider/index/attachment/usage columns would create competing state owners. A general rename in P7.2 is not needed to solve this.

**Selected design remedy:** Design §8.1 now uses retained physical names and role=tool. P4.1 preserves them and explicitly adds/cuts over only provider_session_id as the native handle; provider_state remains provider/model configuration. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N90 — [P3] §17.1 leaves inbox coalescing open, contradicting the companion's one-input-per-attempt rule and the `input.accepted` schema

**Agree.** Agent API says one input per attempt, input.accepted has singular input/reservation; main §17.1 still left coalescing open. Finding independently confirmed; proposed contract change accepted.

**Selected design remedy:** Fix exactly one reservation per attempt, including continuation. Root script removes open coalescing bullet and updates main/plan; system notices ride IDs in same reservation rather than extra ordinary messages. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.6; P4.7. Three queued rows become separate attempts/tasks and singular acceptance events in FIFO order; no concatenation.

### N91 — [P3] Which evidence moves a provider_request from `reply_pending` to `responded` is unspecified

**Agree.** The durable writer must correlate native response delivery precisely; a response version is no longer part of the contract.

**Selected design remedy:** Only TaskService processing journaled provider_request.replied for the exact attempt/request/immutable response_digest moves reply_pending to responded. First accepted answer is frozen; identical retries are idempotent and conflicting answers fail409. C1 is provisional; transport_write proves handoff, not provider consumption. Partial writes invalidate rather than blindly replay.

**Planned implementation and validation:** P4.2b/P4.6/P4.11: lost reply/ack then journal replay, identical request/digest produces no second write, conflicting answer refusal, dead attempt and partial-write uncertainty.

### N92 — [P3] Session-file harvest: an over-cap transcript is skipped and a later resume restores stale files without the §8.5 fallback firing

**Agree.** session_files.rs:734-753 skips too-large files; max128MiB from g5-internal-api/session_files.rs:17; existing persist_failed at runner/session_files.rs:934+ but restore can retain older blob. Finding independently confirmed; proposed contract change accepted.

**Selected design remedy:** Persist session_files_state/generation and warning; stale stored transcript cannot resume as current after partial/failed latest harvest; fresh incarnation uses delivered-history fallback. Raw artifacts private/byte-exact; deterministic failure durably unrestorable, transient failure follows stop policy. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5; P4.6; P4.11. Primary JSONL overcap leaves old digest but fresh incarnation starts new provider ID/from delivered history; stale epoch denied; deterministic failure release vs persistence outage abort.

### N93 — [P3] Fork receipt refs have no bounded retention, unlike merge receipts

**Agree.** Fork proof refs had only “through durable settlement” with no bounded terminal retention.

**Selected design remedy:** Keep pending/aborting proofs; prune seven days after complete, after verified aborted cleanup with late-writer fencing, or deletion once pins settle, using expected-OID deletion. Moving the name claim onto the unfinished fork row does not remove proof refs or permit time-based claim release. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a/P4.5. Active cleanup proof survives, completed age boundary prunes, changed proof is not deleted blindly.

### N94 — [P3] `worker_hosts.kind` and the heartbeat column differ between the host note, §12 and P4.1

**Agree.** Baseline physical heartbeat is last_heartbeat and has no kind, while the companion/plan used last_heartbeat_at and omitted kind migration.

**Selected design remedy:** Made kind constrained/backfilled by provenance; logical/API last_heartbeat_at maps to retained physical last_heartbeat with no unnecessary rename. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1, P4.13. Upgrade fixture covers cloud/token rows and invalid kinds; registration declares kind and API returns it.

### N95 — [P3] The test-strategy checkpoint has no owner, artifact or enforceable gate

**Partially agree.** The strategy checkpoint lacked an artifact, owner and readiness check. A new T0 implementation PR and duplicate ledger are unnecessary.

**Selected design remedy:** Plan §2.2 is the concrete strategy artifact with harness/lane ownership; the tracker has a pending Readiness checkpoint owned by Platform with capability-owner review. §2 readiness checks it before P4.1/runtime work. This revision does not claim that review occurred. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N96 — [P3] The legacy-execution transfer blocker is relaxed piecemeal with no PR owning its removal, and the design names only P4.7/P4.8

**Agree.** The migration prompts narrow a temporary legacy guard but did not own its final removal before its underlying tables disappear.

**Selected design remedy:** Design §13 and P5.1 assign progressive narrowing; P4.9b deletes the inventory/error/banner after producer retirement and tests new-runtime transfer. Transfer acceptance lists P4.9b. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N97 — [P3] The release path from QA to production is undefined

**Partially agree.** Every release candidate needs its own rollout evidence; a historical QA rehearsal is insufficient and this review authorizes no deployment.

**Selected design remedy:** P4.5 introduces the external lease guard around helm-deploy with locked placement, host remaining-lease evidence and post-readiness verification. Plan §7.2 records exact immutable QA/production candidate, approvals, canary/soak and abort/loss evidence; P8.7 reuses that guard for fleet rollout instead of introducing it later.

**Planned implementation and validation:** Positive bounded API restart and negative exceeded/stale/missing lease cases belong to the MVP owners. Actual QA/production observations remain pending until an authorized deployment; documentation is not a release receipt.

### N98 — [P3] §15 bullets without an owning PR test: module input argv and host paths, native `git add -A` excludes, zero-VM-call observation

**Partially agree.** The missing production-boundary observations are real. However typed prompt text containing words such as argv is not itself a control override, and a permanent audit table is not needed to prove zero calls.

**Selected design remedy:** P4.2a tests native Git excludes; P4.2b rejects undeclared process/path controls before spawn; P4.5 supplies a test-injected facade RPC/stream log consumed by reader-route/fork tests. Design §15 names the observation boundary. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N99 — [P3] Lease TTL, start deadline and grace defaults are undefined, and slot reservation atomicity is unstated

**Partially agree.** Defaults and atomic local reservation were not specified, leaving visible outage/placement bounds undefined. A VM survives only its actual remaining lease, not always half the nominal TTL. Added deployment preflight instead of a blanket outage guarantee.

**Selected design remedy:** Added timing defaults and host slot/boot lock plus API org/host quota reservation; any failed validation/boot releases capacity. Initial lease is acquisition-based rather than provisional clock-based. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4b, P4.5, P4.13. Concurrent starts cannot oversubscribe; failure frees reservation; config changes affect both host and API grant behavior.

### N100 — [P3] The idempotency key scope for `workspace_forks` is undefined and has no uniqueness constraint

**Agree.** Retry prose had no uniqueness scope or original-request versus resolved-source rule.

**Selected design remedy:** Fork idempotency remains unique on (org_id, requested_by, idempotency_key); transfer keys remain workspace/requester scoped. Canonical digest binds requested source selector/name/members/carry, while source OIDs pin once after admission. Name exclusion instead comes from pending/aborting workspace_forks rows with frozen members/name: in READ COMMITTED, lock all existing member repository rows canonically, then run a separate fresh same-org unfinished-claim query and insert before provider/Git I/O. Reject overlapping exact/slash-prefix names even for different requesters. With no reservation-table primary key, every creation/import/deletion path must use this service; it sees all relevant claims without exposing private operation details to clients. Completion and verified fenced abort release claims, never TTL. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.5. Same requester/key identical replay returns the same operation; differing payload conflicts. Different requester keys are independent only for idempotency, not name claims. Test blocked-lock fresh snapshots, overlapping/disjoint members, prefix collisions, private claims, repository deletion/import, restart/partial cleanup and late writers; moving source never changes a pinned retry manifest.

### N101 — [P3] `vm.stop graceful` as SIGTERM to the guest init names no delivery mechanism under libkrun; the retained code signals the VMM

**Agree.** microvm.rs signals/reaps the host VMM process group; there is no demonstrated guest-PID1 signal delivery.

**Selected design remedy:** Corrected vm.stop everywhere to VMM SIGTERM/SIGKILL after private flush/harvest/journal completion, freeing slot after reap. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4b. Fixture VMM ignores TERM then is killed at grace; actual VM exit/reap is observed, no guest SIGTERM claim.

### N102 — [P3] The acceptance matrix has no Worker & Host boundary row

**Agree.** The host boundary was described in prose but absent from the concise acceptance matrix.

**Selected design remedy:** Plan §7.1 now has a Host protocol, lease and proxies row plus security rows, naming exact-candidate replay, stale-fence, stop, credential rotation and real guest-egress observations and owning PRs. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N103 — [P3] P4.1 is a migration-plus-model PR of XL breadth labelled M and is the single root of the MVP graph

**Partially agree.** P4.1 breadth was underestimated as M. Its implementation touches migrations, backfill and compatibility across owners. A mandatory four-way split is not established by the evidence and could multiply compatibility cutovers.

**Selected design remedy:** P4.1 is XL, allows ordered forward migrations and requires bounded schema/backfill/runtime/agent review stages under §2.2. Existing PR identity is preserved; any later split requires one coherent plan/graph/tracker amendment. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N104 — [P3] The stated critical chain is wrong, and schedule row 1 starts P4.4a in the same row as its dependency P4.1

**Agree.** The earlier schedule violated prerequisite ordering; the prior ten-node chain ended in the now-removed P4.14 and is not current evidence.

**Selected design remedy:** The reduced active dependency table, prompts, schedule and tracker must agree; plan §5.2/§5.3 is the current register after removal of the dedicated result PR and addition of retained security dependencies. Preserve merged tracker history.

**Planned implementation and validation:** Recompute dependency cycles, longest outstanding path and predecessor completion from the final reduced table. Do not carry the historical depth/count forward or infer runtime readiness from schedule consistency.

### N105 — [P3] P8.9 (search removal) is unblocked today and an MVP gate, yet is scheduled fourth in a column while that executor idles at program start

**Agree.** P8.9 is an independent MVP gate whose only original prerequisite has merged. The original first schedule row unnecessarily idles its executor.

**Selected design remedy:** P8.9 starts in the initial three-person round beside P4.1/P4.2a. Available critical-path work has priority thereafter, and P8.1 remains lower priority. P4.10 must preserve completed search removal and cannot resurrect search state. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N106 — [P3] Prompt Validation lines contradict the §2 docker-ci rule for dispatcher and host PRs

**Agree.** The touch rule and individual Validation lines disagreed; omitting docker-ci from a copied prompt could skip required coverage.

**Selected design remedy:** Plan §2.8 makes touch rules additive. P4.2a/P4.2b/P4.3a/P4.4a explicitly require docker-ci; P4.4a also owns the host registration readiness cutover it exercises. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N107 — [P3] P4.1 acceptance names a docker-ci "bootstrap phase" that does not exist

**Agree.** Justfile enumerates 0/0b/1/1b/2/3/4/4a/4b/4c/5/5b/5c/6/6b/7; no bootstrap phase exists.

**Selected design remedy:** P4.1 now names just docker-ci 4c and its migration/fixture prerequisites, alongside fresh/upgrade/no-op migration checks. No nonexistent recipe or phase is claimed. See [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** See the PR owners named in the amendment and plan §7.1. Documentation contract/schedule checks completed here; the named PR must run its production-boundary acceptance tests.

### N108 — [P3] The worker-host note carries stale plan references: "P4.4" unsplit, `(§6.5)` pointing at the wrong section, placement "settled by plan P4.5" (now P4.13), and a "Needed change" partly made

**Agree.** Host note still pointed to unsplit P4.4, wrong secrets section and P4.5 as placement owner.

**Selected design remedy:** Updated owners/links and replaced stale historical alternatives with settled contracts; placement is WorkerHostService under P4.13 and the full runtime bundle owner is explicit. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** Docs; P4.13 implementation. Docs reference/contract checks and resume delegates to placement in the future production-boundary test.

### N109 — [P1] Self-hosted and macOS registration has no exchange step, single-use rule or lifetime, and its only retained table (`worker_tokens`) is dropped by P4.9b while P4.13 and P9.1 still register hosts with it

**Partially agree.** Reusable worker tokens cannot implement single-use host enrollment, and there is no need for another table when worker_hosts owns one active enrollment.

**Selected design remedy:** All host kinds use the same one-hour single-use POST /api/internal/hosts/:host_id/register exchange: stored hash/expiry/redeemed/revoked state, identity/path/row match and credential generation. GCE differs only in bootstrap delivery. P4.4a implements exchange/mint/verify using P4.1 columns; P4.13 supplies issue/reissue/revoke UX. Lost response requires reissue on the same host row and revokes the former generation.

**Planned implementation and validation:** P4.1/P4.4a/P4.13: exact one-use/expiry/replay/revocation/wrong-host tests for GCE/self-hosted/macOS, response-loss recovery and CI seed migration before worker_tokens retirement. Runtime identity is self-reported inventory, not attestation.

### N110 — [P2] Repo-selected `base_image` has no org allowlist or digest rule: whoever commits to the branch chooses the guest root filesystem

**Agree.** The retained assignment resolver accepts parseable image refs while org default is digest-pinned; host checked only supplied digest equality.

**Selected design remedy:** Require digest pinning and exact approved registry/repository/digest reference before pull on both API/host; org-admin config controls allowlist and no silent fallback exists. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1, P4.4b, P4.5, P4.13. Tag-only/unlisted image fails before start/pull; allowed digest boots; integrity mismatch is separate from transient registry failure.

### N111 — [P2] Inbound mirroring is post-MVP (P1.5, after P4.5) while every consumer of the mirrored protected ref is MVP; the MVP-window path by which provider truth reaches base workspaces, base forks and review pins is undefined

**Agree.** MVP base/fork/review consumers need protected provider truth before post-MVP P1.5.

**Selected design remedy:** P4.3a now owns minimal protected-ref polling/freshness; P1.5 retains later legacy cleanup/outbound/webhooks. Legacy sync cannot write protected/workspace heads behind the new authority. No extra PR ID is needed. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a/P1.5/P4.5. MVP base fork pins real mirrored provider OID and timestamp; later cleanup reuses same writer.

### N112 — [P2] Runtime directory provenance contradicts itself: `/opt/g5/bin` and the catalog are "part of the runtime image" yet a read-only host mount, and the host "adds nothing from its own state"

**Agree.** The runtime directory is actually host mounted, while prose called it part of image and left catalog outside the named mount.

**Selected design remedy:** Distinguished guest userland image from host runtime bundle; mount all /opt/g5 read-only over image content, including g5d/CLIs/catalog, with hashes and Worker & Host build ownership. P4.4b depends on P4.2b for the actual exported catalog; graph, schedule and tracker match. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.2b, P4.4b, P4.5. Malicious/conflicting guest /opt/g5 contents cannot replace runtime; missing/wrong ready catalog prevents running.

### N113 — [P2] Mirror freshness has no data-model home or label: base reads and fork/review pins say `committed` with no observation time, and a persistently failing poll freezes the mirror silently

**Agree.** Committed source label lacked persisted provider observation and failure state.

**Selected design remedy:** Added repo_mirror_refs with attempt/success/OID/rewrite/missing/failure state; all provider-backed reads/update/fork/review manifests expose observation and stale marker after three intervals or failure/missing. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.3a/P2.1/P4.5. Failing poll keeps last OID but shows stale/reconnect; capture time cannot masquerade as provider freshness.

### N114 — [P2] "Notification to the workspace's members" names the read invite list, may omit the only person who can act (the owner), and would fan out to readers who cannot commit or push; the loss and flush-abort notifications share the undefined recipient

**Agree.** Members means read invites and can omit the only owner who can act.

**Selected design remedy:** Lifecycle pending/loss/abort notification targets current owner only, deduplicated by workspace/category/stop operation; readers get durable banner without action notification. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5/P4.10/P8.7. Owner not invited still notified; invited readers not notified; service restart/retry creates no duplicate.

### N115 — [P2] Nudge window has no lifecycle state: the VM must stay `running` for 30 minutes to honor "commit and push", yet `stop_deadline_at` exists only in the `stopping` row and the deadline means both "interrupt turns" and "stop retrying the flush"

**Agree.** The draft conflated nudge deadline with stopping/flush deadline and closed admission before a promised turn boundary.

**Selected design remedy:** Persist a running/open nudge phase and separate stop-phase flush/harvest deadline, bounded by host safety expiry; owner stop/transfer supersedes it and restart resumes timers. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.5/P4.10/P8.7. New work allowed during nudge, interrupted at pending deadline, flush retry uses separate deadline, restart no duplicate.

### N116 — [P2] Inbound mirror has no rule for a non-fast-forward or deleted provider protected ref, and the retained fetch primitive is a forced all-heads refspec that rewinds the mirror and imports workspace branches

**Agree.** GitRepo::fetch on bare repos uses forced all-heads refspec; mirror overwrite/deletion policy was unspecified.

**Selected design remedy:** Use exact protected-ref fetch into staging and CAS under shared lock; provider rewrites apply with old/new audit and visible rewrite state, missing provider ref stays internally marked missing. Names conflicting with mappings or unfinished fork-row claims are refused under the same canonical repository locks and fresh post-lock query; post-merge import uses that path too. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a/P1.5/P6.2. Fast-forward, forced rewind, deletion, workspace-name collision and unrelated heads unchanged against real bare provider.

### N117 — [P2] Image resolution inputs are unspecified: which tree, which member repo, tag-to-digest resolution and malformed or ambiguous YAML behaviour

**Agree.** The draft never specified which multi-repo tree selected base_image or how conflicting/malformed inputs fail.

**Selected design remedy:** Resolve from committed restored code-member heads, never dirty overlays/docs; zero values defaults, one distinct selects, conflicts/malformed/unpinned fail with named errors. Store the full provenance vector before starting. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5. Dirty snapshot image edit ignored; conflicting code images, malformed YAML and tags fail; docs-only declaration ignored; recorded OIDs match selected source.

### N118 — [P2] The webhook trigger has no route, signature verification or payload-trust rule; the plan tells the implementer to grep for handling that does not exist on main

**Partially agree.** No retained production webhook route/signature writer exists merely because repo_secrets has a secret column.

**Selected design remedy:** Explicitly deferred webhook to post-MVP P1.5; MVP uses authoritative polling with visible staleness. Future webhook needs configured verified signature, coalescing and fetch-only payload handling; missing secret refuses. Disagree that a new public webhook route must be added for MVP. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.3a/P1.5/P8.8. Poll path test now; future webhook invalid/missing signature, forged OID and burst/replay tests.

### N119 — [P2] Image change authority is unassigned: agents commit `.g5/workspace.yaml` freely while P4.5 lists config edits as owner mutations, and no rollout, audit or UI surfaces the image a restart will boot

**Partially agree.** A committed YAML edit could change next boot while running/pending image and rollout pin rules were absent. Use the already explicit owner Start/Restart as consent with visible selected image; do not add a redundant confirmation dialog.

**Selected design remedy:** Record image source/OID vector and actor audit; show pending image before explicit Start/Restart. Automatic continuation cannot silently change a repository pin; org rollouts honor allowed pins and hold removed pins. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5, P4.10, P4.11, P8.7. Changed allowed image shown/audited, disallowed image fails before start, automatic continuation holds changed pin and org-default rollout honors repo selection.

### N120 — [P2] The nudge is an MVP deliverable of P4.5 with no MVP trigger, consumer or test: its only orchestrators (drain, rollout) are post-MVP P8.7, P4.13 defers the per-VM sequence, and P4.6/P4.11 never mention the notice

**Agree.** Nudge was assigned to MVP P4.5 but only fleet post-MVP tests exercised it.

**Selected design remedy:** P4.5 owns capacity operator trigger and credential-expiry reconcile plus system notice writer; P4.6/P4.11 consume preamble/acceptance IDs; P8.7 reuses the same primitive. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.5/P4.6/P4.10/P4.11/P4.13/P8.7. MVP service-level nudge with running/held/idle sessions, native prompt receipt and deadline restart tests, then fleet reuse.

### N121 — [P2] The nudge "notice event" has no table, row shape or delivery rule; every candidate (`chat_messages` system row, C2 `notice`, `notifications`) contradicts a stated rule

**Agree.** Platform nudge was neither a defined durable row nor provider input; existing C2 notice is display output.

**Selected design remedy:** Use platform-authored system chat rows for sessions with nonterminal work, no own task/reservation/frame; next ordinary input includes bounded preamble and acceptance system_notice_ids; pending notices withdrawn on release/cancel. Agent companion defines detailed payload/transition fields. See [workspace contract](workspace-vm-api-v9.md), [design](lean-backend-design-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1/P4.5/P4.6/P4.11. No provider/task spawn for notice; one delivery with ordinary input, idle exclusion, held inclusion, undelivered withdrawal and duplicate replay.

### N122 — [P2] The platform-authored continuation input ("continue note", `input_origin continuation`, "persisted continuation input") has no row shape, writer or delivery rule, and the companion's `input_origin` enum omits it

**Partially agree.** Main chat input_origin already included continuation, AgentTurnInput only human/workflow and undefined reservation; rollout promised continue note. No writer/content/delivery rule existed. Do not allocate a future attempt number/row at old-attempt settlement before Continue is authorized. That risks orphan/unrun reservations and collides with explicit redelivery.

**Selected design remedy:** Define reservation and continuation origin schema. TaskService creates exactly one system continuation row at next-attempt admission atomically with attempt; fixed reason/prior-ID prompt, singular acceptance, not editable human text. Explicit unknown redelivery uses original row plus metadata; nudges use system_notice_ids preamble. See [agent contract](agent-api-surface-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1; P4.6; P4.11; P8.7. Drain continuation creates one next-attempt system row and one accepted reservation, original delivered prompt unchanged; duplicated reconcile/cancel race no orphan next attempt; Codex prompt EOF includes fixed continuation.

### N123 — [P2] `runtime_identity_hash` has no producer, canonicalization or comparison rule, the GCE bootstrap credential is minted before any identity exists, and "boot-time attestation" names no retained host-side check

**Partially agree.** host_observer reports no boot attestation; existing host JWT minting lacks runtime hash, and draft did not define hash producer. Do not mint a renewable credential before identity is known or allow changed identity to keep admitting work/grant expansion. GCE uses the same one-use exchange.

**Selected design remedy:** API canonicalizes/validates self-reported runtime identity, stores hash at single-use enrollment, binds credential and hello, and restricts changed identity to control-plane drain. Removed attestation claim. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4a, P4.13. Key/provider order canonicalization, missing fields, changed identity restricted drain and bootstrap/tunnel type separation are tested.

### N124 — [P2] `spec_rejected(image_not_cached)` exists only in the design, and classifying a transient pull failure as `spec_rejected` makes the control plane alert and never retry

**Agree.** Main design used image_not_cached spec rejection while the companion treated spec rejection as nonretryable bug.

**Selected design remedy:** Image fetch/network/registry timeout is boot_failed(image_unavailable), bounded300s and retry once on another eligible host; digest mismatch remains spec_rejected and integrity alert. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.4b, P4.5. Unreachable registry frees slot and retries once; second failure surfaces IMAGE_UNAVAILABLE; mismatched bytes do not retry unchanged spec.

### N125 — [P3] Registration, replay, identity binding and revocation have no acceptance observation anywhere: not in the P4.13/P4.4a tests, worker-host §10/§14, or the §7.1 matrix (including the row N102 proposes)

**Agree.** Registration/identity/revocation lacked explicit boundary acceptance despite being required security properties.

**Selected design remedy:** Added host join/replay/expiry/claim binding/latest-tunnel/identity change/revocation observations to worker-host §14 and P4.4a/P4.13 requirements. Core exchange/mint/verification lands in P4.4a before old JWT removal; P4.13 adds operator provisioning/issuance UX. See [host contract](worker-host-api-v9.md) and [plan](lean-implementation-plan-v9.md).

**Planned implementation and validation:** P4.1, P4.4a, P4.13. A redeemed token cannot join a second host; terminated host tunnel closes and unexpired credential reconnect/renewal is refused.

## Cross-cutting recommendations

The unnumbered test/specification/delivery sections are proposals in addition to N1–N125. They were evaluated as follows; implementation tests listed here remain work for their owning PRs.

| Proposal | Disposition and response |
|---|---|
| Normative transitions, inputs, errors, timers and invariants | **Partially agree.** Added concrete field/state/guard/error/default contracts to the existing normative documents and named the state owners and recovery behavior. Keep plan §7.1 as the acceptance register. A second YAML state-machine source, generated Rust/TypeScript enums, JSON schemas for every message and a new spec compiler are not necessary for this documentation correction; implementation PRs validate their actual wire types. Do not copy the proposed error precedence that tests “not running” before the more specific stopping/transfer condition. |
| Durable start authorization and capacity queue | **Agree with the need; simplify the storage.** One typed `workspace_vms.start_intent`, fenced by owner and lifecycle generation, owns expiry/cancellation/boot retry and queue state. No separate start-authorizations table or second queue state owner. See the workspace contract and N21/N22/N25. |
| Canonical vocabulary and database inventory | **Agree.** Resolve conflicting task/session fields, hold reasons, invocation identities, ordinary fork source vectors, retained physical columns and settings ownership in the existing contracts. New fields are explicit forward-migration work. Broad physical renames and duplicate alias columns are not required. |
| Move all history into ADRs and make every sentence RFC 2119 | **Do not adopt as proposed.** Earlier versions already preserve history; deleting provenance or multiplying ADRs would not fix a runtime contract. Clarified which current contracts supersede historical choices and linked this response. Required behavior remains explicit without imposing a new document framework. |
| Freeze public interfaces before dependent work | **Agree with scope.** Copyable PR prompts name real merged producers and the normative companion contracts. Dependency changes, including agent session-file APIs, are reflected in the table/graph/prompts/tracker. Do not introduce placeholder production services or assert that future services already exist. |
| Shared protocol fixtures and layered test strategy | **Agree.** Plan §2.2 now names the real-boundary host peer, dispatcher fixture, provider fakes, API integration lane, KVM host lane and browser flows, with owners and a pending capability-owner readiness checkpoint. Deterministic clocks/fault injection may drive production state owners; an independent miniature lifecycle implementation is not an oracle. |
| Property tests, race/fault matrix and mutation evidence | **Partially agree.** Owning prompts cover meaningful stale identities, retries, partial Git publication, lease expiry, lost native replies, ordinary task completion, resource cleanup and authorization boundaries. Property tests and mutation checks are useful when they challenge an invariant. A fixed quota of three mutations per review round or exhaustive combinations unrelated to the changed boundary add no guaranteed evidence. |
| New production audit table for zero-VM-call tests | **Disagree.** A test-injected observation log at the actual runtime facade records RPC and stream opens without adding a permanent product table or exposing internal test data. Reader-route and fork tests assert that boundary remains untouched. |
| Local process hosting and KVM tests | **Partially agree.** Permit a test-only process host/dispatcher fixture for KVM-less API and browser development. It cannot prove libkrun, UID, passt or network isolation and is not an MVP production fallback. The required real-VM lane remains blocking. Self-hosted process-mode product support is still a separate decision. |
| Frontend flow fixtures, accessibility and retained-test migration | **Agree with targeted additions.** Plan §2.2 and owning UI prompts assign fixtures and value-loop rows for Save/resume, reader fork, shared conversations, transfer, preview, terminal and search removal. Read-only controls need explanations and zero mutation/VM-call assertions. New terminal output needs an accessible transcript. Stateful retries use fresh fixtures. Do not blanket-ban test setup calls by string search when a legitimate production-boundary fixture needs them; no UI assertion may bypass the boundary being tested. |
| New advisory browser CI lane, quarantine list, or three green docs-only PRs before gating | **Disagree.** Repository policy already requires both current-head lanes to pass. Repair setup, fixtures and crate lists in the introducing/retiring PR; do not weaken a required gate or postpone coverage to P8.6. An inherited failure is a blocker, not permission to make the lane advisory. |
| Mandatory P4.1/P4.5/P4.6/P4.11 splits and ten-item prompt cap | **Partially agree with reviewability, not the prescribed split.** Mark P4.1/P4.5 XL and review bounded stages at capability boundaries. Retain stable PR identifiers until a coherent dependency/cutover change is justified at the documented checkpoint. Item count, diff size and the review’s proposed suffixes are not sufficient reasons to fragment an authorization or migration cutover. |
| Opposite-model reviewer, five-round cap, one-item commits, prompt hashes/scope globs, weekly report | **Do not adopt as mandatory policy.** These are optional team workflow choices, not established meeting agreements or acceptance proofs. Current-head independent review, scoped changes, meaningful regressions and recorded unresolved gaps are required. Do not stop fixing a known issue because a round cap was reached, or call it correct because model identities differ. No recurring reports or notifications were scheduled. |
| Exact candidate CI evidence and QA/release ownership | **Agree.** Plan §2 and §7.2 separate implemented, locally validated, CI-validated, deployed and observed; name deployment/acceptance owners, preserve required CI gates, and require per-candidate release artifacts and loss/abort criteria. This revision neither deploys nor authorizes deployment. |

## Validation of this revision

This is a documentation reconciliation. This pass checks all 125 preserved finding headings, current item-level dispositions, removed-feature references, local file links and Markdown/whitespace in these response artifacts. Broader dependency/schedule/source-preservation checks are recorded by the final reconciliation, not claimed merely from this edit. Proposed runtime tests, deployment guard, kernel isolation and browser/QA evidence remain implementation deliverables; none is claimed to have run against an implemented v9 by this response.
