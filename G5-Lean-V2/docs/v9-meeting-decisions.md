# V9 decisions from the September 17 architecture meeting

Status: draft decision record, 2026-09-17. Applies to [design v9](lean-backend-design-v9.md) and [plan v9](lean-implementation-plan-v9.md), based on v8 merged through [PR #78](https://github.com/G5Labs/G5-Lean/pull/78), commit `233ecd204d99353b19e5e30b27be81fb3ec23b71`. This is a documentation revision; no runtime behavior or security finding is marked implemented by it. V8 and earlier documents remain unchanged.

Evidence: the verbatim transcript in `/Users/zliu/Desktop/dump/meeting-20260917-2.md` (raw transcript begins at line 239), followed by the user's explicit access notes in this task. The generated meeting summary is not the authority where it differs from the dialogue. The transcript is not copied into the repository. Timestamps below locate the discussion independently of local line numbers.

## Latest scope decision

The user's subsequent finalization instruction supersedes MVP inclusion of the durable, source-versioned review/explainer returned-results workflow. The meeting's interest in that workflow remains historical product direction, but it is **post-MVP**. V9 adds no request/result/reply schema, producer designation, result-candidate/publication protocol, special child idle policy, result-return UI/CLI or completion-triggered cleanup. Ordinary checked fork with parent/source linkage, exact common branch names, owner-only execution, normal conversations/history and ordinary lifecycle remain MVP.

## Agreements and subsequent requirements

| Subject | Evidence and distinction | Current disposition |
|---|---|---|
| One human writer | Meeting 00:33:40 and 00:54:15–00:56:59; later user notes reaffirm it | The current workspace owner alone writes and executes. Non-owner contributors fork, work as owner there and use authorized merge/adoption. Passive reading needs no fork. |
| Client Terminal | Explicit agreement 01:02:12 | Only the owner gets client/web Terminal access. Admins take ownership first. |
| Preview permissions | At 00:34:44 v8's permissive reader launch was challenged without a final explicit prohibition; later user notes explicitly prohibit it | Remove reader start/replace/cancel. Preview remains a visible cancellable task. |
| Non-owner repository reads | Git when stopped was discussed at 00:22:49; Git-only for non-owners even with a running VM is an additional explicit user requirement | Non-owner file/tree/docs/ontology/Changes reads use internal Git, never source VM filesystem APIs. |
| Unattached owner | Agreement at 00:24:38–00:25:27; later user notes explicitly say even owner can only read without attachment | Require attachment before platform file mutation or execution; no separate no-VM editor-write path. |
| Upload | 00:15:28; later one-write-path agreement | Same owner/VM boundary as editing, no implicit commit. |
| Fork collaboration | 00:27:25–00:30:26 and 00:54:15 | Ordinary fork and parent/child/source linkage remain MVP. The dedicated review-fork UX and discoverable returned-results workflow discussed here are now post-MVP by the latest scope decision. Transfer remains handover. |
| Transfer | 00:56:12–00:56:59 | Stop/flush/release before owner change; no automatic start under the next owner's account. |
| Returned results | 00:43:09–00:47:00 and 00:58:23 | Historical agreement to pursue durable, agent-readable review/explainer output without a whole-child merge. Its dedicated source-versioned API/CLI workflow is now post-MVP. |
| Cleanup | 00:48:00 clarified earlier “self-destruct”; 00:54:15 agreement | Historical workflow direction: release compute without deleting history/results. Dedicated workflow-completion cleanup is now post-MVP; MVP retains ordinary Stop/idle/transfer without task-completion teardown. |
| Cache/startup | 00:49:03–00:52:40 | Preserve useful Git/restore caches and validate startup behavior. No numeric latency SLA or shared-host scheduler was agreed. |
| Existing simplifications | 00:56:59–01:02:12 | Retain `agent\|command\|preview`, shared sessions, native provider interaction, workspace-search removal and owner-only terminal. “Bootstrap” describes command work, not a task-enum rename. Earlier no Commit/Flush-now controls and lifecycle-only flush remain. |
| Delivery/testing | 01:04:06–01:07:38 and 01:12:20–01:14:12 | Review fragmented PR scopes and API-boundary workstreams; invest early in unit, boundary/integration and end-to-end user-flow tests. Exact grouping and mocks were not settled. |

## Explicit v9 draft choices

These defaults complete the contract for review. They are reasoned design choices under the authorized v9 revision, not additional claims about meeting consensus.

1. **Durable source selection.** Preserve v8's last-complete-flush snapshot fallback, otherwise branch head, with source/OID/capture-time labels. Offer an explicit committed-branch view if newer than the receipt. A reader fork pins the chosen durable vector without flushing the parent.
2. **Strict live boundary.** Non-owner repository reads use authorized durable Git and metadata. Live filesystem, preview/ports, logs and terminals remain current-owner only. Isolated Git object views and scope-bearing host cache artifacts enforce confidentiality even when workspaces share a repository.
3. **Explicit start.** Owners see read-only editors until Start/Resume completes. A clearly labeled compound Start-and-run can launch chat, preview or terminal; Save cannot. User mapped-branch pushes also need current ownership and running admission. Unplanned loss requires Resume; eligible planned rollout may retain explicit owner/generation-bound successor consent unless superseded. No duplicate owner-resume boolean exists.
4. **Ordinary forks and conversations.** Fork records source/target and the pinned member vector, creates every exact-name branch before exposing the stopped child, and never wakes a non-owned source. An owner can ask ordinary agents to review or explain its files; output remains normal conversation/task history. No dedicated returned-results authority or parent mutation follows.
5. **Lifecycle and size failures.** All workspaces use ordinary lifecycle and the org idle default/ordinary workspace override. Deterministic oversized/invalid flushes retain the VM, block automatic idle retries, notify the owner and expose operator status until an explicit repaired lifecycle action succeeds. Only already-authorized force/deadline paths may record loss.
6. **Preview browser isolation.** Separate registrable-domain origins are mandatory with fail-closed configuration. Proved embedded configurations use scoped partitioned Secure HttpOnly tickets; otherwise Open in new tab uses the isolated origin. No platform-origin fallback is retained. P5.1 provisions and tests QA/CI domain/TLS readiness with that browser contract.

## Open or deferred choices

- Autosave default, cadence and conflict/durability UX: the meeting favored it but did not settle semantics. V9 keeps explicit Save with optional autosave-on-blur for now; neither implies Git commit.
- Exact execution workstream/PR consolidation and whether development mocks pay for themselves: review at the plan checkpoint. Existing real-boundary dependency and validation gates stay in force; no blanket mocks or exact three-PR plan is asserted.
- The complete durable source-versioned review/explainer returned-results workflow, including acceptance, publication, replies, child result navigation and completion cleanup, is post-MVP. Its later design must revisit source identity, authority and recovery together; no dormant MVP schema is retained. Advanced parent-child refresh/rebase/adoption and shared-host/slot optimizations remain deferred.
- Keeping a live VM through ownership transfer for debugging was discussed and deferred; no admin bypass flag is added.
- Ontology as a compact product specification and off-platform uplift/contradiction detection is product direction. The meeting does not add an MVP infrastructure gate or approve new external CI integrations.
- The historical v8 security review remains open evidence. Restricting live access does not by itself establish Git object confidentiality, browser-origin isolation, credential/proxy isolation or complete revocation correctness; the planned production-boundary security gates remain required.

## Recorded follow-ups

The meeting called for Liu to update/review the design, Rahul to review it, and a follow-up execution/testing-plan discussion the next day at 11 a.m. Pacific, with Ben to schedule. It also called for informing David/the team about workspace-search removal because it had users. This document records those actions; it does not claim that messages, scheduling or implementation occurred. No replacement agent-search product is promised.


## September 18 design-review amendments

The [review response](lean-backend-design-v9-review-response.md) traces amendments made after the meeting. Security mechanisms, durable transition details, typed runtime/input contracts, limits, recovery and per-PR test ownership are specification decisions addressing the review; they are not newly attributed to meeting consensus. The original [review](lean-backend-design-v9-review.md) is preserved. Plan §2.2 records the proposed test strategy and its pending capability-owner checkpoint; §7.2 separates authorized deployment/QA observation from implementation. Earlier versions and merged implementation evidence remain unchanged.

## September 18 data-model review amendments and final scope

The data-model review originally evaluated simpler result-candidate and completion-policy storage. The latest user scope decision removes that whole dedicated workflow from MVP, superseding those intermediate candidate/publication/cleanup choices rather than freezing them as a deferred schema. The current owner/generation-bound start intent remains the sole compute consent. Fork and merge receipt refs remain atomic evidence of Git effects across a crash; reservations are not substituted for that proof. Same-task dependencies, ordinary chat history, owner-only execution and fork-based collaboration remain. The original reviews and their response history are preserved; they are not evidence that a deferred feature has been implemented.

## Subsequent user decision: fork reservations

The user approved removing workspace_fork_reservations from the MVP after reevaluating its database constraint benefit against duplicate operation/reservation state. Pending and aborting workspace_forks now own the exact proposed-name/member-set claim. This is a later explicit design decision, not a meeting quotation. Main §5.8 specifies canonical repository-row locks, a separate fresh READ COMMITTED conflict query, private-claim-safe lookup, and verified completed/aborted settlement with delayed-writer fencing. Git proof refs and expected-OID cleanup remain required; source OIDs/progress do not change the frozen claim key. No runtime migration is performed by this documentation change.

## September 19 MVP scope amendment

The program owner decided on 2026-09-19 that the MVP is the nine proposals of the high-level requirements (`high-level-req.md`) plus the explicit later decisions. For the microVM, the host and for secret and identity management, the MVP keeps the original G5's mechanisms. The binding statement is the team's: "Environment variables are the initial credential mechanism (today's model); files, secrets managers and HTTPS-proxy capture come later."

Sixteen mechanisms move out of the MVP to P4.12 and P8.10 to P8.14. The three security boundaries of the v8 security review (Git object confidentiality, preview origin and guest egress) become accepted exposures for the MVP, listed by P8.8; this supersedes draft choices 2 and 6 and the last open item above where they make those boundaries MVP requirements. VM start and stop keep the new `ctl` stream. The MVP has no host-side lease kill timer (the server-side lease and the incarnation fence stay), the daily rotation of cloud hosts under the 24 h host JWT is accepted, and the provider key rides the dispatch the way `TaskAssignment.secrets` travel today. P4.1 and P4.2b ship as planned.

[v9-mvp-scope-amendment.md](v9-mvp-scope-amendment.md) is the full record: what the MVP keeps, the consequences per PR, the ids that own the dropped scope, and how the design came to diverge.
