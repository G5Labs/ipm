# V9 data-model review response

Current disposition revision: 2026-09-18, after the explicit MVP scope reduction. This responds to every numbered finding and the structural/product proposals in [the data-model review](lean-backend-design-v9-datamodel_review.md). The review examined commit `33154ac`; the preceding [125-finding review response](lean-backend-design-v9-review-response.md) had already changed some of that text. Both review source files are preserved. The [final scope and review reconciliation](v9-final-scope-and-review-reconciliation.md) records the reduced MVP; the dedicated returned-results product has no MVP schema, endpoint, CLI, UI, task barrier or implementation PR.

“Selected remedy” below means a **design/contract and planned implementation choice**. It does not mean the migrations, services or UI have been implemented, deployed or observed. The work remains local and no PR is opened by this revision. The v1–v8 documents remain unchanged.

## Dispositions

| Finding | Disposition | Result |
|---|---|---|
| D1 result pipeline | Deferred with the feature | Remove the entire dedicated request/result/reply product from MVP, including the intermediate attempt-candidate design. |
| D2 purpose and cleanup | Deferred with the feature | No special child purpose, completion stop, idle policy or cleanup lifecycle. Ordinary workspace lifecycle remains. |
| D3 continuation authority | Partially agree | Remove the duplicate resume flag and continuation-pending boolean. Keep persisted start authority, explicit continuation causes and eligible planned drain/rollout recovery. |
| D4 fork/merge receipt refs | Keep proof refs; simplify name claims | Retain Git application evidence. The approved follow-up removes the separate reservation table: unfinished workspace_forks rows own durable name claims under serialized repository locks. |
| D5 configuration stores | Agree on one authority per key | Separate typed workspace policies from registered org/user config. Retire obsolete user_preferences rather than relabeling it as UI preferences. |
| D6 native provider requests | Partially agree | Inline bounded bodies and derive execution identity; freeze the first answer. Keep durable accepted-but-undelivered state and idempotent retries. |
| D7 inbox and transcript | Agree with splitting | Add session_inputs; delivered messages and assistant streaming remain in chat_messages. Keep editing, reservation and ambiguous-delivery recovery. |
| D8 dependencies/jobs | Agree on normalization; disagree with removing orchestration | Add task_dependencies with foreign keys and graph checks. Preserve optional jobs and stable logical task identity. |
| D9 remaining fields/storage | Mixed; detailed below | Remove the constant already removed, constrain redundant names, identify authoritative attempt fields, retain DB artifacts and make pruning replay-safe. |

## D1 — Result capture and publication

**Deferred with the feature.** The complexity criticism was valid, but the latest user decision removes the dedicated source-versioned review/explainer returned-results workflow from MVP instead of selecting another capture pipeline. The earlier candidate table, later attempt-owned candidate envelope, canonical result and reply tables, producer designation, source/result binding, submit API/CLI and publication barrier are all removed from the active contracts and plan. The former P4.14 is not an MVP deliverable.

No final-message extraction protocol, Git result store or private provider-write bridge is selected as a replacement. Those alternatives need a separate product decision and concrete provider/storage evidence if the feature returns. The retained provider adapters do not establish such a transport merely by supporting ordinary final text. Ordinary shared transcript, task output and existing artifacts remain, including code reviews or explanations requested in a normal conversation. Existing document/comment projections retain their own purpose and are not repurposed into a result discussion store.

**Current action:** design §5.9 and Agent API §2.4 record the exclusion. There are no result-product migrations, tests or acceptance dependencies in MVP. This resolves the current scope conflict; it does not claim the original result feature was implemented or validated.

## D2 — Dedicated child lifecycle deferred

**Deferred with the feature.** No dedicated review child, workspace purpose, Keep conversion, request cleanup fields, completion-triggered stop reason or review-specific short idle default remains. Ordinary workspace creation/fork, normal configured idle policy, explicit owner Start/Resume and the existing stop/flush/harvest/release contract are retained.

The intermediate proposal to publish a result and write an ordinary stop intent is superseded too. Ordinary task completion does not create a special workspace stop or cleanup request. Generic operation/incarnation/generation fencing still prevents stale ordinary lifecycle retries from affecting a replacement VM, but it carries no result or request association.

**Current action:** remove these fields, reasons and tests throughout the active v9 contracts and plan; retain ordinary lifecycle recovery tests under their existing owners. No dedicated child-lifecycle implementation or QA acceptance is promised.

## D3 — Remove duplicate authority, preserve continuation behavior

The earlier review already added a typed durable `workspace_vms.start_intent`. With that in place, `workspaces.owner_resume_required` is a redundant writable authority. It is now removed. Current placement plus valid owner/generation-bound start intent determines admission and the derived UI explanation. Stop, transfer and generation changes revoke stale authority; ordinary queued work, Save and historical answers cannot create it.

The independent `tasks.continuation_pending` boolean is also removed. The task state, ended/current attempt and explicit continuation cause identify a pending continuation; `hold_reason` explains what prevents it from being admitted. These fields have one writer and explicit set/clear rules in the Agent API. A live native question and an ended attempt awaiting continuation remain distinguishable. A native answer goes only to its still-live request; no result-publication state remains.

I did **not** change every planned rollout into a mandatory manual Resume. That is a product/operations behavior change, not a consequence of removing the inverted flag. A previously authorized running owner workflow may retain bounded, revocable continuation authority through a deliberate drain/rollout; user Stop, transfer and ungraceful loss still require the specified explicit owner action. The persisted intent is essential even if all starts were manual: it also supports capacity waiting, restart recovery and stale-owner fencing.

I also did not collapse all interruption causes into the proposed five-value hold enum. Native input is a live protocol wait, while a held ended attempt has different permitted actions. Neither making every VM restart manual nor removing DAG jobs eliminates the need to distinguish them.

**Implementation coverage:** P4.1/P4.5/P4.6/P8.7 retain the owner/generation-bound intent and same-task continuation rules, and test revocation, queued capacity, stale rollout resume, transfer and native-response/delivery-unknown holds. No additional authorization table is introduced.

## D4 — Keep proof refs; move name claims onto unfinished fork rows

Two mechanisms answer different questions. A durable name claim excludes a competing operation; a Git proof ref establishes that this operation's ref transaction applied. The approved follow-up removes `workspace_fork_reservations`, not either invariant. `workspace_forks` rows in `pending` or `aborting` now claim their frozen member-repository set and exact branch name. Source OIDs and per-member progress may be recorded later; the claim exists before provider checks or Git effects.

Claim admission locks the existing repository rows in canonical order in a `READ COMMITTED` transaction. Only after **all** locks are held, issue a separate fresh query over every same-org unfinished fork whose claimed member set overlaps; reject both exact and slash-prefix ref-name conflicts. Resolve idempotency before refreshing source inputs; an unfinished retry uses stored member/name keys and resumes its own row after locked digest/status/authorization checks, while an authorized terminal replay returns history without requiring deleted historical repos. Revalidate the full member set still exists and is eligible after acquiring locks. Insert the pending fork/claim in that transaction, commit, then perform provider/Git work. A query whose snapshot was taken before waiting for those locks is insufficient. No separate `(repo, name)` primary-key backstop remains, so every fork and competing repository delete/create/import path must use this serialized service contract. Its tenant-scoped service query sees all relevant claims despite source-workspace read restrictions, while client conflict errors reveal no private workspace identity or progress.

`completed` releases the claim only after all required refs/proofs and final workspace metadata are verified; `aborted` releases it only after verified operation-owned cleanup and fencing of late writers. `aborting` retains the claim through retries and operator recovery and permits cleanup only. Publication is CAS pending→completed; abort is pending→aborting→aborted. A stale finalizer cannot publish after abort wins, and terminal states never reopen. Timeouts, process death and TTL expiry never release it by themselves. No generic reservation or recovery framework replaces the removed table.

For fork recovery, a destination at the expected OID still does not prove who created it: the ref could pre-exist, including at that same OID. Retain the atomic destination-plus-`refs/g5/forks/<fork_id>` proof transaction and refuse adoption of a foreign same-OID ref. The unfinished row's claim cannot substitute for that Git application evidence across a crash.

For merge recovery, `merge_result_sha` records the intended result before Git changes and ancestry observes the target's present history. Neither proves this operation applied after an API crash or after a later permitted rewrite; a local Git probe demonstrated that limitation. Retain the atomic target-plus-merge-proof transaction. The proof preserves application evidence and the referenced object while recovery needs it; it does not repair or hide later divergence.

**Selected remedy and planned coverage:** P4.1/P4.5 use the existing fork operation row for claims, with no reservation table; P4.3a/P4.5 and the later merge owner retain bounded proof-ref retention and active-operation pins. Test overlapping repository sets, exact/prefix collisions, a blocked locker observing a newly committed claim through the fresh query, disjoint progress, private-claim conflict without disclosure, delete/import races, API restart, identical retry, partial cleanup and delayed writers. The previous probe supports only the proof-ref argument; these claim-admission/runtime tests remain implementation work.

## D5 — One configuration owner per key

Agreed on eliminating competing configuration paths. The revised [inventory and migration contract](lean-backend-design-v9.md#12-data-model-and-migrations) makes ownership explicit:

| Data | Authoritative store |
|---|---|
| Workspace concurrency/spend, idle/native-response limits, retention and permission overrides | Typed `workspace_settings` |
| Registered org/user defaults, org operational settings and feature flags | Tenant-scoped `configs`; permitted scopes and validation are declared per key |
| Provider accounts, credentials and secrets | Existing dedicated encrypted account/secret domain |
| VM/attempt execution settings | Resolved snapshot of those authorities, not a writable configuration source |

There is no generic workspace `configs` scope and no duplicate typed runtime key. Reject misplaced/unknown keys instead of relying on precedence between stores. The legacy `org_configs`, `user_configs` and `workspace_configs` contain settings and encrypted secret data; migration must map each key, preserve encryption/custody and compare effective values before switching consumers. It cannot blindly copy everything into generic JSON or drop the old tables while queries still use them. The existing encrypted secret maps in all three scoped stores remain authoritative until an explicit encrypted destination preserves scope/ACL, resolver/redaction and backup behavior. Migrating non-secret settings alone does not permit deleting those rows; the inventory retains their secret storage without a competing non-secret settings authority.

Independent inspection also changes the review's premise about `user_preferences`: the [baseline](../backend/g5-api/src/db/migrations/001_lean_baseline.sql) and [service](../backend/g5-api-core/src/services/user_preference_service.rs) contain only supervisor delegation, escalation notification and autonomous-task controls. They are not ordinary theme/layout preferences. The autonomous flag has a live indirect consumer: `stamp_task_async_decision_policy()` stamps `tasks.async_decision_policy`, which dispatch and custom question handling read. Retire these controls alongside the already-authorized native permission-policy replacement, including the trigger/function and all stamped-policy consumers. Explicitly resolve retained queued/held-task policy; never interpret autonomous=true as permission to bypass. Forward-drop the table only after that cutover, preserving unrelated notifications. Do not invent a UI preference use to justify retaining it.

**Implementation coverage:** P4.1 declares key ownership and staged migration; P4.7/P4.8 replace the preference trigger/policy path and remove legacy surfaces; P7.2 drops retired schema only after the consumer audit. This is one owner per key, not an assertion that every kind of data for one user belongs in one generic table.

## D6 — Smaller native-request rows, durable answer delivery

Partially agreed. `provider_requests` now references its immutable task attempt; execution ID, VM and epoch are derived and checked at the boundary. Bounded native payload, redacted display and response are stored inline rather than through unspecified body-reference tables. The first valid response is immutable; remove `response_version` and re-answer revision semantics. Current tenant/request/attempt constraints remain explicit.

Keep `reply_pending` or its equivalent durable accepted-but-undelivered state. The API can commit an answer and crash before delivery, or the supervisor can finish the native transport write while its acknowledgement is lost. Calling this transient does not make it safe to lose. The reconciler needs to distinguish no answer, accepted answer awaiting delivery, delivered answer and an invalidated request. An inline JSON marker could encode the same fact, but would not remove the state machine.

The private delivery command and journal evidence correlate the exact execution, provider request and immutable response digest. An identical client retry returns the accepted answer; a conflicting answer returns 409. A blanket409 on every retry would make lost acknowledgements harder to recover without improving first-answer-wins. `responded` means journaled native transport write, not proof of model consumption. Process exit/timeout/cancel invalidates live authority and can never wake or answer a new process with the old response.

Raw protocol payload/answer fields remain private and are not exposed by selecting the whole row. Public cards use the redacted display projection; the reduction in columns does not weaken that boundary. [Agent API](agent-api-surface-v9.md), [Workspace VM API](workspace-vm-api-v9.md) and P4.1/P4.2b/P4.6/P4.11 carry the same first-answer and digest-delivery contract.

## D7 — Separate input queue from delivered transcript

Agreed with the split, rather than removing queued editing. `session_inputs` owns input content/provenance, FIFO position, revision, reservation, delivery generation, task/attempt binding and `queued|reserved|delivered|delivery_unknown|withdrawn`. `chat_messages` retains delivered user/system content and assistant/tool output with its streaming/final status. Assistant messages are not immutable while streaming, so simply labeling the whole old table an immutable transcript would be inaccurate.

Before handoff, the input has a stable transcript identity. Journaled `input.accepted` atomically marks delivery and materializes the uniquely linked message with that same identity; replay returns the same row. Attachments follow the same link. A queued edit cannot change a frozen reservation, ambiguous handoff is not silently retried, and withdrawn/unknown input remains visible as inbox/audit state rather than fabricated delivered history. The client composes inbox and transcript for display; history search, session fork and delivered-history fallback use accepted transcript rows.

Continuation prompts use the same input queue; platform notices remain separately identified system messages. Task and assistant output settlement remains atomic with the authoritative attempt outcome. The migration preserves existing delivered message IDs and maps queued legacy rows without duplicate content or made-up acceptance.

**Selected remedy and planned coverage:** main §8, Agent API, and P4.1/P4.6/P4.7/P4.8. Required tests include edit-versus-reserve, FIFO, attachments, acceptance-before-assistant output, duplicate acceptance, lost handoff, explicit redelivery and delivered-history search/fallback.

## D8 — Normalize edges and retain the requested orchestration

Agreed that `depends_on uuid[]` is a poor target for relational integrity. Use `task_dependencies` with a composite pair key, same-org/workspace foreign keys, no self edge, transactional cycle validation and immutable admitted graph. Preserve referenced logical tasks rather than silently cascading away a prerequisite. Backfill/deduplicate existing arrays and report invalid references/cycles before activation. Scheduler readiness, failure propagation, optional job rollup, APIs/CLI and history all move together.

I disagree with deferring the dependency model or replacing it with an ordered list. The user explicitly required optional jobs and dependencies preserved across continuations. Dependencies also serve ordinary job and jobless workflows independently of ontology profiles. Optional jobs means a task may be jobless; it does not mean dependency semantics disappear. Stable same-task continuation is also needed for session continuity, cancellation and correct dependency release.

The [task contract](lean-backend-design-v9.md#7-task-lifecycle-agents) and P4.1/P4.6/P4.8 therefore retain the graph and fixed MVP failure policy. Tests keep `A → B` blocked through A's native question, interruption, held continuation and retry; only final logical success releases B once. Edges are never copied onto replacement task IDs.

## D9 — Remaining fields, storage and retention

### D9.1 — Constant flush materialization

Agree; already addressed in the preceding review. Snapshot remains the only materialization mode, but `workspace_flushes` no longer stores a constant `materialization` column. P4.1 and the inventory match. Historical flush records are not edited to fake a new protocol.

### D9.2 — Base flag and nullable branch

Agree that independent values would be unsafe. Retain `is_base` for the product distinction and tie it to `branch_name` with an explicit CHECK: base iff branch is NULL. This target constraint is activated only after legacy mapping reconciliation; a temporarily unresolved working mapping during additive migration is not reclassified as base. Include explicit NOT NULL predicates so SQL's unknown CHECK result cannot admit an invalid working row.

### D9.3 — Workspace name and branch name

Partially agree. Keep the explicit mapping field for the current migration/API contract, but constrain every working workspace's `name = branch_name` and deny independent rename/per-repository overrides. Base display names are not working branch names; `slug` remains routing identity. This is deliberate constrained redundancy, not two independently writable authorities. Dropping or generating the column is possible later, but is not necessary to secure the invariant and would not remove the multi-repository mapping. The exact user-selected shared branch name remains enforced.

### D9.4 — VM UUID and epoch

Agree; retain both and document their jobs. The custody CAS issues them together: VM UUID is the opaque exact-match incarnation fence; epoch orders successive attaches and is not reset on release. Epoch does not authenticate an operation or order concurrent harvests within one incarnation. Session manifest generation handles that latter race. DDL comments and runtime contracts now distinguish all three.

### D9.5 — Current-attempt projections

Partially agree, with a correction. `tasks.attempt` is the authoritative selector, not a cache to reconstruct from `max(attempt)`. The old proposed `task_attempts` row did not actually include `start_confirmed`; it now does. The selected attempt owns its VM/epoch/start disposition/outcome. Matching task fields are synchronous projections for dispatch/list paths, updated with the attempt in one transaction. An old-attempt event cannot change the selector or current projection. A mismatch is an invariant error, not permission to accept whichever copy is convenient.

### D9.6 — Provider bytes in Postgres

Do not adopt offload as an MVP simplification without evidence. The retained [session-file service](../backend/g5-api-core/src/services/chat_session_file_service.rs) already stores validated **raw bytes** in Postgres with its metadata and tenant transaction. [Zstd is a wire encoding](../backend/g5-internal-api/src/session_files.rs), not an application-level compression guarantee for DB storage. The design now says this explicitly.

The [RepoStorage abstraction](../backend/g5-storage/src/lib.rs) exposes repository paths; it is not a private provider-artifact service. Moving transcripts beside it needs an authenticated non-repository namespace, durable blob-before-manifest ordering, generation-fenced publication, orphan/dangling-object recovery, GC, byte-exact verification and paired backup coverage. Provider files can contain sensitive raw tool content and must remain inaccessible through Git/file/projection browsing. A hash and size alone define neither authority nor recovery.

The existing 128 MiB per-file cap is not proof that full-file writes are economical. Retain the bounded MVP path (also 1 GiB per session, 30-day inactive-bundle policy and explicit unrestorable/history fallback), and measure actual session distributions, per-turn bytes rewritten, harvest/restore latency, DB/TOAST/WAL growth and backup/restore duration. Those measurements can justify a later private artifact-store boundary. No claim is made that Postgres is universally faster or cheaper.

### D9.7 — Event retention and replay

Agree; the preceding review added a 90-day settled-event default, and this revision makes eligibility precise. Prune only terminal, fully settled attempt payloads at or below the durable acknowledged prefix; preserve active/held and unacknowledged recovery data. Keep attempt outcomes and ack/dedupe watermarks after payload removal. Replay below the watermark is acknowledged without reinsertion or repeated effects. History APIs identify the retained-history boundary rather than pretending the timeline is complete. Canonical transcript and billing aggregates follow their own retention rules.

The existing [timeline cleanup loop](../backend/g5-api-core/src/maintenance/timeline_cleanup.rs) uses legacy status/table rules and a 30-day/newest 100 policy. It must be cut over to the v9 table/status/settlement rules, not left running alongside a new uncoordinated pruner. P4.6/P7.2 require lost-ack, replay-after-prune, held continuation, stale-attempt and retained-transcript tests. Partitioning remains contingent on measured volume.

## Structural proposal: fewer bespoke workflows

The latest scope decision removes the new dedicated request/result/reply workflow completely from MVP, rather than retaining a smaller bespoke version. The retained refactor removes duplicate start/continuation flags and gives the editable inbox and dependency graph explicit ownership. Fork, transfer and merge still cross durable boundaries and retain their existing operation recovery; no generic saga framework is introduced.

Table/column counts alone do not measure complexity or performance. The review's estimates of “a third” or “six tables' worth” are not measured operational savings. The relevant current distinction is which product behavior is retained and which is explicitly deferred.

## Product proposal: reuse a review child across rounds

**Deferred with the dedicated product.** MVP has no automatic source-pinned review child or returned-result round, so it selects neither a new child per round nor reusable/versioned review children. Future review-round reuse requires a separate product decision; this response does not pre-design that feature.

Ordinary checked forks remain, with the exact shared branch name and collision policy. Users may continue ordinary conversations and tasks in workspaces they own. This does not create a versioned result protocol, a hidden branch adoption path or automatic child deletion.

## Retained-inventory reconciliation

| Object or mechanism | Final v9 decision |
|---|---|
| Git ref authority, one VM custody row, three governance tables and projection convergence | Keep the lean spine endorsed by the review |
| Separate fork-reservation table | Remove; pending/aborting workspace_forks rows hold frozen member/name claims under the serialized repository-lock protocol |
| Dedicated request/result/reply tables, candidate table/pointer/envelope and producer linkage | Deferred product; none in MVP |
| Dedicated child purpose, Keep conversion and short idle policy | Remove; ordinary workspace lifecycle/settings remain |
| Result-triggered cleanup/stop and publication holds | Remove; no special task completion or workspace stop |
| Owner-resume and continuation-pending booleans | Remove; derive from the explicit lifecycle/task contracts |
| Fork/merge proof refs | Keep with bounded retention and active recovery pins |
| Generic workspace config and obsolete user preference controls | Retire; one registered owner per retained key |
| Native request body references and response versions | Replace with bounded inline bodies and immutable first answer |
| Native accepted-undelivered reply state | Keep; required for recovery |
| Queued input in transcript columns | Replace with session_inputs and unique delivered message linkage |
| Dependency UUID array | Replace with constrained task_dependencies; keep optional jobs |
| Constant flush field | Already removed |
| Base/name/branch redundancy | Retain with enforced target constraints and staged migration |
| VM/epoch and task projections | Retain with explicit authority and atomic update rules |
| Provider file bytea | Retain for MVP; measure before storage migration |
| Raw task-event history | Bounded, settled and replay-watermark-safe pruning |

## Validation and remaining work

This reconciliation checks numbered dispositions and the structural/product proposals against the current reduced scope, preserving both source reviews. It checks removed-feature references, local file targets, Markdown structure and whitespace in the two response artifacts. The final reconciliation records broader cross-document, dependency/schedule and preservation checks separately.

No database migrations or application source were changed by this response. Production-boundary, fault/retry, pinned-provider and QA tests remain required implementation work; they have not run against an implementation of this design. The earlier local Git receipt probe supports only the D4 mechanism argument, not correctness of an unimplemented service. Selected runtime remedies and deferred product gaps are both explicit rather than reported as completed fixes.
