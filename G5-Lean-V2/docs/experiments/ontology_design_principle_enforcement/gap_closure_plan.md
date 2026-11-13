# Design audit and gap closure plan

## Ontology changes and governance

**Ontology changes:** none against `412cccae8dcb78453745341bb544d1011c35b581`.
This update incorporates independent analysis and validation work, without selecting
new product policy or changing production code. Existing contracts constrain closure;
author-proposed rules require reconciliation and unresolved design questions remain
explicit in section 10.1. The [PR plan](ontology-plan.md) remains the owner of the four workstreams and comparative
experiment; this file supplies the audit findings and their closure sequence. Proposed
mechanisms below are candidates, not amendments to the governing ontology. Before a
selected refactor, record its concrete boundary decisions and any necessary ontology
diff in that plan; retain this file's finding IDs for evidence and closure tracking.

**Baseline qualification.** The supplied audit reports examining `main` at `2bd71661`
on 2026-09-29. That revision does not resolve in this checkout. The present review
examined `412cccae8dcb78453745341bb544d1011c35b581`, which is the local branch, `main`
and `origin/main` tip at review time. Historical measurements and second-reader claims
are retained as reported evidence, not reproduced measurements of the current checkout.
[Appendix E](#e-document-review-and-current-head-spot-checks) records the actual spot
checks and corrections. No application tests or formal models were executed here.

**Independent model-analysis evidence.** On 2026-09-30 this plan incorporated all
18 findings from [G5 state machines in TLA+](slides.html), dated 2026-09-29 and pinned
to `412cccae`. Its reported model runs are a separate evidence source from the original
audit and this document's inspections. Sections 2.1, 6.8 and 8.7 preserve their method,
counterexamples and limits. The referenced `docs/tla/` models and runner were not
present in this checkout at incorporation; none of their results was rerun here.

The following current commitments constrain closure, including where they reject an
overbroad recommendation in the supplied audit:

| Governing contract | Obligation for this plan |
|---|---|
| [g5.architecture.capabilities](../../../.g5/ontology/concepts/software_architecture.md#four-capability-owners) | Keep semantic capability ownership across API, host, guest and UI distinct from deployment and file layout. Existing module pages already exist. |
| [g5.architecture.commands](../../../.g5/ontology/software_architecture/boundaries.g5.md#keep-complete-operations-with-their-state-owner), [g5.design.transaction-composition](../../../.g5/ontology/concepts/system_design_principles.g5.md#compose-owners-without-splitting-atomic-effects) | Complete operations may join a caller transaction; ownership does not require a separate commit, schema or database identity per module. |
| [g5.runtime.custody](../../../.g5/ontology/software_architecture/runtime.g5.md#release-capacity-only-with-exit-proof), [g5.runtime.lease.replacement](../../../.g5/ontology/software_architecture/runtime.g5.md#separate-authority-expiry-from-proof-of-exit) | Distinguish control-plane replacement after last-grant expiry from host resource release, which still needs exit proof. |
| [g5.git.durability.no-implicit](../../../.g5/ontology/software_architecture/durability.g5.md#keep-file-saving-separate-from-git-durability) | Save must not introduce implicit commits or pushes. A separate checkpoint/publication request requires its own governing design. |
| [g5.security.read.cache](../../../.g5/ontology/concepts/security.g5.md#read-membership-on-every-check), [g5.governance.approval](../../../.g5/ontology/features/governance.g5.md#approval-is-a-human-authority) | Preserve the per-check membership read and eligible-human approval independent of workspace ownership. Test each authority path, rather than inferring one universal revocation rule. |
| [g5.scope.later-work](../../../.g5/ontology/scope.g5.md#preserve-explicit-later-work), [g5.runtime.control.versions](../../../.g5/ontology/software_architecture/runtime.g5.md#negotiate-control-and-vm-spec-versions-separately) | Multiple API replicas remain deferred; reject unsupported protocol/spec versions. N-1 compatibility and permissive decoding are not existing requirements. |
| [g5.quality.review.guards](../../../.g5/ontology/concepts/coding_principles.g5.md#calibrate-structural-guards-against-harmless-changes), [g5.quality.migrations](../../../.g5/ontology/concepts/coding_principles.g5.md#preserve-deployed-data-through-forward-migrations) | Guards must accept harmless changes; migration SQL and existing checksums remain immutable. |

The independent analysis also makes these existing contracts material to closure:

| Governing contract | Owner / operation and acceptance boundary |
|---|---|
| [g5.agents.lifecycle.cancel](../../../.g5/ontology/features/agents.g5.md#make-cancellation-terminal-for-the-logical-task), [g5.agents.dependencies.settlement](../../../.g5/ontology/features/agents.g5.md#propagate-final-failure-through-pending-dependents) | TaskService settlement/allocation: cancellation cannot become continuation; permanently failed prerequisites cannot leave dependents waiting for impossible success. Held prerequisites remain valid waits. Cancel-versus-Success precedence is unresolved (O2). |
| [g5.lifecycle.start.queue](../../../.g5/ontology/software_architecture/lifecycle.g5.md#queue-starts-within-organization-capacity), [g5.lifecycle.retry](../../../.g5/ontology/software_architecture/lifecycle.g5.md#keep-the-three-retry-levels-distinct), [g5.lifecycle.retry.invalid](../../../.g5/ontology/software_architecture/lifecycle.g5.md#reject-unchanged-invalid-boot-specifications) | Workspace Start: retain FIFO and three definite outer failures; transport replay and uncertain cleanup do not consume that budget, and an unchanged invalid boot specification is not repeatedly launched. |
| [g5.lifecycle.transfer](../../../.g5/ontology/software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release-and-purge), [g5.lifecycle.transfer.failure](../../../.g5/ontology/software_architecture/lifecycle.g5.md#keep-ownership-when-transfer-fails), [g5.data.identity.operation](../../../.g5/ontology/concepts/data_model.g5.md#keep-operation-authority-distinct-from-resource-state) | Transfer owner: preserve old ownership on failure and enforce the operation's current authority; returning to the same owner ID must not itself authorize stale work. Determine rollback/consent semantics without resurrecting consent fenced by Stop. |
| [g5.lifecycle.deadline.rotation](../../../.g5/ontology/software_architecture/lifecycle.g5.md#replace-cloud-capacity-before-draining-it), [g5.architecture.placement](../../../.g5/ontology/software_architecture/worker_host.g5.md#capacity-and-runtime-placement) | Worker/Host placement, drain and rotation: coordinate admission with removal; reconcile what usable replacement capacity means before choosing a readiness gate. Authorized forced loss retains its separate policy. |
| [g5.lifecycle.delete](../../../.g5/ontology/software_architecture/lifecycle.g5.md#deletion-and-offboarding-preserve-outstanding-custody), [g5.fork.claims](../../../.g5/ontology/features/workspaces.g5.md#unfinished-forks-own-durable-claims), [g5.fork.claims.serialization](../../../.g5/ontology/features/workspaces.g5.md#serialize-all-branch-creators-against-claims) | Workspace deletion and fork admission: serialize source validity and durable claims; safely settle failed admission without expiring a legitimate unfinished fork's claims. |
| [g5.git.flush.identity](../../../.g5/ontology/software_architecture/durability.g5.md#freeze-capture-identity-across-retries), [g5.git.flush-failure.block](../../../.g5/ontology/software_architecture/durability.g5.md#persist-deterministic-durability-blocks) | Receipt/retention owners: preserve capture identity and deterministic failure evidence while recovering transient and obsolete-incarnation work. A cleanup label must not conceal an unresolved size block. |
| [g5.conversation.native-private.attempt](../../../.g5/ontology/features/conversations.g5.md#bind-native-questions-to-one-live-attempt), [g5.conversation.reply.replay](../../../.g5/ontology/features/conversations.g5.md#make-repeated-native-answers-idempotent), [g5.agents.invoke.journal](../../../.g5/ontology/features/agents.g5.md#commit-event-projection-with-acknowledgement) | Provider request and task projection owners: distinguish request identity from transport/provider IDs, retain answer replay semantics, and preserve later terminal progress on malformed input. Exact handling of conflicting reused IDs must be settled before implementation. |

The question behind the audit: can G5 modules support independent reasoning through
declared contracts, so that ontology can identify a change's owners and material
consumers? Is the current code modular enough for that, where does it
fall short, and what would a refactoring have to do? A second question, asked alongside:
how should a formal (temporal-logic) model of G5 be used as a way to think about
modularization and abstraction, without writing proofs yet.

## Contents

1. [Summary](#1-summary)
2. [Scope, method and confidence](#2-scope-method-and-confidence)
3. [What "module" means in G5 today](#3-what-module-means-in-g5-today)
4. [Cross-cutting evidence](#4-cross-cutting-evidence)
5. [The design flaw, explained](#5-the-design-flaw-explained)
6. [Findings by area](#6-findings-by-area)
7. [State machines and protocols](#7-state-machines-and-protocols)
8. [Formal verification as a design instrument](#8-formal-verification-as-a-design-instrument)
9. [Gap closure plan](#9-gap-closure-plan)
10. [Ontology impact](#10-ontology-impact-not-updated-could-be-affected)
11. [Appendix](#11-appendix)

Independent-analysis additions: [method](#21-independent-tla-analysis-reported-evidence),
[all 18 findings](#68-independent-state-machine-findings),
[model reuse and validation](#87-incorporating-the-independent-models), and
[ontology questions](#101-ontology-gaps-raised-by-the-independent-analysis).

## 1. Summary

**Assessment.** The supplied audit identifies useful protocol kernels: journal apply with a dense watermark,
host-event ingest that commits before it acknowledges, Git landing by compare-and-swap
with compensation, fork proof refs, a monotonic dispatcher lease, `ScopedDb` tenancy and
a schema with RLS, policies and immutability triggers. Its reported counts of 207 tables
and 127 policies have not been reconciled against a migrated schema; repeated migration
statements must not be counted as distinct live objects.
The recurring concern is incomplete enforcement of ownership around those kernels. State is shared,
transitions are scattered, complete operations are assembled in HTTP handlers, side
effects interleave with database commits, several durable protocols have no recovery
owner, and contracts between components are maintained by hand in two or three places.
The result is a codebase where a single-capability change still touches several crates
and where connecting a specification to all implementing paths requires additional work.

**Historical rubric, not a validated baseline score.** The supplied weights and values
sum to **3.35 / 10**, not 3.5. Keep them as the original reviewer's heuristic assessment;
the scoring anchors and executable measurement receipts were not supplied, and some
premises below needed correction. Do not use this number to claim improvement after
refactoring. Section 9 defines evidence-based closure and comparison instead.

| Criterion | Weight | Score | Basis |
|---|---:|---:|---|
| Changes stay in few modules | 35% | 4 | 35% of changes fit one deployable group; 67% fit two; 13% of backend changes fit one crate |
| Boundaries are enforced | 20% | 4 | Cargo constrains crate dependencies; the audit reports bypasses within core and frontend. Existing visibility and runtime guards still count as enforcement on their covered paths. |
| Code matches the ontology's modules | 20% | 2 | Capability architecture pages exist; the concern is their connection to actual owners and bypasses, not absent module nodes. |
| Explicit contracts between modules | 15% | 3 | `g5-internal-api` is typed; control plane to guest is string-matched on both sides; 425 hand-mirrored TS types |
| Module size and cohesion | 10% | 3 | 153k-line core crate; 11k-line service, page and editor files |

**The recurring flaw.** Named services and some centralized transitions exist, but the
audit reports multiple paths that can bypass those owners.
`workspace_vms` is written from 8 files with 10 differing copies of "end this
incarnation"; `tasks` from 9 files in 6 areas; the flush receipt's transitions from 5
files plus the guest; approval requests from 2 services; document projections from 3
places including a route. Scattered writes make complete enforcement harder and can
force callers to re-derive policy (the incarnation fence is reported at 16 sites,
the "can read workspace" check 6 times with disagreeing answers) and important operations are
assembled at the entry point (273 commit sites in routes against 159 in core). Long
operations then run as call stacks without persisted phases, so a crash leaves them
where they stopped, and the reconcilers that should resume them run without claims.

**Eight themes** (detail in section 5):

| # | Theme | Sharpest evidence |
|---|---|---|
| T1 | State without an owner | 32 tables written from more than one area; `workspace_vms.status` and `worker_hosts.status` each changed from 4 files |
| T2 | Operations assembled in adapters | Delete, merge, Save, send-message and flush proof live in route handlers; Stop is a 360-line inline saga with 3 of ~8 phases persisted |
| T3 | Effects interleaved with commits | Git refs deleted inside the delete transaction before the tombstone; GitHub merge inside an open transaction; events published before commit and dropped between the `api` and `indexer` processes |
| T4 | Recovery without a recovery owner | A live open flush lacks a general retry/retirement path (#146); pending forks advance only on client retry; a rejected dispatch secret can block a tenant's dispatch loop |
| T5 | Duplicated policy, untyped contracts | Dispatcher methods are string literals on both sides (23 vs 38); flush ref layout in 5 files; authority decided from six scope strings |
| T6 | Proof by assumption | Host cleanup paths discard evidence; generation is `txid_current()`; API-key role changes and established-stream authority need distinct lifetime checks |
| T7 | Misplaced responsibilities create cycles | Publication and merge live in `workspace_service.rs`; `WorkspaceVmService` owns a `TaskService` and a second journal reconciliation loop; one daemon binary spans host, root guest and CLI |
| T8 | Broad surfaces and weak internal boundaries | ~1,000 reported public service functions; substantial schema-coupled fixtures; an 11k-line page component |

**What to do first.** Re-establish a reproducible baseline, confirm high-impact findings,
and freeze evaluation tasks before tailoring the refactors. Repair confirmed custody,
authority and data-loss defects without waiting for a repository-wide framework. Then
consolidate selected owners and add targeted guards with valid controls. Physical splits
follow demonstrated ownership and dependency needs. Section 9 gives the closure order,
acceptance cases, skill work and experiment handoff.

**Additional temporal evidence.** The independent TLA+ analysis reports 18 problems,
including overlaps with this audit and new cancellation, dependency, transfer, fork
admission and provider-request cases. It supplies bounded event sequences, not observed
production failures. Its strongest additional design lesson is that a single owner
still needs explicit ordering, replay and terminal-state promises. Section 6.8 maps
every case to the backlog; do not add 18 to the original finding count as unique defects.

## 2. Scope, method and confidence

**Reported original scope.** The supplied audit describes the repository at `main`
`2bd71661` (2026-09-29): 28
Cargo workspace members (about 580k lines of Rust including tests), the React frontend
(about 630 files), migrations, CI configuration and the ontology under `.g5/ontology/`
as the statement of intent. Seven area reviews were run in parallel, each read-only:
control-plane core structure; workspace lifecycle, runtime and hosts; Git durability,
fork and governance; agents, tasks and conversations; documents, platform and
authorization; the frontend; the daemon and shared contract crates. Cross-cutting
metrics were computed with two scripts described in [appendix A](#a-method-and-scripts).

**Evidence classes.** Each claim carries one of:

- *reported measurement*: attributed to a script over the original tree/history;
  the scripts and raw outputs are unavailable here. Regex precision and recall are
  unknown, not established to be within a few percent;
- *source-inspected*: a reviewer read the code path and cites `file:line` at `2bd71661`;
- *inferred*: a runtime consequence that follows from the inspected structure but was
  not executed;
- *unverified*: cited by a reviewer, not re-checked.

**Reported original verification.** The supplied audit says a second reader checked
the highest-severity claims; [appendix B](#b-verification-log) retains its 40 entries,
including one marked unverified. The underlying area reports and review sessions were
not supplied. This review does not independently certify that process or every finding.
Historical line references are for the claimed snapshot; current spot checks are separate.

**Limits.** No service was started, no test was run, and no runtime evidence was
collected. Consequences labelled inferred should be confirmed by a test before a repair
is claimed. The area classification used for the ownership metrics assigns a service
file to a capability by its name prefix (appendix A); a few files are misfiled by that
rule, and the reviews say so where it matters.

### 2.1 Independent TLA+ analysis: reported evidence

Source: [slides.html](slides.html), slides 1–6 and 13–16, dated 2026-09-29 at
`main@412cccae`. The authors report eight parallel modeling agents, followed by a
coordinator rerunning the full check set. States came from database representations,
transitions from code, and obligations from ontology, the v9 design and explicitly
author-proposed rules. They reread code to adjudicate failures, permitting model
corrections rather than deleting a violated rule to manufacture success.

Reported artifacts/results: eight models totaling 5,132 TLA+ lines, about 220 cited
code locations, 25 distinct ontology rules, 21 passing normal checks covering about
19.8 million model states, and 31 intentionally failing problem checks representing
18 findings. Severity is 5 high, 8 medium, 2 low and 3 uncertain. The claimed obligation
sources are 11 ontology, 5 author-proposed general rules and 2 v9 design rules; the
deck does not provide a complete per-finding source ledger. Do not invent that mapping.

The reported counterexamples take 2–16 transitions and at most one second each to
find. One WorkspaceVm check uses two workspaces, organization capacity one, host slots
two and at most two starts; its 1,024,237 states take about 1.5 minutes. These are
bounded abstract executions and selected run costs, not millions of Rust test runs or
an estimate of modeling effort. Section 8.7 records the eight model scopes and reuse work.

**Evidence class: reported model result.** No model sources, configurations, raw TLC
logs or complete counterexample traces accompany the slides in this checkout;
`docs/tla/run-tlc.sh` is referenced but absent. The authors report zero running-system
reproductions. Their line citations and judgments are leads to validate, not new
independent confirmations by this plan. The 21 normal passes cover their configured
properties, not all 18 violated obligations. The 31 failing checks are not 31 unique bugs.

The authors say ontology supplied useful exact rules, known-risk navigation (R01), and
stable IDs connecting intent to code. This is useful qualitative evidence; their
claim that at most five problems could have been found without ontology is an untested
counterfactual, not an ablation result. They report no feature-delivery benchmark,
modularity improvement, production fix or whole-system proof. GitHub comment publication
and organization invitations were started as modeling topics and abandoned; they remain
unassessed by this analysis. Our broader authorization, frontend, publication and agent
efficiency work remains required.

## 3. What "module" means in G5 today

Three useful decompositions overlap: deployment, compilation and semantic ownership.
They answer different questions and need not coincide. The claimed new harness blocks
`g5.harness.module-boundary` and `g5.harness.module-node` are absent from this checkout;
they cannot establish a requirement here.

### 3.1 Deployables and top-level directories

| Group | Paths | Size |
|---|---|---|
| Frontend | `frontend/` | 630 files; `src/components` 287, `lib` 98, `hooks` 57, `pages` 33 |
| Control plane | `backend/g5-api`, `g5-api-core`, `g5-api-routes`, `g5-api-data`, `g5-api-foundation`, `g5-api-contracts`, `g5-authz`, `g5-blob`, `.sqlx/` | core 153k lines, routes 48k, api 95k (94k of it tests) |
| Daemon | `backend/g5d*` (host agent, guest dispatcher, worker, isolation, agent turn, tooling, CLI) | one binary; `g5d-tooling` 47k lines in 18 files |
| Shared crates | `g5-contracts` (29k, 25 dependents), `g5-internal-api` (7.5k, 14), `g5-scm` (13k), `g5-semantic` (15k), `g5-auth`, `g5-storage`, `g5-workspace-ops`, `g5-common` | |
| Operations | `infra/`, `scripts/` | change together (38 co-changes) |
| Tests and tools | `tests/e2e`, `tools/fake-provider-cli`, `tools/validation-checks` | |
| Agent assets | `assets/agents` | 127 files |

### 3.2 Cargo crates: layers, not capabilities

The control plane is cut horizontally. `g5-api-core` holds every service for every
capability, `g5-api-routes` every HTTP handler, `g5-api` the composition root and the
integration tests. A feature therefore crosses all three even when it belongs to one
capability. Only 13 of 99 backend changes on `main` stayed inside one crate.

The dependency graph is acyclic and explicit, which Cargo guarantees. Its shape:

- `g5-contracts` is depended on by 25 of 28 members; `g5-internal-api` by 14; `g5-scm`
  by 12 in the reported inventory. This creates broad potential rebuild impact;
  measure affected build targets and cache behavior rather than assuming every edit
  recompiles the whole workspace.
- `g5-api-core` depends on 12 crates; `g5d-cli`, the daemon's real composition root,
  depends on every `g5d*` crate and is itself depended on by the `g5d` binary.
- Control plane and daemon do not depend on each other except through
  dev-dependencies: `g5-api` dev-depends on `g5d` and `g5d-dispatch` to reuse the
  guest's per-method authority rule in tests (`backend/g5-api/Cargo.toml:92-96`).

### 3.3 Ontology capabilities

The [architecture overview](../../../.g5/ontology/concepts/software_architecture.md)
organizes one control plane around Workspace, Agents, Worker & Host, Git/governance and
Shared platform, with protocol pages for lifecycle, runtime, durability and substrate.
Named services implement parts of these capabilities. Their code is spread across
service files identified only by prefix, route files, maintenance loops and daemon
crates:

| Capability | Where its code lives |
|---|---|
| Workspace | `services/workspace_*.rs`, `workspace_reader.rs`, `workspace_runtime.rs`, `routes/workspaces.rs` (6.7k lines), `routes/files.rs`, `maintenance/workspace_*`, `g5d-dispatch` file and flush modules |
| Agents | `task_lifecycle.rs`, `task_service.rs`, `agent_dispatch.rs`, `chat_service.rs`, `job_*`, `routes/chat.rs`, `maintenance/agent_tasks.rs`, `g5d-agent-turn`, `g5d-dispatch/modules`, `g5d-worker` |
| Worker & Host | `worker_host_service.rs`, `host_control_state.rs`, `ws/host_ctl.rs`, `cloud/`, `routes/worker_hosts.rs`, `g5d-host`, `g5d-isolation` |
| Git/governance/documents | `branch_commit.rs`, `coordinated_repo_publication.rs`, `approval_*`, `remote_integrations/`, `doc_service.rs`, `ontology_service.rs`, `routes/git.rs`, `routes/docs.rs`, `g5-scm`, `g5-workspace-ops`, plus publication code inside `workspace_service.rs` |
| Platform | `auth_service.rs`, `org_service.rs`, `permission_service.rs`, `spend_service.rs`, `notification_service.rs`, `config_service.rs`, `middleware/`, `g5-auth`, `g5-authz` |

### 3.4 What enforces a boundary today

| Mechanism | What it enforces | What it does not |
|---|---|---|
| Cargo | No dependency cycles between crates; explicit manifests | Anything inside a crate; which crate may depend on which |
| `cargo deny` (`deny.toml`, run in Cloud Build) | Licenses, advisories, duplicate versions | Dependency direction between workspace crates |
| `scripts/check-layout.py` | Build inputs exist after moves | Structure |
| `g5d/tests/architecture/source_contracts.rs` | Crate-edge rules for the `g5d` family (shared capability dependencies, no `g5-common` in production, builder-hash coverage) | Trust domains; anything in the control plane |
| `AuditedRouter` / `RequireScopes` | Every route declares scopes or a written reason | Whether the scope is the right authority |
| Database: RLS, CHECK, partial unique indexes, triggers | Tenant isolation; one active agent task per session; immutable attempt identity; one open flush per workspace; acyclic dependencies | Which code may write a table (all writers share the API role) |
| Frontend ESLint | Style | Import boundaries between feature folders (none configured) |

These mechanisms enforce different obligations. The audit identifies gaps in internal
ownership and protocol coverage; that does not establish the absence of all enforcement
inside core, the guest boundary or the frontend.

## 4. Cross-cutting evidence

### 4.1 Change locality (reported measurement)

152 code-touching changes on `main` since the current layout landed on 2026-09-13,
excluding six reorganizations of 300+ files. A change is one first-parent commit (a PR
merge or a direct commit). Documentation, ontology, harness and build files are excluded.

| Module definition | 1 module | 2 | 3 or more |
|---|---:|---:|---:|
| Top-level directory | 41% | 33% | 26% |
| Deployable group (frontend, control plane, daemon, shared, infra, scripts, tests, tools, assets) | 35% | 32% | 33% |
| Crate | 29% | 16% | 55% |

Most frequent co-changes: control plane with frontend (52 changes), infra with scripts
(38), control plane with daemon (34), daemon with frontend (28). Among backend changes,
crates touched: one 13, two 16, three 27, four 13, five 12, six or more 18.

Reading: a browser-to-API-to-VM feature may legitimately cross deployables and crates.
The layered control plane is a candidate source of unnecessary coordination, but counts
alone cannot establish that. Classify representative changes by semantic responsibility,
required contract changes and incidental edits before judging locality.

### 4.2 Data ownership (reported measurement)

Non-test backend code writes 91 tables. 37 have more than one writer file; 32 are
written from more than one area.

| Table | Writer files | Areas |
|---|---:|---|
| `tasks` | 9 | agents, workspace, git/governance, platform (spend), maintenance, `g5-api-data` |
| `workspace_vms` | 8 | workspace, worker host, agents |
| `chat_sessions` | 6 | agents, workspace, platform, maintenance |
| `job_merge_runs` | 6 | agents, git/governance, workspace |
| `worker_hosts` | 5 | worker host, workspace, cloud |
| `jobs` | 5 | agents, documents, platform, maintenance |
| `workspaces` | 5 | workspace, platform, routes |
| `workspace_flushes` | 4 | workspace, maintenance |
| `session_inputs` | 4 | agents (4 files) |
| `task_attempts` | 4 | agents, maintenance |

Status-like columns changed from several files: `worker_hosts.status` (4),
`workspace_vms.status` (4), `org_invitations.status` (3), `job_merge_runs.status` (3),
`tasks.status` (2), `approval_requests.state` (2). Routes write four tables directly
(`workspaces`, `docs`, `doc_node_links`, `ontology_nodes`). About 1,150 SQL call sites
sit in 68 service files; 51 more in 6 route files.

Transactions: the original totals are inconsistent: 388 commit sites overall versus
273 in routes plus 159 in core (432). Recompute with one pinned scope before using these
as a baseline. The report also lists 21 `begin()` sites and 243 functions taking a transaction or connection
parameter (so owners can join a caller's transaction, a strength) and 98 take a bare
pool.

### 4.3 Public surface and coupling (reported measurement)

- `g5-api-core/src/services` exposes 69 `pub mod` modules and about 1,000 public
  functions (90 `pub(crate)`). The ontology's module pages list roughly 50 public
  operations in total.
- Of 142 distinct service-to-service import edges inside core, 91 cross capability
  lines (by name prefix), with mutual dependencies workspace to Git/documents (25 and 12
  edges) and workspace to agents (11 and 7). Some of those edges are misfiled services
  rather than true coupling; the Git and agents reviews separate them in section 5.
- Fourteen core files take the whole `AppState`.

### 4.4 Size (reported measurement, production lines)

| File | Lines |
|---|---:|
| `frontend/src/pages/WorkspacePage.tsx` (the `WorkspacePage` function alone spans lines 5171-11196: 61 `useState`, 54 `useEffect`, 43 `useRef`, 133 `useCallback`, 118 imports from 21 folders) | 11,196 |
| `frontend/src/components/editor/SpecEditor.tsx` (contains the markdown document model) | 11,163 |
| `backend/g5-api-core/src/services/workspace_service.rs` | ~9,800 (+1.1k tests) |
| `backend/g5d-tooling/src/tools/grounding_ops.rs` | ~9,700 (+12k tests) |
| `backend/g5-api-core/src/services/doc_service.rs` (230 functions, 65 SQL sites) | ~9,300 |
| `frontend/src/components/chat/ChatPanel.tsx` | 7,991 |
| `backend/g5-api-routes/src/routes/workspaces.rs` | 6,728 |
| `backend/g5-scm/src/repo.rs` (124 public functions) | ~6,700 (+5k tests) |
| `backend/g5-contracts/src/types/ontology_markup.rs` | 6,136 |

### 4.5 Contracts (reported measurements and source inspection)

- Frontend types are hand-written three times: Rust serde DTOs, 117 Zod schemas in
  `frontend/src/lib/api-schemas.ts` that only tests import, and 425 exports in
  `frontend/src/lib/types.ts` plus DTO shapes declared inline in hooks. No generator
  exists for them; one does exist for the harness catalog (`harnessCatalog.gen.ts`,
  drift-checked in `infra/ci/cloudbuild-ci-rust.yaml`).
- Control plane to guest dispatcher: method names are string literals on both sides
  (23 in `workspace_runtime.rs`, 38 in `g5d-dispatch/src/server.rs`), parameters are
  read as `params["mode"].as_str()`, and the per-method authority table exists only in
  the guest.
- Host control protocol: typed (`g5-internal-api/src/host_ctl.rs`), with one supported
  version and reported strict-decoding annotations. Compatibility depends on the actual
  wire change: existing optional/defaulted fields are counterexamples to a blanket
  claim that every edit requires lockstep deployment. No compatibility matrix was run.
- The flush ref layout is a string literal in 5 files across 4 crates; the receipt DTO
  exists twice; flush error names live in three lists; Git subprocesses are built in
  about 22 files across 11 crates.
- 128 literal `"workspaces:write"` strings in routes; 169 inline `/api/v1` literals in
  the frontend bypassing `routes.ts`.

### 4.6 Background work, realtime and tests (reported measurements)

- 22 background loops in the control plane (16 interval timers in `maintenance/`, three
  two-second reconcilers, cloud loops); 178 `tokio::spawn` sites, 22 of them in route
  files (21 in `routes/preview.rs`).
- Realtime events go through an in-process `WsHub`; Helm deploys `api` and `indexer`
  as separate processes, and the Redis relay section in `ws/mod.rs` is an empty header.
- 88 of 108 API integration test files use raw `sqlx`, with 780 raw write statements
  (47 into `workspace_vms`, 43 into `tasks`).

### 4.7 What the ontology already records (reported metadata counts)

The supplied report counts about 570 blocks, with 32 fully implemented, 317 at 70-94%,
81 below 70%, and 118 unassessed. These bins total 548; the omitted band and denominator
need reconciliation. Footer estimates are not measured test coverage. Eighteen entries
are reported to name a specific bypass or incorrect path, among
them: raw task and workspace writers bypassing complete operations
(`g5.design.minimal-api`), JobService bypassing TaskService allocation
(`g5.architecture.allocation_composition`), route-owned flush verification
(`g5.architecture.runtime_facade`), approval routes requiring workspace ownership
(`g5.architecture.policy`), and `admit_stop` leaving queued consent in place
(`g5.lifecycle.stop`). The supplied audit reports source confirmation; appendix E names
the subset rechecked during this document review.

## 5. The design flaw, explained

Each theme states what was found, which principle it violates, why it matters, and the
refactoring direction. Principles are the ontology's
[system design principles](../../../.g5/ontology/concepts/system_design_principles.g5.md)
and [boundary contracts](../../../.g5/ontology/software_architecture/boundaries.g5.md).
Paths are under `backend/g5-api-core/src/services/` unless a crate is named; `R/`
means `backend/g5-api-routes/src/routes/`.

### T1. State without an owner

**Found.**

- *VM lifecycle.* `workspace_vms` (about 40 columns: lifecycle, custody, lease,
  preview, flush, activity, loss) is written from 8 files, 41 write sites. The "end this
  incarnation" update (about 20 columns set to NULL plus loss rules) exists in 10 copies
  (`worker_host_service.rs:346,459`; `host_control_state.rs:453,603,661`;
  `workspace_vm_service.rs:1817,2609,2855,2971,3221`) and they disagree: a VM lost while
  stopping becomes `stopped` through lease expiry but `failed` through the Exited event;
  some copies fail the Start intent, some requeue it, several never touch it. There is no
  Rust enum for VM or intent status; the literal `status IN ('starting','running','stopping')`
  appears 31 times in 7 files. `start_intent` is JSON written at 12-13 sites in 3 files.
  The epoch is allocated in placement (`worker_host_service.rs:243`), not by the lifecycle.
- *Tasks.* `INSERT INTO tasks` at 5 sites in 3 files; `tasks` written from 9 files in
  6 areas including spend accounting and Codex auth. Control state lives in an untyped
  `tasks.spec.lifecycle` JSON written by 4 files including `workspace_vm_service.rs:1056,1168`;
  `agent_dispatch.rs:407` replaces the whole object. The mapping from stop reason to
  hold reason exists three times with different defaults for unlisted reasons.
- *Transcript.* `chat_messages` is written 10 times in `task_lifecycle.rs` and twice in
  `chat_service.rs`; `MAX(message_index)+1` is computed in four places under different
  locks; platform notices run a second delivery state machine (queued, reserved,
  delivered, withdrawn) inside transcript JSON written from 4 files, with no constraint.
- *Flush receipt.* Start, complete and fail in `workspace_flush_service.rs`; proof in
  `R/workspace_flush.rs::verify_manifest`; ref admission in `R/git.rs`; orphan sweep and
  prune in `maintenance/workspace_flush_retention.rs`; fork-copy receipts inserted by raw
  SQL in `workspace_fork_service.rs:1928-1943`; delete in `workspace_service.rs`; the guest
  keeps its own `CaptureState` with different terminal rules. The "selected durable cut"
  is computed six different ways (reader, runtime, PR readiness via SQL, durable fork,
  restore clean-advance, retention), with different currency and VM-status rules.
- *Approvals.* `approval_requests` transitions written from `approval_config_service.rs`
  (4 sites) and `approval_review_service.rs` (3); request-key hashing duplicated.
- *Documents.* `docs`, `doc_node_links` and `ontology_nodes` written by DocService or
  OntologyService, by `R/internal.rs` (including a delete-all at `:1128`) and by
  maintenance indexers. A document has two identities (UUID and `overlay:` handle) parsed
  twice with different path checks.
- *Hosts.* `worker_hosts.status` written from 4 files; "draining" means two things
  (operator drain sets `drain_id`, rotation does not), and rotation's resume sweep assumes
  it is the only writer.
- *Notifications and spend.* Four raw `INSERT INTO notifications` sites bypass
  `NotificationService` and emit no event. `SpendService::record_usage`,
  `check_spend_limit` and `aggregate_usage` have no production callers; the real usage
  writer is `task_lifecycle.rs:2961-3057`, which turns unknown cost into 0.0, and
  `max_spend_usd` is enforced nowhere.

**Principle.** `g5.design.authority` (one authority per fact), `g5.design.cohesion`,
`g5.architecture.commands` (complete operations with their state owner),
`g5.design.information-hiding`.

**Why it matters.** An invariant such as "a slot is freed only with exit proof" can be
specified regardless of representation, but enforcing it requires accounting for every
path that can release custody. Scattered, divergent transitions increase that work.
Several implementations may refine the same abstract action; the concern is missing or
inconsistent obligations, not that such a mapping is mathematically impossible.

**Direction.** One owner module per state machine: typed state enums, a pure
`transition(state, event) -> (state', effects)`, one persistence adapter that performs
the compare-and-set, and a boundary check that rejects unauthorized writers. Select
database privileges only after checking transaction composition and credential custody;
roles available to the same unrestricted caller do not establish module isolation.
Concretely, investigate an `incarnation` owner for lifecycle fields in `workspace_vms`;
`TaskService::allocate` and typed `task_control` columns; `ChatService::append_accepted_turn`
and `settle_assistant`; a `DurableCut::select(tx, workspace, purpose)` used by all six
callers; one `ApprovalRequests` aggregate; a `DocumentCatalog` as sole projection writer;
`SpendService` as the only usage writer.

### T2. Operations assembled in adapters

**Found.**

- Routes hold 273 commit sites (core 159) and 210 `ensure_*` policy calls (39
  `ensure_workspace_owner`).
- `R/chat.rs:1572-1745` composes a follow-up message from launch resolution, TaskService
  enqueue and a ChatService patch, records the cross-service lock order in a comment, and
  has no request key; `WorkspaceMessenger::send` re-implements the same path in another
  order.
- Delete chains stop, purge and tombstone across three commits in
  `R/workspaces.rs:3847-3888`, tombstone last; an interrupted delete is never resumed.
- The merge protocol runs in `R/workspaces.rs:5380-5550` across at least three
  transactions, calls another route module, and the three merge and publication services
  contain no approval check at all; merges settle without current approvals.
- Flush proof (`verify_manifest`) runs in the route while `WorkspaceFlushService::complete`
  is `pub` and trusts its caller; flush-ref admission is raw SQL in `R/git.rs:1391-1424`
  while `git_policy.rs` is 32 lines.
- The editor Save write primitive lives in the routes crate and is called from other
  routes (`R/docs.rs`, `R/ontology.rs`, `R/files.rs`); the issue-comment publication
  broker lives in `R/external_issues.rs:976-1092` and is imported by `R/internal.rs`.
- The capacity-stop route constructs a `WorkspaceVmService` itself; forced host
  termination is assembled in `R/worker_hosts.rs:1105-1200`.
- Stop (`workspace_vm_service.rs:473-831`) is a 360-line inline saga of about ten
  phases with a flush retry loop and sleeps; three phases are persisted; the
  defer/abort/record-loss block is copied four times; the HTTP handler waits for the whole
  saga; `reconcile_stopping` selects rows without claiming them and runs the same `stop()`,
  the only guard against two drivers being `stop_retry_at = now()+320s`.

**Principle.** `g5.design.adapters`, `g5.architecture.commands`, `g5.architecture.api_layers`,
`g5.design.enforcement`.

**Why it matters.** A new entry path can omit guards when the shared operation does not
own them (section T5). A call stack is not durable progress evidence; inspect what is
already persisted and which external outcomes can be reconciled before deciding what
additional phases or driver coordination are needed.

**Direction.** Per-capability command operations that take an `Actor`, check policy
themselves, and either commit or join the caller's transaction (`send_turn`,
`delete_workspace(op)`, `complete_flush_verified`, `FileMutationService::apply`,
`IssuePublicationService`). Routes shrink to translate, call one operation, map the
result. Evaluate persisted Stop progress and fenced driver claims. Changing the response
to 202 is a public API change, not a necessary consequence of modularization; preserve
current caller semantics unless the selected design explicitly changes them.

### T3. Effects interleaved with commits

**Found.**

- `delete_workspace` deletes Git refs (`workspace_service.rs:3652`) inside the caller's
  transaction before five more statements and before the route commits.
- `merge_provider_pull_requests(db: &mut ScopedDb, ..)` performs the GitHub merge over
  HTTP inside a transaction the route opened and commits afterwards; PR creation and
  source pushes run inside the uncommitted PR transaction; provider failures are only
  logged and no loop reconciles PRs.
- `create_doc` keeps its insert transaction open across the VM write.
- `WorkspaceRuntime::call` holds `FOR SHARE` on the VM row for the whole dispatcher RPC
  (`workspace_runtime.rs:311-334,488-493`) on the 20-connection system pool that host
  lease renewal also uses; `record_start_intent` holds `FOR UPDATE` while reading Git.
- `NotificationService::create_notification` and `CommentService::create_comment`
  publish events inside the caller's transaction; `ScopedDb` has `on_rollback` but no
  after-commit hook. Tasks and chat, by contrast, publish after commit.
- `api` and `indexer` run as separate processes with separate in-process hubs; indexer
  loops publish `OntologyChanged` and `RemoteTipAdvisoryUpdated` into a hub with no
  clients; the Redis relay is an empty section.
- The internal HTTP client retries POSTs on 5xx or timeout without an idempotency key;
  usage reporting inserts a fresh `Uuid::now_v7()` row each time, so an ambiguous reply
  duplicates usage rows and task costs.

**Principle.** `g5.design.transaction-composition`, `g5.design.commit-notify`,
`g5.architecture.recovery`, `g5.design.cleanup-proof`.

**Why it matters.** A commit that fails after an irreversible effect leaves state the
system cannot describe (a live workspace whose branch and flush refs are gone). Events
before commit let clients see rows that were rolled back; events that never leave the
indexer process are silently lost in production. Locks held across RPCs turn one slow
guest into a stall of lease renewal for every VM on that host (inferred).

**Direction.** For effects that must survive process loss, persist the owner's intent
atomically and give delivery/reconciliation a durable owner. An after-commit callback
alone can lose an effect if the process crashes after commit. An outbox normally allows
repeated delivery; bind replay to operation kind, actor/resource scope and a payload
digest, and reconcile ambiguous remote acceptance rather than blindly resending.
Apply idempotency to effectful retryable operations, not to POST as a syntax rule.
Separate slow RPCs from locks only with a replacement authority/fencing protocol that
prevents check/use races and preserves the documented callback-lock exception.

### T4. Recovery without a recovery owner

**Found.**

- *Flush.* The one-open-flush unique index is per workspace, not per incarnation
  (`003_p4_1_runtime_schema.sql`); `fail` accepts only `FLUSH_TOO_LARGE`; the orphan
  sweep closes rows only for non-live incarnations older than 30 minutes; a new request
  key makes the guest call `start` again, the API answers `FLUSH_IN_PROGRESS`, the guest
  does not map it, and the host treats it as non-transport and calls `abort_stop`; live
  fork turns any runtime error into a terminal abort. Consequence (inferred): one
  transient Git failure during a live fork makes every later Stop or live fork on that
  incarnation abort until deadline teardown records loss; after VM loss the successor
  waits up to about 35 minutes. This is issue #146.
- *Fork.* No worker scans `workspace_forks`; `pending` and `aborting` advance only when
  a client retries; there is no attempt counter; an unfinished fork blocks source
  deletion, pins receipts and holds name claims indefinitely.
- *Dispatch.* Invoke parameters are rebuilt on every replay and no digest is checked;
  the replay "lease" is `updated_at <= now()-2s`, rolled back on any error so the same row
  is selected first again, and `dispatch_ready_tasks` then breaks out of the tenant loop;
  invoke errors are not classified; only one kind of projection rejection is consumed from
  the journal. Consequence (inferred): a rejected dispatch secret or an account that
  became invisible after reservation blocks all agent dispatch for that tenant; a
  malformed module frame pins its attempt, slot and session until VM loss.
- *Delivery.* The agent-turn runtime writes and flushes the prompt to provider stdin
  before it journals `input.accepted`; a dispatcher restart settles lost supervision as
  `interrupted`, which requeues the input. A crash between the flush and the journal
  append therefore redelivers input the provider may already have received, without
  asking the owner (`g5.conversation.delivery.unknown`).
- *Reconcilers.* The lifecycle loop runs seven reconcilers sequentially per org and a
  `?` on one org aborts the tick; `reconcile_stopping` and `reconcile_start_queue` claim
  nothing; rotation takes a session advisory lock on a pooled connection; the replica
  model is undeclared (some code assumes several API replicas, scope defers them).
- *Others.* An issue-comment retry after a crash returns `pending` without sending;
  `session_manifest_generation` is never incremented, so the bundle publication fence
  does nothing and `PersistFailed` is only read for the same task, which a new chat turn
  never is; `attempts` on Start consent is incremented but never read, and the first
  definite failure ends consent instead of the three failures `g5.lifecycle.retry` allows;
  `excluded_host_ids` is read but never written.

**Principle.** `g5.architecture.recovery`, `g5.architecture.recovery.progress`,
`g5.design.durability`, `g5.conversation.delivery.unknown`.

**Why it matters.** These are candidate liveness failures for an explicit model: an
open row that no action can close, a pending operation with no fair actor, a poison entry
that starves its queue. Each one currently ends in a human intervention or a timeout.

**Direction.** Give each protocol durable progress and an accountable recovery owner,
with per-item failure isolation and fenced claims where drivers can overlap. Reuse an
existing owner/outbox when sufficient; a universal operations table is an option, not
a prerequisite. Consider an explicit `state` column on flushes
with a supersede rule; retirement of open flushes inside the release transaction; a fork
recovery worker with attempt count and escalation; persisted canonical invoke parameters
and digest; transient/permanent error classification with `next_attempt_at` backoff; a
synchronous `input.handoff_started` fact and a pure `delivery_after(settlement, facts)`.

### T5. Duplicated policy and untyped contracts

**Found.**

- The owner/vm/epoch/status/lease incarnation fence is re-implemented in
  `workspace_runtime.rs` (three places), `workspace_flush_service.rs`,
  `host_control_state.rs`, `R/internal.rs` (two places) and `R/git.rs` (two places), with
  different accepted status sets, clocks (`now()` vs `clock_timestamp()`) and transfer
  handling; the lease-expiry check appears in 16 sites across 9 files. `R/internal.rs:656-709`
  accepts VM tokens while `starting`/`stopping` and ignores a pending transfer; the
  facade refuses both. Different status sets are not automatically defects: current
  `g5.security.admission` permits lifecycle callbacks while starting/draining without
  admitting new user work. Compare equivalent actions before consolidating these checks.
- Authority is decided from six scope strings in `db/scoped.rs:114-167`
  (`workspaces:write`, `jobs:execute`, `jobs:write`, `tasks:write`, `sessions:write`,
  `docs:write`), three of which are not in the typed `Scope` enum; the "can read
  workspace" check has six copies, and three of them use the historical `created_by`
  instead of the current owner although the primary copy explicitly rejects `created_by`;
  the role-to-scope tables in `auth_service.rs` and `permission_service.rs` differ
  (`jobs:execute`). Consequence (inferred from source): after a transfer, the original creator still
  passes `PermissionService`; recording an approval decision, mark-all-read and comment
  replies all require workspace ownership, and code passes `workspaces:read` for a write
  to obtain reader semantics.
- The control-plane-to-dispatcher protocol is string literals on both sides; a
  300-line string `match` in `g5d-dispatch/src/server.rs:1487-1784`; the envelope is built
  with `json!` and parsed by a private `Context`; deadlines are absolute wall-clock
  milliseconds judged by the guest clock while host control uses relative budgets.
- The flush ref layout is a literal in 5 files; `FlushRepoReceipt` and `VmRestoreRepoSpec`
  describe the same receipt; three lists of flush error names; three Git stacks (git CLI
  in guest, route and host; git2 through `g5-scm`); about 40 path-normalization functions
  across 11 crates; front-matter parsing in 5 places, cycle detection in 5.
- Frontend: the TS event union differs from Rust `WsEvent` (the client ignores
  `workspace.policy_changed`, `job.paused`, `job.resumed`, and handles
  `workspace.authoritative_cut_advanced`, which nothing emits); errors are classified by
  message text (`/push review again/i`); the sidebar file delete sends no `expectedHash`
  although the dispatcher requires one (inferred: refused); the changes report reaches the
  browser in snake_case straight from a `json!` in the dispatcher.
- Provider policy is split between `agent_dispatch.rs:1777-1819` and
  `g5d-agent-turn/src/policy.rs`; Codex subscription login is woven into generic code
  (`task_attempts.codex_auth_state`, `auth_before_delivery`, `supersede_codex_auth` inside
  the generic enqueue); `g5d-agent-turn/src/runtime.rs` has 27 `ProviderId` branches and no
  adapter trait; `AgentTurnInputV1`/`OutcomeV1` exist only in the daemon and the API
  builds them with `json!`.
- `g5-contracts` says it performs no parsing yet holds a 6,136-line ontology parser;
  `G5Error` carries HTTP status codes with 75 `axum`/`StatusCode` references and is the
  error type of 10 daemon crates; by a name-based scan only about 46 of roughly 440 public
  items are used by both control plane and daemon; `g5-common` is a re-export facade used
  only as a dev-dependency; 335 `Conflict{reason: String}` constructions; about 45
  environment reads outside `AppConfig`, some per request.

**Principle.** `g5.design.information-hiding`, `g5.design.minimal-api`,
`g5.design.evolution`, `g5.design.change-locality`, `g5.architecture.policy`,
`g5.security.admission`.

**Why it matters.** Duplicated policy has already produced contradictory answers to
security questions. Untyped contracts make renames break at runtime, force the API tests
to compile the daemon, and let the frontend drift without any test noticing. A shared
broad shared crate can increase rebuild and coordination cost; actual impact depends
on consumers and the selected build. Measure that cost before choosing an extraction.

**Direction.** Investigate purpose-specific checks behind `IncarnationFence::lock(tx, claims, purpose)`; a pure
`decide(principal, action, &AuthorityFacts)` with `scoped_db_for_*` taking an `Action`;
a typed call enum, result and error types and a data-driven authority table for the
dispatcher protocol in a shared crate, with relative budgets stamped on receipt; ref
names, receipt types with validating constructors and a `FlushErrorCode` enum in the
contracts crate; generated TypeScript for `WsEvent`, error codes and the most-used DTOs
by extending the existing generator, with an exhaustive event switch; a provider-profile
trait and `agent.turn` contracts in `g5-internal-api`; `g5-contracts` split into a small
id/value crate plus one wire crate per boundary, API DTOs moved to `g5-api-contracts`,
the parser to `g5-semantic` where this improves a demonstrated dependency boundary.
These are candidates: do not share policy merely because code looks similar, assume
the harness catalog generator handles arbitrary DTOs, or split every wire contract
into a crate without a concrete consumer and compatibility argument.

### T6. Proof by assumption

**Found.**

- In the host's VM start path any boot error, including a failed child reap, removes
  the VM record, which frees the port, the slot and the workspace lock, and journals
  `BootFailed` or `Exited`; cancellation paths ignore the result of `kill`
  (`g5d-host/src/workspace_vm.rs:342,351`); `purge` and expired-root pruning drop the
  inventory entry before deleting files, and a retry after a failed delete returns
  `NotFound`, which the API counts as purged; journal writes for `Exited`/`BootFailed` are
  discarded with `let _`; the API also treats disappearance from the heartbeat as purged,
  and `workspace_vm_service.rs:1787-1790` states that lease expiry "proves the old
  incarnation no longer has custody", which contradicts `g5.runtime.lease.replacement`.
  Inferred: two VMMs can share a root or port; transfer and delete can proceed while the
  old owner's credential-bearing root is still on disk.
- The heartbeat "generation" is `txid_current()` (`host_control_state.rs:532-538`); the
  host kills VMs when `generation > previous`; transaction ids are ordered by assignment,
  not commit, so the ordering is not guaranteed. The host's command-result cache is in
  memory for 600 s and returns stale `Accepted`/`Busy` answers to retries that reuse the
  key; `vm.start` admission checks only the in-memory active map with no tombstones, so a
  late start boots an orphan until the next heartbeat reconcile; the incarnation fence
  is an `AtomicU64`.
- Delivery: acceptance is journaled after the prompt bytes were flushed (T4).
- Authorization: API-key authentication creates synthetic claims with `exp: i64::MAX`,
  **after** `ApiKeyService::validate_api_key` checks database `revoked_at` and optional
  `expires_at`. This does not establish that API keys never expire or cannot be revoked.
  `InProcessAuthz::is_org_member` caches with a 60-second TTL, consistent with the
  declared membership-cache allowance. Still investigate stored scopes after role
  changes and established WebSocket subscriptions separately; neither issue is settled
  by checking a synthetic JWT expiry or the membership cache alone.
- Editor Save: the request carries only content and `expected_hash`; `expected_vm_id`
  is resolved from a handle inside the request, not from the editor; the idempotency key
  is `workspace-file:{op}:{Uuid::new_v4()}` so the dispatcher's replay cache never serves
  a client retry; the branch check is a separate RPC with its own handle lookup; the
  authorizing transaction rolls back before the write. Inferred: a delayed Save whose
  base bytes match writes into a replacement VM. On the client, `SpecEditor.tsx:6919-6928`
  stores the post-request buffer as the saved baseline and clears dirty, so text typed
  during a save shows as saved; `CodeEditor.tsx:755-763` stores the submitted bytes.

**Principle.** `g5.design.cleanup-proof`, `g5.runtime.custody`, `g5.runtime.purge`,
`g5.runtime.control.replay`, `g5.security.read.cache`, `g5.documents.save`,
`g5.documents.save.acknowledgment`.

**Why it matters.** Each of these is a safety property whose proof the code assumes
rather than carries: "the child is dead", "the files are gone", "the newer generation
is later", "the token still represents this person", "the bytes went to the VM the user
saw". A faithful model with relevant crash/delay actions can challenge these claims;
no counterexample or runtime reproduction has been executed in this review.

**Direction.** Receipts as types: a `Custody` handle releasable only with a
`ChildReapReceipt`; `ReapUncertain` and `PurgePending{generation}` states; journal
before release; a generation tied to a serialized authoritative snapshot, with an
explicit stale-message admission rule; host tombstones and persisted command outcomes;
a synchronous handoff fact; operation-specific role/membership revalidation and stream
revocation where the contract requires it. An authority epoch is a candidate mechanism,
not an automatic new requirement. A database sequence alone is not a fix for snapshot
ordering: allocation and commit order differ ([PostgreSQL sequence guidance](https://wiki.postgresql.org/wiki/FAQ)).
For Save, carry
`{vmId, epoch}` in the file read DTO and the Save request, a client-generated request key,
and one dispatcher operation that checks branch and hash atomically.

### T7. Misplaced responsibilities create cycles

**Found.**

- *Workspace and Git.* `workspace_service.rs` contains `publish_branch_repo`, a duplicate
  `create_only_publish_head`, `push_to_remote`, `approved_rebase_publish`,
  `merge_workspace` and `prepare_pr_source_publication`, while Git services import the
  workspace reader, the repository inventory and `ensure_workspace_owner` (four times in
  `remote_integrations/merge.rs`), plus a pure forwarding shim (`branch_commit.rs` to
  `workspace_ops.rs` to `remote_integrations`). The ontology diagram says Workspace depends
  on Git while the Git page says Git consumes "Workspace reading".
- *Workspace and Agents.* `WorkspaceVmService` holds a `TaskService` and runs its own
  replay/apply/ack/settle loop (`workspace_vm_service.rs:1198-1423`) that copies
  `maintenance/agent_tasks.rs` but diverges (it treats a dispatcher `unknown` as VM loss
  unconditionally and publishes no events); `pub(super)` free functions in `task_lifecycle`,
  some taking a raw `PgConnection`, are called from the VM service (5) and host control
  state (3), including releasing held work on the host's VM Ready event rather than an
  owner Resume; `WorkspaceMessenger` orchestrates Chat, Task and Job services;
  `JobMergeService` is Git merge orchestration and `IntentLogService` a document projection,
  both filed under Agents. `task_lifecycle` in turn writes `workspace_vms.preview_intent`.
- *Documents and platform.* `doc_service.rs` imports `home_service` and `home_service.rs`
  imports `DocService`; `ConfigService` and `ProviderAccountService` depend on each other
  with the encryption key held in `ConfigService`; DocService writes `jobs.target_doc_id`
  and `timeline_entries` directly.
- *External issues* ship about 5.4k lines including agent endpoints while the ontology
  node still says nothing is implemented; agents post free text as the owner with the
  owner's GitHub token behind a hidden marker.
- *Daemon.* One executable serves four trust domains: `g5d/src/main.rs` hands off to
  `g5d-cli`, whose dispatch routes the privileged host supervisor, the guest dispatcher
  (root inside the VM), guest modules, the VMM child and about 18 developer commands; the
  only guard on the privileged entrypoints is a working-directory check
  (`g5d-cli/src/stable_context.rs:203-208`). `g5d-host` declares a dependency on
  `g5d-worker` and `g5d-worker` on `redis` without using them; about 25k lines of code
  analyzers sit in the `g5d` facade crate; the guest flush runs `git -C <repo>` as the
  dispatcher's own identity (root when euid is 0) reading repository-controlled
  configuration (issue #176).

**Principle.** `g5.design.dependencies`, `g5.architecture.dependencies`,
`g5.design.concerns`, `g5.design.cohesion`, least privilege
(`g5.security.mvp-exposure.root`).

**Why it matters.** Cycles can reveal misplaced decisions or missing lower-level
contracts, but imports alone do not prove that independent reasoning is impossible.
Trace the actual obligations and variants before breaking a dependency. A shared binary
can broaden the delivered code and qualification surface; it is not itself proof of
cross-domain authority. Inspect credentials, reachable entrypoints and deployment.

**Direction.** Investigate a lower-level repository-inventory contract (membership,
mapped branch, binding names, OID reads) that Workspace and Git can both consume. Keep
publication, merge and remote tracking with their semantic owner. If callers supply an
authorization capability, define its scope/lifetime and the execution-time checks;
a token is not a substitute for current authority. Consider an Agents-owned collaboration
port for prepare/quiesce/settle/abort and incarnation events, with one execution-recovery
owner. Separate DocService responsibilities where consumers benefit. Evaluate daemon
binary separation against actual privilege and rollout needs. For platform-owned Git
materialization/capture, centralize required hardening and credential handling; do not
disable legitimate customer Git configuration for every native Git operation. Test the
operation-specific boundary before mandating one executor or banning all subprocess
construction elsewhere.

### T8. Broad surfaces and weak internal boundaries

**Found.**

- Layered control-plane crates (section 3.2); about 1,000 public service
  functions and 69 public modules; 14 core files take the whole `AppState`.
- God files (section 4.4). `WorkspacePage` alone imports from 21 component folders and
  is imported by 26 test files; `editor` and `workspace` import each other; `shared/`
  imports from `editor` and `execution`; selection (tab, session, task) is written by the
  URL, component state and localStorage, with races patched by refs.
- 88 of 108 API test files use raw `sqlx` (780 writes); helpers copy the ChatService
  lock key; `route_auth_static.rs` pins authorization checks to route handlers. Tests can
  build states the domain cannot reach, and the ownership refactors will break the seeds.
- The audit reports about 1,010 frontend comments citing unavailable design IDs and
  stale paths in `FRONTEND_AUDIT.md`. This is a documentation/navigation gap, not absence
  of all current frontend requirements: feature ontology already governs user behavior.
- Queue and retry policy is partly dead code: FIFO exists only as the reconciler's
  `ORDER BY`, `place()` has no queue-position check, `StopReason::Rollout` and
  `StartKind::RolloutContinuation` are constructed only in tests.

**Principle.** `g5.design.minimal-api`, `g5.design.enforcement`, `g5.design.cohesion`,
`g5.design.minimal-mechanism`.

**Why it matters.** Broad visibility can let callers bypass a service owner.
`pub(crate)` still permits every module in that crate to call the item; privacy of
writers and raw connections must match the intended module boundary. Schema-coupled
fixtures add maintenance work, but raw writes are legitimate in migration tests and
explicit tests of corruption, RLS and forbidden paths.

**Direction.** Narrow visibility to the actual owner and expose sufficient complete
operations. Prefer domain fixture builders for ordinary valid-state setup, retaining
labelled raw fixtures where they are the behavior under test. Add calibrated import
checks. Investigate a single navigation-state owner and shared editor-session behavior;
choosing the URL, a content provider or a new directory is an implementation decision
that must preserve existing flows. Remove an unused variant only after distinguishing
dead code from an unimplemented governing obligation.

## 6. Findings by area

Severity and "verified" labels below are inherited from the supplied audit. "Verified"
means its reported second-reader source inspection, not runtime reproduction or a
recheck on this checkout. Appendix E records this review's narrower evidence and
supersedes corrected claims. Directions are summarized in section 5.

### 6.1 Control-plane core

| ID | Finding | Sev. | Status |
|---|---|---|---|
| C1 | VM lifecycle has no single owner: 8 writer files, 10 differing release copies, raw string states | high | verified |
| C2 | Lifecycle sagas run inline, unpersisted, and block each other; one hung guest can stall the lifecycle tick for every org (inferred) | high | verified (structure) |
| C3 | Routes own commits (273 vs 159) and policy (210 `ensure_*`); send-message, merge, delete and flush proof composed in handlers | high | verified |
| C4 | Irreversible effects inside open transactions: ref deletion before tombstone; RPC-long `FOR SHARE`; `FOR UPDATE` while reading Git | high | verified |
| C5 | Realtime events process-local and sometimes pre-commit; `api`/`indexer` hubs disconnected; empty Redis relay | medium | verified |
| C6 | Other modules write Agents state (`tasks` writer count needs reconciliation with section 4's 9 files; `spec.lifecycle` JSON) | medium | original source report; count unresolved |
| C7 | 22 background loops with ad-hoc concurrency control; unclaimed reconcilers; undeclared replica model | medium | source-inspected |
| C8 | Untyped contracts, errors and configuration: string-matched dispatcher codes; 335 string conflicts; ~45 env reads outside `AppConfig` | medium | source-inspected |
| C9 | Tests coupled to the schema (88/108 files, 780 raw writes) | medium | verified |

### 6.2 Workspace lifecycle, runtime and hosts

| ID | Finding | Sev. | Status |
|---|---|---|---|
| L1 | No single owner for VM and consent state; several release paths never touch `start_intent`, so a queued request can disappear | high | verified |
| L2 | Stop neither complete nor fenced: `admit_stop` early-returns for stopped/failed rows without cancelling queued consent; `vm.stop` sent without checking the phase update's row count | high | verified |
| L3 | Host reports exit and purge without proof; API trusts it | high | verified |
| L4 | Incarnation check copied in 9 places; facade holds a row lock across RPCs, serializing the host control loop (inferred) | medium-high | verified (copies) |
| L5 | Multi-step operations and cross-capability writes in routes and the VM service (delete, forced termination, task interrupt JSON, chat notices) | medium | source-inspected |
| L6 | Host command replay and reconcile weaker than they look: in-memory cache, no tombstones, `txid_current()` generation | medium | verified |
| L7 | Queue and retry policy partly dead code | medium | source-inspected |

### 6.3 Git durability, fork and governance

| ID | Finding | Sev. | Status |
|---|---|---|---|
| G1 | Live open flush lacks a general retry/retirement path; size failure and non-live orphan sweep are distinct existing exits | high | original source report; reproduce live stranding |
| G2 | Receipt proof, write admission and failure state have no single owner; `flush_block` has two writers with different fingerprints | high | verified |
| G3 | The selected durable cut is computed six ways; receipts have two writers | high | source-inspected |
| G4 | Merge and publication in a route with no durable identity and no approval gate; provider HTTP inside open transactions | high | verified |
| G5 | Fork has sound Git proofs but no recovery owner, and finalization writes other modules' tables | medium | source-inspected |
| G6 | Workspace and Git cycle from misplaced operations and a shared inventory in the wrong module | medium-high | verified (edges) |
| G7 | Approval requests have two writers; authority decided in the route | medium | verified |
| G8 | Git protocol duplicated across control plane and daemon (ref literal in 5 files, receipt DTO twice, three Git stacks) | medium | verified |

### 6.4 Agents, tasks and conversations

| ID | Finding | Sev. | Status |
|---|---|---|---|
| A1 | Five task allocators; JobService inserts workflow inputs without `task_id`, which reservation requires, so its agent tasks stay `queued` (static reading) | high | verified |
| A2 | Lifecycle control state is untyped JSON in `tasks.spec` written by four files; three copies of the stop-reason mapping with different defaults | high | verified |
| A3 | Workspace and Host drive agent transitions and duplicate journal reconciliation with divergent rules | high | source-inspected |
| A4 | "No acceptance fact" treated as "not delivered": prompt flushed to stdin before `input.accepted` is journaled | high | verified |
| A5 | Invoke replay rebuilt rather than frozen; no poison handling; a permanent error re-selects the same row and breaks the tenant loop | high | source-inspected |
| A6 | TaskService owns the transcript in practice; notice delivery hidden in transcript JSON; chat fork writes `completed` against a CHECK allowing `complete` | medium | verified |
| A7 | Provider-specific logic in generic services; `agent.turn` contract only in the daemon | medium | source-inspected |
| A8 | Session bundle generation never advances | medium | verified |

### 6.5 Documents, platform, authorization and external issues

| ID | Finding | Sev. | Status |
|---|---|---|---|
| P1 | No single policy decision point; authority read from six scope strings; approvals, mark-all-read and comments wrongly require ownership; the approval path without a request id accepts any org member | high | verified |
| P2 | Permission checks copied and disagreeing (`created_by` vs owner; differing role tables; VM-token admission differs between routes and facade) | high | source-inspected |
| P3 | Role changes and established-stream authority need end-to-end checks; API-key expiry/revocation are checked in the database, and membership cache TTL is 60 seconds | high if bypass reproduced | original broad claim corrected; remaining paths unverified |
| P4 | Save does not bind the VM the editor saw; fresh idempotency key per request; branch check a separate RPC | high | verified |
| P5 | DocService spans several capabilities; projections have several writers; docs/home cycle | medium | verified (size, cycle) |
| P6 | Notifications and comments publish before commit; four raw notification inserts push nothing | medium | verified |
| P7 | Spend: declared owner has no callers; real writer elsewhere; limits unenforced | medium | verified |
| P8 | External issues shipped ahead of a "future" spec; publication broker in routes; agent attribution unsettled | medium | source-inspected |

### 6.6 Frontend

| ID | Finding | Sev. | Status |
|---|---|---|---|
| U1 | Editor Save has no single owner: two divergent state machines; no VM identity in reads or saves; results applied to caches unconditionally; unsaved buffer dies with the tab; no `beforeunload`; suggestion Accept ignores the dirty registry | high | verified (SpecEditor vs CodeEditor) |
| U2 | Hand-mirrored contract drifts silently: Zod schemas used only by tests; TS event union differs from Rust; errors matched by text; sidebar delete lacks `expectedHash` | high | verified (schemas, generator) |
| U3 | Realtime events unsequenced and droppable (1000 drops/minute before close); no refetch on first subscribe; `chat.chunk` without offset; ~45 cache keys hard-coded in one switch | high | verified (drop policy) |
| U4 | Oversized components and an implicit navigation state machine with three writers of selection | high | verified (sizes) |
| U5 | Domain rules recomputed in the UI and already disagreeing ("blocked" finished vs active; approval gates recomputed from decision log) | medium | source-inspected |
| U6 | A request that commits and pushes is modeled as a cached query (`useReviewPreview`), re-triggered by invalidations | medium | verified |
| U7 | Auth state split across two refresh paths and three token stores; JWT persisted to localStorage against its own comment | medium | source-inspected |
| U8 | Stale client design references need reconciliation with current feature contracts (about 1,010 comments reported to cite absent design ids) | medium | original source report; count not rerun |

### 6.7 Daemon and shared contracts

| ID | Finding | Sev. | Status |
|---|---|---|---|
| D1 | One executable and one crate graph span four trust domains; privileged entrypoints guarded by a working-directory check; unused declared dependencies | high | verified |
| D2 | One supported protocol version and strict decoding require a change-specific compatibility matrix; blanket lockstep and automatic N-1 requirements are unsupported | depends on changed contract | source-inspected declarations; mixed-version behavior unexecuted |
| D3 | Control-plane-to-dispatcher protocol untyped and defined twice; authority rules only in the guest; absolute deadlines by guest clock | high | verified (literals) |
| D4 | Privileged guest Git runs repository-controlled configuration (#176); Git subprocesses built in ~22 files | high | source-inspected |
| D5 | Custody and purge released without proof (same paths as L3, from the host side) | high | verified |
| D6 | Replay identity volatile or missing: in-memory caches, `AtomicU64` fence, internal POST retries without keys, guest journal on the deleted rootfs | medium | verified (usage insert, journal path) |
| D7 | Shared crates are grab-bags; daemon logic duplicated (parsers, path normalization, front matter, cycle detection) | medium | verified (parser, error coupling) |

### 6.8 Independent state-machine findings

`SM01`–`SM18` are stable references assigned here in slide order (six detailed cases,
then twelve listed cases); they are not IDs supplied by the authors. All are **reported
model results, not runtime-reproduced findings**. Severity below is the authors'
assessment; the three uncertain cases retain that status. "Overlap" identifies an
existing specific concern; "extends" adds a case to a broader concern; "new" means
the concrete failure was not previously recorded here. Related original IDs remain
unchanged. Each row's last column is a required closure challenge, not evidence of a fix.

| ID / slide | Model / severity | Reported failure sequence and source detail | Relationship / responsible owner | Closure challenge |
|---|---|---|---|---|
| SM01 / 7 | HostVm / high | Stop during boot; successful boot then removes the active record before awaiting kill (`workspace_vm.rs:349–351`). A second workspace can take the released slot while the first child lives. Reported traces: 11 steps to early release, 16 to two VMs on one slot; no kill failure required. | Extends L3/D5 and R01; Host VM custody | Pause a successful kill before exit, attempt a competing Start, and retain slot/port/exclusion until proof. Also retain the failed-reap case and allow reuse after proved exit. |
| SM02 / 8 | WorkspaceVm / high | Start queues; Stop returns "already stopped" without cancelling consent (`workspace_vm_service.rs:2685`); queue reconciliation (`:2100`) later launches it. Three steps; authors report no existing test. | Overlap L2; Workspace lifecycle | Queue Start on stopped and failed rows, issue Stop, then release capacity: old consent cannot launch. Fresh explicit Start still works. Verify test-coverage claims during implementation. |
| SM03 / 9 | TaskLifecycle / high | Dispatcher interruption occurs, Cancel sets a flag, then result application chooses `waiting` and ignores it (`task_lifecycle.rs:3600`). Continue or VM restart clears the flag and relaunches. Nine steps; also reported for native-question timeout/suspension, but not command tasks. | New concrete case under A2/A3; TaskService | Interleave Cancel with interrupted and suspended outcomes and restart/replay. Accepted cancellation remains terminal; uncancelled recoverable interruption still permits continuation. Keep Cancel-versus-Success policy separate (O2). |
| SM04 / 10 | WorkerHost / high | Delete observes zero VMs without locking (`worker_hosts.rs:1118`); placement commits; deletion marks the host terminated without recounting (`worker_host_service.rs:824`). Five steps. | Extends C3/L5 and section 7 host atomicity; WorkerHostService | Race placement with ordinary deallocation through the real database boundary. Serialize refusal/removal with admission; separately preserve authorized forced-loss semantics. A second count alone is not proof of correct locking. |
| SM05 / 11 | TaskLifecycle / medium | Prerequisite fails or is cancelled; existing dependents settle; a new dependent is then inserted (`job_service.rs:811`) and waits forever for impossible success. Two steps. | New case adjacent to A1; TaskService allocation/dependencies | Exercise failure both before and during dependency creation and ensure terminal propagation. A held prerequisite must remain waitable; successful prerequisites still allow dispatch. Preserve atomic graph creation. |
| SM06 / 11 | WorkspaceFlush / medium | Stop opens a flush; transient capture failure exhausts retries; Stop aborts but the flush stays open. New Stop/idle-stop/fork requests conflict until VM loss or lease expiry. Shortest trace six steps. | Overlap G1; Workspace flush/recovery | Recover or safely retire the same admitted capture after transient error/lost reply; subsequent valid operations progress without weakening proof requirements or requiring VM death. G1 retains its original severity until triage reconciles impact. |
| SM07 / 12 | WorkspaceVm / high | API gives up after 120 seconds while a Start send may take 250 seconds; the late command boots an uncounted VM. | Extends L6 with timing evidence; lifecycle and Host admission | Delay delivery past control-plane timeout/release, then deliver and replay it. Obsolete authority cannot admit work or bypass capacity; valid current Start remains possible. Preserve last-grant-expiry replacement versus physical exit distinctions. |
| SM08 / 12 | WorkspaceVm / medium | Transfer cancels Start; failed transfer restores old state and revives that cancelled Start. | New case adjacent to L1/L5; transfer/lifecycle owner | Fail transfer after consent fencing, retain the old owner, and verify old consent cannot revive. Test a subsequent fresh authorized Start and determine which operation owns cancellation/rollback. |
| SM09 / 12 | WorkspaceVm / medium | A new Start places immediately ahead of an older queued Start. | Overlap L7; Workspace admission/queue | Race immediate admission with queue reconciliation under organization capacity; preserve FIFO for eligible requests and explicit expiry/cancellation behavior. |
| SM10 / 12–13 | WorkspaceVm / medium | The first definite Start failure exhausts consent; the incremented attempt count is never read (`workspace_vm_service.rs:2588`). Four-step model trace. | Overlap L7/T4; Workspace retry policy | Check first, second and third definite outer failures, plus transport replay and uncertain cleanup. Preserve the distinct rule refusing unchanged invalid boot specifications. |
| SM11 / 12 | WorkspaceFlush / medium | An old VM's unfinished flush blocks its successor's Stop/fork for 30+ minutes. | Overlap G1/T4; flush and incarnation recovery | Exercise release/successor admission before orphan-sweep age. Reconcile obsolete capture without admitting stale completion or pruning pinned evidence. Select an explicit progress expectation during design. |
| SM12 / 12 | WorkspaceTransfer / medium | Two transfers remain pending; a stale A→B transfer completes after ownership has changed A→B→A. | New case adjacent to L5; transfer owner | Replay an old transfer after an ownership round trip and competing transfer; current owner equality alone cannot establish operation authority. A valid current transfer and its identical retry still succeed. |
| SM13 / 12 | WorkspaceFork / medium | Fork admission succeeds after source deletion and leaves its destination name reserved. | Extends G5 with admission/deletion race; fork and workspace deletion | Interleave source tombstoning/deletion with claim admission. Refuse invalid admission or safely settle it; preserve legitimate prior claims and pins until their operation settles. |
| SM14 / 12 | WorkspaceFlush / low | Cleanup overwrites the reason an oversized flush was rejected. | New case under G2; flush/retention owner | Run retention after a size failure and preserve truthful failure/block evidence. Cleanup and repaired explicit retry must still work under their contracts. |
| SM15 / 12 | ProviderRequest / low | Two questions use the same ID; the second answer is rejected and the request times out. | New case adjacent to A7; provider adapter/request owner | Distinguish identical replay from a conflicting new question, across pending and settled requests. Test each supported provider; preserve valid reply retries without silently enabling unsupported questions. |
| SM16 / 12 | WorkerHost / uncertain | Background drain may remove a host concurrently receiving a VM; PostgreSQL locking behavior is explicitly untested. | Extends C7 and host atomicity; host maintenance/placement | Reproduce with actual isolation, row locks and concurrent transactions. If locks prevent the trace, correct the model and mark the finding disproved rather than weakening valid database protection. |
| SM17 / 12 | WorkerHost / uncertain | Rotation may drain old capacity before its replacement is ready. | New concrete case under host rotation; rotation/placement owner | Establish the governing replacement-readiness meaning, then delay provisioning/readiness and test normal rotation and authorized deadline exceptions separately. Keep severity unresolved pending that evidence. |
| SM18 / 12 | ProviderRequest / uncertain | Reuse of a question ID may stall recording that task's results. | New progress case adjacent to A5/A7; task journal projection/request owner | Inject conflicting request IDs followed by terminal events; verify explicit error handling and continued settlement/acknowledgement. Do not silently drop events or merge distinct requests to get progress. |

Sources and trace availability are qualified in section 2.1. Section 9.4 assigns every
SM ID a closure slice. The deck's model-proposed host fix and all three uncertain cases
need independent validation; this incorporation does not promote them to confirmed bugs.

## 7. State machines and protocols

This inventory is the bridge between the findings and a formal model: for each machine,
where its transitions were reported, what identifies and fences it, and which property
is at risk. "Sites" counts files that implement transitions. Historical "holds" means
the reviewer found supporting structure, not proof; "violated" identifies an alleged
counterexample requiring reproduction. Blank cells mean unassessed, not inapplicable.
Distinguish at most one *authorized* current incarnation from physical child liveness:
last-grant expiry can permit API replacement without proving old host exit.

The independent analysis adds the following challenges to this inventory: cancellation
and late dependency creation (SM03/SM05), host removal/rotation (SM04/SM16/SM17),
transfer identity and rollback (SM08/SM12), and native-request identity/progress
(SM15/SM18). Earlier "holds" annotations describe only their named property; they do
not establish correctness of the whole machine. In particular, a single native-reply
writer does not settle the duplicate-ID or later-event progress questions.

| Machine | States | Transition sites today | Identity and fencing | Safety property | Liveness property | Gap |
|---|---|---|---|---|---|---|
| VM row (`workspace_vms`) | stopped, starting, running, stopping, failed | 8 reported files; 4 release paths, 5 failure paths; a fence trigger redefined in 5 migrations | (workspace, vm_id, epoch) CAS; lifecycle generation; stop operation id | one authorized current incarnation; host custody release needs exit proof, independently of API last-grant-expiry replacement | terminal progress depends on explicit recovery, capacity and authorized loss assumptions | T1, T6 |
| Start consent (`start_intent` JSON) | pending, queued_for_capacity, starting, consumed, expired, cancelled, failed | 3 files, 12-13 reported statements; deferred trigger binds to owner and generation | intent id, generation | consumed only by Ready of the same vm | queued intent is placed, cancelled, expired or failed under the owning rules; L1/L2 challenge the path | T1 |
| Stop saga | admitted, nudged, prepared, quiesced, drained, flushed or loss recorded, host-stop sent, released or aborted | one reported 360-line function; multiple request/reconciler drivers | operation id; `stop:{op}:<phase>` keys; `stop_retry_at` coordination | preserve required capture or authorized recorded loss; reject stale phase effects (L2) | progress under declared dependency/recovery assumptions | T2 |
| Host (`worker_hosts`) | provisioning, ready, unreachable, draining, terminated | 5 files; "draining" has two meanings | host id; drain id | drained count and status atomic (violated) | | T1 |
| Placement and host admission | reserved, booting, ready | control plane `place` + host `start` | org advisory lock, `SKIP LOCKED` host row; host slot map in memory, no tombstones | slots never exceeded (relies on sequential control loop and boot-record-before-start ordering, undocumented) | | T6 |
| Flush receipt (`workspace_flushes`) | open, complete, pruned; open to failed(size) or orphaned; fork_copy born complete | 5 files plus guest `CaptureState` | flush id; unique (workspace, vm, request key); fence (org, owner, vm, epoch, lease, status) | at most one open per workspace; selected receipt complete and unpruned; flush refs immutable | open row eventually terminal (violated: G1) | T4 |
| Durable cut selection | derived | 6 independent computations | `last_flush_id` | all readers agree (violated: G3) | | T1 |
| Fork (`workspace_forks`) | pending, completed; pending, aborting, aborted | one file (good) but client-driven only | (org, requester, idempotency key) + request digest; proof ref `refs/g5/forks/{id}`; advisory and row locks | names unique; completed implies every proof equals its oid; delete only with matching proof | pending eventually completes or aborts (no fair actor: G5) | T4 |
| PR and publication | open, merged, closed; per-repo provider status; merge runs | route plus three services; `job_merge_runs.status` from 6 files | none durable across phases; retry recomputes preflight | merged implies target contains pinned source and approvals current (violated: no approval gate) | PR eventually reflects the target (no reconciler) | T2, T3 |
| Branch landing | fence, verify, consumers, Git CAS, COMMIT, compensate | one function (good) | `advance_id` created in memory, never stored | refs restored if commit refused (holds); unknown-commit settlements reconcilable by identity (not possible) | | T4 |
| Remote integration queue | leased, classified, acted | one pure decision function plus scheduler (good) | owner token and generation | | recovers lost enqueues (holds) | |
| Logical task | queued, running, waiting, succeeded, failed, cancelled; × hold reason × `spec.lifecycle` | 21 status/hold write sites in 3 files; 6 reachable only from Workspace or Host | exec id `task.{id}.{attempt}.{vm}`; one nonterminal agent task per session (index) | running iff a current (attempt, vm, epoch) (must hold across two tables) | bound queued input on a running VM eventually reserved (violated: A1, A5) | T1, T7 |
| Attempt | unconfirmed, confirmed, ended(reason); sub-machine `codex_auth_state` in 3 files | `apply_*` functions; identity immutable by trigger (good) | `acked_event_seq` dense watermark | each event applied once; ack never ahead of commit (holds) | | |
| Input delivery | queued (unbound, bound), reserved, delivered, delivery_unknown, withdrawn | 4 files; requeue-vs-unknown decision split across two functions | reservation id, revision, generation, attempt | delivered implies provider received the bytes; requeued only if handoff never began (violated: A4) | | T4, T6 |
| Notice delivery | queued, reserved, delivered, withdrawn (in transcript JSON) | 4 files, no constraint | none | | | T1 |
| Native reply | pending, reply_pending, responded, invalidated | single writer plus trigger (good) | request id + digest | settled only on transport-write evidence (holds) | | |
| Dispatch protocol | reserve, commit, invoke, confirm, apply, ack | `agent_dispatch`, `agent_tasks`, `task_lifecycle`; a second loop in `workspace_vm_service` | idempotency `agent-invoke:{exec_id}` | dispatch strictly after commit (holds); replay params equal reserved params (not checked) | committed reservation eventually confirmed or ended (violated by poison: A5) | T4 |
| Host control session and journal | hello, heartbeat, events, acks, commands | `ws/host_ctl.rs`, `host_control_state.rs`, `g5d-host/src/ctl.rs` | (vm_id, seq) never reused; contiguous ack; command key + payload hash | ack implies durable ingest (holds); same key same identity (volatile cache) | events eventually acked after reconnect (holds) | T6 |
| Guest lease, admission, execution | lease seq and expiry; admission open/closed by op; execution journal and disposition | `g5d-dispatch` lease, protocol, journal, modules | (vm, epoch, seq); operation id | delayed lease reply never extends custody (holds); first disposition wins; never relaunch (holds); flush only when drained | | |
| Authorization decision | authenticate, token scopes, membership, workspace read, owner-if-scope, handler guard, service check, live admission, dispatcher precondition | multiple paths across middleware, `db/scoped.rs`, routes, services, facade | key/resource identity and current operation-specific authority | membership alone does not grant write; approval independent of ownership; starting/draining callback exceptions preserved | membership TTL is 60 seconds; key, role and established-stream revocation require separate path checks | T5, T6 |
| Approval request | pending, settled, next pending | 2 services | fingerprint replay | approver eligible and can read (checked only at evaluation) | | T1 |
| Issue comment publication | pending, published, failed, outcome_unknown, reconciled, discarded | route-owned broker | request key; exact payload recorded first (good) | never resend after unknown outcome (holds) | crash then retry returns pending without sending (violated) | T2, T4 |
| Platform report outbox | pending, claimed, completed, failed, reconciling | one owner (good) | | | | |
| Editor session (client) | clean, dirty, saving, saved, conflict, error, deferred external change | `CodeEditor`, `SpecEditor`, `dirtyEditors.ts`, `ContentDispatch`; two divergent copies | none (no VM identity, fresh request key) | "Saved" implies the server accepted exactly the shown bytes (violated in `SpecEditor`); accepted only by the VM the bytes were read from (violated) | clean editor eventually matches the server (violated by missed events) | T6 |
| Realtime channel (client) | idle, connecting, open, backoff, grace, destroyed | `ws.ts` booleans and timers | no sequence numbers reported | no stale/duplicate delivery corrupts accepted state | eventual convergence under connectivity/resync assumptions; subscribe gaps, drops and skipped keys need probes | T3 |
| Chat turn (client) | pending, streaming, complete, error, stop handoff | three modules | chunk without offset | shown text is a prefix of the server's text (violated) | | T5 |

## 8. Formal verification as a design instrument

### 8.1 Interpretation

The supplied audit used "TSL" without a linked clarification. This plan does not assume
that TSL and TLA+ are interchangeable. Start with explicit state, operations, effects and
failure cases; select a notation only for a question worth checking. TLA+ with TLC is a
candidate for temporal protocols; a decision table may suffice for a bounded authority
question. A policy evaluator is not automatically a temporal model checker.

No model or proof was run in preparing or updating this document. The independent
analysis reports executed models; section 8.7 records their import and validation needs.
The candidates below remain design/verification scopes, not a commitment to model every
machine or proof that modeling will discover each reported defect.

### 8.2 Where a model can improve the design

A useful specification asks three questions that also help modularity:

1. **State and authority.** Which facts are authoritative, derived or private? Who may
   change each fact? A model can describe many writers, but the implementation mapping
   must account for them rather than silently assuming one owner.
2. **Operations and composition.** What must each public operation establish, preserve
   and produce? A code operation may refine an abstract action through several steps;
   multiple code paths may implement the same action. Identify atomic/linearization
   points, intermediate states and interference explicitly.
3. **Failure and progress.** Include commit uncertainty, lost replies, duplicate/reordered
   messages, late callbacks and overlapping drivers. State the scheduling, resource and
   eventual-recovery assumptions needed for progress.

Writing such a model may reveal missing owners, hidden prerequisites and incomplete
operations. It does not prescribe a crate layout, a universal transition function or an
outbox for every effect.
The reviewers named what makes mapping hard today: implicit states in nullable timestamps
and two shapes of JSON, transitions in routes, SQL functions, maintenance loops and the
guest, string error codes deciding transitions, timer-driven transitions, and state whose
lifetime follows React component keys.

### 8.3 A candidate structure: decide, persist, effect

Some durable protocols benefit from this shape, which several places in G5 already approximate
(`outcome_decision`, `vm_loss_decision`, `AttemptDecision`, `classify_and_act`,
`Lease::due_at`, `Admission`):

```text
decide(snapshot, event) -> proposed transition + required effects or refusal
persist(tx, transition, expected_identity) -> validate prerequisites + atomic owner writes
recover(operation) -> perform/reconcile durable external effects; record their outcomes
```

The pure decision can improve testing, but persistence must validate that the snapshot
and authority still apply. Owners may compose writes in one caller transaction. Required
secondary effects, such as audit or receipts, belong in the contract alongside the
main transition. Remote effects cannot be rolled back with a database transaction;
give their identity, unknown outcome and recovery an explicit protocol.

An outbox, privileged stored operation or separate database role is useful only when its
specific enforcement and failure behavior are demonstrated. A role reachable by the same
raw connection does not establish independent custody. Generated constraints cannot prove
that the generating transition table captured the right policy; immutable migrations
also require a deliberate forward transition when that policy changes.

Trace validation needs instrumentation, a representation/abstraction mapping and a trace
checker that detects omitted or malformed events. This is engineering work, not a free
result of a reducer or a one-to-one table. A passing trace supports the executions observed;
it does not prove all implementations, schedules or liveness properties.

### 8.4 A verification ladder

The ontology's [formal verification story](../../../.g5/ontology/features/ontology_and_harness.g5.md#formal-verification)
asks G5 to distinguish what tests, review and formal verification can establish.
A practical evidence selection for this study:

| Property class | Instrument | Examples |
|---|---|---|
| Row-shape and local invariants | Database constraints and triggers | one open flush; immutable attempt identity; one active agent task per session |
| Decision correctness | Unit and property tests on `decide` | stop-reason to hold mapping; delivery-after-settlement; authorization `decide` |
| Protocol safety and liveness under failure | Explicit finite model with justified bounds and fairness/environment assumptions | current authority uniqueness; release only with proof; eventual terminal flush where recovery assumptions hold |
| Observed executions match the model | Instrumented integration tests plus an explicit trace adapter/checker | Stop across API restart; fork across crash; dispatch across dispatcher restart; not a general refinement proof |
| Cross-process contracts | Fixtures for declared supported versions and generated-type/runtime compatibility checks | reject unsupported host/spec versions; preserve optional fields and unknown-event recovery as designed |
| Human judgment | Ontology review | what counts as loss; who may approve; consent lifetime |

Proofs are optional future work, not an exit criterion for every refactor. Keep model
results, code correspondence and runtime checks distinct. See the primary
[TLA+ overview](https://lamport.azurewebsites.net/tla/high-level-view.html) and
[liveness/fairness tutorial](https://lamport.azurewebsites.net/tla/tutorial/session9.html)
for the difference between safety checks and progress assumptions.

### 8.5 Candidate models

Each entry lists the state, the actions, the failure actions to include, the invariants
to challenge and implementation questions. The labels are stable candidate IDs, not
priority or promised results; section 9 selects slices by risk. No candidate must adopt
the proposed mechanism before the model can be written. Invariants must retain the
owning ontology's scope and exceptions.

**M1. Workspace lifecycle: VM incarnation, Start consent, Stop saga, host events.**
State: per workspace owner, generation, vm {status, id, epoch, host, lease, stop op,
phase, flushed, loss}, intent {status, generation}; per host the VM set; the event
journal; held tasks. Actions: RecordIntent, Place, SendStart, HostBoot, Ready, AdmitStop,
StopPhase, Flush, AbortStop, SendStop, Exited, Fail, LeaseExpire, InventoryMiss,
ReconcileQueue, Release, ForceLoss, Transfer, Delete. Failure actions: API crash between
any two commits; host crash; duplicate or reordered events; lost start, stop or flush
replies; HTTP and reconciler driving the same stop; force-delete taking over an
operation. Invariants: `intent = starting ⇒ status ∈ {starting, running}`; after a Stop
no intent from an older generation is placed; `SendStop ⇒ stopOp is current`; owner
change ⇒ no active VM and roots purged; capacity freed ⇒ exit proof (`g5.lifecycle.stop`,
`g5.lifecycle.stop.release`, `g5.runtime.custody`). A Start can also expire or fail;
eventual placement needs capacity and eligible-host assumptions, not just a fair
reconciler. Stop termination requires explicit persistence/host recovery or the
authorized loss path. Challenge L1, L2's early return and stale phase, and L6's late
command concern. Candidate implementation: the `incarnation` owner,
typed statuses, persisted `stop_phase`, an outbox for `vm.*` commands.

**M2. Flush × incarnation.** State: vm {epoch, status}, flushes[id] {epoch, key, state},
refs, guest capture state per epoch, selected cut. Actions: Start(k), PushRefs, Complete,
FailSize, GuestCrash, LostReply then Retry(k), NewKey(k'), Release, SuccessorStart,
OrphanSweep(timer), Prune. Invariants: at most one open flush; selected is complete and
unpruned; only the current epoch writes or completes (`g5.git.receipt`,
`g5.git.receipt.write-admission`). Liveness: an open receipt can reach an appropriate
terminal outcome under stated recovery assumptions. Challenge:
the possible stranding trace Start(k1), transient failure, NewKey(k2) (G1, #146). Candidate design: an
explicit `state` column, one transition module, atomic retirement on release, one
`FlushErrorCode`.

SM06/SM11 add live and successor-incarnation progress cases; SM14 adds failure-reason
preservation under cleanup. Model transient failure separately from deterministic size
failure, and include cleanup updating a terminal record. Recovery must retain required
proofs and failure blocks while allowing a repaired explicit retry.

**M3. Dispatch and delivery.** State: task, hold, attempts, input, generation,
dispositions, journal, acked, a ghost `got` (provider received the bytes), a lossy and
duplicating network, an owner redelivery grant. Actions: Reserve+Commit, Invoke/Replay,
Admit, Handoff, EmitAccepted, Finish, DispatcherCrash, APICrash, Cancel, VmLoss,
Apply+Ack, OwnerRedeliver/Cancel/Continue. Invariants: `delivered ⇒ got`;
requeue after possible handoff requires the contract's redelivery authority; at most
one open attempt; each exec spawned at most
once; replay params equal reserved params (`g5.conversation.delivery.unknown`,
`g5.architecture.dispatch_phases`). Liveness: every committed reservation is confirmed
or ended, assuming an eligible worker, progress of dependencies and bounded handling of
poison entries. Challenge A4 (crash between Handoff and EmitAccepted) and A5 (poison).
Candidate mechanisms:
the handoff fact, a persisted reservation snapshot and digest, error classification,
`TaskService::allocate` as the only producer.

Extend the task portion with SM03/SM05: Cancel before interrupted/suspended result
application, retry/Continue/restart, and dependency creation after or concurrent with
prerequisite failure. Keep a held prerequisite distinct from permanent failure.
The acceptance rules include terminal cancellation and eventual failure propagation;
do not infer Cancel-versus-Success precedence while O2 remains unresolved.

**M4. Host custody, purge and the event journal.** State: control-plane VM view, host
VM map {Reserved, Booting, Ready, Stopping, ReapUncertain}, child {alive, reaped},
ports, retained roots in memory and on disk, journal, acked prefix, volatile result
cache and generation, per-session FIFO channels. Actions: Bind/SendStart, Admit, Launch,
Reap (ok or fail), ChildExit, Stop, Purge, Journal, Replay, Ingest+Ack, Heartbeat and
Reconcile. Failure actions: HostCrash (volatile state lost), TunnelDrop, Duplicate,
LostReply, delete error. Invariants: release of a launched child's slot/port requires
child reap or valid independent exit proof;
`apiPurged(ws, g) ⇒ files gone`; `Exited in journal ⇒ child reaped`; no seq reuse; acked
prefix ingested (`g5.runtime.custody`, `g5.runtime.purge`, `g5.runtime.events`). Challenge
L3/D5 and heartbeat snapshot ordering. Candidate mechanisms: `Custody`
handle with reap receipt, `PurgePending` state, persisted results and tombstones, a
generation serialized with the represented snapshot and its acceptance rules.

For SM01, expose the interval between removing an active record, starting kill and
observing exit; treating them as one atomic action would hide the reported no-failure
race. For SM07, let command delivery outlive the API wait and cross incarnation release.
Preserve valid delayed/replayed commands only when their current authority still holds.

**M5. Fork.** State: operations {status, claim, manifest, source flush}, Git refs per
repo {branch, proof, copy}, workspaces, pins. Actions: Admit, ConcurrentAdmit(same
name), CaptureLive, CreateRefs(r), Finalize, BeginAbort, Cleanup(r), Crash anywhere,
Retry by key or digest, ExternalBranchCreate, Prune. Invariants: names unique across live
workspaces and active claims; completed ⇒ every proof equals its oid; nothing deleted
without a matching proof; a pending fork's source is not pruned (`g5.fork.replay`,
`g5.fork.proof`). Liveness needs a recovery actor and eventual dependency progress; audit the reported
absence of a worker before claiming a current gap. Candidate mechanisms:
a recovery worker, attempt count, interrupt in the digest.

Add source tombstoning/deletion concurrently with admission and claim creation (SM13).
Distinguish invalid late admission from a legitimate preexisting pinned operation;
safe cleanup must not erase another operation's reservation or proof.

**M6. Publication and merge.** State: provider state per repo, target refs, PR status,
merge runs, approvals, in-flight transactions. Actions: Preflight, ProviderMerge(r),
CommitProviderTx, Import, Land (CAS then COMMIT), CommitRefused, CommitUnknown, Crash,
Retry, ConcurrentPush. Invariants: merged ⇒ target contains the pinned source and
approvals current at that SHA; no double application (`g5.governance.merge`,
`g5.governance.publish.proof`, `g5.governance.approval.version`). Prerequisite: persisted
`merge_operations` and `publication_intents` with a phase enum, provider calls outside
transactions, approval check at the landing commit point.

**M7. Authorization policy (Alloy or Cedar).** State: principals (User, ApiKey,
Incarnation(ws, owner, vm, epoch), System), org members with roles, workspaces {owner,
visibility, invitees, pending transfer, vm}, approval requests {approver}. Invariants:
MutateLive follows current admission including operation-specific gates; lifecycle
callbacks retain their starting/draining exception. Approve requires an eligible human
who can read; membership alone never grants write. Test credential revocation,
membership-cache delay, role changes and established streams against their respective
contracts, without inventing one universal epoch bound (`g5.security.admission`, `g5.security.tenant.write`,
`g5.governance.approval`). Adversary: demote, remove or transfer between token issue and
use; subscribe then revoke. Candidate design: shared policy with operation-specific
facts and effects. A one-to-one function mapping is not required and does not prove
that every entry path supplies current facts.

**M8. Editor session (client) with the server Save path.** State: server bytes, hash and
current VM; client baseline {content, hash, source, vm}, buffer, in-flight save {submitted,
expected hash, vm, request id}, cache entry, label. Actions: Edit, Save, ServerApply
(accepted iff hash and VM match), Reject, Response, AgentWrite with `file_changed`,
Refetch start and resolve, Stop/Start/Transfer, Unmount, Reload, Discard, AcceptSuggestion.
Failure actions: late response after Transfer; lost or duplicated `file_changed`; a refetch
resolving after a save; a transient fetch error. Invariants: no false "Saved"; only
submitted content acknowledged; accepted only by the VM the bytes were read from; late
results from an old VM change nothing; editing only on live bytes from the current VM;
unsaved edits discarded only by explicit Discard, Reload or Overwrite; at most one
application per request key (`g5.documents.save`, `g5.documents.save.acknowledgment`,
`g5.documents.read.live-cache`, `g5.documents.files.replay`). Treat expanded buffer
persistence/discard behavior as a design question rather than silently adding a product
guarantee. The Save acknowledgment race has current source evidence; other violations
need their own reproduction. Candidate design: a shared editor-session owner,
`{vmId, epoch}` in the read DTO and Save request, client request ids, one
`FileMutationService::apply`. Do not include the cited #238 commit-on-Save proposal in
this refactor: `g5.git.durability.no-implicit` explicitly forbids a commit on Save.
If a separately authorized request changes that policy, reconcile its ontology and
keep the original benchmark requirements distinct from the amendment.

**M9. Realtime coherence per channel.** A server log of committed changes; deliveries
that may drop, duplicate or reorder against REST responses; reconnect; the gap before
subscribe; chunk appends. Safety: duplicate chunks do not duplicate text and stale
responses do not overwrite newer accepted state. Liveness: eventual convergence under
eventual connectivity and successful resynchronization (`g5.design.commit-notify` plus
the owning consumer contract). These are separate claims.
Prerequisite: `{channel, seq}` on events, a resync signal on drop or gap, refetch on
subscribe acknowledgement, chunk offsets, per-feature event-to-key maps.

**M10. Native reply and host control regression candidates.** The audit describes useful
existing guarantees alongside host replay gaps. Recheck their scope before treating
either protocol as conformant. Select lost-reply, retry, timeout, late evidence and
restart cases when an adjacent refactor can affect them; estimate modeling cost rather
than assuming it is cheap.

Extend native replies with SM15/SM18: provider request ID, attempt identity, private
request digest, accepted answer and journal progress; duplicate open/request frames,
conflicting reuse before/after settlement, late replies and later terminal events.
Separate legitimate identical replay from a new question and preserve exact transport
evidence. Unsupported provider questions remain refused under existing provider policy.

**M11. Worker host placement, deallocation, drain and rotation.** Added scope for
SM04/SM16/SM17: host state and admission eligibility, VM placement, drain purpose,
replacement capacity and removal. Split occupancy observation, placement commit,
drain admission and termination to expose interference. Translate the actual database
locking/isolation into the model during implementation validation. Challenge ordinary
removal with a concurrent placement, both request and maintenance paths, and rotation
with delayed replacement readiness. Preserve authorized force/deadline behavior and
keep the readiness/unreachable-host questions in section 10.1 explicit. No new host
service or deployment split is implied.

**M12. Transfer identity, consent and rollback.** Added scope for SM08/SM12: current
owner, lifecycle generation, transfer identity/authority, consent, recipient eligibility
and release/purge progress. Include overlapping transfers, A→B→A ownership, delayed
completion, failures after Stop and same-operation retries. Check that stale authority
cannot complete merely because an owner ID matches again; failed transfer retains old
ownership without reviving fenced consent. Preserve valid administrator transfer of
base workspaces and the distinct legacy-execution refusal. Coordinate with M1/M4;
separate passing models do not prove their composition.

### 8.6 Working method

- One spec per owned machine, kept next to its owner's architecture node or under
  `docs/formal/<module>/`, with each invariant naming the ontology block it enforces.
  Where a spec contradicts a block, the block or the spec is wrong, and the ontology
  review decides which (`g5.ontology.formal-verification`, `g5.ontology.lineage`).
- Start with small instances, record the bounds and why they cover the selected risk,
  then challenge the abstraction and enlarge the instances where multiplicity matters.
  No fixed two-workspace/one-crash cutoff establishes general correctness.
- Write a model before consolidation only where it helps resolve a material question.
  Let domain contracts determine the public API; record how implementing paths refine
  the abstract behavior, including intermediate steps and legitimate alternatives.
- Require a valid scenario and a meaningful negative control, such as removing the
  guard under study. Preserve counterexamples and inconclusive runs. Report fairness,
  resource and dependency assumptions alongside any liveness result.
- Budget trace instrumentation and its adapter explicitly. A missing event must not
  silently turn a failing run into a passing trace.
- Use proportional evidence. A single writer or database constraint does not eliminate
  remote-effect, concurrency or recovery questions; conversely, routine internal changes
  need not acquire a model.

### 8.7 Incorporating the independent models

The following inventory is transcribed from slide 5. Counts are for each model's main
check, not the entire 21-configuration suite; do not expect their sum to equal 19.8
million or interpret the suite total as globally distinct system states. The per-model
"rules" counts are not counts of distinct ontology blocks.

| Reported model | Scope | Main-check states | Rules | Finding IDs | Plan model scope |
|---|---|---:|---:|---|---|
| TaskLifecycle | Task outcomes, retries, cancellation, dependencies | 656,526 | 14 | SM03, SM05 | M3 |
| WorkspaceVm | Start/Stop, consent and queue | 1,024,237 | 14 | SM02, SM07–SM10 | M1; transfer boundary M12 |
| HostVm | Boot, stop, cleanup and child custody | 866,348 | 14 | SM01 | M4 |
| WorkerHost | Provision, drain, rotate and remove | 10,346,514 | 10 | SM04, SM16, SM17 | M11 |
| WorkspaceFlush | Shutdown/fork capture and retention | 66,215 | 16 | SM06, SM11, SM14 | M2 |
| WorkspaceTransfer | Workspace ownership handoff | 19,852 | 10 | SM12 | M12 |
| WorkspaceFork | Fork admission, claims and completion | 406,248 | 7 | SM13 | M5 |
| ProviderRequest | Native question, reply and timeout | 1,652,488 | 10 | SM15, SM18 | M10 |

**Additional result to recover.** Slide 10 reports checking a proposed host fix:
recount occupancy before termination, plus a related drain-race repair. Its property
reportedly holds over 8.6 million states. Recover the changed model, exact property and
configuration before using that result. Confirm that the real implementation serializes
the check with placement; two unlocked counts can still race. No production fix or
database locking result follows from that abstract pass.

Reuse procedure for the selected slices:

1. Obtain and pin the eight source models, all 21 normal and 31 problem configurations,
   model-to-code citations, TLC/tool version, constants, bounds, fairness assumptions,
   raw logs and counterexample traces, including the proposed-fix variant. The advertised
   command is `TLA2TOOLS_JAR=/path/to/tla2tools.jar docs/tla/run-tlc.sh`; recover its actual
   configuration selection and separate problem-check commands. Do not recreate files
   under that name and present them as the authors' artifacts.
2. Replay unchanged models. Distinguish an expected property violation from parser errors,
   resource exhaustion or unrelated deadlock. Record exactly which properties each normal
   configuration checks and why problem configurations differ. Preserve the old failing
   traces and source snapshot as evidence even after a repair.
3. Inspect the production path during implementation and translate the counterexample
   into a deterministic boundary test with a valid counterpart. Model successful-but-slow
   cleanup as well as failed cleanup; use real database concurrency for SM04/SM16.
   If code guards or locks invalidate a modeled trace, correct/disprove the finding.
4. Repair code under the owning contract and update the handwritten model's transition
   mapping to match. Rerun the corresponding fixed-model property and production test;
   code changes alone cannot change a handwritten model. Keep a deliberate weakened
   guard as a negative control and avoid deleting the obligation to get a pass.
5. Exercise interactions across the models: transfer/Stop/custody, incarnation/flush,
   placement/removal and native replies/task settlement. Record abstraction and atomicity
   assumptions, unmodeled effects, bounds and liveness conditions. Passing isolated models
   or a sampled runtime trace is not an implementation or composition proof.

Missing original artifacts do not block independent regression work on a confirmed case;
label newly reconstructed models separately. Preserve the authors' zero-runtime-reproduction
baseline until a dated executed receipt establishes a stronger result. Treat the slide
cases as known regression material, never as withheld efficiency-evaluation tasks.

## 9. Gap closure plan

### 9.1 Principles and priority

- **Confirm the path before repairing it.** Pin the implementation snapshot, trace the
  actual owner and every relevant caller, and reproduce the affected contract. A
  historical source finding is a lead, not current runtime evidence.
- **Repair safety and authority before broad restructuring.** Failed-reap/purge custody,
  stale Stop consent, ambiguous delivery, wrong approval authority and false Save
  acknowledgment merit bounded regression fixes when confirmed. They need not wait for
  ten models, new database schemas or a crate split.
- **Preserve complete operations.** Consolidate the policy and effects that belong
  together, including legitimate exceptions and caller-owned atomic transactions.
  A table may contain fields owned by different protocols; a one-table/one-file rule
  is not a substitute for defining their coordinated authority.
- **Choose mechanisms proportionately.** A pure decision core, outbox, typed capability,
  generated contract or new crate must protect an identified invariant or change need.
  Keep existing useful services and protocol kernels. Do not impose one generic saga
  engine, schema-per-capability layout or identical API pattern on every owner.
- **Make transitions compatible.** Migrate all production callers and relevant fixtures
  through coherent slices. During a transition, identify the temporary authority and
  cutover; do not leave two independent policy owners writing indefinitely.
- **Measure behavior and maintenance.** Retain initial findings, validation failures,
  repair cost and later task performance. A structural count is a diagnostic, not a
  goal that can override correctness or legitimate composition.

### 9.2 Module definition and enforcement

Use the existing semantic owners: Workspace, Agents, Worker & Host, Git/governance and
shared platform capabilities. Their current architecture pages provide the contract
homes. Maintain a mapping from each owner's public operations, private state/effects
and allowed collaborators to its actual Rust/TypeScript paths. Code paths may span
processes; route/test adapters can belong to the same semantic owner as a service.

The SM cases require behavioral boundaries as well as private writers: state what wins
when operations overlap, when an effect becomes irrevocable, which identity a retry
reuses, and how terminal decisions remain terminal. One TaskService can still implement
the wrong cancellation transition; one host-removal method can still race placement.
Include these promises in the owning contract before relying on an interface name.

Deployment groups and Cargo crates remain useful build and rollout views. Calling all
of `g5-api-core` one module would hide the within-core coupling this study is trying to
repair. Conversely, semantic boundaries can start being enforced inside a crate through
private modules and narrow facades; they do not have to wait for physical extraction.
`pub(crate)` alone does not isolate sibling modules in that crate.

For each slice, state the expected owners and affected consumers in the existing plan.
A dependency/scope checker should explain a new edge or writer and flag it for review.
It must allow a justified scope amendment for a discovered consumer, migration or test
instead of encouraging an incomplete fix. Keep one maintainable source for ownership
mapping; do not create a parallel catalog of every ontology rule.

### 9.3 Phases and evidence

| Phase | Work | Exit evidence |
|---|---|---|
| 0. Establish baseline | Resolve/re-pin the audit snapshot, retain metric scripts and raw output, recover the independent model artifacts under section 8.7, reconcile totals, classify known failures, freeze evaluator requirements and withheld tasks | Exact code/model commits/configuration and reproducible measurements; original versus reconstructed models distinguished; inherited defects separated from new-task acceptance; missing evidence explicit |
| 1. Select and confirm slices | Rank by impact and dependencies; translate selected SM traces into production-boundary reproductions and valid counterparts; inspect sibling paths and current ontology | Concrete owner, affected paths, failure and valid case, governing IDs and commands for each finding; uncertain cases confirmed, disproved or explicitly unresolved; section 10.1 questions resolved where needed |
| 2. Design and close | Update the existing plan and necessary ontology; implement complete owner operations, migrate callers and fixtures, add proportionate boundary checks | Regression fails without the fix and passes with it; valid variants, races/retries and partial recovery preserved; no relevant bypass left in the declared scope |
| 3. Preserve through skills and contracts | Improve the existing skills and narrow dependency/API guards; generate or centralize selected wire contracts; validate declared version behavior | An agent can add a valid caller without private writes; a forbidden caller is rejected for the intended reason; harmless transformations pass; skill guidance has one maintained source |
| 4. Verify selected protocols | Reuse validated models and model only additional material interactions; retain old counterexamples, update fixed-model correspondence, add adverse controls and explicit assumptions | Reproducible scoped model and production-test receipts, or an honest inconclusive/untested result; configured properties and composition limits explicit; no implied whole-system proof |
| 5. Compare incremental work | Freeze the improved snapshot; run matched fresh-session tasks against current and improved conditions using the PR protocol | Initial and repaired results, task success, code quality, cost and uncertainty; no claim of causal benefit from inspection alone |

Phases may overlap by slice, but snapshot/task freezing precedes evaluation and dependent
implementation follows its governing design. This review only edits the document; the
PR's original refactor, skills and comparison goals remain in scope. The backlog below
is not a claim that a single implementation step can safely repair all subsystems.

### 9.4 Closure backlog and dependency order

The existing C/L/G/A/P/U/D IDs remain the original finding identities; SM01–SM18 add
the independent source cases with overlaps identified in section 6.8. The table maps
both sets to closure slices; every case needs current evidence and a disposition.
Priority is provisional until reproduction. Grouping uncertain cases with a high-risk
slice means investigate them, not presume they are confirmed or require the same fix.
A slice name does not mandate new code types.

| Priority / slice | Finding IDs | Owner and required closure evidence | Dependencies / model candidate |
|---|---|---|---|
| First: custody, purge and stale host commands | L3, L6, D5, D6; SM01, SM07 | Host retains custody during successful-but-pending kill and failed reap; purge failures/restart remain discoverable; delayed Start after API timeout cannot bypass current admission/capacity. Preserve valid cleanup and current Start. | Keep API replacement distinct from host exit proof; M4 with M1 |
| First: Stop and Start consent | C1, C2, L1, L2, L4, L5, L7; SM02, SM09, SM10 | Workspace lifecycle owns fencing and consent. Stop cancels queued consent even when already stopped; an obsolete phase cannot send a current command; FIFO and permitted Start retries work with expiry, invalid-spec and cleanup exceptions. | Coordinate with host custody and Agents release; M1 |
| First: terminal task cancellation | SM03 (extends A2/A3) | TaskService preserves accepted cancellation through interrupted/suspended result application, retry and restart; uncancelled continuation still works. Settle Cancel-versus-Success separately under O2. | Task lifecycle and native timeout; M3/M10 |
| First: host deallocation; assess drain and rotation | SM04, SM16, SM17 (extends C3/C7/L5) | Worker/Host serializes placement and ordinary removal through the actual transaction boundary. Test maintenance locking and replacement readiness; retain uncertainty where evidence is missing and preserve authorized force/deadline paths. | Current placement/custody contracts, O3; M11 |
| First: authority mediation | P1, P2, P3, G7 | Policy owners enforce the exact actor/action/resource grant at each relevant path. Eligible readable nonowner approvers work; ineligible readers cannot approve; key expiry/revocation, role change and stream lifetime are tested separately. | Preserve live/durable and callback exceptions; M7 if useful |
| First: Save correctness | P4, U1 | One editor-session contract and file-mutation owner preserve submitted bytes, observed incarnation, replay identity and stale-response rejection. Type while Save is pending; retry a lost reply; preserve ordinary valid Save without Git side effects. | Both editor consumers and server boundary; M8 |
| Next: flush and recovery source selection | G1, G2, G3; SM06, SM11, SM14 | Receipt owner verifies proofs and recovers/retires failed work. Transient capture and old-incarnation receipts cannot strand valid operations; retention preserves deterministic failure evidence. Selection variants retain their distinct purposes. | Incarnation/retention authority, O4; M2 |
| Next: task allocation, dispatch and delivery | C6, A1-A8; SM05 | Task/Chat/provider owners share complete allocation and exact identity; dependency failure before/during allocation settles downstream work while held prerequisites remain valid waits. Preserve graph atomicity, delivery uncertainty, poison isolation and truthful transcript/bundle outcomes. | Lifecycle collaboration through declared operations; M3 |
| Next: transfer identity and rollback | SM08, SM12 (extends L1/L5) | Transfer owner retains old ownership on failure, never revives consent fenced by Stop, and rejects stale authority after overlapping transfers or an owner round trip. Valid retries and eligible base-workspace transfer remain supported. | Release, purge, operation authority; M12 with M1/M4 |
| Next: fork admission and recovery | G5; SM13 | Serialize deletion/source validity with claim admission; resume or safely abort unfinished work after client/API loss. Retain legitimate claims/pins until proof permits cleanup; same identity/digest cannot create a different operation. | Workspace deletion and Git proof boundaries; M5 |
| Next: provider request identity and settlement | SM15, SM18 (extends A5/A7) | Provider/request owner distinguishes valid replay from conflicting ID reuse; task projection preserves terminal progress and exact reply evidence. Refuse unsupported provider interaction; confirm or disprove the reported progress stall. | Native request/attempt contracts; M10 |
| Next: publication and merge | G4, G6, U6 | Governance owns pinned identity, current approval and reconciliation of known/unknown remote effects. Retry converges without adopting unrelated equal-OID publication; a read/query cannot silently commit or push. | Preserve retained governance scope; M6 |
| Next: notifications and realtime | C5, P6, U3 | Committed facts remain recoverable; rollback publishes no authoritative success; process restart, subscribe gaps and dropped/duplicate events converge via the declared recovery path. | Durable event owner plus frontend consumers; M9 |
| Supporting: shared and wire contracts | C8, G8, D2, D3, D7, U2 | Centralize equivalent meanings, validate wire compatibility and source-of-truth generation; test existing optional/defaulted fields and explicit refusal of unsupported versions. | Change-specific compatibility design; no automatic N-1 or permissive decoding |
| Supporting: documents, spending and integrations | P5, P7, P8 | Trace projection writers and usage identity; preserve unknown cost; close selected duplicate effects and authority paths. Reconcile external-issue implementation with its explicitly conditional scope before changing it. | Separate these owners into bounded designs, not one generic platform rewrite |
| Supporting: composition, recovery infrastructure and tests | C3, C4, C7, C9 | Entry adapters use complete owner operations; required durable effects survive commit uncertainty; ordinary fixtures use owners while deliberate raw boundary tests remain possible. | Apply within selected slices; do not require all route commits or test SQL to vanish |
| Supporting: daemon boundaries and Git execution | D1, D4 | Assess actual privileges and executable entry paths; harden applicable Git operations against repository-controlled behavior while preserving required legitimate Git flows. | Separate process trust from crate layout; binary splits/hardening need deployment and behavior evidence |
| Supporting: frontend composition and policy | U4, U5, U7, U8 | Make selection, authentication and consumer state owners explicit; preserve navigation/editor flows and remove independently duplicated policy where semantics match. | Size is a signal, not a 2k-line release gate; update useful documentation at its owner |

For each selected finding, record **status, pinned source path, governing obligation,
owner, allowed case, counterexample, validation command/result and remaining gap** in
its existing section or linked receipt. Use proposed → reproduced/source-confirmed →
implemented → validated, with explicitly justified deferred or superseded outcomes.
“Source-confirmed” remains weaker than an executed reproduction. Closing a shared cause
can resolve several IDs, but count the repair once and retain each distinct obligation.

### 9.5 Guard calibration and metrics

Recompute the section 4 counts before installing any baseline. Match violations by
semantic edge/writer/operation, not only by a count that can hide a new bypass when an
unrelated file is removed. Record intentional exceptions and retire them as their
owners change; demonstrate at least one forbidden and one harmless change per guard.

| Signal | Useful target | What must not become a gate |
|---|---|---|
| Raw state access and route policy | All selected protected operations pass through the correct owner; bypass probes fail | Zero SQL/commits in every adapter, regardless of ownership and composition |
| Dependencies | No undeclared policy reversal or private-state access in the scoped graph | Zero cross-capability calls; legitimate collaborators are expected |
| Public interfaces | Small sufficient domain operations; private writers and appropriately scoped capabilities | A fixed count of public functions or indiscriminate `pub(crate)` |
| Fixtures | Valid setup through owners, with explicit raw corruption/migration/RLS fixtures | Zero raw test writes, which would erase adverse evidence |
| Types and protocols | One authority per equivalent wire meaning, reproducible generated outputs where justified | Treating every TS export or domain adapter type as a duplicate |
| Locality and complexity | Less private knowledge and fewer unrelated responsibilities changed on matched tasks | One crate per change, a 2k-line maximum, or an unmeasured percentage target |
| Verification | Each selected obligation has evidence and explicit limits; valid cases remain possible | A mandatory model/state count, one function per abstract action or trace checks presented as proof |

Keep critical failures visible regardless of an average. Report denominators, unknowns
and valid exceptions. Generated code, relocation-only edits and altered task mixes must
not manufacture an apparent productivity or locality improvement.

### 9.6 Skills and experiment handoff

Update the existing `.agents/skills` authoring/planning/review guidance only where a
confirmed failure shows missing help: retrieve the owner, inspect a reused operation's
actual policy/effects, preserve complete transaction composition, check consumers and
exceptions, and require the matching adverse and valid cases. Keep one maintained source
and generate client copies through the existing mechanism. Evaluate guidance on an
unseen valid caller and a forbidden bypass; do not add a new mandatory workflow or
experiment skill merely to make the plan look enforceable.

Use [workstream 4 in the PR plan](ontology-plan.md#workstream-4-compare-incremental-agent-work)
for the matched comparison. Before repairs, freeze the baseline and evaluator task suite,
including genuinely withheld follow-ups; known audit probes are regression knowledge,
not unseen discoveries. Before dispatch, fix model/settings, context, tools, budgets,
repair policy, acceptance criteria and independent-review conditions. Record preparation
and maintenance cost separately from per-task time, tokens, interventions and regressions.

SM01–SM18 and their variants are disclosed regression knowledge. Neither their discovery
count nor the reported 19.8 million states demonstrates faster feature delivery. Record
model construction, correspondence review, runtime reproduction and maintenance costs;
the slides' TLC runtimes omit those costs and do not replace the matched experiment.

Evaluate initial submissions before feedback, then equal-budget repairs and follow-ups.
Report existing baseline defects separately from regressions and task success. The current
versus improved comparison measures the combined package; attribute benefits to an
individual skill, ontology format or formal method only with an appropriate ablation.
A null or negative result still completes a well-executed experiment.

### 9.7 Migration, rollout and completion constraints

- Use append-only numbered SQL migrations with new SHA384 entries. Never edit old SQL
  or checksums. If schemas/roles change, verify fresh install, populated upgrade,
  repeat no-op, RLS and composed owner transactions using real runtime/migration roles.
  Generated transition definitions do not exempt a change from this rule.
- Preserve current supported API/wire behavior. Add mixed-version fixtures for versions
  actually supported; choose N-1 support only if a separately justified rollout design
  needs it. Removing `deny_unknown_fields` can weaken validation and is not a universal fix.
- Keep multiple API replicas deferred under `g5.scope.later-work`. Overlap between a
  request and a reconciler still exists with one replica, so test claims/fencing and
  restart recovery without silently expanding deployment scope.
- Authority repairs must close both refusal and overgrant paths. Removing an owner guard
  from approval handling must be paired with eligible-human and current-read checks.
  Publication proof and replay convergence must be repaired together.
- A database sequence alone cannot make a snapshot fresh; bind generation production
  and consumption to the relevant serialization/snapshot rules and test reordered replies.
  A type named receipt must be constructible only from the evidence it represents.
- Select checks through [local validation](../../local-validation.md), starting with
  `just validation-recommend main`. Use preflight and relevant production-boundary
  slices; prepare once before repeated Docker slices and run `just ci-docker-down` when
  finished. Inspect receipts for nonzero selected test execution and report skipped checks.
- Before merge, both `g5-ci-rust` and `g5-ci-e2e` must pass on the current PR head.
  An inherited failure remains a failure. Keep implementation, validation, deployment
  and observed behavior distinct in the final report.

Completion requires demonstrated closure for the selected findings, a usable modular
boundary and skill/check coverage, plus the comparative results required by the PR plan.
Name deferred findings and unassessed modules. Finishing a pilot does not establish
that the entire codebase is modular or formally verified.

## 10. Ontology impact (not updated; could be affected)

No ontology node was edited for this document review. The following are investigation
and metadata-update candidates. Corrected or unreproduced audit claims must not be copied
into ontology as established gaps. Existing valid commitments need no wording change
merely because their implementation is defective. Proposed new architecture requires
its actual owning-block/metadata diff in the PR plan before dependent implementation.

**Structure and module rules.**

- `concepts/software_architecture.md` (`g5.architecture.capabilities`, module map):
  retain the semantic capability map; connect concrete ownership and dependency findings
  to the existing module pages. Inspect whether Workspace/Git's mutual collaboration
  needs a lower-level contract; two directional descriptions alone do not prove a
  semantic contradiction.
- `software_architecture/boundaries.g5.md` (`g5.architecture.dependencies`,
  `g5.architecture.api_layers`): shared-crate list changes with the `g5-contracts`
  split; "services cannot import routes" holds, but routes owning operations does not.
- `.agents/skills/ontology-authoring/references/structure.md`: preserve the distinction
  between semantic module, crate and deployment. A node for a module is compatible with
  that distinction. Change guidance only where the audit demonstrates a useful gap.
- `features/ontology_and_harness.g5.md`: the cited `g5.harness.module-boundary` and
  `g5.harness.module-node` IDs do not exist in the reviewed checkout. Resolve their
  source before relying on them; do not invent metadata or new obligations for them.

**Blocks whose implementation assessment this audit sharpens or contradicts.**

| Node | Blocks | What changes |
|---|---|---|
| `software_architecture/lifecycle.g5.md` | `g5.lifecycle.stop`, `g5.lifecycle.stop.release`, `g5.lifecycle.retry`, `g5.lifecycle.start.queue`, `g5.lifecycle.transfer.base`, `g5.lifecycle.deadline.loss` | Stop hole confirmed with line references; retry allows three failures but code ends consent on the first; FIFO exists only in the reconciler; `admit_stop` rejects base workspaces before absent-VM handling |
| `software_architecture/runtime.g5.md` | `g5.runtime.custody`, `g5.runtime.purge`, `g5.runtime.lease.replacement`, `g5.runtime.control.replay`, `g5.runtime.control.versions`, `g5.runtime.events` | Code comment claims lease expiry proves loss of custody, contradicting the block; replay cache volatile; single version, no negotiation matrix; late old-attempt facts rejected |
| `software_architecture/durability.g5.md` | `g5.git.receipt`, `g5.git.receipt.write-admission`, `g5.git.flush-failure.block`, `g5.git.clean-advance`, `g5.git.durability.no-implicit` | Investigate receipt proof ownership, selection variants and live retry/retirement; preserve Save without implicit Git commits |
| `software_architecture/workspace.g5.md` | `g5.architecture.runtime_facade`, flush ownership | Facade holds locks across RPCs; Save lacks observed-VM precondition; publication code lives in `workspace_service.rs` |
| `software_architecture/agents.g5.md` | `g5.architecture.allocation_composition`, `g5.architecture.dispatch_phases`, `g5.architecture.task_owner`, `g5.architecture.job_owner`, `g5.architecture.provider_adapter`, `g5.architecture.conversations` | JobService omits `task_id`; second reconciliation loop in the VM service; transcript written by TaskService; provider matrix split |
| `software_architecture/platform.g5.md` | `g5.architecture.usage_owner`, notification delivery | Declared spend owner has no callers; four raw notification inserts |
| `features/agents.g5.md`, `features/conversations.g5.md` | `g5.agents.allocation.input-identity`, `g5.agents.allocation.specification`, `g5.conversation.delivery.unknown`, `g5.conversation.delivery.redelivery`, `g5.conversation.bundle.publication`, `g5.agents.outcome.message-status` | Confirmed gaps; bundle generation never advances; failed turns marked complete; fork status violates CHECK |
| `features/documents.g5.md` | `g5.documents.save`, `g5.documents.save.acknowledgment`, `g5.documents.read.live-cache`, `g5.documents.files.replay`, `g5.documents.association` | Fresh idempotency key per Save; `SpecEditor` acknowledges post-request buffer; results applied to caches without attachment check |
| `concepts/security.g5.md` | `g5.security.admission`, `g5.security.tenant.write`, `g5.security.read.cache`, `g5.security.owner.retirement`, `g5.security.mvp-exposure.root` | Investigate action-specific authority, role changes and established streams; retain the existing database key-revocation/expiry checks and 60-second membership-cache allowance |
| `features/governance.g5.md` | `g5.governance.approval`, `g5.governance.merge`, `g5.governance.publish.proof`, `g5.governance.approval.version` | Merge without approval gate; approval authority in route; decision path without request id accepts any member |
| `features/external_issues.g5.md` | `g5.external-issues.broker`, `.publication.payload`, `.publication.automation` | Node says future; about 5.4k lines shipped with the broker in routes and agent attribution unsettled |
| `concepts/system_design_principles.g5.md` | `g5.design.minimal-api`, `g5.design.authority`, `g5.design.commit-notify`, `g5.architecture.recovery.progress`, `g5.architecture.policy`, `g5.design.dependencies` | Assessments should name verified implementation paths, remaining gaps and evidence limits; reported counts alone do not establish support |
| `concepts/data_model.g5.md` | `g5.data.outcome-evidence`, `g5.data.identity.operation` | Delete and merge in routes have no operation identity; `advance_id` never stored |

**Resolved by existing authority:** last-grant expiry permits control-plane replacement
under its contract but does not prove host exit; eligible readable humans may approve
independently of ownership; multiple API replicas remain later work; semantic modules
need not be deployables. These do not require reopening user decisions to repair code.
External-issue publication has conditional intent that must be reconciled with its actual
implemented path. Ask for a human decision only when the existing scope and lineage leave
a concrete consequential ambiguity, or when proposing an actual policy change.

### 10.1 Ontology gaps raised by the independent analysis

Slides 14–15 identify four specification/navigation gaps in addition to the 18 code
findings. These are analysis questions, not newly adopted rules or claims that this
update changed ontology. Use existing authoritative homes and amend the PR plan plus
affected ontology only when the selected slice requires a consequential decision.

| ID / reported gap | What this plan already has | Required reconciliation before dependent implementation |
|---|---|---|
| O1: state vocabulary is missing from ontology; examples are `reply_pending` and `queued_for_capacity` | Section 7 already inventories both states, but an audit inventory is not an authoritative behavioral contract. | At each protocol owner, decide which state distinctions change externally meaningful promises and document their allowed transitions. Map model/database representations to that contract without making every storage enum a duplicated ontology rule. |
| O2: Cancel-versus-Success precedence is unspecified; authors report Success currently wins | `g5.agents.lifecycle.cancel` makes cancellation terminal, but this plan has not established the acceptance/ordering boundary for competing terminal outcomes. | Read task identity, outcome and cancellation authority together. Define when Cancel is accepted, when completion is final and the observable result of each ordering; seek a human decision only if existing authority cannot resolve it. Do not silently select Cancel-wins or Success-wins to make a model pass. Keep SM03's interruption/suspension defect distinct. |
| O3: a silent-host cleanup path is reportedly unreachable, with no other removal path; rotation readiness is also uncertain | Host custody, placement and rotation contracts exist; SM04/SM16/SM17 add concrete challenges. The dead-cleanup-path claim has no full trace in the deck. | Recover the cited path and distinguish stale/unreachable status from physical exit, drain purpose and removal authority. Reconcile automatic cleanup versus explicit operator action and what replacement readiness requires. Preserve the existing provision-before-drain obligation and scoped force/deadline exceptions; do not invent deletion authority from heartbeat silence. |
| O4: much of the flush policy is said to remain in the v9 design | Current durability ontology already governs identity, receipts, limits, failure blocks, cooldown and retention, and links to protocol detail. The deck's broad claim is not a completed coverage audit. | Map each checked property to its exact canonical rule or referenced v9 clause. Add only missing consequential meaning to its owner, with lineage and deduplicated references. Reconcile the five author-proposed general rules and two design-document-derived findings before treating them as approved ontology violations. |

The missing per-finding rule ledger is an evidence gap, not permission to infer that
all 18 have explicit ontology authority. In the selected slices, also resolve precise
handling of conflicting provider IDs (SM15/SM18), transfer rollback/operation authority
(SM08/SM12), and successor-flush progress expectations (SM11). These decisions remain
at the existing conversation, lifecycle and durability owners rather than in a second
parallel policy catalog.

**Design cross-check.** Existing commitments constrain the additions: dependency
settlement excludes held prerequisites; Start retries exclude unchanged invalid boot
specifications and distinguish transport replay; transfer failure retains old ownership
without authorizing new consent; unfinished forks keep legitimate durable claims;
provider policy still refuses unsupported native questions. The lease-replacement
exception permits control-plane replacement without proving host child exit. Apply these
allowed cases alongside the new counterexamples. Report model/source/runtime evidence
separately; author-assigned severity or a heuristic score cannot settle open intent.

## 11. Appendix

### A. Method and scripts

The supplied audit refers to two scratchpad scripts, but their executable files, exact
arguments and raw receipts are not included. The description below is enough to begin
reconstruction, not to reproduce the reported numbers reliably. Retain scripts and raw
results at a pinned commit before using the counts as CI baselines or experiment metrics.

- **Change locality.** For each first-parent commit on `origin/main`, take the files
  changed against its first parent, drop documentation, ontology, harness, CI and
  build-manifest paths, and map the rest to (top-level directory, deployable group,
  crate or directory). Deployable groups: control plane = `g5-api*`, `g5-authz`,
  `g5-blob`, `.sqlx`; daemon = `g5d*`; shared = `g5-contracts`, `g5-internal-api`,
  `g5-scm`, `g5-auth`, `g5-workspace-ops`, `g5-semantic`, `g5-storage`, `g5-common`;
  otherwise the top-level directory. Reported with and without changes above 250 files.
- **Data ownership.** Over non-test Rust files, match `INSERT INTO t`, `UPDATE t SET`
  (excluding `ON CONFLICT DO UPDATE`) and `DELETE FROM t`; record writer files and areas
  per table. Areas: capability by service-file name prefix (`workspace`, `worker_host`,
  `agents`, `git_governance`, `documents`, `external_issues`, `platform`), plus
  `core/maintenance`, `core/cloud`, `g5-api-routes`, `g5-api-data`, `g5-authz`. Also counts
  status-column updates, `begin()` and commit sites, transaction-taking parameters,
  `tokio::spawn` sites and interval timers.

Other counts reportedly used `git grep` and `wc`. Regex counts over SQL embedded in Rust
can miss or double-count multi-line/dynamic statements; no error bound was measured.
Resolve the 300-file exclusion in section 4 versus the 250-file sensitivity threshold
above, the commit-site arithmetic and the ontology bins. Distinguish migration statements
from live schema objects and path-based areas from true policy owners.

### B. Verification log

Original audit's reported rechecks at `2bd71661`, retained for traceability. They are
not results executed by this document review. Corrections and current source inspections
in appendix E take precedence over any broader reading of these entries.

| Area | Claim | Result |
|---|---|---|
| Lifecycle | `admit_stop` commits and returns `AlreadyStopped` for stopped/failed rows without touching generation or consent | held |
| Lifecycle | `stop_command_sent_at` update result unchecked before `vm.stop` is sent | held |
| Lifecycle | boot-cancel discards `kill` result and journals `Exited` | held |
| Lifecycle | purge removes the retained entry before deleting files | held |
| Lifecycle | generation from `txid_current()` | held |
| Lifecycle | hard-coded active-status set: 31 sites in 7 non-test files (reviewer: 33 in 11) | held, count adjusted |
| Git | one-open-flush index keyed on `workspace_id` only | held |
| Git | `fail` accepts only `FLUSH_TOO_LARGE` | held |
| Git | `complete` is `pub`; `verify_manifest` lives in the route | held |
| Git | no `approv` in the three merge and publication services | held |
| Git | merge route region commits three transactions | held |
| Git | provider merge takes `&mut ScopedDb` and the route commits after the HTTP call | held |
| Git | flush ref literal in five non-test files | held |
| Core | routes 273 commit sites vs core 159; 210 `ensure_*` calls (39 owner) | held |
| Core | `delete_workspace_refs` called at `workspace_service.rs:3652` inside the delete path | held (transaction extent per reviewer) |
| Core | `api` role in Helm values; Redis relay section empty; notification publish inside `create_notification` | held |
| Core | 88 of 108 test files use raw `sqlx`; 780 raw writes | held (reviewer: 85 of 106) |
| Core | `routes/git.rs:1493` re-implements write permission over grant tables | cited by two reviewers; not re-checked (unverified) |
| Agents | five `INSERT INTO tasks` sites in three files | held |
| Agents | reservation requires `queued_input.task_id = task.id` | held |
| Agents | JobService input insert omits `task_id` | held |
| Agents | prompt written and flushed before `input.accepted` is journaled | held |
| Agents | chat fork writes `completed`; CHECK allows `complete` | held |
| Agents | no production increment of `session_manifest_generation` | held |
| Daemon | `g5d` main hands off to `g5d_cli`; working-directory guard on Dispatch, Bootstrap, Host | held |
| Daemon | `g5d-host` declares `g5d-worker` and `g5d-worker` declares `redis` without using them | held |
| Daemon | one supported protocol version; 37 `deny_unknown_fields` | held |
| Daemon | method literals: 23 in `workspace_runtime.rs`, 38 in `server.rs` | held |
| Daemon | usage insert with fresh `Uuid::now_v7()`; guest journal at `/var/lib/g5d` | held |
| Daemon | `g5-contracts` says no parsing; `ontology_markup.rs` 6,136 lines; 75 axum/StatusCode refs in `error.rs` | held |
| Daemon | architecture tests exist in `g5d/tests/architecture/source_contracts.rs` | held |
| Platform | six scope strings decide owner semantics; 128 literal `"workspaces:write"` in routes | held |
| Platform | API keys `exp: i64::MAX` | held |
| Platform | Save idempotency key `workspace-file:{op}:{Uuid::new_v4()}` | held |
| Platform | `record_usage` has no production callers | held |
| Frontend | `SpecEditor` stores `localContentRef.current` after `await onSave`; `CodeEditor` stores the submitted content | held |
| Frontend | Zod schemas consumed only by tests; `harnessCatalog.gen.ts` generator with CI drift check exists | held |
| Frontend | server drops events; `max_dropped_events: 1000` per 60 s window | held |
| Frontend | `useReviewPreview` is a query over an endpoint that commits and pushes | held (both files' comments state it) |
| Cross-cutting | RLS on 207 tables, 127 policies, 368 grant/role statements, one `authz` schema | held |

### C. Crate inventory

| Crate | Kind | Rust lines (incl. tests) | Internal dependencies |
|---|---|---:|---|
| g5-api-core | lib | 152,662 | api-contracts, api-data, api-foundation, auth, authz, blob, contracts, internal-api, scm, semantic, storage, workspace-ops |
| g5-api | bin | 94,954 (94k tests) | core, routes and the shared set; dev: common, internal-api, workspace-ops, g5d, g5d-dispatch |
| g5d | bin | 49,494 | auth, contracts, internal-api, scm, semantic, storage, workspace-ops, every g5d-* crate |
| g5-api-routes | lib | 48,110 | api-contracts, core, auth, authz, contracts, internal-api, scm, semantic, storage, workspace-ops |
| g5d-tooling | lib | 47,277 | contracts, internal-api, scm, semantic, workspace-ops, g5d-foundation, g5d-runtime-core |
| g5d-cli | lib | 31,545 | every other g5d-* crate plus contracts, internal-api, scm, semantic, workspace-ops |
| g5-contracts | lib | 29,469 | none (25 dependents) |
| g5d-foundation | lib | 21,228 | auth, contracts, internal-api, scm, workspace-ops |
| g5d-dispatch | lib | 16,501 | contracts, internal-api, g5d-foundation, g5d-runtime-core, g5d-worker |
| g5-semantic | lib | 14,718 | contracts |
| g5-scm | lib | 13,275 | contracts |
| tests/e2e | bin | 10,814 | auth, contracts, internal-api, semantic, g5d |
| g5d-isolation | lib | 10,697 | contracts, g5d-foundation, g5d-runtime-core |
| g5d-worker | lib | 9,935 | auth, contracts, internal-api, scm, workspace-ops, g5d-foundation, g5d-runtime-core |
| fake-provider-cli | bin | 9,214 | none |
| g5-internal-api | lib | 7,542 | contracts (14 dependents) |
| g5d-host | lib | 7,258 | contracts, internal-api, g5d-foundation, g5d-isolation, g5d-runtime-core, g5d-worker (unused) |
| g5d-agent-turn | lib | 3,993 | contracts, internal-api, g5d-foundation, g5d-runtime-core, g5d-worker |
| g5-api-data | lib | 3,826 | api-foundation, contracts, internal-api |
| g5-workspace-ops | lib | 3,098 | contracts, scm |
| g5-api-foundation | lib | 2,916 | contracts, storage |
| g5-auth | lib | 1,942 | contracts |
| g5d-runtime-core | lib | 1,790 | contracts, internal-api, scm |
| g5-storage | lib | 1,700 | none |
| g5-authz | lib | 1,443 | contracts |
| g5-common | lib | 571 | auth, contracts, internal-api, scm, semantic, storage, workspace-ops (dev-only consumers) |
| g5-api-contracts | lib | 524 | auth, contracts, scm |
| g5-blob | lib | 387 | authz, contracts, storage |

### D. Related documents

- [G5 state machines in TLA+](slides.html): independent analysis at `412cccae`, with
  its complete finding crosswalk in section 6.8 and model inventory in section 8.7.
- [PR candidates for testing the ontology skills](../ontology_harness_tuning/ontology-skills-pr-candidates.md):
  experiments 2 (stale Save), 3 (allocation ownership), 4 (#146), 5 (#155), 6 (#176) and
  8 (#238) concern candidate areas in section 9.4. Reconcile each brief with current
  policy before reuse; #238's described commit-on-Save behavior conflicts with the
  current durability contract. Known cases are regression knowledge, not withheld tasks.
- [Ontology experiment results](../ontology_harness_tuning/ontology-experiment-results.md) and the
  [balance study](../balance-study/README.md) for what earlier runs found in the same
  code paths.
- [Architecture overview](../../../.g5/ontology/concepts/software_architecture.md) and
  [system design principles](../../../.g5/ontology/concepts/system_design_principles.g5.md)
  for the intent this audit measured against.

### E. Document review and current-head spot checks

This review corrected the supplied document on the local
`412cccae8dcb78453745341bb544d1011c35b581` snapshot. Source locations below refer to
that commit. These are static inspections and document checks; no application suite,
database instance, model checker or controlled agent comparison was run. The original
audit's 40 reported rechecks must not be combined with these as 40 new confirmations.

| Claim examined | Current evidence | Conclusion and remaining check |
|---|---|---|
| Audit baseline | `git rev-parse HEAD main origin/main` resolved all three to `412cccae…`; `git show 2bd71661` failed to resolve | Original audit snapshot unavailable locally; retain its findings as reported leads and re-pin before measurement |
| Missing module nodes/new harness rules | [Architecture overview](../../../.g5/ontology/concepts/software_architecture.md) links existing module pages; repository search found neither claimed `g5.harness.module-*` ID | Remove the absent-node premise and the supposed conflict between module nodes and semantic modules |
| API keys never expire/cannot be revoked | [Auth middleware](../../../backend/g5-api-core/src/middleware/auth_middleware.rs), lines 636-653, calls [ApiKeyService](../../../backend/g5-api-core/src/services/api_key_service.rs), lines 249-317, before constructing synthetic claims | Database revocation and configured expiry are checked. Stored-scope changes and long-lived subscriptions remain distinct investigations |
| Membership cache has unbounded lifetime | [InProcessAuthz](../../../backend/g5-authz/src/lib.rs), lines 192-193 and 440-494, uses `SETEX` with a 60-second TTL | Absence of eager invalidation does not prove violation of the accepted cache bound; test full revocation paths and relevant races separately |
| Stop cancels queued consent when already stopped | [WorkspaceVmService::admit_stop](../../../backend/g5-api-core/src/services/workspace_vm_service.rs), lines 2640-2687, commits/returns for stopped or failed status before consent/generation changes | Source supports L2's early-return concern; runtime regression must create queued consent and prove it cannot later launch after Stop |
| Failed boot cancellation retains custody until reap | [Host boot result handling](../../../backend/g5d-host/src/workspace_vm.rs), lines 349-363, removes the active entry and discards kill/journal errors | Source supports the L3/D5 concern on this path; inject failed reap and verify slot, port, exclusion and journal outcomes |
| Failed purge remains retryable | [Host purge](../../../backend/g5d-host/src/workspace_vm.rs), lines 1135-1158, removes retained inventory before filesystem deletion; expired-root pruning has similar ordering at 1400-1421 | Source supports the lost-inventory concern; exercise delete failure, retry and restart through the API before claiming an observed unsafe transfer |
| Synthetic generation orders authoritative snapshots | [Host control](../../../backend/g5-api-core/src/services/host_control_state.rs), lines 532-538, uses `txid_current()`; [host runtime](../../../backend/g5d-host/src/runtime.rs), lines 166-172, accepts larger generations | Ordering needs an explicit snapshot/serialization argument; changing to a sequence alone is insufficient. Concurrent/reordered response test remains unexecuted |
| SpecEditor acknowledges submitted bytes | [SpecEditor](../../../frontend/src/components/editor/SpecEditor.tsx), lines 6919-6929, reads `localContentRef.current` again after awaiting Save and clears dirty state | Source supports the U1 race when the buffer changes during the request; execute a deferred-response typing regression to establish behavior |
| Every protocol edit requires lockstep deployment | [Host control types](../../../backend/g5-internal-api/src/host_ctl.rs), lines 15-16, declare one version; the same file has optional/defaulted fields as well as strict decoding | Select compatibility cases for the actual wire change. Neither universal lockstep nor a new N-1 guarantee follows from the declarations alone |
| Commit-on-Save belongs in this closure | [Durability contract](../../../.g5/ontology/software_architecture/durability.g5.md#keep-file-saving-separate-from-git-durability) explicitly excludes a commit on Save | Remove that proposed behavior from M8 and preserve the existing contract |
| Quantitative baseline is ready for comparison | The displayed weighted score is 3.35; commit subtotals sum to 432 versus reported 388; ontology bins sum to 548 versus about 570 | Correct arithmetic/labels, retain unresolved totals, and require scripts and receipts before creating a baseline or CI guard |

The revised closure plan follows the current contracts, prioritizes demonstrated risk,
maps all finding IDs to candidate slices, and adds skill/evaluation work. It removes
blanket schema, outbox, N-1, line-count and model-count requirements. Source inspection
supports some defects and corrects other claims; it does not certify the remainder of
the original audit or establish that any production gap has been repaired.

### F. Independent-analysis incorporation

Incorporated on 2026-09-30 from [slides.html](slides.html), titled *G5 State Machines
Slides*, dated 2026-09-29 and identifying `main@412cccae`. Source file SHA256:
`4ffdbd8b69bc6832878c1943587b17b98c8b7717d7972f2095f89632a7db62f0`.

The update preserves all 18 source cases as SM01–SM18, their reported severity and
overlap with original findings; all eight model scopes; the normal/problem check
counts; the proposed host-fix result; the four ontology gaps; and unmodeled/uncertain
areas. The closure backlog includes each case, with adverse and valid checks. Existing
C/L/G/A/P/U/D and M1–M10 identities are retained; M11/M12 add host-management and
transfer scopes. A shared defect is counted once even when several sources describe it.

Only this plan was edited for incorporation. The original model directory/runner is
absent here. No TLC run, application regression, production fix, ontology amendment or
agent-efficiency experiment was performed in this update. Original slide claims remain
reported evidence until the section 8.7 procedure produces dated receipts.
