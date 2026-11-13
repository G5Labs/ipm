# Workspace/repository model cleanup

Implemented on `codex/data-model-cleanup`, created from
`main` at `04f0de5`. The final application, schema, frontend, CLI, workers and
seed workflows ship together after a database reset. There is no live-data
migration, backfill, compatibility endpoint or old-payload conversion.
Existing migrations 001–026 remain immutable. After rebasing onto main at
`2ecf6ca3` (GitHub App support), this branch's unmerged migrations are numbered
027–028. Main's review workflows use repository membership and published Git
refs; review copies derive their upstream from their parent workspace.
See [migration policy](database-migrations.md).

## Decisions

- Remove `workspace_branches` and `workspace_branch_repos` entirely.
- Canonical membership is `(org_id, workspace_id, repo_id)` in `workspace_repos`.
  Membership stores repository role and mount path, with same-org foreign keys.
- Git owns current heads, materialization, ancestry and divergence. Remove
  `base_branch`, stored fork/diff bases, cached heads, duplicate workspace ref
  names and the `head_cache` projection. No fork-point reporting is required.
- Working refs are `refs/heads/<workspaces.name>`. The working name is immutable
  and Git-valid. Base uses each repository's configured `repos.default_branch`.
- The immediate parent is `workspaces.cloned_from_id`. For `Base → A → B`, B's
  ordinary comparison is with A. Explicit PR targets are separate choices.
- Retain remote Git integration, Make Remote, background integration, explicit
  push/pull, sync history, conflict resolution, provider PRs and approvals.
  Put remote integration behind a canonical workspace/repository abstraction.
- Base and working workspaces share one remote integration on the pull side:
  provider transport, ancestry classification and per-repository import. Base
  never publishes to its provider. An explicit pull (or
  enabled auto-pull) accepts provider fast-forwards into the G5 branch, without
  changing VM files or saved recovery snapshots. Only diverged histories need an
  agent-assisted rebase in the running VM.

These decisions supersede stored Git-state fields and synchronization retirement
in [backend design v9](lean-backend-design-v9.md). This cleanup does not authorize
unrelated product removal or repository relocation.

## State ownership

| State | Owner and lifetime |
| --- | --- |
| Workspace identity, ownership, visibility and immediate parent | `workspaces` |
| Repository identity, provider URL and default/protected-ref policy | `repos` |
| Repository membership, role and root path | `workspace_repos`; no Git versions |
| Current local/provider heads, ancestry and missing/unborn refs | Git, read for the current operation |
| Selected external branch, auto-pull and pause policy | Optional `remote_integrations`, keyed to canonical membership |
| Disposable remote-tip hints | Shared Redis cache with five-minute expiry; never publication authority. **Amended 2026-10-09 (G5Labs/G5-Lean#405, PR #411):** one Postgres row per remote integration (`remote_tip_observations`), read as absent after five minutes; still never publication authority |
| Last acknowledged synchronization version | Git ref `refs/g5-tracking/<local-branch>/baseline`; not a workspace fork point |
| Integration queue, generation and leases | `remote_integration_queue` and `remote_ref_leases` |
| Exact sync/merge attempt inputs and outcomes | `remote_sync_runs` and `job_merge_runs`; only evidence needed by that operation |
| Docs and ontology indexing progress | `projection_state.applied_oid`; the version actually indexed |
| Flush/fork/attach recovery and immutable boot inputs | Exact operation snapshots and their existing Git pins |
| PRs, approvals, comments and review evidence | Exact versions selected by those retained workflows |

No replacement table, JSON field or permanent `fork_oid` may recreate a current
Git-state registry. A surviving SHA must answer an application question such as
which version was reviewed, published, indexed or captured for retry. An OID in
SQL does not keep a Git object alive; retain the necessary operation pins until
recovery/retention ends.

Docs, folders, ontology, grounding and intent use workspace/repository ownership.
Keep nullable repository ownership for supported non-Git document records. All
repository-bound rows retain same-org membership constraints. Repository-scoped
parent and uniqueness constraints prevent a projection from attaching content
to a sibling repository.

## Provider transport and publication

Agents use ordinary `git push` to origin, the internal G5 repository. The
runtime configures Git's upstream and a local relay for that exact repository;
provider credentials stay server-side and the internal token stays in the
relay. There is no agent-facing publish command. A local ref-refresh command
remains available for delivering provider observations without changing files.

The integration fetches objects and classifies the exact provider tip using the
shared pull engine. For every workspace, Base included, it stores the observation in Git
under `refs/g5/provider/<workspace>/heads/<provider-branch>`. If G5's branch is an
ancestor of the provider tip, explicit pull/push or enabled auto-pull accepts a
fast-forward through the shared publication boundary. The branch, provider ref,
watermark and `imported` receipt advance together after rechecking the complete
Git vector, policy and worker leases. When a cycle imports, it also lands every
other member of the same workspace whose import is due in the same cut: a
member that is bound, unpaused, eligible under its own policy (auto-pull on, or
its own queued manual pull) and a strict fast-forward of a freshly fetched
provider tip, whose remote-pair lease the cycle could claim without waiting.
A paused, failing, non-fast-forward or busy member is left for its own cycle
and never blocks the import. Co-landing is opportunistic, not an atomic
multi-repository guarantee. Background auto-pull holds a member's import
while the workspace is the target of an open G5 workspace PR whose provider PR
for that repository is merged, or being merged, while a sibling provider PR is
not; the merged-PR import or an explicit pull lands it instead. A provider PR
merged directly on GitHub is recognized only after G5 records it as merged.
A landing refused because another cycle moved the workspace first releases its
lease and retries promptly rather than waiting for the lease to expire. Published content projections and review
notifications follow that landing. Automatic cycles with auto-pull disabled
only deliver refs and record `fetched`. Neither path edits the VM checkout or
recovery snapshots. Manual fetch intent outranks automatic queue signals.
Divergence/rewrite policies still apply. Observations are retained
in Git while paused; the existing pause policy suspends subsequent provider
fetches until explicit reconciliation resumes. Immutable task inputs stay pinned. SQL contains binding/policy and
operation evidence, never a replacement current-head registry.

Automatic auto-push divergence pauses without opening a resolution workflow.
The chip still offers **Resolve with agent** in that state. The owner action
creates or reuses one resolution record under the workspace fence, then uses
the shared start path to fetch fresh provider input and launch with confirmed
agent settings in the running VM. A provider rewrite becomes an explicit
approval checkpoint; resolving divergence does not approve a provider rewrite.

The VM receives those refs as `refs/remotes/provider/*`. Immutable resolution
inputs use a separate workspace-scoped transport namespace and local pins.
Startup materialization, periodic runtime refresh (30 seconds), and synchronous
refresh before agent launch deliver the objects without touching HEAD, the
index or local edits. A failed launch-time delivery prevents starting a task
with missing inputs. `g5d workspace refresh` requests the same trusted operation;
it updates saved workspace/accessible-parent observations under `origin/` and provider refs, not the
working tree. Reading the UI or refreshing a provider never starts a VM.

An ordinary VM can `git pull --ff-only` to pick up G5's accepted fast-forward.
For a rebase task, G5 prepares and delivers immutable input refs before handoff;
the agent verifies those supplied inputs without fetching, refreshing or
updating them. The agent rebases its local commit series onto the
captured incoming commit and resolves semantic conflicts using the task
instructions. It verifies the incoming tip is an ancestor, the replay contains
no merge commits, and the final diff preserves local intent. The old local tip
normally is not an ancestor after rebase. Publication uses ordinary `git push`
with `--force-with-lease=<workspace-ref>:<captured-g5-head>` so a concurrent G5
advance rejects the rewrite. Flush preserves recovery state without publishing edits.

G5 permits rewrites of an existing workspace branch by its owner or current VM
under the same ownership, lifecycle, protected-ref and namespace admission as
fast-forwards. Git remains the head authority and atomically checks the old OID;
no database head tracking or resolution-specific rewrite grant is introduced.
The server cannot distinguish bare force from a leased force-push: agents must
use an explicit expected OID and never widen a stale lease without reconciling.
Internal rewrite permission alone does not authorize arbitrary provider replacement;
the [workspace rewrite policy](../.g5/ontology/features/governance.g5.md#distinguish-workspace-rewrites-from-provider-rewrites)
decides when an accepted workspace rewrite reaches the provider under the exact
destination lease, and keeps the approval checkpoint for a provider rewrite.

Accepted Git branch updates schedule the configured remote integration using
its current push policy; flush refs and rejected updates never schedule it.
The existing auto-push toggle controls implicit publication. New workspaces
default to auto; manual mode saves to G5 without implicit provider publication.
Automatic synchronization requires a binding and an existing provider branch.
Outbound ancestry classification and an exact destination lease reject a moved or
removed provider branch, including deletion after fetch, instead of recreating
it. Provider branch creation is a separate explicit operation. A background
sweep repairs missed queue receipts for auto-push even with auto-pull disabled.

Saving to G5 and reaching the provider remain separate outcomes. Git push
returns after G5 accepts and queues the update; it does not wait for the provider
or CI. Provider failure leaves the G5 commit durable. CI uses the provider's
normal triggers after forwarding succeeds. Mere existence of a same-named
provider branch never creates a binding or grants publication authority.

Base has no VM and no normal VM snapshot. Its remote-tracked repositories use
this same integration's pull side, acting under Base's current
owner ([Base parity](../.g5/ontology/features/workspaces.g5.md#track-base-like-any-other-workspace)).
Auto-pull starts enabled for Base. Base never publishes: its push mode is
always manual, Push to Remote is refused, and a Base that is ahead of or
diverged from its provider pauses until the owner accepts the provider history. No mirror or other poller writes a workspace
branch outside this integration.

## Reconciliation in the workspace

Explicit UI source rebases also follow [workspace rebase ownership](../.g5/ontology/software_architecture/workspace.g5.md#execute-workspace-rebases-in-the-running-vm):
a running VM executes even clean replay and fast-forward/no-op cases. Only a wholly
clean, fully published stopped workspace may use the central path, fenced against
Start. This does not change background provider imports, which leave VM files alone.

Working reconciliation requires the already-running owner VM, even when Git
could perform a clean server-side merge. **Resolve with agent** creates a normal
workspace session/task with exact local and provider commits pinned in Git.
Retries reuse the task and repair only abandoned uncommitted input preparation.
The runtime delivers these inputs before launching the agent.

The task checks its selected local branch/head and preserves existing edits,
merges the exact locally available input, resolves conflicts, validates, and
pushes to origin with an explicit Git lease for the captured local tip. It never
fetches from or pushes directly to a provider. The existing completion check
verifies both captured tips are ancestors of the published result. External
history rewrites retain their explicit approval checkpoint. Git owns sequencer
and conflict state; SQL owns only the attempt and its immutable evidence.

## Reader-visible state

The attached owner reads live VM files. Other readers and a stopped owner see
the saved flush only while its complete vector of observed workspace branch
heads is still current. A subsequent publication selects the complete current
branch cut instead, so an old flush cannot mask new published content. Recovery
still retains and restores the exact saved local HEAD and dirty snapshot;
changing browsing precedence does not discard recovery data.

A provider fast-forward updates the G5 branch and published reader view. The
attached owner's VM checkout stays unchanged until `git pull` (or explicit
rebase for local divergence). Existing snapshots retain their original HEAD
and bytes for recovery. Background fetches with auto-pull disabled deliver
provider refs only. Diverged G5/provider histories preserve the G5 branch and
require rebase; the handoff delivers immutable inputs before launching the
agent, which must use those refs without fetching or rewriting them. The UI
labels live, saved and published workspace sources.

## Git reads and comparisons

One canonical resolver selects each member's working or Base ref. Reads use Git
or the selected immutable flush view. Missing working refs fail closed instead
of recreating them; expected unborn Base repositories remain distinguishable.

Comparisons require a live, same-org immediate parent, current read permission
and exactly matching repository sets. Resolve parent and child refs once and
compute `merge-base(parent_oid, child_oid)` for each repository. Changes uses
that base to the captured child; rebase previews use the same selected OIDs.
There is no fallback to `main`, a repository default, a PR target, reflogs or an
old operation receipt when the parent is unavailable. No common ancestor and
multiple best ancestors produce explicit unavailable results.

Parent ref movement can change the comparison range. Parent deletion or revoked
access invalidates the comparison. Hard deletion must not silently reparent
children. Default-ref changes invalidate affected Base projections and derived
upstreams. The VM/CLI may receive an upstream derived for its immutable boot or
operation input, without a separately configurable stored `base_branch`. An
unavailable parent leaves that upstream absent; it does not block starting or
editing the child. Comparison and update operations still report unavailable.

## Implementation work

1. **Schema and contracts.** Append migration 039 and its SHA384 entry. Re-key
   content and remote-operation tables, replace authority SQL functions, preserve
   FORCE RLS and role grants, drop obsolete columns/functions and use restrictive
   table drops. Verify same-org FKs, parent deletion, immutable working names and
   projection invalidation. Remove legacy Rust models and operation identities.
2. **Core lifecycle.** Cut readers, org/signup bootstrap, checked forks,
   partial-fork recovery, runtime/restore, Git admission, publication and deletion
   over to canonical membership. Preserve complete-cut fencing, incarnation and
   ownership checks, stale-head refusal and exact approval/review evidence.
3. **Remote integration.** Use the abstraction and shared pull path described
   above for Base, explicit working pulls and background integration. Preserve
   Make Remote, push, sync history, pause/retry and conflict resolution.
4. **Content and clients.** Re-key docs, ontology, intent, grounding, comments,
   issue links and projections. Replace branch-ID routes with
   `/api/v1/workspaces/:workspaceId/repos/:repoId/...`. Update frontend selections,
   query keys, schemas, WebSocket events, CLI/tooling, VM contracts and E2E callers
   together. Do not reinterpret an old branch UUID as a repository UUID.
5. **Fixtures and remaining writers.** Update retained API tests, seeds,
   validation fingerprints and browser/system workflows to create the final model
   directly. Retire pre-reset mapping/backfill tooling from active workflows.
   Regenerate/prune SQLx metadata against the final schema. Historical migration
   SQL and explicitly historical migration-stage fixtures remain intact.
6. **Validation.** Complete the focused behavior tests and final-schema checks
   below, review all surviving Git state, then run the relevant broader checks.
   No deployment or user-database reset is part of implementation validation.

Production cutover code, clients, retained fixtures and seed scripts use the final
model. Obsolete SQLx cache entries are removed. The restricted-role migration
replay and no-op rerun pass, as do the final-schema, tenant-scope, Git-authority
and remote integration regression suites. Local validation and the remaining
VM/Cloud CI gates are recorded below.

## Validation

Use the [local validation workflow](local-validation.md), including
`just validation-recommend main` and preflight. Prepare/reuse the relevant Docker
slices and remove this worktree's validation services when finished.

- Replay the full migration chain as the deployment-like `g5_migrator` role in
  an isolated PostgreSQL 17 database, seed final workflows and rerun the migrator
  as a no-op. Existing populated databases must fail the reset-only gate.
- Assert absence of both branch tables and head/base/fork columns. Exercise
  membership/content/remote-policy FKs and tenant isolation, working-name
  immutability, parent retention and default-ref projection invalidation.
- Exercise fresh Base bootstrap, differing repository defaults, nested forks,
  exact source cuts, partial creation/recovery, missing refs and selected flush
  views. Plain Git changes must be visible without SQL head updates.
- Test comparisons after parent movement, merges and rebases; denied/deleted
  parents, mismatched repository sets and missing/multiple merge bases.
- Test Base and working pulls against local Git remotes: equal/FF/diverged tips,
  external rewrites, no watermark, unchanged siblings, stale membership/policy/
  credentials and actor access, retries and partial Git/SQL failure. Preparation
  must not mutate refs. Preserve exact push leases and remote-pair serialization.
- Retain production-boundary tests for docs, ontology, projections, history,
  previews/explainers, Make Remote, sync/merge history, PRs, approvals, Git push
  policy and ownership transfer/deletion races.
- Run the offline all-target Rust check, affected Rust suites, frontend typecheck,
  build and tests, source-contract/preflight checks, and relevant E2E slices.

Both `g5-ci-rust` and `g5-ci-e2e` must pass on the current PR head before merge.
Record local validation, Cloud CI, deployment and observed QA behavior separately.

### Local evidence (2026-09-25)

- Full migration replay and a no-op rerun passed on PostgreSQL 17 using the
  restricted `g5_migrator` role with the deployment prerequisites provisioned.
  The reset-only gate rejects populated pre-cleanup databases.
- `SQLX_OFFLINE=true cargo check --workspace --all-targets --offline` passed.
  The full API test run had two failing targets; both were repaired and their
  focused reruns passed. Related VM, remote integration, conflict completion,
  tenant isolation, projection, CLI and Git-authority suites passed. The
  cross-process remote lease regression also passed.
- Conflict resolution tests exercise the real API, SQL, Git repositories and
  durable dispatcher with a test VM peer. They cover stopped-workspace refusal,
  concurrent retries producing one agent task, provider movement before launch,
  pinned exact inputs, and refusal of a result missing either captured parent.
  This does not establish a successful live LLM turn in a microVM.
- Frontend build and all 3,104 unit tests passed. After removing duplicate
  remote-control IDs, the build and 64 affected tests passed again. Nine Chromium
  scenarios plus authentication passed against the rebuilt API/frontend:
  stopped-workspace forks and name collisions, workspace creation, Base's VM
  gate, and the two chat timeline recovery regressions.
- The multi-repository fixture seed completed against the final schema. The
  main seed successfully reseeded through the lifecycle API, created content
  and published its code/docs Git refs. This caught and fixed missing settings
  org IDs, stale fixture refs and malformed ontology ownership queries.
- Preflight, migration checksums, source architecture checks and changed shell
  syntax passed. Historical migrations 001–017 were not modified.

`just ci-docker-prepare` built the services and seeded the CI organization, then
failed at Phase 4b because this laptop's Docker environment lacks `/dev/kvm`.
The main fixture's final VM-readiness step and the VM-dependent Rust/browser
slices remain unvalidated here. The browser checks above ran independently
against the rebuilt disposable stack, not as a passing full Docker receipt.
Cloud CI is required on the PR head. No deployment, customer database reset or
live-provider mutation was performed during local validation.

## Coordinated landing

Stop old API/background writers and workspace runtimes, reset the database and
SQLx ledger, replay unchanged migrations plus 036–037, and seed with the final
scripts. Deploy matching API/worker/frontend/CLI revisions. Inventory external
state tied to old IDs, including worker registrations, VMs, caches, client
selections and internal Git mirrors; a database reset does not erase Git storage.
Keep cleanup limited to the reset environment and preserve customer remotes.

Use the operational reset pattern in [the QA reset runbook](qa-database-reset.md).
Do not reopen its historical baseline-edit exception or run old binaries against
the new schema. Validate bootstrap, fork, edits/push/pull, stopped reads,
projections/Changes, provider PRs and restart/restore before resuming normal use.


## Review follow-up: integration ownership and recovery

The `remote_integrations` module owns policy reads and transitions, scheduler
queue admission, worker leases, convergence receipts, reconciliation and
resolution-attempt pins. Workspace summaries and provider PRs consume its views.
The old worker and merge service paths only expose compatibility entry points.
An architecture check rejects policy/queue/lease table literals outside this
module in API production code. The data-layer queue repository remains the
owner of the canonical SQL enqueue operation.

Worker and approved rewrite publications register synchronization effects during preparation and
return an opaque `PreparedRemotePublication`. Its generic branch commit cannot
be extracted. Worker publications require a live claim, including Base's, which
uses the same worker as every workspace. Admission runs under the workspace
fence before any consumer effects; the integration constructs the convergence
receipts. Make Remote returns an
opaque binding publication that must apply provider identity, policy, audit and
watermark changes together. The generic commit layer applies generic ref
effects; SCM alone names synchronization refs.

Remote-tip hints use Redis TTL entries shared across API/indexer processes,
scoped by organization and canonical membership and validated against the
current remote binding. Reads reject expired or malformed hints. Cache outages
suppress hints without stopping Git operations or authorizing publication.

**Amended 2026-10-09 (G5Labs/G5-Lean#405, PR #411):** the hint is one
`remote_tip_observations` row per remote integration (tenant-scoped, cascading
with the policy row), written by the advisory pinger and the sync worker inside
their own transactions after locking the policy row, and read by the fork
advisories and the tracking summary. Reads ignore rows older than five minutes,
stamped in the future, or bound to another remote URL or branch. There is no
cache outage any more: a failed hint statement is a Postgres error on the
caller's connection, so the two readers that treat the hint as optional (fork,
tracking summary) run it under a savepoint and degrade to "no hint", and a
writer's failure fails the operation it belongs to. The hint still never
authorizes publication.

Conflict-resolution launch holds the merge-run row lock through task enqueue
and commit. A committed task freezes its two Git input pins. If no task exists,
old or partially written pins are abandoned preparation and can be replaced
under that same lock. This repairs retries after enqueue failure, commit
failure, process loss, or an uncertain commit response without accumulating a
new set of pins per attempt.

Regression coverage now includes a producer subprocess followed by an API
advisory read, enqueue and deferred-commit failures followed by remote advance,
disable/permission changes at the publication fence, and the SQL queue
precedence matrix. A deterministic
resolution fixture verifies delivered pins, conflict merge, resolution commit and
leased internal publication from the dispatched operation inputs.

## Provider handoff validation (2026-09-25)

The implementation adds credential-free runtime refresh/publication, delivers
provider and immutable resolution refs, and keeps working refresh separate from
workspace publication. Git operations explicitly read the published cut;
browsing may select a current flush. Parent updates preserve published ancestry.

Before the local approval service became unavailable, native tests passed for
953 core cases, 22 remote-integration cases, 17 background-integration cases,
nine Git-authority cases, and the active-merge guard. The new dispatcher tests
verified dirty-file/index preservation, exact local input delivery, publication
without agent credentials, stale-lease rejection and preservation of another
publisher's commits. The broader dispatcher run passed 25/27 cases; its catalog
expectation was updated for `refresh`, and its journal timing test passed alone.
Six of eight multi-workspace cases passed; the remaining fixtures now provision
the required running VM and select explicit reconciliation triggers.

Final focused frontend tests pass (45), shipped-agent hook tests pass (46), and
the frontend production build passes. The final workspace-wide Rust compile,
strict Clippy for affected crates, formatting and source preflight pass. Runtime reruns of the last
changes—local result acknowledgement, optional parent-ref delivery, explicit
published-cut selection and the adjusted multi-workspace fixtures—remain pending.
Automatic approval review rejected elevated local test execution with a 401
from its authentication service. No current-head cloud-CI or deployment claim
is made for this update. The disposable `g5-provider-pg` and
`g5-provider-redis` containers must be removed after those reruns.


## Native Git forwarding update (2026-09-26)

Ordinary `git push` replaces the introduced `g5d workspace publish` interface.
The agent profiles, resolution prompts and regression workflows now use Git.
Origin's relay and upstream are installed by the runtime per repository. The
remote integration consumes accepted branch writes, applies the existing
per-workspace auto-push policy and never creates a provider branch implicitly.
The API accepts authorized rewrites of existing workspace branches with Git's
old-OID concurrency check; agent workflows use explicit destination leases.
Provider forwarding still requires an existing ancestor tip. Background outbound recovery does not
require auto-pull; flushes and rejected pushes do not schedule publication.

Focused tests exercised native Git through the real dispatcher without agent
credentials, real smart HTTP through the integration worker to a local provider,
auto/manual modes with auto-pull disabled, missed-queue recovery, and missing or
deleted provider branches. Broader validation results are reported separately
from this implementation description; no deployment is implied.

## Provider credentials after the GitHub App rebase

`RepositoryCredentials` owns credential selection for a repository and operation.
Remote integration, advisory polling, fork verification, Make Remote, and
provider pull requests all use that service.
Callers supply the organization, repository, acting user, captured URL and
operation; installation IDs, binding versions, permission checks, token minting
and caching remain private to the credential service and GitHub App adapter.

For GitHub, any organization binding selects App credentials. Its latest snapshot
must be active, cover the exact repository named by the validated GitHub URL, and
grant the operation. Disconnects, missing coverage, missing permissions and token
mint failures never fall back to a personal token. An organization with no App
binding continues to use the acting user's personal connection. Other providers
retain personal credentials; providerless local remotes remain unauthenticated.
Every resolution checks current organization membership and the captured
repository URL. Make Remote has a separate adoption entry point that checks the
unchanged local identity and current administrator role before resolving the
proposed provider URL. No provider credentials enter the workspace VM.

Git reads request `contents:read`; pushes request `contents:write`. Reading a PR
requests `pull_requests:read`, and creating/updating one requests
`pull_requests:write`. Merging a PR requests `contents:write`, as required by
[GitHub's merge endpoint](https://docs.github.com/en/rest/pulls/pulls#merge-a-pull-request).
Each installation token is restricted to one verified repository and one
permission. The Git transport converts GitHub SSH clone syntax to HTTPS for
token authentication without rewriting the stored repository identity. Native
agent `git push` still saves to G5; auto mode queues provider
publication only for an existing configured provider branch.

An Issues-only installation must grant Contents and Pull requests permissions
before it can serve these operations. After the administrator approves those
permissions in GitHub, refresh the organization binding in G5 so its recorded
permissions are current, and include the code repositories in its coverage.
