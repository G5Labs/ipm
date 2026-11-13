# Ben ontology uplift

## Rebase on organization-scoped tabs — October 15, 2026

Advance from main `970b28f6` to `d679e862` (PR #567), preserving the published
ontology/harness branch at `c915b061`. The only upstream ontology file changed is
the old `workbench.g5.md`; audit all three affected owners before resolving its
modify/delete conflict:

| Upstream change | Reconciled owner |
| --- | --- |
| `g5.workbench.tabs`: route-organization tabs and picker, retained other-org tabs, membership and route evidence | Refine the existing UI block under the same ID. Preserve session-return rules and record the reserved `fork` name's existing-workspace limitation. |
| `g5.workbench.landing`: suppress phase rows and filter choice while keeping records and API results | Keep this stable ID in UI, including local filtering and the current phase in the header; add one comparison row. |
| `g5.workbench.phases.state`: recording remains; visibility belongs to Activity | Preserve the current `g5.workspace.phase-state` body and add a reference to the display owner in its metadata. |

No application changes belong to this PR. Preserve main's code and tests exactly,
keep every unrelated ontology body, and retain the large unfinished directory move
in the primary checkout untouched. Rebase only in the attached isolated checkout;
`codex/pr500-before-main-d679e862` retains the published history. Validate the source,
structure, links, harness and exact application parity, then push with an explicit
lease on `c915b061`. Do not monitor CI.

Completed: 27 commits replayed onto `d679e862`; the old workbench file's
modify/delete conflict was resolved using the owners above. The other 26 patches
are unchanged. All 421 unrelated ontology bodies remain unchanged; the tabs block
is refined and one Activity block is added. The phase-state body is unchanged.
Source preservation, 423-block structure, 1,305 generated links, six harness tests,
generated skill consistency and whitespace checks pass. Application code, tests,
dependencies and CI configuration exactly match main, and the net PR diff adds no
TLA/TLC artifacts. No application tests or CI monitoring were rerun for this
ontology-only reconciliation. The primary checkout's unfinished directory move
remains untouched.

## Rebase on workspace-tab and corner-control changes — October 15, 2026

Advance from `eedcc8ae` to main `970b28f6`, retaining the published language pass
at `77ad0621`. Main adds PR #559's workspace-tab return, PR #560's corner controls
and PR #563's two frontend test timing fixes. Application files must remain identical
to the new main; this PR changes ontology and harness only. Drop the cancelled runtime
repair/revert pair (`5e8ad052`, `0e801626`) when replaying history.

Audit every upstream ontology delta before resolving the rewrite conflicts:

| Old owner / change | Minimal disposition in the rewritten ontology |
| --- | --- |
| `workbench.g5.md`: tabs body, landing exception and return-state evidence | Keep `g5.workbench.tabs` in UI: session-local return, fresh-open/reload distinction, workspace-ID isolation, forget-on-close/person-change, branch/path guard and URL exceptions. Retain the upstream evidence link and its limits. |
| `platform_reports.g5.md`: new trigger block | Keep `g5.quality.platform-reports.trigger` in UI, linking the existing platform-bug owner. Preserve the shared controller, conditional fallback, drawer reachability and shortcut-only limit. |
| `workspaces.g5.md`: new Chat corner block and Base exception | Keep both stable IDs in UI. Preserve desktop-only right-edge reuse, existing gates, disabled Base/read-only exception and the intentional lack of floating-placement restoration. |

The four retained blocks have no equivalent owner in the rewrite. Add concise bodies
and provenance to UI, disclose them in the existing Ben comparison, and retain main's
request/design files with working references. Do not resurrect old ontology pages or
add proof artifacts. Preserve the primary checkout's uncommitted plan changes; work
in the attached isolated checkout. A backup at `codex/pr500-before-main-970b28f6`
retains the original published history. Validate semantic preservation, source/structure/
links, harness consistency and exact application parity before a force-with-lease push.
Do not monitor CI.

Completed: 25 commits replayed onto `970b28f6`; three ontology conflicts were resolved
as above. Range comparison preserves every later patch and omits only the cancelled
runtime repair/revert pair. All 418 published ontology bodies are unchanged, with four
upstream blocks added under their existing IDs. Application code, tests, scripts,
dependencies and CI configuration exactly match main. Source preservation, 422-block
structure, 1,301 generated links, three local links in the imported design, six harness
tests, generated skill consistency and whitespace checks pass. The PR adds no TLA/TLC
artifacts. No application tests or CI monitoring were rerun for this ontology-only
reconciliation; upstream test evidence retains its recorded scope. The primary
checkout and its uncommitted plan remain untouched.

## Final language and structure pass — October 15, 2026

Review all 20 ontology pages for plain English, repeated commitments and consistent
terms. Keep the 418 stable IDs and governing meaning, including unresolved questions
and known implementation gaps. Record faithful edits to Ben-derived statements in
the existing source-edit register; leave `docs/ben_new_ontology.md` unchanged.
Shorten metadata to useful provenance and one or two sentences stating each model's
checked result and important limit. Use the archived evidence; restore no proof files
and run no new TLC campaign.

Rewrite `docs/code_gaps.md` as issue and architecture tables: concrete failure,
how it was found, an understandable repair using existing components, and what remains
unverified. Keep finding IDs and evidence baselines; do not present a model comparison
as an implemented repair or settle policy questions. Identity, runtime and workspace/Git
edits are divided among the three previously authorized reviewers; the coordinator
handles the other pages, shared records and integration. Finish this iteration with
source, structure, link and semantic-preservation checks, then commit and push.

Completed review: all 418 IDs and provenance labels remain in 20 pages; all ten
questions stay open. The source register records 54 faithful wording edits in this
pass. Independent reviews corrected ambiguous shortening of permissions, invitation
guards, backup/sharing wording, deletion/merge exceptions and model limits. The issue
report retains every finding ID and separates bugs, missing features and policy
choices. Source, structure, comparison coverage and link checks pass; no application,
harness or proof files changed. The concurrent remote fix and its revert have no
net content difference and will be retained when publishing this iteration.

## Remove proof artifacts from the PR — October 15, 2026

The human requested removal of all TLA+/TLC proof file changes and another push.
Remove the 537-file `ben-quantitative` campaign, three generated proof ledgers and
both proof-dependent measurement scripts. Remove per-case proof payloads from the
mixed coverage ledger while preserving all 63 behavior rows and semantic counts.
The original symbol inventory, ontology, Ben comparison and code-gap findings remain.

Evidence and reproduction tooling remain available in immutable commit `345b546e`;
retarget local proof links there. Preserve all 418 ontology bodies exactly, changing
only evidence-link destinations in metadata. Validate source preservation, structure,
remaining links and the absence of proof artifacts from the net PR diff. No TLC
rerun, application/harness change or history rewrite is part of this cleanup.

Completed: 542 proof/tooling files removed, plus per-case payloads from the retained
coverage ledger. All 418 ontology bodies and 63 behavior rows/counts are unchanged;
source, structure, 1,280 local-link and whitespace checks pass. Fifteen immutable
archive targets resolve, and the net PR diff contains no TLA+/TLC proof artifacts.

## Quantitative refinement — October 15, 2026

The human approved adopting the quantitative audit’s proposed commitments and fresh
TLA+/TLC checks. Continue from `312b0960`: eight new D blocks, two amendments and two
explicit policy Qs. Preserve Ben’s source, existing shared owners and harness.
No application repairs or new runtime tests are included. The archived campaign stays
archived; a compact new campaign lives in `TLA_verification/ben-quantitative`.

Parallel ownership: identity/chat handles keyed creation, admission and membership;
runtime handles command replay, event ingestion and execution recovery; workspace
handles review rounds, discard and conditional review approval. The coordinator
handles Home, explainer templates, shared evidence and final integration. Findings
refine the existing owners and gap ledger; provisional choices remain conditional.
Inspect source bindings, exercise legitimate completions and relevant broken controls,
then independently review the models and preserve exact execution receipts.

The 63-row quantitative inventory remains frozen as the before-measurement. Record
same-row intent dispositions and fresh property coverage separately, without claiming
repository-wide symbol or execution-path coverage. Commit and push once the complete
iteration passes its evidence/source/structure checks.

Completed: eight new D blocks, two amended D blocks and two provisional Qs; 418
blocks across 20 pages. The same inventory improves from 47 to 57 adequate groups
out of 63. Thirteen models execute 170 configurations with zero unexpected outcomes;
all final receipt fingerprints, source-preservation and artifact checks pass.
Independent review corrected source/transport abstraction and recovery-witness errors.
Three new source-bound gaps and scoped minimal repair candidates are recorded in
`docs/code_gaps.md`; no application repair is claimed. This completes the logical
iteration for one ordinary commit and push.

Publication reconciliation: the remote branch was independently rebased while this
iteration was in progress. Replayed only this iteration onto `afccd820`, preserving
its issue-551 provider target/live-tip checks, error reporting and governing
`g5.git.pr-recovery-cut` amendment. The only textual conflict joined the two plan
sections; no source/ontology conflict remained. The initial local result is retained
at `backup/ontology-quantitative-32281e5c`. All 170 saved-evidence checks match the
integrated source exactly; no modeled input changed. The ontology-hash summary is
regenerated against the integrated tree. Application, harness and Ben source remain
identical to the published rebase; the follow-up uses a normal push.

## Main advanced again — October 15, 2026

Main advanced from `3d17915d` to `eedcc8ae` in two issue-551 commits (`fc02e085`,
`eedcc8ae`). Their only ontology change extends `g5.governance.merge.target`
metadata in the old `features/governance.g5.md`, which the rewrite removed.
This is the new modify/delete conflict; it is not an unresolved first-rebase conflict.

Reconcile that evidence into existing `g5.git.pr-recovery-cut`: provider target branch
and live remote tip must match the pinned publication, creation-time base SHA does
not establish freshness, and accepted import retains pinned-target ancestry. Preserve
per-repository error reporting and the preflight/merge race limitation in metadata.
No duplicate block or old ontology page is needed. The existing comparison row is
updated; all other canonical bodies and the immutable Ben source remain unchanged.
Main's application fixes and regression tests are retained exactly.

The published branch now includes `312b0960`, the subsequent quantitative coverage
report; preserve it. Its measurements remain tied to their recorded source baseline,
not recomputed evidence for these upstream fixes. Work proceeds in an isolated checkout
because the primary checkout has unfinished ontology/refinement edits. Those edits
and untracked artifacts are not committed or deleted by this rebase. No CI monitoring
or TLC execution is authorized by this follow-up.

Completed: all 20 commits replayed onto `eedcc8ae`, with one modify/delete conflict
resolved through the prepared ontology reconciliation. Range comparison preserves
all 19 subsequent patches; only the initial rewrite's deletion context changes.
Application/configuration parity with main and exact preservation of the prepared
ontology/quantitative tree pass. Source, 408-block structure, 1,208 generated links,
comparison disclosure, three harness tests, generated-skill synchronization,
formatting and whitespace checks pass. Local skill mirrors were generated in this
new worktree before checking them; no tracked harness edits resulted. No application
or database test suite was rerun for this documentation-only resolution.

The human confirmed the primary checkout is still being edited. Leave its branch,
tracked edits and untracked files untouched; publish from the isolated worktree with
an explicit lease against `312b09609d76d539de502e5eb68f787fa37c96ef`. Local `main` is
fast-forwarded to `eedcc8ae`. The primary work branch intentionally retains its old
base until its unfinished edits can be reconciled with the published rebase.

## PR size cleanup — October 15, 2026

The human requested removal of unnecessary material and another push after the PR
grew to **+103,777 / −23,439**. The newer `ben-coverage` TLA campaign contributed
**92,137 added lines across 637 files** (about 89% of additions): models,
configurations, repeated receipts, logs, manifests, bindings, runner and READMEs.
Remove this campaign from the current branch. Its complete historical evidence
remains in [commit `072a12d3`](https://github.com/G5Labs/G5-Lean/tree/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage),
and retained findings/provenance link directly to that immutable snapshot.

Preserve all 408 ontology bodies, Ben's immutable source, source mapping/edit
registers, unresolved gaps, useful audit/rebase records and harness changes.
No application, configuration or dependency changes are part of this cleanup.
The historical sections below describe their original checkpoints; campaign
retention and reproduction statements refer to their archived commits. No current
executable TLA suite or new proof is claimed.

Validation: source/structure/comparison and whitespace checks pass. All 64 retargeted
evidence links resolve to retained Git objects and anchors; no local campaign link
or tracked TLA artifact remains. Every canonical body is unchanged from `072a12d3`.

The preceding rebase push used `--force-with-lease` explicitly pinned to
`refs/heads/ziniuw/ontology_update_based_on_ben:695df79c32205c7b46f3a800f8220169eb5c187a`.
This cleanup is an ordinary follow-up commit; normal push rejects concurrent
remote advancement. CI remains unmonitored until requested.

## Rebase audit — upstream main into the ontology rewrite (October 15, 2026)

**Status: rebase completed and locally validated.** The human asked to
inspect the updated ontology and execute the rebase. The completed coverage commit
`43f39cca` and subsequent compatible harness commit `695df79c` are retained. Main was
refreshed to `3d17915d`, adding native Claude steering after the original audit.
No CI monitoring or new TLC execution is part of this rebase. The original 57-entry
ledger below remains the historical audit; the execution amendment records later
owner reuse and the nine additional steering changes.

### Execution result

Replayed all 16 branch commits onto `3d17915df39bcf4cbe0cabc08500de63ff46fbb2`.
The first rewrite commit produced 18 ontology conflicts (16 modify/delete and two
content conflicts); no application conflict required resolution. The prepared
reconciliation is applied after the retained rewrite and coverage commits. The
canonical tree matches the prepared pre-rebase reconciliation exactly.

Recovery references are `codex/pr500-before-rebase-20261015` at `43f39cca` and
`codex/pr500-before-rebase-harness-20261015` at `695df79c`. The latter preserves the
concurrent harness follow-up observed before replay. Range comparison accounts for
all 16 commits: only the initial rewrite's conflict resolution and the cleanup
commit's already-upstream rustls hunk differ; the other 14 patches are equivalent.

Validation and limits:

- All 285 source identities and immutable Ben text validate; 53 editorial/reference
  transformations and four separately disclosed human-authorized amendments validate.
- All 408 blocks, source-comparison disclosure and 1,241 generated links validate.
  Repaired 84 incoming documentation links (59 current owners, 25 immutable historical
  targets), checking their anchors and retaining fenced historical diffs verbatim.
  Eight already-stale code links in `docs/external-issue-integration-design.md` predate
  the rebase and remain outside this ontology-path repair.
- Application, infrastructure configuration, scripts, tests and Cargo.lock equal
  main. The only documentation change in those directories retargets ontology links
  in `infra/qa/README.md`. Both branch harness guidance and main's `g5d issue` guidance
  remain; skill synchronization and all three synchronization tests pass.
- `cargo fmt --all --check`, `cargo deny --locked check` (all four categories) and
  `git diff --check` pass. Cargo deny retains non-failing duplicate/source warnings.
  No application suite or runtime fault-injection test was rerun for this documentation
  reconciliation; equal application trees are not a fresh runtime-conformance claim.
- The old `ben-ontology` campaign remains absent. The separate `ben-coverage` models,
  bindings, configurations and results are byte-for-byte preserved; only its README
  gains the rebase evidence boundary. Its 199 saved receipts matched original inputs
  before rebase, without running TLC. No new formal validity is claimed after rebase.
- Current source narrows RUNTIME-005 to configurable host-count min/max; RUNTIME-006,
  RUNTIME-007, conditional RUNTIME-008 and the four refinement findings remain. The
  gap and coverage ledgers distinguish current source checks from historical models.

Publication uses an explicit lease against observed remote head `695df79c`; concurrent
remote advancement must be reconciled rather than overwritten. CI remains unmonitored
until the human says it has finished.

### Execution amendment — updated branch and main

Compared all changed ontology bodies and relevant metadata in `832651d0..43f39cca`: 35
new D blocks, two Qs and comment/viewer refinements remain. The latest harness-only
commit `695df79c` is compatible and preserved. Reuse the new credential-class/current-
authority, quota capacity, materialization and lifecycle owners. The source plan's
org-settings consolidation becomes two small contracts because authority and typed
storage can change independently; stories reference those owners. Idle-default policy
lives beside the newly recovered VM idle lifecycle. No coverage commitment is dropped.

Main `0134daf4..3d17915d` adds two blocks and changes seven bodies in conversations:

| Upstream ID | Incorporated owner and decision |
| --- | --- |
| `g5.conversation.inbox.reservation` | Amend `g5.chat.frozen-attempt` to freeze initial input/configuration while steering binds additional immutable revisions. |
| `g5.conversation.inbox.accepted` | Preserve atomic initial acceptance; steering owns flush projection and stable in-progress unit segmentation. |
| `g5.conversation.delivery.unknown` | Amend `g5.chat.uncertain-delivery` to distinguish initial input from the human-authorized steering queue fallback. |
| `g5.conversation.delivery.steering` | Add the native-steering owner in chat: current-authorized ordinary text, FIFO, frozen capability, exact revisions, complete stdin flush, no restart/consumption handshake, durable receipt at output exhaustion. |
| `g5.conversation.delivery.steering-recovery` | Add the explicit accepted-loss/rare-duplicate policy, one bounded RPC and ordinary queue fallback; retain initial-input recovery. |
| `g5.conversation.providers.claude-background` | Amend `g5.chat.background-work` using the worker's shared input/close decision and pending stdin write, without reinstating a consumption hold. |
| `g5.conversation.providers.claude-followups` | Add the same-attempt/segmented-transcript rule; all started native turns still need successful results. |
| `g5.conversation.providers.claude-background.input` | Record background-wait exclusion in steering and follow-up ownership; do not inject into waiting background attempts. |
| `g5.conversation.providers.later` | Preserve Codex/App Server and cross-attempt resident-process exclusions; steering only uses the already-running Claude attempt. |

The later owner's explicit request in issue-544 resolves the conflict with Ben's
blanket queue/uncertain-delivery wording. Four source statements receive disclosed
human-authorized amendments, with source-line lineage, authority and reasons in
`source-edits.json`; the source document is unchanged. This is not counted as four
editorial transformations. Confirmed launch remains an agreement for first-attempt
admission; steering cannot replace that attempt's provider, permissions or limits.

The canonical reconciliation adds 28 focused D blocks and refines existing owners;
source comparison lists every addition once. Existing Qs remain open. New credentials,
placement and materialization owners replace the original proposal's duplicate-owner
possibilities. Automatic refresh supplements the human PR-refresh block rather than
rewriting it. Main's application, infrastructure configuration and lockfile are retained exactly.

Stored evidence was verified before rebase: all 199 saved coverage receipts match
their original model/source inputs. No TLC ran. After rebase their source hashes may
differ; retain the original campaign and label it historical instead of rewriting
hashes or claiming a new proof. Update code-gap status only from inspected source,
keeping unverified races and broader gaps visible.

### Comparison and completeness

- Rewritten branch: `ziniuw/ontology_update_based_on_ben` at
  `832651d0bb3b1754eb9511d1cc5ccde7c2aafde4`, including the coverage audit.
- Fetched main, also fast-forwarded locally:
  `0134daf41623a0c8858eb30500ff7fce08330de9`.
- Common ancestor: `20e393c02906459e442e23e3e7ab867aced8945d`.
- Scope: every upstream ontology change since that ancestor, compared with the
  rewritten branch's committed bodies and metadata. This is not a fresh uplift
  of every unchanged pre-rewrite commitment.
- Net inventory: 18 files, **22 added blocks, 27 body edits, eight metadata-only
  edits**, no removed blocks, plus three navigation links. Some body edits only
  retarget links; the table identifies those rather than inventing new meaning.
- Historical cross-check: all 70 non-merge commits touching ontology in the range
  touch 61 distinct blocks. The four outside the 57-block net inventory return
  to their ancestor state; their dispositions are recorded below. Intermediate
  changes within the other 57 are reconciled to their final, unreverted intent.
- A final-tree `git merge-tree --write-tree --messages 832651d0 0134daf4` preview
  reports 18 conflicted paths: 15 modify/delete and three content conflicts
  (`concepts/data_model`, `concepts/security`, `features/documents`). All are
  ontology files. Application code, Cargo.lock and the harness merge in that
  preview. This predicts the combined tree, not conflict-free commit replay.

The safe result keeps the rewritten ownership/layout, ports the upstream semantic
deltas and their necessary conditions, and retains main's application changes.
Resolving all ontology conflicts with either branch's files would lose intent.
Minimality means one governing owner per rule, not deleting failure cases or
copying an old story, feature block and architecture block that say the same thing.

### Decisions that guide reconciliation

1. **Permissions are composed, not replaced.** Preserve Ben's organization role
   ceiling. Main's account-only session restrictions, refresh-cookie protection,
   transaction-time SCM authorization and admin-settings gates strengthen existing
   owners. “Any organization member” creating an issue means a member eligible for
   `workspaces:write`, consistent with the incoming route and retained request;
   it is not a new viewer-write exception. The existing viewer-collaboration Q stays.
2. **Close the legacy preview exception.** Amend the agent-authored
   `g5.workspace.preview-origin-security`: remove its permission for sandboxed
   platform-origin previews. Upstream's explicit removal decision governs. Missing
   suffix disables preview with a clear error while the API remains available.
   Keep credential stripping, per-preview isolation and WebSocket origin checks.
   Separate registrable domains remain deferred; subdomains alone do not promise them.
3. **Normal completion and cancellation have different cleanup.** Preserve
   `g5.chat.cancellation` unchanged. Add the upstream explicit normal-completion
   release rule alongside module execution: a provider ending its own turn may
   leave agent-managed tools alive and release capacity; Stop, forced End, limits,
   failure or supervisor interruption require reaping and veto that release.
   Do not rewrite “cancellation retains capacity until exit proof” to require all
   normal turns to kill databases, or to let cancelled turns release early.
4. **Capacity policy is not a substitute for min/max host limits.** Main supplies
   bounded demand-driven growth and empty-host retirement, both enabled by default,
   with configurable VM quota and machine type. Ben's `g5.hcp.autoscale` and
   `g5.organization.host-limits` still require separately configured host-count
   limits. Preserve those commitments and narrow RUNTIME-005 from “no demand-growth
   owner found” to the remaining min/max gap after source verification. Do not
   reinterpret the VM quota as a host-count setting or claim the whole gap closed.
   Qualify `g5.runtime.host.rotation`'s replacement prerequisite as **rotation**:
   the new, separately guarded empty-host retirement needs no replacement.
5. **Creation recovery is explicitly stateless.** Issue creation's marker search
   and accepted duplicate risk do not inherit the durable publication ledger used
   for comments or the separate platform-bug delivery protocol. The owner rejected
   a creation ledger. Keep parent-attachment failure, fork failure and link failure
   distinct from successful issue creation, retaining the created issue.
6. **Upstream decisions can supersede intermediate upstream designs.** All newly
   created workspaces, including forks of non-Base workspaces, receive the current
   organization idle default; existing workspaces retain their value. Default VM
   quota is 10, not the reverted 20. Preserve the owner's final output-bound scope,
   not extra safeguards explicitly removed after review. IAM/indexer restrictions
   reverted by #557 must not be presented as implemented or silently reinstated.
7. **Preserve source and uncertainty.** Keep Ben's immutable source and all 285
   mapped statements. The selected changes can extend existing agent D blocks or
   add scoped children without rewriting those source bodies. Record new human
   authority separately from agent design; retain upstream evidence limitations,
   not stale percentages. If execution discovers a necessary semantic amendment
   to a Ben statement, record the actual new authority and amend the source
   comparison honestly; do not label it an editorial transform to pass validation.
   The process-local versus microVM Q remains unresolved; implemented development
   mode is not evidence that Ben authorized an unisolated production fallback.

### Complete upstream disposition ledger

`Add` means a genuinely missing obligation in an existing rewritten page; reuse the
old stable ID when the same obligation moves. `Fold` means amend or annotate an
existing owner without retaining a duplicate block. `Reference` means link/evidence
maintenance only. Proposed IDs remain proposals until implemented. Source files
below are pinned to the fetched main, so this ledger remains readable after deletion
of their old paths. Destination links point to the current rewritten pages.

#### Organization settings, identity and security

Sources: [data model](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/concepts/data_model.g5.md),
[security](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/concepts/security.g5.md),
[settings](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/features/settings_and_usage.g5.md).
Owners: [organizations](../../ontology/features/organizations.g5.md),
[users](../../ontology/features/users.g5.md), [Git](../../ontology/software_architecture/git.g5.md),
[workspace VM](../../ontology/software_architecture/workspace_vm.g5.md).

| # / upstream block | Delta | Minimal disposition and retained meaning |
| --- | --- | --- |
| 01 `g5.data.configuration.org-admin-settings` | Add | Fold with 07/30 into one organization-admin-settings D. One typed, database-validated configuration owner; absent keys select defaults. Retain the six registered settings in evidence, schema-only migration 083 and no rewriting/seeding existing values. Do not restore the large old data-model/configuration hierarchy. |
| 04 `g5.security.login.delegated-keys` | Body | Extend `g5.user.api-key-authority`: org keys, even with all scopes, cannot exercise person-account authority. Session-only includes profile, organization list/create, invitation accept/refuse, SCM connection/discovery, CLI device approval and WebSocket tickets. A generic “key is org-scoped” is insufficient for these account routes. |
| 05 `g5.security.login.refresh-exchange` | Add | Add a scoped child of session authority. Cookie refresh admits platform/configured CORS origins, refuses cross-site Fetch Metadata and sibling-preview origins; headerless non-browser clients remain allowed. Cookie exchange returns the access token in JSON and refresh credential only as a cookie. Explicit body-token exchange retains its credential response and is not subjected to browser-cookie origin admission. |
| 06 `g5.security.mvp-exposure.preview` | Body | Fold into preview-origin security as decision 2: isolated hosts only; legacy paths redirect/refuse, not relay. API startup remains available without preview configuration. Keep the accepted same-registrable-domain limitation visible. |
| 07 `g5.security.org-admin-settings` | Add | Fold into the organization settings D with 01/30: both read and write require `org:admin` plus current owner/admin role for the target org. This does not change a workspace owner's authority over workspace settings. |
| 08 `g5.security.tenant.scm-import` | Add | Add an SCM-import authority D in Git. Person session plus `workspaces:write` in the credential and current target-org authority, checked in the write transaction. Catalog listing needs read authority rechecked after provider I/O. A scoped database label is not an authorization grant; eligible ordinary members remain supported. |
| 09 `g5.security.tenant.write` | Metadata | Reference the SCM-import owner from tenancy evidence. Preserve the limited audit of alternate writers; do not copy an old completion percentage or add a second generic tenant-write contract. |
| 30 `g5.settings.admin-page` | Add | Fold with 01/07: the whole Admin page and its entry are owner/admin-only, and linked destinations enforce their own gates. Preserve UI visibility separately from server admission. |
| 31 `g5.story.set-default-idle-timeout` | Add | Fold into the single idle-default contract in 47 and link it from org settings. Keep the admin's visible edit/save outcome; no separate story block duplicating the default and new-workspace rules. |
| 32 `g5.story.set-managed-capacity` | Add | Fold into org settings and link the two HCP policies in 55/56. Admin controls growth, allowlisted machine type, automatic removal and delay. Do not duplicate operational algorithms in the UI story. |
| 33 `g5.story.set-org-vm-quota` | Add | Fold into org settings plus the quota owner in 53. Keep value/range feedback and that lowering the quota does not stop existing VMs. |

#### Agent execution and bounded reads

Sources: [agents](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/features/agents.g5.md),
[conversations](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/features/conversations.g5.md),
[agent architecture](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/software_architecture/agents.g5.md),
[documents](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/features/documents.g5.md).
Owners: [chat](../../ontology/features/chat.g5.md), [workspace VM](../../ontology/software_architecture/workspace_vm.g5.md),
[documents](../../ontology/features/documents.g5.md).

| # / upstream block | Delta | Minimal disposition and retained meaning |
| --- | --- | --- |
| 10 `g5.agents.execution-deadlines` | Body/link | Reference the relocated output-bound owner; no new deadline rule. Preserve the existing distinction between bounded setup/harvest or requested limits and no default post-input execution deadline. |
| 11 `g5.agents.execution-limits` | Body | Fold into 13: the catalog module-output ceiling is an outer backstop on the same recorded bytes, not an unrelated raw-provider-byte budget. Keep request/policy/catalog composition in execution evidence. |
| 12 `g5.agents.outcome` | Metadata | Fold evidence into module execution/43: classify before harvest, clean up non-released turns before harvest, and let later Stop/deadline veto release. Preserve the explicitly unproven cached process-group-ID reuse window and unexecuted full outcome matrix as limits; they are not newly solved by the rewrite. |
| 13 `g5.agents.output-bounds` | Body | Add one output-recording D in chat. Bound each provider line before parsing; an overlong line is discarded through newline with a notice, not automatic turn failure. Mask before truncation, retain full-content digest/truncation indication, bound serialized recorded events, charge full lines that record nothing and reserve terminal settlement output. Overflow ends the attempt and retains its recorded output. Stderr has its separate existing log cap. Retain the final 16-MiB line ceiling in source evidence, not the abandoned 1-MiB proposal. |
| 14 `g5.conversation.providers.claude-background.untracked` | Body | Fold into 43. Untracked descendants do not prolong a completed attempt; normal completion leaves their lifecycle with the agent/VM, while stop, limit and failure cleanup remains mandatory. No independent Claude-only cleanup authority. |
| 15 `g5.documents.read.bounds` | Body | Add bounded-read D beside captured references. One listing allowance across all batch roots; dirty overlays only below requested roots, bounded from headers. Retain necessary surrounding guarantees: header-first 32-MiB file refusal (or narrower caller cap), partial Files-tree results marked `truncated`, and whole-operation `TREE_TOO_LARGE` for service enumerations requiring completeness. Do not copy the old document feature catalog. |
| 43 `g5.architecture.agent_execution_tree` | Body | Add this execution-tree D to workspace VM and link cancellation to it. Owner retains dispatcher privilege while the supervisor uses module identity so cleanup reaches changed-UID descendants. Only the explicit clean-provider/clean-question-deferral release path frees capacity with tools alive; forced supervisor End/Stop, failed or limited runs reap. The VM adopts released tools; lifecycle containment is not a hostile-code sandbox. |
| 44 `g5.architecture.agent_lifecycle` | Body/link | Reference 13 from module-execution evidence; only an output-bound anchor changes. Do not recreate the old lifecycle narrative. |

#### External issue creation and its user flows

Sources: [external issues](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/features/external_issues.g5.md),
[workbench](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/features/workbench.g5.md),
[scope](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/scope.g5.md).
Owners: [issues](../../ontology/features/issues.g5.md), [UI](../../ontology/features/ui.g5.md),
[question decisions](../../ontology/features/documents.g5.md#g5.documents.question-decisions).

| # / upstream block | Delta | Minimal disposition and retained meaning |
| --- | --- | --- |
| 16 `g5.external-issues.broker` | Body | Fold into creation admission/19 and existing comment authority: reuse one guarded external-publication boundary for comments and creation. Status mutations remain deferred. Do not restore a broad integration-broker architecture promise. |
| 17 `g5.external-issues.cli` | Add | Add concise agent-surface D linked to the same creation/link owners: noninteractive `g5d issue create` with gh-style title, body/body-file, assignee including `@me`, optional parent and URL/JSON result. Creation does not automatically link; `link` defaults related with explicit primary, `view` aliases show. CLI Create & Fork and close/reopen remain excluded. |
| 18 `g5.external-issues.creation` | Add | Add the creation contract: eligible org members create under their own provider account; agents under the workspace owner's. GitHub only, organization's default target, title/Markdown body/optional assignees and parent. Start unassigned; no templates/labels/default assignee. Jira remains future. |
| 19 `g5.external-issues.creation.admission` | Add | Add admission child reusing current comment guards: external issues and G5-comments enabled, currently private/internal destination, connected author's account and secret screen. Untrusted domains require review of exact rendered payload including marker, assignees and destination; changed target requires review again. Retain the source's check-to-provider-send race limit, not a claim of atomic external-policy revocation. |
| 20 `g5.external-issues.creation.parent` | Add | Add parent-settlement child: parent must be an issue in the creation target, validated before creation. Create first, then attach. Failure preserves the created issue and reports partial success. Retry finds the existing issue and reads its parent; attach only if absent, never move it from a different parent. Check-only does no writes; no parent ledger and no conflation with workspace links. |
| 21 `g5.external-issues.creation.recovery` | Add | Add stateless recovery child. Same submission key/org/author yields the same hidden marker; search the author's recent target issues before send and on read-only checks. Retain bounded 24-hour/1000-result lookup and eventual-listing duplicate risk in evidence. Timeout/network/5xx is unknown; a retry uses the same key and checks first. No exactly-once claim or creation ledger. |
| 22 `g5.external-issues.publication.automation` | Body | Fold into creation and existing comment identity. Agents may create/comment directly as the owner; comments retain agent-origin attribution, creations show the owner's provider identity only. Do not restore superseded drafts-only approval or invent a bot identity. |
| 34 `g5.story.create-issue-from-home` | Add | Fold into one issue-creation entry-points D with 38; preserve the actor's result and partial outcomes, not a duplicate story. |
| 35 `g5.story.track-workspace-issues` | Body | Fold into the same entry-points D: quick-add creates and links related work, with parent behavior and owner gate from 37. Existing primary/related invariants continue. |
| 36 `g5.workbench.deferred` | Body | Record lifted creation/quick-add deferral and retained Done/Reopen deferral in creation scope. Do not recreate the entire historical mock/deferred-feature list. |
| 37 `g5.workbench.issues` | Body | Entry-points D: owner-only quick-add, unavailable on review copies; preselect the primary issue as parent only if it is in the creation repository, allow change/clear, then link the created issue as related. If linking fails, retain the issue and offer existing import/link recovery. |
| 38 `g5.workbench.issues.create` | Add | Entry-points D: Home alignment/workbench Create, empty form regardless of active filters, visible disabled reason and displayed account/target. Create opens the issue page. Create & Fork reuses Start workspace with issue preselected primary; fork failure leaves the issue. Do not add creation to the ordinary fork dialog. Preserve the upstream interpretation to reuse the existing Base-based dialog, rather than invent a source picker. |
| 40 `g5.workbench.meeting` | Body | Add a narrow cross-reference to question decisions: future delayed/out-of-scope decision cards reuse creation with prefilled content and link the result through the same question's Answer/comment. No new decision store; cards remain unimplemented in inspected main. |
| 41 `g5.scope.internal-issues` | Metadata | Fold a scope note into issue CLI evidence: the new commands/routes concern external issues. No revival of the removed internal issue tracker or old scope page. |

#### PR publication, previews and delivery

Sources: [governance](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/features/governance.g5.md),
[preview](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/features/preview_terminal.g5.md),
[deployment](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/concepts/deployment.g5.md).
Owners: [Git](../../ontology/software_architecture/git.g5.md), [UI](../../ontology/features/ui.g5.md),
[workspace VM](../../ontology/software_architecture/workspace_vm.g5.md),
[hosts](../../ontology/software_architecture/hosts.g5.md), [operations](../../ontology/features/operations.g5.md).

| # / upstream block | Delta | Minimal disposition and retained meaning |
| --- | --- | --- |
| 02 `g5.quality.delivery.failure-alerts` | Add | Add one operations D: every QA non-PR Cloud Build trigger reports failure/internal error/timeout/expiry to deployment-notifications through an external status-event notifier; successes/cancellations stay quiet and new non-PR triggers enter coverage by default. Webhook stays operator-owned/out of Terraform state. Retain the explicit QA reuse-of-dev-hook exception and live-delivery qualification gap in metadata. |
| 03 `g5.scope.deployment.images.g5dev-toolchain` | Metadata | Attach source evidence to host compile-cache/image/rootfs owners: uid-1000 baked registry/sccache writability relies on faithful layer modes, not just Dockerfile chmod. Preserve cache warmth and ability to fetch newer-lock dependencies. The image's build gate is not proof that a deployed host has the fixed assembler. |
| 23 `g5.governance.pr.follow-push` | Body | Extend `g5.git.pr-refresh`: automatic settling retries do not become Activity failures prematurely; terminal/exhausted and manual attempts report actionable step/reason with internal details in logs. Preserve bounded retries/restart and the pinned accepted publication cut; no silent reset to newer heads. |
| 24 `g5.preview.service.close` | Metadata | Retarget profile evidence from `dev-up.yaml` to `full-stack.yaml` while keeping Close/cleanup ownership. No new cleanup rule or duplicate block. |
| 25 `g5.preview.service.exit-diagnostics` | Add | Add preview-service child: own application exit status/signal plus bounded last 4 KiB stderr persists into the outcome/guest diagnostic, masked under existing log limits. Cleanup failure after clean application exit is a distinct message. Forward customer stdout/stderr outside module framing; descendants cannot forge protocol outcomes or keep its channel open. |
| 26 `g5.preview.transport` | Body/link | Retarget the governing preview-origin link. Fold any missing prerequisite into preview admission: every stream stays bound to task/incarnation and closes on Stop or transfer. Substantive fallback removal belongs to 06; do not duplicate transport promises. |
| 27 `g5.preview.transport.credentials` | Body/link | Preserve credential stripping in existing preview-origin security, retarget its link; the credential rule itself did not change. |
| 28 `g5.preview.transport.writes` | Body/link | Retarget the origin link and retain the actual authority boundary. Profiles promise no tracked writes, but MVP has no read-only mount; writes remain visible and flushable. Preserve that distinction in preview-service evidence rather than invent filesystem enforcement. |
| 29 `g5.story.preview-my-application` | Body | Fold the incremental visible behavior into a concise preview-profile UI child: show the running profile ID, with description only when loaded. Retain full-stack, full-stack-no-worker, frontend-only and workspace-default distinctions in source/evidence, not another full preview story. |
| 39 `g5.workbench.landing` | Body | Add one publication-activity UI D linked to PR refresh: collapse unresolved automatic publication failures into newest plus count measured since the last **completed publication**; completed publication or PR end resolves them. A private/member-repository push alone is not that boundary. |
| 42 `g5.scope.security-hardening.preview-origin` | Metadata | Fold into preview-origin evidence: fallback removal is now delivered; separate registrable domain/DNS/certificate hardening is not thereby complete. No revival of a separate historical scope owner. |

#### VM lifecycle, host capacity and image assembly

Sources: [boundaries](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/software_architecture/boundaries.g5.md),
[lifecycle](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/software_architecture/lifecycle.g5.md),
[runtime](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/software_architecture/runtime.g5.md),
[worker host](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/software_architecture/worker_host.g5.md).
Owners: [workspaces](../../ontology/features/workspaces.g5.md),
[hosts](../../ontology/software_architecture/hosts.g5.md),
[HCP](../../ontology/software_architecture/host_control_plane.g5.md).

| # / upstream block | Delta | Minimal disposition and retained meaning |
| --- | --- | --- |
| 45 `g5.architecture.commands.writers` | Metadata | Fold owner/evidence into HCP capacity: supply records and VM wait diagnostics belong to capacity maintenance; Start consent and VM transitions remain lifecycle-owned. Preserve the writer guard and its SQL-attribution limits, not stale “43 writers” counts or a duplicate ownership page. |
| 46 `g5.lifecycle.deadline.rotation` | Body | Amend `g5.runtime.host.rotation` to distinguish daily replacement from independent idle retirement. Replacement rechecks whether retirement already won; neither adopts the other's drain. |
| 47 `g5.lifecycle.idle.settings` | Body | Add one workspace idle-settings D referenced by org settings. Every newly created workspace, including a fork of any source, gets the current org idle default (initially eight hours); owner may override later. Org-default edits do not rewrite existing workspaces. Preserve other fork settings; do not restore the superseded Base-only/default-by-copy rule. |
| 48 `g5.runtime.boot.backend.process-local` | Body | Add a narrowly scoped development-preview child linked to `g5.runtime.local-mode-scope`, without resolving that Q; keep the provisional development-only/no-isolation-assurance boundary. Full-stack builds host first using available cores, starts stack without/removes old edge, proves ready host registration, then publishes edge from the same phase without recreating API. Preserve profile distinctions and unsupported-reflink-only copy fallback; production microVM failure still must not fall back to processes. Do not paste the old enormous backend narrative into the new tree. |
| 49 `g5.runtime.boot.image.rootfs` | Add | Add host-image assembly D: apply recorded layer content and explicit directory metadata in order, including chmod-only later layers; implied parent directories do not overwrite lower modes. Retain no ownership preservation and non-root setid stripping. Invalidate templates when semantics change. Keep reserved cache-name and non-root/read-only-layer limitations in evidence rather than claiming unrestricted OCI fidelity. |
| 50 `g5.runtime.lease.binding` | Add | Fold into `g5.runtime.lease.renewal`: both private host-control and control-plane relay paths bind renewal to the trusted connection's workspace/VM/epoch; refuse a different tuple with `STALE_INCARNATION` before bridge/token/sequence effects. This closes guest-selected sibling identity, not compromise of the host credential. |
| 51 `g5.runtime.materialization` | Metadata | Fold into the local-mode Q's evidence and host checkout evidence: copy only when process-mode reflink reports unsupported; fresh roots remain required and other hosts retain reflink-or-fail. No blanket fallback on arbitrary filesystem errors. |
| 52 `g5.runtime.placement` | Body | Add one host-placement D: eligible ready hosts pack running VMs first, then greater starting/running/stopping occupancy, then age/ID. Respect org/region, free slots, per-workspace refused hosts, 30-second transient cooldown and final host admission. Do not restore the old least-occupied policy. |
| 53 `g5.runtime.placement.org-quota` | Add | Add one quota D in HCP, referenced by settings and placement: default 10, admin range 0–100, count starting/running/stopping under the current org quota lock; lowering it stops no VM and queued work waits. Existing values are not normalized by migration 083. |
| 54 `g5.runtime.placement.scope` | Body | Fold into 55/56 and the min/max-gap note: remove the old blanket no-autoscaling exclusion. Allow only bounded demand growth and guarded empty retirement; no proactive spare pool or broader scheduler promise is newly authorized. |
| 55 `g5.architecture.host_demand_capacity` | Add | Add HCP demand-capacity D, linked to existing cloud recovery: enabled by default, default machine `c3-standard-4`, admin changes restricted to the shared Add Host allowlist. Valid unexpired owner/generation-bound queued Starts below quota drive FIFO headroom across regions; subtract usable ready and healthy in-flight supply, respecting refusals/replacements. Growth-off blocks new claims, not existing claims. Reuse Add Host validation/image/credentials/sizing operation; maintenance authority never manufactures user Start consent. Persist deterministic claims before cloud work; bounded retries retain exhaustion, cancel/expiry cannot create new authority, and unused never-attempted claims release their Start reservation. Confirm stopped-object deletion/name absence before retry. Count uncertain inserts/deletion failures toward automatic resource bound `VM quota + 1`, with one separately bounded replacement overlap per retained host. Existing replacement obligations need no new demand. Surface distinct wait reasons. |
| 56 `g5.architecture.host_idle_retirement` | Add | Add HCP idle-retirement D: enabled by default, continuously empty longer than org delay (default one hour, range 15 minutes–24 hours), healthy and another healthy same-org/region host with free slots. Active reservations/reported inventory and any starting/running/stopping VM block it; demand wins. Unknown custody or heartbeat gaps over 30 seconds reset continuity; only fresh unchanged empty Hello preserves it. Serialize with placement, exclude drains/replacement pairs, retire without replacement, retain audit/cleanup retry and fairness. Never auto-delete self-hosted hosts. |
| 57 `g5.architecture.placement` | Body | Fold ownership into 55/56 and existing HCP ownership. WorkerHostService owns supply and capacity-wait diagnostics; workspace lifecycle owns Start consent and VM transitions; task outcomes stay elsewhere. The expanded operation-table names are implementation evidence, not a new duplicate API contract. |

The eighteenth file, [core user stories](https://github.com/G5Labs/G5-Lean/blob/0134daf41623a0c8858eb30500ff7fce08330de9/.g5/ontology/concepts/core_user_stories.g5.md),
adds only three links to the org idle-default, VM-quota and managed-capacity stories.
Do not restore that deleted navigation page: use the existing organization/settings
links to the single owners above. Outside-block worker-host operation-table additions
(`prepare_host`, `reconcile_capacity`) are accounted for in 57; section renames and
relationship links follow the same owner mapping.

### Reverted and superseded history

| Historical block/change | Final disposition |
| --- | --- |
| `g5.security.cloud.api-impersonation` | Added/amended by `472b8636`/`c963ca11`, removed by `387a9f56` and main #557. Do not add as a new governing amendment or claim the IAM gap fixed; preserve the rewrite's existing credential-isolation intent and gap evidence. |
| `g5.security.cloud.indexer` | Added by `472b8636`, reverted by `387a9f56`. Do not restore the reverted indexer identity/Helm implementation via ontology resolution. |
| `g5.security.org-workspace-defaults` | Temporary owner from `a982b87c`, replaced in `d42b8c05` by the final typed org-admin-settings design. Fold under 01/07/47; no duplicate defaults-security block. |
| `g5.runtime.bundle` | Intermediate metadata-heading displacement in `6a03c867` repaired by `4f49d846`; no net change. Retain the rewrite's runtime-bundle owner unchanged. |
| Default quota twenty (`926d758a`, `bc776c17`) | Reverted by `999eacd0`/`01d6344f`. Keep ten plus explicit admin configuration; no old-data backfill to twenty. |
| Output-budget experiments before `8fcb81c4` | Owner cut scope back: do not restore pending-native-input charging, oversized-ID guards, digest-only pending-reply storage or the extra stderr scheme. Port the final 13 contract; preserve existing unrelated protections. |
| Base-only idle timeout / initial one-MiB line proposal / issue creation without parents or drafts-only agents | Superseded within the retained blocks. Final decisions in 47, 13 and 18–22 govern; neither replay nor evidence refresh should resurrect earlier restrictions. |

### Source assessment and remaining limits

Primary retained requests inspected: org Admin page (including managed-capacity
follow-up), idle-host recycle and corrections, normal-turn leftover release, issue
#478's 34 answers and later parent decision, legacy-preview removal, bake permission
and QA Slack alerts. The #503 plan records the owner's scope cutback. These arrive
from main with their implementation designs; retain them as provenance rather than
copying their old ontology layouts. Incoming authentication/lease changes also have
their scoped code, tests and metadata evidence. Human requests, approved decisions,
agent interpretation and code observations must keep their separate attribution.

Targeted main-source checks corroborate the non-obvious reuse boundaries:
`org_admin_settings.rs` owns six typed keys and the shared admission lock;
`host_capacity.rs` counts undeleted uncertain automatic resources and treats
replacement overlap separately; `issue_creations.rs` and the create routes implement
marker recovery without a creation table and read-before-parent-attach;
`execution_owner.rs` distinguishes the release receipt from forced cleanup;
host `ctl.rs` and `tunnel.rs` enforce both renewal entry paths. The corresponding
tests and recorded upstream evidence were inspected, not executed by this audit.
This is not a fresh whole-application security or concurrency verification.

The rewrite's coverage audit was against older application code. Before declaring
reconciliation complete, re-evaluate its 23 findings and six candidates against the
new tree, retaining historical conclusions with their original baseline. In
particular, narrow RUNTIME-005 as above; do not close RUNTIME-006 from the reverted
IAM work; recheck RUNTIME-007/008 against actual new cloud/retirement writers and
locks instead of assuming that new capacity code solved the old races. Coordinate
with the in-progress coverage refinement: use its final owner if it already covers
a proposed addition, and amend the same owner rather than adding another.

### Execution sequence when implementation is requested

1. Re-read branch/main heads and working status. Finish or explicitly preserve the
   concurrent coverage work; do not stash, discard or commit another task's partial
   changes. Refresh main and audit any newly arrived deltas before applying this
   ledger. Save a recoverable reference to the exact pre-rebase branch and record
   the remote branch's expected head for a later lease-protected push.
2. Reconcile the affected canonical ontology using the ledger, updating bodies,
   metadata, source comparison and this plan together. Resolve semantic ownership
   before dependent conflict choices. Keep unchanged Ben bodies and existing Qs.
   No old ontology file is retained merely to make a modify/delete conflict vanish.
3. Replay the branch onto the pinned main, carrying the reconciled canonical tree
   through conflicts. During rebase, Git's ours/theirs terminology is reversed from
   an ordinary merge intuition; choose by intended content, not a global strategy
   flag. Preserve every branch uplift/harness/coverage change and every compatible
   upstream product change. Review each replayed commit for later patches that
   could overwrite an already reconciled block.
4. Keep main's Cargo.lock resolution: it already includes rustls 0.23.46. The cleanup
   commit still matters for artifact removal even if its dependency hunk becomes
   empty. Do not skip that entire commit just because main independently fixed the
   advisory. Preserve main's other lock resolution, including its tempfile edge.
   The final application/infra/dependency tree should equal main because this
   branch's only application change was that same rustls repair.
5. Preserve both harness intents: this branch's natural sections/selective metadata
   guidance and main's `g5d issue` agent instructions in `.agents/AGENTS.md`. Run the
   repository's harness sync and checks; verify generated copies follow their source.
   Keep the removed historical `ben-ontology` TLA campaign absent. Do not resurrect
   it from intermediate replay commits or alter the separate concurrent campaign
   under the authority of this audit.
6. Repair incoming **current** references to removed ontology paths/heading anchors
   using the owner mapping. Upstream designs and code comments also refer to old
   IDs: preserve meaningful IDs when moved and retarget live links. Historical
   baseline discussions may use immutable main-commit permalinks, clearly marked
   historical. Do not manufacture empty alias blocks or restore old files just to
   satisfy links. Expand the link audit to changed incoming design/request/docs
   files; the existing validator only scans its declared subset.
7. Update the delta ledger for every real added/amended commitment and preserve
   provenance/known limits. Reassess coverage/gap evidence against new code; old
   source line numbers and model results are historical, not rebased test receipts.
   Review the final main-relative diff for unintended code, old ontology or artifact
   resurrection. Use a range comparison to account for the replayed branch commits.
8. Validate and publish only after integration is complete. If rebasing the already
   published branch is then requested, use a push lease bound to the observed remote
   head, and stop/reconcile if another writer advanced it. Do not monitor CI: the
   human explicitly asked to check only after they say it has finished.

### Verification for the reconciled tree

First verify all 57 ledger entries and four net-zero blocks have a recorded outcome,
stable IDs are unique, body/footer order and local links agree, all 285 source
statements remain accounted for, and every new commitment appears once in the Ben
comparison. Run `validate_source.py`, `validate_artifacts.py`, harness synchronization
checks and `git diff --check`. Verify historical campaign absence and no conflict
markers. These document checks are necessary but do not prove behavior.

If the final production/configuration tree matches main exactly, do not invent
application repairs for this rebase. Retain and review main's focused tests as
acceptance evidence, and run appropriate available integration checks once on the
reconciled tree; record environment limits. If any production conflict resolution
changes main's behavior, execute the affected suites before publishing:

- Auth/session/SCM/admin settings and migration 083: role/scope combinations,
  post-provider-I/O revocation, cookie versus body exchange, every fork default and
  unchanged existing settings. Use the upstream API/real-database tests.
- External issues and UI/CLI: exact confirmation, unknown creation and same-key
  lookup, parent read-before-attach/partial success, quick-add races and failed
  link/fork recovery (`api_external_issues`, `issue_commands`, dialog/Home/rail tests).
- Execution/read bounds: normal leftover release versus Stop/limit/failure cleanup,
  root/changed-UID cases on Linux, recorded-byte/ignored-line budgets, redaction and
  settlement reserve, multi-root bounded reads and dirty overlays. Preserve main's
  nested-rlimit regression (#511) and attachment HTTP/storage bounds (#541), even
  though the latter introduced no ontology delta.
- Runtime/preview: both lease-renewal paths, real-DB quota/claim/retirement races,
  stopped/uncertain cloud objects through provider doubles, OCI directory layers
  and template invalidation, masked stderr outcome persistence, profile scripts,
  and isolated-host browser preview flow. Source tests do not prove live GCE or
  deployed guest image behavior; retain those qualification limits.
- PR refresh/Activity: settling versus terminal/manual failures, completed-publication
  reset boundary and retry without repinning. QA notifier script tests cover config
  and event selection, not a live Slack delivery.

Audit self-review completed against the final ledger: all net changes and net-zero
history are accounted for; permission composition, lifecycle ownership, cancellation
and external partial-success cases retain their conditions; semantic conflicts have
explicit dispositions; source-preservation and competing-writer constraints are
part of execution. No additional human policy decision is required to prepare this
plan. Existing unresolved Qs and the residual min/max requirement remain visible;
they are not resolved by selecting a side of a Git conflict.


## Current follow-up — coverage commitments and formal refinement (October 15, 2026)

The user now authorizes adding the supported coverage findings and running TLA+/TLC
to refine and validate them, updating `docs/code_gaps.md` incrementally. This supersedes
the preceding audit-only scope and authorizes a new focused campaign at
`.g5/TLA_verification/ben-coverage/`; it does not restore the removed historical campaign.
Baseline: `832651d0bb3b1754eb9511d1cc5ccde7c2aafde4`. Preserve Ben's original source,
the 285 imported bodies/current registered transformations, and all skills/application
code. Extend existing semantic owners or add concise blocks with source provenance.
The 23 supported findings are the amendment denominator, not whole-system coverage;
the six candidates require evidence-based disposition, not automatic promotion.

Continue three authorized parallel owners: identity/chat models and their feature pages;
workspace/Git/document models and document/Git pages; runtime models and its architecture
pages. The coordinator integrates shared workspace/security contracts, phase and deployment
secret models, provenance/delta/clarifications, model runner and coverage/gap reports.
Shared files have one editor; agents deliver exact proposals and gap evidence for integration.

For each changed obligation, bind code actions and interruption boundaries, distinguish
as-built from intended behavior, execute safety/conditional progress, legal completion
witnesses and discriminating broken controls, and refine wording from findings. Report
structural premises, source bounds and missing runtime evidence separately. Existing
code gaps remain unless evidence closes them; new code-bound traces need source validation.
No production repairs or new runtime trajectory tests are authorized by this uplift.
Complete and publish each integrated refinement iteration only after source/structure,
formal receipts, code binding and cross-domain consistency checks. Do not commit partial
agent output or publish claims based on unexecuted configurations.


### Integrated refinement checkpoint

Implemented the supported 23-area amendment list under existing owners: 35 new recovered
D blocks, two explicit Q blocks, and comment-placement/viewer-question refinements. Two
additional candidates (quota and sizing) were reconciled into those additions; spend/grants
remain Q and Home/explainer scope remains unpromoted. All 285 source statements retain
their registered content; no skill or application code changed.

The focused campaign contains 26 models and 199 executed configurations, each with retained
formal/source hashes and named expected outcomes. Independent source/model reviews corrected
actual boundary and witness mistakes before the final reruns. Four code gaps are recorded
incrementally with minimal repair proposals; the unreachable-host temporal counterexample
is an explicit limit, not a fifth defect. Source/structure/delta/link and receipt checks
qualify this iteration only; no runtime tests, full-system percentage or physical proof.

Publication is one integrated coverage commit after final checks. A concurrently added
rebase-audit section is preserved in the working tree and excluded from this coverage
commit. This task does not execute that rebase or alter its proposal.

### Minimal uplift-harness follow-up

The user now authorizes a minimal skill improvement from the completed refinement.
Tighten the existing uplift entrypoint at four demonstrated boundaries: follow commitments
through queued/durable handoffs to later consumers; inspect actual retry/recovery traces;
name the independent capability denominator and exact inspected ranges; and judge intent
coverage from governing bodies/referenced owners. Existing scope, source-confirmation,
coverage checkpoints and authorization remain unchanged. No new workflow, reference,
ontology block or application/model change is needed. Maintained mirrors match; the skill
validator and all three existing synchronization tests pass, as does whitespace validation.
PyYAML was supplied only to a temporary validation environment; no dependency files changed.

## Previous follow-up — missing-intent coverage audit (October 15, 2026)

The user requests applying the existing uplift skill to analyze ontology coverage and
report missing intent, explicitly without editing the skill. Audit baseline:
`2d3bb3c586e0423d04bb2565a946f39602ba3f2b`. Keep skills, canonical ontology, Ben's source
and application code unchanged. Continue the existing coverage ledger with independent
capability discovery, inspected behavior owners, primary source/history reconciliation,
replacement counterexamples and minimal proposed ownership. Three authorized parallel
audits cover identity/chat, workspace/Git/documents and runtime; the coordinator covers
UI/transport and adjudication. Distinguish missing intent from implementation defects,
verification limits, unresolved policy and implementation freedom.

This pass does not recreate removed TLA artifacts or run runtime tests. Earlier model
results remain historical evidence only. The earlier reconstruction completion language
does not establish present whole-system intent coverage: supported consequential
omissions reopen that coverage checkpoint. Findings and the inspected denominator are
recorded in [coverage](coverage.md); proposed commitments are not silently adopted.

Completed bounded audit: four domain reports retain their independent obligation
inventories, direct read ranges, source distinctions, reconstruction counterexamples
and explicit unread limits. The consolidated ledger identifies 23 missing/incomplete
areas and six further candidates; shared credential findings are counted once. This
finishes the requested analysis/report, not the larger coverage checkpoint or any
ontology amendment. Structural/source/link checks validate report consistency only;
they do not execute the newly identified behavioral obligations.

## Previous follow-up — CI repair and artifact removal (October 15, 2026)

The user requested fixing the failed CI, removing all TLA artifacts and pushing PR #500.
This supersedes the earlier artifact-retention scope and authorizes the dependency-only
application change: update `rustls` from 0.23.45 to 0.23.46 in `Cargo.lock` to address
RUSTSEC-2026-0346 through RUSTSEC-2026-0349. Keep the existing dependency security gate.

Remove the complete `.g5/TLA_verification/ben-ontology` campaign, including models,
configurations, logs, reports and runners. Retain ontology commitments and source-backed
code-gap findings. Earlier investigations below are historical; their evidence links use
commit `38046b28` instead of deleted local files. The ontology validator now covers the
retained structure, local links and source comparison, without model receipt checks.

Verify dependency auditing, source preservation, ontology consistency and relevant Rust
compatibility before pushing. Check the published commit's CI and address further failures.
The earlier application-read-only and completed-publication statements describe the
historical milestones below, not this newly authorized dependency repair.

Local validation passed: `cargo deny --locked check` (all four categories),
`cargo test --locked -p g5-auth --features sqlx --lib` (68 tests), source preservation
(285 statements), ontology/link validation (343 blocks, 788 local links), harness
synchronization and its three tests, workspace formatting and whitespace checks.
All ontology contract bodies remain unchanged. Published CI is checked after the push.

## Historical uplift scope and milestones

The human's five-step request in this chat governs this work. The authorized branch
is `ziniuw/ontology_update_based_on_ben`; the inspected application baseline is
`20e393c0`. Application code is read-only. Scope includes the supplied backend and
user-facing commitments; UI assessment concerns behavior and authority, not visual
design. TLA+/TLC checking and iterative ontology refinement are explicitly requested.

## Sources and authority

Step 1 uses only [Ben's supplied statements](../../../docs/ben_new_ontology.md), an
exact Markdown export of the [Google document](https://docs.google.com/document/d/1SfDX_AcWsYPmfbkfiL-4ZLiUzu6_ecwA/edit).
All 285 nonempty source bullets are copied verbatim, including punctuation, existing
ID annotations, comment anchors and links. Only file organization, stable block IDs,
navigation and human-provided source metadata are added. The blank UI bullet is
preserved without treating it as a requirement. Comments remain in the source.
Source fingerprints and one-to-one locations are in [source-map.json](source-map.json).
Known duplication and malformed source links are intentionally retained at this checkpoint.

For steps 4–5, the human additionally authorizes code, TLA models, merged Git history
and relevant repository docs to find missing coverage. Ben's stated intent remains
governing; code observations and inferred refinements do not acquire human provenance.
Later contradictions receive evidence-backed dispositions or explicit questions.
No external records other than the supplied document have yet been inspected.

## Work and publication boundaries

| Step | Scope and output | Completion check | State |
| --- | --- | --- | --- |
| 1 | Organize source into `.g5/ontology`; retire the old uplift. | 285 verbatim statements, human provenance, IDs and source links verified. | Published as `8571e389`. |
| 2 | Remove per-block heading requirements from the harness and synchronize copies. | Natural sections, stable ID anchors, format consistency and skill checks. | Published as `0c5ca9cb`. |
| 3 | Publish the task changes and create the PR. | Remote branch and attached PR verified. | Complete: [PR #500](https://github.com/G5Labs/G5-Lean/pull/500). |
| 4 | Inspect implementation, build/run concern models, annotate evidence and report `docs/code_gaps.md`. | Faithful code bindings, safety, reachability and broken controls; scoped gaps recorded. | Complete bounded first pass: 22 concern models / 125 executed cases; breadth continues in step 5. |
| 5 | Independent breadth and reconstruction audit; refine missing commitments and models until coverage is justified. | Capability-to-contract-to-code-to-property evidence and explicit remaining limits. | Iterations 1–2 complete; source reconstruction, independent review corrections and final evidence recorded. |

Commit and push only after each numbered step and each completed step-5 iteration,
as requested. Do not add intermediate checkpoints. Each iteration updates this plan.
The inherited untracked experiment directory is outside this task and is not published.

## Verification and limits

The source-preservation check is executable with
`python3 .g5/uplift/ben-ontology/validate_source.py`.
No implementation conformance, runtime tests, model coverage or whole-system
completeness is claimed by the initial import. Later work keeps intent coverage,
direct code inspection, finite model properties and runtime evidence separate.

## Step 4 evidence checkpoint

Four authorized parallel investigation owners covered workspace, runtime, Git, and chat/identity. Fresh TLC results include safety/conditional liveness, actual as-built divergences, deliberate broken controls, and valid-completion witnesses. All 125 executions meet their specific expectations; counterexamples are not counted as safety passes. Original 285 statements remain verbatim. Metadata records source-backed gaps and three explicit questions, with no application repair. See [coverage](coverage.md), [investigations](investigations/workspace.md), [clarifications](clarifications.md) and [code gaps](../../../docs/code_gaps.md).

At the step-4 checkpoint, step 5 remained open for independent source/history breadth, missing capability contracts and additional model obligations. Source-inspection denominators are not whole-file coverage or global completeness percentages.


## Step 5 iteration 1 checkpoint

Reconstructed feature-local decisions from independently discovered owners and merged
primary records, added documents/operations ownership and compact recovery/authority
blocks, and extended formal checks to 60 concern models / 379 retained executions.
The counts include intended safety checks, known divergences, broken controls and
legal-completion witnesses; all match their stated expectations. Original 285 source
bodies and their provenance remain intact. Six policy questions retain provisional
choices. Code gaps are recorded, including missing repo removal and incomplete Git
backup coverage; earlier external-check freshness concern is resolved by original
admission-time policy. No application changes or runtime tests were made.

The next iteration used fresh-context reviewers to challenge code/model correspondence,
reconstruction completeness and natural Markdown ownership; its completed disposition follows.


## Step 5 iteration 2 and final milestone

Three fresh-context reviewers challenged the reconstructed contract and its formal
bindings. Supported omissions and correspondence errors were corrected in the existing
owners, with affected checks rerun and focused independent rechecks retained. Added
selected-file Save, completed-output retention, retained-receipt semantics, host inventory
and cloud recovery checks. Legacy approval compatibility, equal-content fork ownership,
lease replay and actual cloud/rotation interleavings now have discriminating evidence.

The final campaign contains **65 concern models / 459 distinct executed configurations**.
Every retained outcome matches its stated expectation; these include safety/conditional
liveness, modeled divergences, broken controls, stronger-policy limits and legal witnesses.
All **285 human statements remain exact**, with **50 agent decisions and six questions**
added across the final 19 files. Source fingerprints, formal receipts, links, structure and
unchanged application scope are validated. The source denominator is not a claim of
whole-program or runtime completeness.

The [final review](uplift-review.md) and [coverage ledger](coverage.md) close the requested
uplift and two reconstruction iterations with explicit bounded assurance, preserved
initial review findings, recorded code gaps and provisional questions. No production
repair or runtime test was performed. This completed iteration is the final commit/push
boundary; no intermediate model-authoring checkpoint is published.


## Follow-up full review and source comparison

The human explicitly requested review of all prior work, TLA/TLC correctness, consistent
minimal ontology with solid code links, a concise source-delta table, and minimal designs
in the code-gap report. This is a continuation of the same uplift and an authorized model
review/design comparison. Three fresh-context reviewers own runtime, chat/identity and
workspace/Git; the coordinator owns canonical ontology, source comparison and synthesis.
Application repairs remain proposals; no dependent implementation is undertaken here.

Keep `docs/ben_new_ontology.md` byte-for-byte unchanged. The earlier exact-copy checkpoint
remains in Git. This follow-up permits documented canonical editorial normalization and
reference consolidation: remove duplicated exported I/D labels and comment delimiters,
repair obsolete links, preserve all 285 source identities/meaning/provenance, and retain
an exact transformation register in `source-edits.json`. Code divergence does not make a
valid human requirement wrong. Source comments are interpreted with their date and current
evidence. Agent additions use one semantic owner and references for shared obligations.

Review every existing model and retained evidence; execute changed configurations and
selected representative checks, keeping fresh runs distinct from validated saved results.
Compare minimal candidates under the same successful/failure behavior and environment;
model controls alone do not prove implementation feasibility or global minimality.
Record runtime/instrumented-coverage limits explicitly. Commit/push only at the completed
review iteration, after final source/link/receipt checks and reviewer reconciliation.

Completed review outcome: all 65 original models have source-bound dispositions and
representative fresh executions; 11 additional models check dirty inheritance and repair
comparisons. The final inventory is 76 models / 532 configurations: 378 freshly executed,
154 validated unchanged receipts, with 101 positive runs and 431 expected negative cases
whose purpose is stated separately. Construction failures and repeated smoke runs are not
counted as additional coverage. A receipt-based replay runner avoids rebuilding historical
campaign partitions and passed a four-domain smoke replay.

Canonical result: 343 blocks / 20 pages, retaining all 285 human identities and meaning,
52 agent decisions and six unresolved questions. The original source hash is unchanged;
53 documented source transformations remove export noise, repair links or consolidate
shared rules, and an empty UI spacer is removed. The root overview and separate principles
page clarify structure. Agent corrections cover OAuth consumption, partial projection,
dirty-fork authority and live-host compensation; shared rules are referenced rather than
copied. `docs/ben_ontology_delta.md` covers every agent addition and source difference.

`docs/code_gaps.md` now includes a minimal-design section for all 20 gap IDs, with actual
reuse boundaries, competing writers, executed comparisons and required runtime validation.
Autoscale, literal process isolation, notification policy and in-use removal extensions
retain their stated limits. Application code and infrastructure remain unchanged; the
review does not approve policy amendments or claim instrumented code coverage.

This complete follow-up review is one publication checkpoint after validation; no partial
agent checkpoint was committed. Continue any implementation from this plan and the affected
gap/model owners, preserving the original requirements and unresolved decisions.

## Quantitative coverage follow-up — October 15, 2026

The user requested measurement along symbol mapping and execution-path analysis, updates
to the existing coverage report, and proposed missing intent; explicitly no harness edit.
The source/contract baseline is `072a12d3`; subsequent evidence archival does not change
those sources or governing bodies. Three authorized domain investigators and the
coordinator selected 63 behavior groups and recorded exact source reads. A reproducible
Rust/TypeScript declaration inventory and archived-receipt/witness assessment are saved
in `quantitative/`. The report separates candidate links, intent adequacy, bounded
enforcement tracing, historical model evidence and current source freshness.

The final recommendation contains eight new D blocks, two amendments, two policy questions
and one secondary candidate; all remain proposals in coverage evidence. Canonical ontology,
Ben's original document, harness, application and code gaps are unchanged by this task.
No new models, TLC executions or runtime tests were created. Validation checks inventory
reproduction, current-body/source stability, IDs/ranges, archived evidence integrity,
compound test-only classification and documentation links.
