# Retire ontology and document projections

[Issue #620](https://github.com/G5Labs/G5-Lean/issues/620) supplies the retirement
decision and three-PR rollout. The authorized publishing branch remains
`retire-ontology-document-projections`. PR A merged as `c5e01b1a` in
[PR #623](https://github.com/G5Labs/G5-Lean/pull/623). B merged as `213c2b69` in
[PR #630](https://github.com/G5Labs/G5-Lean/pull/630); C merged as `0679c3b1` in
[PR #631](https://github.com/G5Labs/G5-Lean/pull/631).

## Delivery boundary

B removes the projection worker and request-time producers, ontology-node routes
and browsing/editing UI, and historical node identities from public contracts.
Files and current Markdown remain the supported surface. C drops the legacy
tables and referencing columns after B deploys in QA. The owner's
[2026-10-18 decision](../../chat_intent/qa-workspace-lock-contention/forward-only-retirement.md)
removes rollback and old-host compatibility requirements; incompatible hosts may
be manually reprovisioned. Monitor B's deployment while implementing C. Do not
introduce a compatibility cache, dual writes or a waiting period for rollback.

A's caller exceptions now lose their active node consumers: `get_node` supported
node creation, `LinkedNodeDTOSchema` decorated document detail, and the linked-doc
resolver supported metadata writers. Remove those consumers with their owners.
Canonical frontmatter references remain owned by `doc_reference_service`; no
replacement persistent cache or full-repository HTTP indexing is introduced.

## Owners and retained behavior

[Read-driven documents](../../ontology/features/documents.g5.md#g5.documents.projection-recovery)
replace projection recovery. The captured workspace reader owns repository choice,
source versions, bounded reads and the authorized live overlay. DocService owns
creation and document metadata; reads derive membership/content from Git and live
files. Unreadable mirrors fail, and notifications never delete metadata or drafts.
The docs-role repository binding remains. Document Save, review, explainers,
reference resolution and failure/retry behavior keep their existing owners.

Remove only the projection task from the production maintenance runner. Keep the
maintenance role (renamed to `daemon` in the follow-up below) and every unrelated maintenance job, provider sync, branch-update
publishers and the pull-request refresh subscriber. B retained the projection
invalidation hooks and privileged lookup because the pre-B image called that
lookup and needed binding/default-branch changes to invalidate unchanged-OID
checkpoints. C removes both triggers, their functions and
`authz.lookup_repo_org_for_projection` with the retired cache storage. B has
already removed the long-lived worker and request producers that held projection
and membership locks. Preserve
current admission, RLS, commit fences and provider metadata locks; the superseded
incident-specific global timeout/lock changes are absent and remain excluded.

## Schema contraction

Migration 091 repeats 090's unfiltered, locked node-comment guard using the
deployment migrator's existing definer-role pattern. Drop the node index and
scope constraint, `comment_index.node_id`, `tasks.node_id`, and
`docs.scope_node_id`/`scope_mode`; restrict comment scope to line and suggestion.
Remove `doc_node_links`, then `ontology_nodes` and `projection_state`, with no
broad cascading DDL. Remove the privileged lookup as its owner and restore
borrowed grants and role membership. C preserved all other document fields, task
rows, line/suggestion threads, replies and mentions. It deferred the optional
document-column cleanup permitted by the issue.

Remove the remaining `Doc.scope_mode` reader and adapt current-schema fixtures,
maintenance assertions and administrative seed clients. Historical migration
tests retain their explicitly bounded old schemas. The Helm pre-upgrade hook
runs before new pods; a B pod reading that field during C's cutover can fail
until the new image starts. The release moves forward rather than supporting
the earlier binary against the contracted schema. No host or API reprovisioning
is performed as a precaution by this change.

The QA deployer currently adds Helm `--atomic` for ordinary deployments. Remove
that automatic application rollback under
[forward-only QA delivery](../../ontology/features/operations.g5.md#g5.quality.delivery.qa-forward-only),
while retaining `--wait`, its deadline and failure reporting. A committed
pre-upgrade migration is not undone by Helm rollback. Both ordinary and explicit
reset deployments must exercise the corrected argument path; no reset becomes
implicit and the worker-image selection and readiness gates remain unchanged.

The [node-retirement contract](../../ontology/features/documents.g5.md#g5.documents.node-retirement)
removes the legacy node-backed Core Ontology sidebar, spec tabs/URLs, node popovers/deep links,
node citations, chat-session node decorations, unused node sync contracts and the
explainer node picker. Keep the shared Project Doc editor and
file navigation. The [2026-10-19 file-backed integration](../saehan-499-ontology-editor/ontology-plan.md#file-backed-integration--2026-10-19) permits a current-format Core Ontology file view without restoring node APIs or projections. Retained trace/frontmatter links identify repository files; a
canonical SemLift component maps to its defined `.semlift/specs/<component>.semlift.md`
path without a node lookup, while explicit repository paths stay unchanged.
A cross-workspace trace is refused; file authorization still validates repository membership.
Repoint browser workspace materialization to the Files tree
before removing ontology endpoints. Invalid retired node fields must not recreate
projection data or silently select a different object.

Seed and canary clients follow the same retirement boundary. Native Git
publication completes without projection-refresh requests; retain its pinned
commits, leases, manifest membership checks and lost-response recovery. Seed
validation reads committed spec files and repository-qualified document handles
and references. The Azure document-creation canary omits the retired node field.
Retire the format-panel-only seed along with that removed UI. Manual self-hosted
canary execution remains separate from local syntax and reader-contract checks.

## Unused document metadata follow-up

After C merged, the owner asked why unused document metadata had not been cleaned
up. Complete that cleanup for the seven columns with no semantic runtime reader:
`persona`, `instruction`, `is_future`, `review_iterations`, `custom_persona_label`,
`source_files` and `mapping_content`. Migration 092 removes their obsolete stored
values and dependent persona constraint, without deleting document rows. Remove
their model fields, null assignments and seed writes. View classification and
source references continue to come from repository files and frontmatter under
the existing document-read owner; do not remove those file fields.

The issue's wider "writer-less" list is not equivalent to unused storage.
`docs.content` still supplies UUID-document fallbacks for missing/unborn files and
starter views; the two commit-SHA columns still reach document-detail responses.
Preserve those three fields and their current behavior in this cleanup. Removing
them requires a separate behavior/data decision, not just removing dead schema.

Validate the populated 091-to-092 upgrade through the production migration
runner as a restricted migrator, comparing every retained document field and
discussion row, plus a fresh install and no-op rerun. Exercise document reads and
updates, internal file metadata updates, view listing, and both affected seed
generators on the contracted schema. Obtain Claude CLI fable/max approval before
publishing the follow-up; apply the same CI, review and conflict monitoring.

Local follow-up validation on 2026-10-18 passed 54 selected regressions, including
the populated restricted-role migration, public document creation/read/view
flows and the internal canonical-content refresh. Every retained field survived
the upgrade. The production `g5_migrator` CLI passed populated 091-to-092 and
empty-to-092 runs plus no-op reruns. Both affected seed generators executed on
092; migration checksums, shell syntax, Rust formatting and workspace Clippy
across all targets with warnings denied passed. These results do not establish
authenticated deployed-QA journeys.

## Maintenance daemon naming follow-up

The owner requested `g5-daemon` now that the process no longer indexes documents
or ontology. Rename the maintenance-only role to `G5_ROLE=daemon`, its Kubernetes
Deployment and Compose service to `g5-daemon`, and the Helm values/template owner
to `daemon`. The shared `g5-api` executable and image remain the deployment
artifact. Keep the same maintenance tasks, provider synchronization, secret
projection, storage mounts, resources and shutdown behavior; this adds no HTTP
listener or replacement indexing work.

Update every active deployment, local/self-hosted start/restart/reset command,
secret rotation script and operator instruction. Helm removes the old Deployment
when applying the new chart. Compose cutover must retire the old service rather
than leave a second maintenance process running; self-hosted upgrade must stop
it before migrations. Recognizing an existing old container for retirement or
credential-data protection is not an old-role compatibility alias. Preserve
historical incident records and migration references.

Validate the actual daemon role through startup, maintenance and graceful
shutdown, render QA and local/self-hosted deployment configuration, and run the
affected operational command/secret-delivery regressions. Obtain a fresh Claude
CLI fable/max review before updating the open follow-up PR; monitor CI, blocking
review comments and conflicts after publication as before.

Local rename validation on 2026-10-18 passed five Rust regressions for startup,
secret delivery, maintenance task lifecycle and projection retirement. The actual
`G5_ROLE=daemon` process started on an isolated schema-092 database without an
HTTP listener, joined maintenance tasks on SIGTERM, and rejected the old role.
QA Helm rendering changed only the intended names; image pins, secrets, storage
and resources matched the prior manifest, and invalid daemon storage settings
were refused. A real isolated Compose cutover removed the old container,
preserved the existing database container and started `g5-daemon`. Operational
Python checks, self-hosted bundle checks, preview checks, Rust formatting,
Clippy for all g5-api targets with warnings denied, shell syntax and the Just
command audit passed. This is local validation, not a deployed rename claim.

## Retained-flow corrections

View deletion accepts the repository-qualified handle already returned by document
listing, as well as retained UUID metadata identities. Resolve and validate the
workspace/repository/path and view type before the existing owner file facade;
its hash fence protects concurrent writes. Remove matching metadata only after
successful file deletion. Never use a bare path to select another repository.

[Generate inputs](../../ontology/features/documents.g5.md#g5.documents.generate-inputs)
remain the current workspace's committed Project Docs, excluding dirty paths and
workflow artifacts. All eligible documents remain selectable. Outstanding review
actions come from the captured committed ledger; absent projection marker rows
mean unknown generation history, not that every file changed. Remove spec/node
choices and database-marker dependence, without inventing a new completion store.

## Historical discussions — verified empty-node boundary

[Comment identity](../../ontology/features/documents.g5.md#g5.documents.comments.identity)
and [authority](../../ontology/features/documents.g5.md#g5.documents.comments.authority)
remain governing. Bodies, replies, authors, timestamps and resolution state must
survive; ambiguous/missing identities cannot become guessed file/line anchors.

The issue's 2026-10-17 read-only inventory records zero QA node threads and zero
non-null task/document node references or document-node links. QA retains 44 line
and 217 suggestion threads, with 22 replies and no node-thread mentions. Production is outside
the Lean migration ledger and requires migration 039's coordinated empty-database
reset; its one unbound node thread is not eligible for this forward migration.
These are the owner's reported counts, not queries executed from this VM.

B therefore makes no row conversion and adds no legacy listing. Its transactional
migration asserts that no comment has node scope or a node ID, refusing the upgrade
without changing schema or the ledger otherwise. The production runner refuses
DO-block data reads under a temporary NO FORCE sequence. Use migration 088's temporary `SET ROLE authz_lookup_definer` pattern
instead: borrow only SELECT on scope/node_id, hold a table lock through the check,
and restore exact prior column grants and role membership. FORCE RLS stays enabled
throughout, and any failure rolls back the entire migration. All line/suggestion
rows and replies are untouched. C must repeat the guard before dropping the column. The read-only
[inventory query](legacy-comment-inventory.sql) remains available for rollout checks.

## Validation and rollout

Exercise the actual maintenance runner with deterministic scheduling, duplicate
and stale branch hints and restart; observe repository reads, projection rows,
advisory locks and remaining jobs/subscriber. Exercise real HTTP document/file,
comment, Save, review and explainer paths with empty projection caches, authorized
live and committed sources, changed Git refs and unavailable sources. Retain
bounded reads, failure retries, owner transfer/revocation and competing writers.

Rehearse the real migration runner as `g5_migrator`, with a refused node-thread/reply seed and a successful line/suggestion seed,
retained node-linked tasks/docs,
fresh installation and no-op rerun. For C, compare all retained fields before and
after migration, verify that legacy tables/columns/hooks are absent, reject node
scope, and exercise repository binding/default-branch changes with no projection
objects. Preserve B's bounded historical migration test. Test application startup
and document/task/comment consumers on the contracted schema.
Use a small real PostgreSQL pool and barriers around source/provider activity,
maintenance hints and authenticated requests; retain the historical held-node
transaction as discriminating pre-retirement evidence without claiming arbitrary
future blockers cannot exhaust a pool.

Run affected Rust, frontend and browser regressions, then obtain independent
Claude CLI `fable`/max review before publishing C. Keep the approved source frozen
except for verified acceptance blockers; monitor CI, blocking reviewer findings
and conflicts after publication. B's QA execution evidence requires deployment
after merge, including authenticated Base/other-workspace source/provider changes;
a health check alone does not satisfy that rollout gate. Record what deployed
traffic actually demonstrates; do not substitute CI results for that evidence.

Local validation on 2026-10-17 passed the 245 selected backend integration cases
across the main run and corrected-fixture reruns, 73 affected core unit tests,
23 writer/authority/flush cases, and the provider background-sync regressions.
The actual `g5_migrator` rehearsal passed populated success, node-thread/reply
refusal with unchanged ledger/schema/history, fresh installation and no-op rerun.
Frontend validation covered all 288 test files across the full run and corrected
retirement expectations, production build, TypeScript, lint and formatting.
Eight real browser editor/review cases passed (one cold-start timeout passed on
retry); Playwright discovered all 748 full-stack cases. SQLx regeneration removed
ten obsolete query cache files; Rust formatting and workspace Clippy passed.
The seed-client follow-up passed 25 Rust guards, native Git publication
regressions, 11 reader response/identity cases and shell/canary syntax checks;
workspace Clippy passed again. Manual self-hosted/Azure canaries were not run.
Both Rust and VM-backed E2E CI passed on B's approved head `29adbdda` before merge.
These are B's baseline results, not validation of C or the postmerge QA deployment.

C validation on 2026-10-18 passed 170 selected backend integration cases across
migration, document/file/comment/explainer, provider synchronization, maintenance,
seed and retirement regressions. The production `g5_migrator` runner passed a
populated B-to-C upgrade, fresh installation and no-op reruns; an unexpected node
thread with a reply and mention refused C with fixture data, schema, ledger and
role membership unchanged. The migration regression also covers dangling
task/document references, every retained field and both prior definer-role SET
states. Compliance and mini-calc metadata generators execute on schema 091;
BankWorkflow preparation preserves source files and trace records. Native Git
publication, self-hosted packaging, the real Codex seeder and 39 QA delivery/reset
tests passed. These are local results, not authenticated postmerge QA journeys.
