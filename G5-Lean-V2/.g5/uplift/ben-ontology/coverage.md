# Coverage and reconstruction ledger

> Proof archive boundary: all TLA+/TLC models, configurations, runners, receipts,
> logs and generated proof ledgers have been removed from this PR at the user's
> request. The completed quantitative campaign and detailed evidence remain in
> [commit `345b546e`](https://github.com/G5Labs/G5-Lean/tree/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative).
> Counts below describe recorded checks at their stated baselines; this cleanup
> performs no new TLC execution. Ontology commitments and coverage judgments remain.

The final language pass reviewed all 20 pages and retained all 418 blocks. It changes
wording and evidence summaries, not the inventory or coverage judgments below:
57 of the 63 selected behavior groups still have adequate intent. It makes no new
claim about code execution, production correctness or formal coverage.

## Adopted quantitative refinement — October 15, 2026

The approved follow-up adds **eight D blocks and two Q blocks** and amends two
existing decisions. Ben’s source and the harness are unchanged. The ontology now
contains **418 blocks** (257 I, 151 D, ten Q) across the same 20 pages. The
[Ben comparison](../../../docs/ben_ontology_delta.md#8-quantitative-coverage-follow-up--october-15-2026)
explains each addition and amendment; questions remain provisional.

Against the **same 63 selected behavior groups**, adequate intent increases from
**47 to 57 (74.6% → 90.5%)**. Ten groups gain sufficient governing text; admission
continuity moves from partial to an explicit question. The remaining group with
partial intent concerns SCM connection independence, still a secondary candidate.
Five groups contain unresolved policy. These are equally counted, nonrandom groups,
not repository-wide code coverage or an implementation pass rate.

| Domain | Before adequate / groups | After adequate | Partial | Question |
| --- | ---: | ---: | ---: | ---: |
| Identity, authority and chat | 14 / 20 | 16 | 1 | 3 |
| Runtime | 15 / 19 | 18 | 0 | 1 |
| Workspace and review | 14 / 17 | 16 | 0 | 1 |
| Platform and UI | 4 / 7 | 7 | 0 | 0 |
| **Selected inventory** | **47 / 63** | **57** | **1** | **5** |

The [after-disposition ledger](quantitative-refinement/coverage.json) preserves every
original row ID and before status. The archived [measurement script](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/uplift/ben-ontology/quantitative-refinement/measure.py)
derived counts and formal associations from those reviewed judgments and exact receipts.
The retained ledger omits per-case proof payloads and links their immutable snapshot.
It does not overwrite the original symbol inventory or promote nearest-owner links
into verified function mappings. The earlier 15,732-declaration denominator and 429
candidate associations remain the original before-measurement, not newly measured
semantic or runtime coverage.

Twelve selected rows now have **fresh, partial formal associations** covering the
new decisions, amendments and provisional choices. Passing a concern model does not
cover every behavior in its row: Home candidate producers, general publication,
native filesystem races, child-hunk reconciliation and actual PostgreSQL behavior
remain among the explicit limits. No enforcement grade is upgraded from a model pass. IDN-11 is downgraded from
traced to partial after the ordinary-fork bypass was found: 17 bounded slices remain
traced and 46 partial. The sidecar preserves both grades. New source gaps are recorded in
[code gaps](../../../docs/code_gaps.md#quantitative-refinement--october-15-2026).

The fresh [campaign](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative/README.md) contains
**13 concern models and 170 configurations**, all executed with the final pinned
runner and checked against their expected outcomes. Independent source/model review
corrected witness ordering, transport ordering and an unreachable retry predicate;
[review notes](quantitative-refinement/review.md) retain those corrections and limits.

| Fresh evidence | Result | What it establishes |
| --- | ---: | --- |
| Safety / conditional liveness | 32 / 3 | Intended or qualified source variants satisfy the named properties within their bounds and assumptions. |
| Broken controls | 66 | Removing a relevant guard reaches the named failure; parser errors and timeouts do not count. |
| Reachability witnesses | 60 | Named legitimate completions/recoveries are reachable; 53 distinct model/action-name sequences, not production execution paths. |
| Classified divergences / alternatives | 9 | Three new source-bound gaps, two retained gaps, one schema-conditional limit and three policy comparisons. These are expected counterexamples, not passing implementation guarantees. |
| Saved-evidence integrity | 170 / 170 | Model/configuration, manifest row, source bindings/files, runner, tool and log fingerprints match the retained expected outcome. |
| Explored state sums | 658,019 generated; 287,486 distinct | Sums overlap across configurations; neither is a unique system-state or code-path denominator. |

No production tests or runtime coverage instrumentation were run. Fresh models cover
selected properties of twelve inventory rows; they do not refresh all archived evidence,
establish full-row conformance or close the code gaps. In particular, retaining a pending
terminal-event obligation survives restart but does not reconstruct an exit payload that
was never persisted; that repair candidate remains incomplete.

Before publication, this iteration was replayed onto the independently rebased
`afccd820`, retaining main's provider-merge fix and its existing ontology amendment.
All 170 model receipts still match every bound source file byte-for-byte; those
provider changes are outside this campaign. The sidecar retains the integrated base
and row judgments; exact ontology/input hashes remain in its archived snapshot.
The original 63-row baseline stays frozen.

## Quantitative assessment — October 15, 2026

This assessment measures the application and governing ontology at `072a12d3`.
Subsequent evidence archival changes links, not these governing bodies or application
sources. The harness and canonical ontology are unchanged by this assessment. The
[reproducible inventory and methodology](quantitative/README.md),
[summary](quantitative/summary.json), and four domain ledgers retain the denominators,
source hashes, actual inspection ranges, evidence limits and draft proposals.

**47 of 63 selected behavior groups have adequate governing intent (74.6%).** Six are
partial, six are missing, and four contain unresolved policy. This is a risk-directed,
nonrandom inventory with differently sized groups: it is **not a repository-wide
ontology-to-code coverage percentage**, a code-conformance score, or evidence that
uninspected behavior is covered. The counts describe this declared inventory only.

### Symbol-to-ontology measurement

| Measurement | Result | Interpretation |
| --- | ---: | --- |
| Application source files inventoried | 1,345 | All tracked backend Rust and frontend `src` TypeScript/TSX at the pinned revision. |
| Named declarations recovered | 25,232 | 9,500 test-scoped; **15,732 potentially production**. Source declarations across configurations, not compiled symbols. |
| Production declaration breakdown | 11,589 callable; 4,143 type | Callable count includes 131 declaration-only signatures. Anonymous callbacks and macro expansions are not fully represented. |
| Parser recovery required | 20 files, four outside test paths | Counts are approximate recovered declarations. Error locations are retained; no compiler failure is inferred. |
| Symbols overlapping reviewed rows with owner candidates | 429 / 15,732 (2.7%) | Row-mediated candidate links, **not verified function-to-contract entailment**. A row's nearest owner may still omit the requirement. |
| Symbols with unique-basename metadata references | 89 / 15,732 (0.6%) | Weaker file-level references, kept separate; indirect evidence links are not expanded into whole-file credit. |
| Symbols with no link from either extraction | 15,217 | Unclassified by this map, not 15,217 missing ontology blocks. Helpers, types and incidental mechanics may share an existing owner. |
| Current-pass source inspection footprint | 19,884 / 910,785 inventoried lines (2.2%), in 81 files | Union of directly read ranges; denominator includes tests. This credits this pass only, not all historical reading. |
| Production declaration span inspection | 320 full spans; 137 partial | A span read is neither semantic proof nor executed coverage. |

[Every declaration](quantitative/symbols.tsv) retains its path, range, kind, test
classification, overlapping audit rows, candidate owners and inspection extent.
[Every file](quantitative/files.tsv) is retained, including unmapped areas. Another
457 source-like files outside the declared Rust/TypeScript inventory are explicitly
[listed](quantitative/excluded-source.json); SQL, scripts, infrastructure and separate
tools/tests are not silently treated as covered. Some are directly read as supporting
evidence, outside the symbol denominator.

The 429 associations are deliberately not called semantic mappings: for example, a row
covering both profile and CLI operations references both owners, but each CLI helper does
not thereby implement user settings. Likewise, a source hash detects drift; it does not
prove that an entire file was inspected. The small mapping footprint reveals missing
traceability and unclassified scope, not the percentage of product intent absent.

### Behavior and execution-path measurement

| Independently discovered scope | Groups | Adequate intent | Partial | Missing | Question | Bounded enforcement slice traced |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| [Identity, authority and chat](quantitative/identity.md) | 20 | 14 | 4 | 0 | 2 | 4 |
| [Runtime, hosts and guest execution](quantitative/runtime.md) | 19 | 15 | 1 | 2 | 1 | 3 |
| [Workspace, Git and review](quantitative/workspace.md) | 17 | 14 | 1 | 1 | 1 | 9 |
| [Platform and UI](quantitative/platform.md) | 7 | 4 | 0 | 3 | 0 | 2 |
| **Selected inventory** | **63** | **47** | **6** | **6** | **4** | **18** |

The remaining 45 enforcement slices are partial; none is silently counted as passing.
Rows name actual producers, handoffs, durable effects, consumers or recovery, plus
uninspected limits. Their references involve 169 distinct current ontology IDs, often
shared across rows. Intent adequacy and implementation conformance remain separate:
adequate text can govern code with a known defect. Rows are not equally sized or
statistically representative, and their lineage must be preserved before comparing a
future ratio. Important unread areas remain listed in each domain report.

| Formal/path evidence measurement | Result | Boundary |
| --- | ---: | --- |
| Selected rows with a named historical formal association | 22 / 63 | Some refer to older archived models; partial property associations do not verify the whole row. |
| Focused archived campaign | 26 models / 199 configurations | 52 safety, five conditional liveness, 79 broken controls, 58 witnesses, five divergence/limit cases. |
| Archived integrity checks | 199 / 199 | Model/config, manifest case, binding, runner and log hashes match their pinned receipts and recorded expected outcomes. No TLC rerun. |
| Named witness goals with retained reached traces | 58 / 58 | 55 distinct model/action-name sequences; equal names can still carry different state values. |
| Current bound source files unchanged | 47 / 72 (65.3%) | 25 changed files require correspondence review. This is freshness, not correctness. |
| Receipts whose entire declared source input set is unchanged | 0 / 199 | Domain-wide binding is conservative: this does not imply every model is wrong. |
| Instrumented production line/branch/path coverage | **Not measured** | No runtime tests, fault injection or coverage instrumentation were run here. |

The AST inventory also finds 38,842 selected decision/loop/exception syntax sites. That
is **not a path denominator**: implicit branches, callbacks, generated code, data values,
loops, asynchronous handoffs and interleavings prevent interpreting it as all executions.
No interprocedural call graph or all-code-path recovery is claimed. The archived traces
show specific reachable paths through the models; no TLC action-coverage statistics
were collected, and a model cannot expose a source action omitted from its abstraction.
The focused manifests name 38 ontology IDs; that is traceability, not a completeness score.
[Formal evidence details](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/uplift/ben-ontology/quantitative/formal-evidence.json) are retained without restoring
the archived suite or changing its receipts.

### Proposed important missing intent — frozen before adoption

The reconstruction test is whether a replacement could obey the existing bodies while
omitting the named outcome. The following **eight new D blocks and two amendments** are
recommended as concise recovered decisions, with provenance and uncertainty retained.
They are proposals only; no ontology, harness, application, model or code-gap file was
changed by this assessment. These ten changes address ten of the twelve partial/missing
rows; the remaining two are the qualified policy/candidate cases below.

| Proposal / owner | Minimal proposed commitment | Evidence and why the current text is insufficient |
| --- | --- | --- |
| New `g5.runtime.control.replay` | Bind replay to command method, key and original payload; changed payload conflicts. Timeout remains unknown, so retry the same identity within its budget. Retain completed results across reconnection for the supported window, without promising survival across host-process restart. | [RT-P01](quantitative/runtime.md): v9 protocol and retained prior decision; version/incarnation rules do not define replay or uncertain results. |
| New `g5.runtime.events` | Persist per-VM event sequence/facts until acknowledged. Deduplicate identical replay, reject conflicting sequence reuse, and commit applicable effects before acknowledging the contiguous stored prefix. Stale-incarnation facts cannot mutate replacement custody. | [RT-P02](quantitative/runtime.md): journal and ingestion owners plus prior contract. Inventory/fencing alone does not require durable events or safe acknowledgment; enforcement has explicit limits. |
| New `g5.chat.creation.idempotency` | A keyed chat-start or chat-history-fork request has one person/organization-scoped identity. Equal retries return the original result; changed payload conflicts and deleted results are not recreated. Resolve mutable persona instructions only on original acceptance. | [IDN-P02](quantitative/identity.md): retained designs and `workspace_messages` claim path. Atomic creation alone allows duplicate conversations after a lost response. This does not govern every workspace-fork API. |
| New `g5.workspace.review-rounds` | Successive review-copy rounds supersede prior open changes/findings of the same reviewer and review kind, while retaining persistent discussion/artifacts. Carry a decision only for the same evidenced proposal occurrence and content; changed proposals need a fresh decision. | [WG-P01](quantitative/workspace.md): saved comments design and merged fixes. Existing review history does not define cross-round identity or supersession. |
| New `g5.workspace.review-discard` | Reviewers can discard selected previewed changes from their own copy without changing the source. Refuse stale selections, preserve intervening edits, and disclose completed versus skipped changes. | [WG-P02](quantitative/workspace.md): original discard rationale and stale-preview fixes. Fork isolation does not require the discard capability or honest partial results. |
| New `g5.ui.home-attention` | Project authorized actionable approvals and owned-workspace questions from their existing owners. Loaded/filtered counts describe that slice; absence, loading or failure must not imply no outstanding work. | [P01](quantitative/platform.md): retained workbench plan and current backend/UI path. Guided-step completion does not entail an attention projection or qualify partial counts. Recovered scope remains agent provenance. |
| New `g5.documents.explainer-catalog` | Offer built-ins plus valid saved explainer templates from committed repository configuration. Owner edits follow ordinary live-file/publication boundaries; saving a template does not claim publication. | [P02](quantitative/platform.md): separate catalog loader and retained design. Workbench phase override rules describe a different configuration owner. |
| Restore `g5.documents.explainer.templates` | User-template replacement requires an explicit choice; built-in template identities remain reserved. | [P03](quantitative/platform.md): prior contract plus current conflict/overwrite flow. This does not assert native filesystem atomicity. |
| Amend `g5.workspace.transfer.formal` | Refuse member removal or viewer demotion while the person owns a non-deleted workspace; require explicit transfers/deletions first. Never silently choose a replacement owner. | [IDN-P01](quantitative/identity.md): explicit v9 ownership policy, organization writer and schema. Eligible transfer targets do not define existing-owner removal. |
| Amend `g5.workspace.module-execution` | Before serving after dispatcher restart, durably settle executions whose supervision was lost without relaunching those identities; refuse startup on settlement persistence failure. Prune only acknowledged event prefixes and retain unacknowledged events beyond the soft target. | [RT-P03](quantitative/runtime.md): retained dispatch/event contract and actual recovery/journal code. Already-settled no-replay rules leave interrupted identities and intermediate events unspecified. |

Two **question proposals** should remain separate from recovered D commitments:

- **Review verdict → formal approval authority** ([WG-P03](quantitative/workspace.md)):
  whether a reviewer's Approve verdict may bypass an addressed pending request, including
  withdrawal and reviewed-cut authority. Recommended resolution: reuse explicit request/
  owner authorization while keeping review publication independent. Retaining a separate
  reviewer exception needs an explicit scope decision; current code alone cannot authorize it.
- **Instance allowlist versus member re-entry** ([IDN-P04](quantitative/identity.md)):
  whether first-admission restrictions permit existing members/invitees to return. The
  observed exceptions are plausible, but their original authority was not recovered.
  Provisionally preserve member/invitation continuity pending policy reconciliation; do
  not label the observed rule human-confirmed.

SCM connection independence ([IDN-P03](quantitative/identity.md)) remains a secondary
candidate: current ownership/credential separation may already imply enough of it to
avoid another block. Existing invitation-timing, spend and literal-process-isolation
questions remain open. The 63-row review does not reopen them as duplicate proposals.

The next useful coverage work is to resolve those policy boundaries, review the uncovered
critical handoffs and classify uninspected source groups. Filling every symbol with an
ontology ID or adding a model per block would inflate the map without establishing the
missing behavior. Any later modeling should check the proposed obligations and their
actual source transitions, including retry/recovery and successful completion, with
current bindings. This assessment supplies the measured baseline and the concrete worklist.

## Rebase evidence boundary — October 15, 2026

The rebase onto main `3d17915df39bcf4cbe0cabc08500de63ff46fbb2` preserves every
coverage-refinement addition and open question below. The [existing plan](uplift-plan.md)
records the complete upstream reconciliation: 28 further focused decisions and four
disclosed human-authorized amendments to Ben's statements, with immutable source text
and original statement identities preserved. The [comparison](../../../docs/ben_ontology_delta.md)
accounts for these changes separately from the earlier editorial transformations.

The 199 saved receipts were verified against their original inputs before rebase;
no TLC was rerun. Incoming application changes can stale source bindings, so the
[campaign](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/README.md) is retained as historical
evidence. The following refinement describes its original baseline, not proof of
the rebased application. [Current gap dispositions](../../../docs/code_gaps.md#rebase-disposition--october-15-2026)
narrow the autoscaling gap and retain unresolved source findings without claiming
runtime confirmation. The older coverage sections remain historical records.

## Coverage refinement — October 15, 2026 (before rebase)

The authorized follow-up addresses the **23 supported audit areas** under their existing
semantic owners, adds two evidence-reconciled candidates, and keeps four candidate areas
explicitly unresolved. This is closure of a named audit list, **not 100% system coverage**.
All 285 supplied statements and their registered transformations remain unchanged. The
[comparison](../../../docs/ben_ontology_delta.md#6-coverage-driven-refinement--october-15-2026)
accounts for every new agent decision/question and the two existing-block refinements.
No skill or application code is edited.

The [focused TLA campaign](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/README.md) distinguishes
intended contracts, as-built transitions, deliberately broken controls and successful
reachability witnesses. Exact source ranges and hashes bind each domain; a hash of a
whole file is a staleness check, not inspection credit. Formal results are limited to
named finite properties and stated assumptions. No runtime tests or line/branch coverage
measurement were performed in this refinement.

| Audit area | Adopted owning intent | Formal/structural coverage and important limit |
| --- | --- | --- |
| C01 | `g5.workspace.audience` | WorkspaceAudience checks direct audience/owner decisions and a separate child-origin path; direct human ancestor browsing remains a policy boundary, not a proven defect. |
| C02 | Four `g5.workspace.approval-*` policy/administration/human/definition owners | ApprovalPolicy checks protected requirement/version/decision admission. Existing source-cut/artifact eligibility is referenced, not re-proved. Viewer collaboration remains Q. |
| C03 | `g5.user.cli-authorization`, `g5.security.credential-classes`, `.browser-tickets` | CliAuthorization and BrowserTickets check admission, one-use delivery, purpose/audience and owner boundaries. Cryptography is structural; synthetic purpose probes are not actual issuer outputs. |
| C04 | `g5.security.guest-egress` | GuestEgress checks install-before-execute and failure closure against representative policy classes. Real kernel rules/TSI equivalence and external flush recovery remain unproved. |
| C05 | `g5.security.deployment-keys`, `.secret-lifetime` | DeploymentSecrets checks two managed initializers and flush-failure retry. Key parsing, OS fsync/rename and deliberate rotation are premises. |
| C06 | `g5.lifecycle.start` | Consent checks Start/Resume admission, cancellation, expiry and owner-generation relationship. Transactional cancellation and authorized rollout continuation remain qualified separately. |
| C07 | `g5.lifecycle.stop`, `.stop.window` | StopWindow separates preparation, settlement, drain, capture/retry, durable end-saving and termination. Old restore data is not a completed receipt for this stop; unreachable-host physical death is outside the save-window guarantee. |
| C08 | `g5.runtime.materialization`, `.purge` | RootLifetime checks path custody and cleanup/replacement races; no host Git in guest trees is a structural source constraint, and fresh paths do not prove old process death. |
| C09 | `g5.lifecycle.idle` | IdlePreview checks current activity/work/preview readiness at idle admission; TerminalIdle distinguishes quiet connected terminals, input and disconnect. |
| C10 | `g5.preview.service.stop-reason` | IdlePreview retains lifecycle reason across representative settlement paths. Complete UI rendering and every producer remain outside the model. |
| C11 | `g5.runtime.placement.host-disk` | HostDisk checks derived-versus-explicit sizing through Add/rotation/retry, one coefficient authority and headroom. Uses scaled finite quantities, not a benchmark or cloud-capacity proof. |
| C12 | `g5.chat.execution-defaults`, `.execution-headroom` | ExecutionCapacity checks finite admission/catalog/headroom cases. Exact sixteen/default and custom/old catalog values are checked; task identities and competing reservations are finitely bounded. |
| C13 | `g5.chat.attachment-delivery` | AttachmentDelivery separates authorized resolution, content verification, supported encoding and recovery history. Byte/hash implementation and every provider adapter are outside the abstract model. |
| C14 | `g5.documents.reader-suggestions` | ReaderSuggestions checks additive no-compute proposals, detected stale-base confirmation and source isolation. It does not promise an atomic latest-head comparison or grant viewers new writes. |
| C15 | `g5.workspace.review-decisions`, `.review-acceptance` | ReviewAcceptance checks selected edits, failed publication and interrupted file/SQL settlement. The source-bound interruption divergence remains an implementation gap. |
| C16 | `g5.documents.comments.drafts` | DiscussionDrafts checks principal/scope and page-lifetime retention/send semantics. No cross-reload storage promise is added. |
| C17 | Existing `g5.documents.comments.identity` refined | CommentPlacement distinguishes unique evidence from ambiguous/unresolved anchors. Text matching/diff correctness and browser rendering remain structural limits. |
| C18 | `g5.git.pr-write-identity` | PullRequestIdentity checks update/create, terminal provider identity and failed first-publication recovery. Full external reconciliation remains with existing PR recovery intent. |
| C19 | `g5.workspace.content-policy`, `.content-policy.transition` | ContentPolicy checks finite operation classes, drain/quiescence and reversible worker loss. Every route/admission producer and native filesystem writers are not exhaustively audited. |
| C20 | `g5.workspace.phase-state` | PhaseState checks owner transaction/event and fork-capture boundaries, including valid successor/fork paths. DB transaction atomicity is a premise. |
| C21 | `g5.chat.confirmed-launch` | LaunchAgreement and QueuedLaunchAgreement separate resolver-local checks from queue/first-dispatch preservation. Current credentials and unconfirmed inheritance remain allowed; the confirmed queued-selection discrepancy is recorded separately. |
| C22 | `g5.chat.account-continuity` | AccountAssignments checks assignment/deletion serialization and scoped compatibility. Original human request was not located; this remains clearly agent-recovered design. |
| C23 | `g5.security.current-authority` | CurrentAuthority checks inspected transactional consumers. BrowserTickets exposes fresh Org-subscription admission from stale membership; no existing-stream cancellation promise or whole-endpoint conformance claim. |

### Disposition of the six additional candidates

| Candidate | Disposition in this refinement |
| --- | --- |
| Organization VM quota | Added `g5.runtime.placement.capacity`: active v9 separates quota from host capacity and requires enforcement before start. Placement checks competing admissions/counts; old numeric defaults are not restored. |
| Sizing publication/stability | Added `g5.runtime.placement.sizing-publication`: reconciled human sizing request and current Hello writer, including legacy custody. Placement checks publication and live-size stability. |
| Workspace spend thresholds/accounting | Added `g5.chat.spend-scope-question`; provisional accounting retention/threshold deferral, with no model claiming a working cap. Scope and admission owner still need a decision/investigation. |
| Granular grant retirement | Added `g5.organization.grant-lifecycle-question`; preserve role ceilings while reconciling retirement and active consumers. Do not turn additive code into governing intent. |
| Home actionable attention | Still a candidate. Original audience/completeness/ordering scope is not fully reconciled; no new inbox mandate or model claim. |
| Saved explainer catalog | Still a candidate. Original scope/override/publication intent is not fully reconciled; no new catalog mandate or model claim. |

### What the measurement means

The useful measure is a **traceable obligation ledger**: independent discovery → evidence
and authority → minimal owning commitment → actual code/action boundary → checked property
or explicit limitation. The replacement test asks whether an implementation could obey
all current bodies while omitting the discovered outcome. Model counterexamples then test
concurrency, failure and recovery details; they cannot discover all missing product goals.
The uplift skill supplies that completeness obligation. Model judgment supplies discovery,
source reconciliation and abstraction, and independent review catches missed boundaries.
Neither instructions alone nor a capable model alone guarantees completeness.

Coverage remains open for the uninspected consumers and candidate areas in the four prior
inventories, as well as runtime/system tests, deployment behavior and the historical code
gaps. Twenty-three addressed findings is not a denominator for all behavior in the repo.

## Prior missing-intent audit — October 15, 2026

**The ontology does not yet justify a “very high coverage” or completed reconstruction
claim.** Independent source/code discovery found **23 missing or incomplete obligation
areas**, with the confidence and policy qualifications below. This is a count of
deduplicated findings, not a coverage numerator or denominator. Some are compact extensions
to existing owners; they do not imply 23 new blocks. Additional candidates remain unresolved.

Audit baseline: `2d3bb3c586e0423d04bb2565a946f39602ba3f2b`. The user explicitly asked to
apply the existing uplift skill without editing it. Skills, canonical ontology, application
code and [Ben's source](../../../docs/ben_new_ontology.md) remain unchanged. No runtime
tests or TLC checks were executed in this audit. Proposed wording is evidence-backed advice,
not an adopted governing amendment. The previous TLA campaign was removed at the user's
request; its results below are historical, not checks of these newly identified obligations.

### How coverage was examined

Four investigators discovered behavior from public routes, service/worker owners, durable
records, browser flows, operator paths, primary request/design records and focused merged
history. We read ontology bodies first and asked: **could a replacement obey those bodies
while omitting a required user outcome or authority boundary?** Each supported finding
names that omission, its independent source, current implementation, nearest commitment
and smallest proposed semantic owner. A historical block or code guard alone is insufficient
to establish human intent; recovered agent decisions remain identified as such.

The inspected obligation denominators and exact source ranges are retained in four
overlapping domain inventories:

| Scope | Discovery and inspected evidence | Current disposition |
| --- | --- | --- |
| [Workspace, Git and documents](reviews/coverage-workspace-git.md#obligation-inventory) | Sharing/admin paths, content-policy transitions, reader proposals, review decisions, composers/anchors, PR creation/update, issue linking and question/history entry points | Seven missing/partial areas; existing Save, question and Git rules retained. Full review lifecycle, Git object/receive consumers and document producers remain partly unexplored. |
| [Identity, approvals and conversations](reviews/coverage-identity.md#obligation-inventory) | Permission admission, CLI authorization, approval administration/quorum, attachment materialization, account/configuration owners, spend settings and execution-capacity admission | Seven missing/partial areas; budgets and granular grant lifecycle remain questions. Full OAuth, notification fanout, metering and job/task lifecycle not freshly audited end to end. |
| [Runtime and hosts](reviews/coverage-runtime.md#obligation-inventory-and-dispositions) | Start/Stop/idle/release writers, fresh materialization, guest egress, all located boot-disk provisioning paths, lease/inventory and preview outcome writers | Seven missing areas; quota and sizing stability need further reconciliation. Full cloud retries, process census, cache/image internals, native writers and module cleanup remain partly unexplored. |
| [UI, browser transport and deployment secrets](reviews/coverage-ui-transport.md#inventory-and-negative-controls) | Explicit phase writer/fork admission, ticket minter/validators/upgrades, subscription authorization, reconnect, Home/template owners and secret startup/bootstrap | Phase and secret-lifetime omissions; transport finding consolidated with identity credential separation. Home and explainer catalogs remain candidates. |

“Present” in these inventories means a governing commitment exists. It does **not** mean
the whole capability was inspected or its implementation passed. Partial reads receive
only their recorded range credit. Inventories overlap at shared owners and must not be
summed into a percentage. Original statement preservation, behavioral intent coverage,
implementation conformance, model-property coverage and executed runtime coverage are
different measurements. No line/branch instrumentation or exhaustive obligation inventory
exists for this checkout, so no defensible global ontology-to-code percentage is available.

### Missing or incomplete governing intent

Confidence concerns the omission and retained evidence, not implementation correctness.
“Derived” marks recovered design rather than a direct new human requirement. Follow the
linked finding for exact source lines, proposed wording, exceptions and remaining limits.

| ID | Missing area and a behavior the current text permits | Minimal owning change | Evidence / confidence |
| --- | --- | --- | --- |
| C01 | **Workspace audience:** expose every same-org workspace; retain creator/former-owner access to invite-only workspaces. | Define org-visible/invite-only reads, admin/current-owner privileges and owner-only audience changes; reconcile ancestor access explicitly. | [Workspace F1](reviews/coverage-workspace-git.md#f1); high, retained sharing design and current guards. |
| C02 | **Approval policy:** retain fresh decisions but allow owner-controlled requirements, partial quorum or the wrong merge-target policy. | Own org-admin administration, addressed human authority, all-required-human/configured-agent IE requirements and invalidating definition versions. Reference existing cut/settlement rules. | [Identity F1](reviews/coverage-identity.md#f1); high omission; some quorum/version details are recovered design. Viewer exceptions remain unresolved. |
| C03 | **Credential classes and G5 CLI login:** exchange a delegated API key for a broader personal session, or accept a long-lived session credential at a WS query-token boundary. | Shared user/security owner for human-approved, bounded CLI authorization; prohibit service/API-key escalation; require purpose-bound short-lived WS tickets and exact terminal workspace binding. | [Identity F2](reviews/coverage-identity.md#f2), [transport SEC-1](reviews/coverage-ui-transport.md#supported-omissions); high, merged security corrections. Codex native login is a different flow. |
| C04 | **Guest egress:** isolated VMs can still reach cloud metadata or host-local services. | A security rule for the retained Linux deny boundary before execution, fail-closed boot and narrow supported exceptions. | [Runtime R-INT-1](reviews/coverage-runtime.md#r-int-1); high, explicit SEC-002 fix. No broader default-deny private networking inferred. |
| C05 | **Deployment secrets:** encrypt database values with publicly known keys, or regenerate keys over retained data. | Require deployment-owned credentials, reject public fixture identities and preserve keys for their data lifetime; deliberate recovery/migration on loss. | [SEC-2](reviews/coverage-ui-transport.md#supported-omissions); high, SEC-005 request and retained design. Isolated test fixtures remain allowed. |
| C06 | **Explicit Start consent:** queued work silently wakes a stopped workspace or a cancelled Start later boots it. | Durable owner/generation-bound Start/Resume consent with bounded capacity-wait lifetime and supersession; separate authorized rollout continuation. | [Runtime R-INT-5](reviews/coverage-runtime.md#r-int-5); high omission, detailed consent design has medium-high provenance confidence. |
| C07 | **Bounded Stop:** retry an unsavable flush indefinitely or return to running on save failure. | Fence work/older Starts, bound ordinary saving to the human-chosen two minutes, force termination on deterministic capture failure or save-window expiry, retain loss and restart from the last complete receipt. | [Runtime R-INT-2](reviews/coverage-runtime.md#r-int-2); high, explicit later human correction. Unreachable-host/physical-death limits remain; no new loss UI required. |
| C08 | **Fresh roots and release cleanup:** retain unused guest roots indefinitely or run host Git inside guest-written metadata. | Fresh trusted materialization; no host Git in former guest trees; safely delete unheld released roots with cleanup retry and separate trusted caches. | [Runtime R-INT-3](reviews/coverage-runtime.md#r-int-3); high, explicit “stop keeping roots” decision. No secure-erasure promise. |
| C09 | **Idle activity:** stop a requested preview while it is still building, or let unused output keep compute alive forever. | Define meaningful owner activity and rechecked idle admission; protect not-ready previews, distinguish unused ready services. | [Runtime R-INT-6](reviews/coverage-runtime.md#r-int-6); high for preview rule; complete activity matrix needs reconciliation. |
| C10 | **Preview termination reason:** show every stop/loss as a generic cancellation. | Retain and display the lifecycle reason in the preview outcome. | [Runtime R-INT-7](reviews/coverage-runtime.md#r-int-7); high, explicit incident follow-up. Separate from forced-stop loss display. |
| C11 | **Host disk capacity:** provision a fixed-size disk regardless of VM slots, or discard explicit operator sizes during replacement. | Derive shared boot disk from scratch/slots/host-cache allowances; preserve explicit overrides, rederive omitted defaults through Add/rotation/retry, publish one calculation. | [Runtime R-INT-4](reviews/coverage-runtime.md#r-int-4); high, human sizing and “derive” correction. Do not restore stale constants. |
| C12 | **Execution defaults and headroom:** default to two turns or allow agent turns to starve Changes/diagnostics. | Extend chat admission with default 16, API/catalog agreement, operational headroom and safe limits for old catalogs. | [Identity F8](reviews/coverage-identity.md#f8); high, direct human default and accepted compatibility design. Completed rollout mechanics need no permanent block. |
| C13 | **Attachment delivery:** store/freeze descriptors but omit bytes, accept corrupt content, or redeliver old attachment bytes as a new action. | Extend chat input/delivery with authorized integrity checks, supported native encoding, explicit refusal and recovery-history distinction. | [Identity F3](reviews/coverage-identity.md#f3); high, issue-231 request/design and active runtime. |
| C14 | **Reader file suggestions:** require a fork or running VM for every reader proposal. | Add the distinct no-fork/no-compute path for existing text files, additive proposals/withdrawal and disclosed stale-base confirmation. | [Workspace F2](reviews/coverage-workspace-git.md#f2); high, direct accepted FILES decisions. Preserve owner-only source writes and existing viewer Q. |
| C15 | **Review acceptance:** treat acceptance as an unpublished dirty write or reset it to unaccepted after push failure. | Own per-suggestion authority, matching-source application and the trigger into selected-file Save; retain accepted/pending-publication status after applied bytes. | [Workspace F3](reviews/coverage-workspace-git.md#f3); high, issue-238 and comments design. Reuse Save rather than duplicate it. |
| C16 | **Discussion drafts:** lose a typed reply/rejection/replacement when its target refreshes, disappears or becomes read-only. | Extend discussion ownership with page-lifetime draft/send/error retention, explicit dismissal and principal/scope separation. | [Workspace F4](reviews/coverage-workspace-git.md#f4); high, repeated explicit human repair decisions. File drafts alone do not cover this. |
| C17 | **Truthful comment placement:** retain anchor evidence but display the thread beside an arbitrary duplicate. | Extend comment identity with unambiguous placement or a visible original unresolved anchor. | [Workspace F5](reviews/coverage-workspace-git.md#f5); high, demonstrated failure and accepted design. Algorithm remains free. |
| C18 | **PR update/replacement:** silently create another G5/provider PR when updating a closed one. | Update requires an open G5 PR; no silent replacement of closed/merged provider PRs; explicit create replacement. Preserve first-provider-publication recovery. | [Workspace F6](reviews/coverage-workspace-git.md#f6); high, issue-387 request and later correction. |
| C19 | **Workspace read-only mode:** omit user-controlled content policy or equate it with stopped compute. | A workspace content-policy owner independent of VM state; preserve authorized teardown/admin/Git operations. | [Workspace F7](reviews/coverage-workspace-git.md#f7); high capability evidence; exact drain/exceptions medium, not physical filesystem immutability. |
| C20 | **Explicit phase state:** infer phase from approvals/Git, lose it on reload or reset ordinary forks. | Extend workspace stages with owner-selected durable state/events and admission-fixed inheritance; reference approval policy separately. | [UI-1](reviews/coverage-ui-transport.md#supported-omissions); high omission, detailed lifecycle is retained design. |
| C21 | **Confirmed launch agreement:** freeze a different current model/account from the resolution the user submitted as confirmed. | Extend launch/config resolution: reject a changed confirmed resolution and require fresh review, only where confirmation was supplied. | [Identity F5](reviews/coverage-identity.md#f5); medium, retained agent design plus current resolver. No universal confirmation step inferred. |
| C22 | **Account assignment continuity:** mutate account kind/scope beneath assignments or delete still-assigned accounts. | Extend provider accounts with stable identity/scope, compatible visible assignments and deletion prerequisites. | [Identity F6](reviews/coverage-identity.md#f6); medium, recovered design; original human request not located. Management authority needs reconciliation. |
| C23 | **Current operation authority:** honor old token roles after a completed revocation for operations beyond the specifically covered API-key/org-switch paths. | One shared admission commitment: resolve current membership/permissions and preserve relied-on authority through transactional mutation. | [Identity F7](reviews/coverage-identity.md#f7); medium-high precision gap. No immediate cancellation of all admitted effects/open sockets inferred. |

### Unresolved candidates, not silently adopted requirements

| Area | Required disposition before promotion |
| --- | --- |
| [Workspace budgets and usage](reviews/coverage-identity.md#f4) | Decide whether prior admission thresholds/period-wide accounting are retained, deferred or retired. Current settings and unused spend-limit helpers do not resolve scope or establish enforcement. Token visibility alone does not describe the previous cost-control outcome. |
| [Granular permission grants](reviews/coverage-identity.md#unresolved-policy-and-rejected-apparent-gaps) | Reconcile planned grant retirement, additive implementation and Ben's viewer ceiling. Do not restore the old union rule over that ceiling. |
| [Organization VM quota and occupied custody](reviews/coverage-runtime.md#secondary-incomplete-commitments--handoffs) | Recover the configured org-wide quota and starting/running/stopping occupancy contract if retained; no source justification yet for preserving a particular numeric default or fairness algorithm. |
| [Slot-size publication and compatibility](reviews/coverage-runtime.md#secondary-incomplete-commitments--handoffs) | Clarify immutable advertised sizing while custody is active and legacy-host placement eligibility; current equal CPU/RAM derivation is not the entire protocol contract. |
| [Home attention](reviews/coverage-ui-transport.md#additional-candidates-requiring-reconciliation) | Reconcile original inbox scope/audience and partial-page semantics before adding one projection commitment. Do not invent a second approval/question authority. |
| [Saved explainer templates](reviews/coverage-ui-transport.md#additional-candidates-requiring-reconciliation) | Recover original product scope/override policy; distinguish committed discovery from owner live edits. A route's stale “commits” comment is not authority. |

These are six additional discovery/decision areas, not six confirmed new blocks. Existing
questions about physical VM death, process isolation, invitation timing, Base publication
and viewer collaboration remain in [clarifications](clarifications.md). Their presence is
coverage of uncertainty; it is not missing intent merely because the choice is unresolved.

### What this audit deliberately does not count as missing intent

- Parent deletion cascade, review-copy merge, committed-branch reads, selected-file Save,
  filesystem ignores, preview stop-before-cleanup, cache credential scope and cloud custody
  already have governing intent. Their recorded code divergences remain in
  [code gaps](../../../docs/code_gaps.md); copying observed code into ontology would weaken it.
- Missing runtime tests, unmodeled interleavings and absent refinement proofs are assurance
  gaps. They must not inflate the missing-intent count or be treated as positive coverage.
- Exact SQL locks, retry constants, helper APIs, React store structure, parser algorithms
  and UI spacing are implementation freedom unless a source explicitly makes the outcome
  consequential. Human-chosen two-minute Stop and 16-turn defaults are different cases.
- Historical policies superseded by later decisions stay retired: retain-running after
  failed Stop, warm-root retention, prohibiting all provider creation on PR Update and
  mandatory new unsaved-loss UI are specifically excluded from recovery.

### Assessment and next amendment boundary

The previous pass was strong at preserving supplied statements and modeling selected
mechanisms, but incomplete at independently recovering product policy and local lifecycle
contracts. Metadata pointing at code/models did not fill those missing governing bodies.
Compression removed some consequential details along with redundancy. The skill already
requires the independent reconstruction exercise used here; changing the skill is not
necessary to acknowledge these findings.

The source-reading/report checkpoint is complete within the recorded ranges. The wider
uplift coverage checkpoint remains **open**: consequential omissions are known, candidates
need reconciliation and important owner paths remain unread. A future amendment should
first consolidate shared security/approval/lifecycle owners, then extend existing chat,
document, Git and UI owners, preserving provenance and exceptions. Each changed obligation
needs its own code/conformance disposition and appropriate fresh evidence; old TLA counts
cannot be reused as validation of new wording. No such amendment or new model campaign is
performed by this report.

## Historical investigation and model coverage

> Historical investigation: model counts, TLC outcomes and reproduction commands below
> describe [commit `38046b28`](https://github.com/G5Labs/G5-Lean/tree/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology). On October 15, 2026, the user requested
> removal of all TLA campaign artifacts from this branch. Source findings and unresolved
> gaps remain; the archived model evidence is not a current executable test suite.

The source denominator is **285 human statements in 17 source sections**, individually mapped with original hashes and explicit reviewed transformations in [source-map.json](source-map.json). Repository
breadth discovery also uses registered public/internal routes, API service owners,
host/dispatcher/isolation/cache crates, frontend pages/hooks, schema and infrastructure,
relevant current designs and merged history. Initial discovery counted 48 route source
files, 123 service files and 85 first-parent commits; these are inventory counts, not
inspected-file coverage. Direct ranges and fingerprints are recorded per investigation.
Imports, search results and commit subjects earn no code or human-intent coverage.

Step 4 established concern models and discrepancies. Step 5 iteration 1 independently
reconstructed missing contracts from implementation owners and primary merged records.
It added focused agent decisions under their feature owners; at that checkpoint the supplied
bodies remained verbatim. The later full review preserves human provenance and meaning while
normalizing exports, repairing links and consolidating repeated rules by reference.
Iteration 2 challenged the reconstruction with three fresh-context reviewers and closed
their supported ontology/model omissions. No implementation tests were created or executed. Finite TLC checks are not runtime
reproductions, full program proofs or a global coverage percentage.

| Capability / ontology owner | Inspected evidence and executable obligations | Disposition and assurance limits |
| --- | --- | --- |
| Product goals, terminology, ontology/harness | Ben source, authoring skill/mirrors, source and artifact validators | Qualitative goals and file organization use reading/structure checks. Natural sections and stable anchors replace redundant block headings. No new formal predicate invented for taste or usability. |
| Org/user identity, invitations and roles | Chat/identity: OAuthAttempt, Invitation, ViewerCeiling; org creation and grants | Adds login and invitation authority; invitation race and viewer ceiling remain gaps. Login-triggered versus immediate membership remains Q. Full tenant-schema/runtime audit not claimed. |
| Sessions, API keys and browser identity | RefreshAuthority, ClientAuth, root ApiKeyAdmission; settings/profile owners | Adds absolute renewal lifetime, token/org/current-scope boundaries and late-response continuity. Token admission does not make revocation atomic with in-flight effects; initial UI principal capture is not universal. |
| Preferences and notifications | UiPreferenceScope, Notification, all mention audience/savepoint branches | Private inbox and post-commit delivery recovered. Browser-global preferences and retired notification settings remain gaps. No eventual delivery/channel-choice implementation claim. |
| Workspace identity, owner, Base, transfer | WorkspaceAuthority, current-row owner/shared locks, org/member schema | Logical tenant/owner/epoch/pending/stop fences and legal transfer. Physical death is separate Q; Base Make Remote creation conflicts with later absolute no-push source. |
| Workspace fork and saved state | ForkManifest, DirtyFork, SavedRead, FlushProof; native restore/retention | Immutable complete cut, proof-owned adoption/cleanup, replay and restore provenance; Live capture requires source owner, while eligible Durable/Branch forks admit authorized readers. Public browsing of dirty recovery state remains GIT-GAP-5. Partial file restore/retention-pin concurrency and OS durability are explicit limits. |
| Workspace delete/review/stages/approvals | WorkspaceProducts, ApprovalRequest, ApprovalEligibility, Git PRRecovery; mutation and approval owners | Cascade/review merge differences remain. Recovered versioned approval requests, cut eligibility and compatible absent-details/pre-modern approvals; malformed modern evidence fails closed. Artifact producer execution and all reviewer membership writers are not proved. |
| Repository membership and durability | Git import/bootstrap/fork, registration, storage, hosted/selfhosted backup reads | Every workspace docs membership traced through creation/fork; repo removal discovery and backup dispositions in Git note. Retention/GC are not disaster recovery. |
| Git read/save/rebase/publication | SavedRead, RebaseBoundary, FlushProof, PRPublication; native/git owners | Separates live versus stopped authority, VM flush receipts and selected-file Save. Stopped browsing and Save staging/hash ABA are documented divergences. Git algorithms and transport/backend implementations are inspected premises, not verified DAG/filesystem implementations. |
| Provider binding, import, merge and CI | RemotePolicy, MakeRemote, ProviderImport, ExternalChecks, PRRecovery; merged original requests/designs | Durable accepted-cut recovery and check admission policy added. Automatic incomplete multi-repo import remains gap. Same-SHA check rerun is documented policy limit, not an invented effect-time defect. |
| Issues and PR conversations | IssueLinks, IssueRead, CommentRecovery; domain, author and conversation owners | Adds primary-race fallback, partial search and edit observation. Provider PATCH lacks external CAS; future Jira and broader issue proposals remain future work, not current capabilities. |
| Host admission, custody and retirement | HostAdmission, HostRotation, CloudLinkage, HostInventory, CloudRecovery; allocation/cloud/reconnect owners | Capacity, tenant, drain, complete/partial inventory, accepted-insert recovery and absence classification checked. Recovery/compensation and placement/rotation interleavings remain RUNTIME-007/008. Demand autoscale missing; physical cloud deletion, arbitrary replica fairness and all cloud failures remain operational limits. |
| VM identity, boot and lease | LeaseRenewal, StaleRelease, BootContract; guest/host/catalog/tool/image construction | Separate compatible immutable boot admission from actually ready guest. Lease checks include rollback and repeated-sequence expiry; stale notice settlement remains a gap. Logical custody does not establish physical singleton; per-request process interpretation remains Q. |
| Filesystem and watcher | FileVisibility FileMutationCAS, WatcherRecovery; RPC/native/file-save owners | Ignored-file mismatch remains. Expected-hash RPC guards do not exclude concurrent native writers. Watch recovery and concrete syscall behavior have separately stated bounds. |
| Terminal and preview | TerminalOwnership, TerminalReplay, PreviewLifecycle; terminal and browser security owners | Generation/replay contract added; unconfirmed preview termination and subsequent overlapping restart remain a gap. Browser origin protections have separate security model limited to one live API process; restart/multiple replicas and same-site residuals remain outside the proof. |
| Cache, isolation and artifacts | CacheAccounting, CacheTrust, CacheCredentialScope, BootContract; producer/IAM/config/mount reads | Cache lifetime added; credential scope mismatch recorded. Artifact verification premises and explicit development-mode Q remain. No hypervisor, IAM deployment or supply-chain proof. |
| Chat input, history, native requests and forks | ChatDelivery, TranscriptRetention, ProviderAuth, NativeRecovery, ChatFork; queue/stop/continue and attachment owners | Adds exact native restoration, independent fork history and ordered completed-output preservation after replay, interruption or failure. Module lifecycle/output owner contributes cancellation/retention checks; no universal scheduler fairness or provider comprehension claim. |
| Module execution and outputs | Dispatcher catalog/admission/slot/cancel/settlement and runner inspections; ModuleLifecycle | Execution identity, bounded slots, durable cancellation and retained outcomes distinct from chat transport. Actual process reap, every provider protocol and all malformed payloads remain runtime qualification. |
| Documents, references, projections, editor | DocumentReference, ProjectionConvergence, SaveAcknowledgment, SelectedSave; docs/files/ontology/grounding/history and React owners | Recovered canonical repo/path ownership, captured reference cuts, rebuildable indexes, partial protected-path progress before a complete checkpoint, and draft completion. Selected-file publication has explicit path/index/ref checks and a failed-push retry witness; its as-built checked-versus-staged byte gap remains open. Retired search/node-write mechanisms not revived. Native filesystem atomicity and every parser shape not proved. |
| Discussion, project questions and votes | CommentAuthority, QuestionDecision, QuestionOwnership, QuestionReceipt, QuestionPublication, QuestionVotes | Recovered owner/author/read exceptions, human-versus-agent answer, selected-file publication/replay, current-receipt idempotency (including valid A→B→A) and advisory votes. Votes do not follow Git forks or confer approval. Question publication inherits the Save defect and filesystem RPC/native distinction; its exact-publication property is conditional on that owner. |
| Workbench, guided work and attention | WorkbenchConfig, JourneyEvidence; workbench/chat admission, journey/home/template consumers | Adds committed configuration/revision/phase and outcome-based completion. Attention is a projection; saved template editing is not automatically publication. Layout never grants authority. Visual/accessibility E2E qualification remains unexecuted. |
| Platform reports, telemetry, quota, distribution | ReportDelivery, SharedQuota, TelemetryAdmission, LicenseAdmission; intake/network/license and maintenance owners | Recovered durable delivery/admission and license contracts. Report reclaim race remains gap. Spend helpers have no discovered production callers: no hard budget guarantee inferred from comments. |
| Secrets and security architecture | Encryption/config/API-key/hash, scoped transaction/RLS, redaction scanner/runner inspected; authority models above | Existing encryption/redaction intent retained. Cryptography, key management, full RLS coverage, all string/chunk combinations and deployed boundary tests are qualification limits, not fabricated TLA proofs. |

## Reconstruction challenges

The discovery pass asked what could be omitted while satisfying every original block.
It found project question sets/advisory votes, durable recipient inboxes, approval
version/cut semantics, native-answer recovery, attachment-preserving chat forks,
absolute session expiry, API-key ceilings, workbench revision/phase binding, guided
outcome evidence, draft-session save acknowledgment, projection repair, cache lifetime
and provision/retirement compensation. Agent D blocks now own these rules; details,
primary history, executable bindings and limits are in the domain notes. Existing
Ben rules already own most Git, preview, filesystem and module surfaces, so additional
model guards usually enrich metadata rather than creating a block per helper.

The fresh-context review found missing selected-file Save ownership, legacy approval
compatibility, completed-output preservation, current-receipt semantics, and runtime
interleavings. Iteration 2 corrected the owning contracts and model bindings, added
separate broken controls and reachable completion/retry witnesses, and retained the
resulting code gaps. Review reports preserve the initial findings and focused recheck
dispositions. Known implementation gaps and conditional policy questions coexist with
this completed reconstruction; neither is silently converted into a passing proof.
The checked obligations and explicit abstraction limits below define the assurance scope.

## Full-review coverage and remaining limits

The follow-up review gives an explicit disposition for **every one of the original 65
models**, with governing canonical IDs, concrete code links, assertions, fresh representative
executions and abstraction limits. Eleven additional models cover dirty-fork inheritance
and compare repairs using existing owners. There are now **76 models / 532 configurations**;
378 current configurations were freshly executed and 154 unchanged receipts were checked
against their current inputs. Of the 532, 101 are successful safety/conditional-liveness
runs; 431 intentionally seek named controls, witnesses, divergences or design/policy limits.
Those categories are not interchangeable coverage credit.

The canonical ontology has **343 blocks in 20 pages**: 285 human-provided statements,
52 agent-derived decisions and six agent questions. All human statements retain original
source hashes; 53 explicit editorial/reference transformations are recorded. The original
source file is unchanged. The [Ben comparison](../../../docs/ben_ontology_delta.md) lists
all additions and disputed or incomplete requirements. The [gap report](../../../docs/code_gaps.md)
adds minimal repair proposals and marks unmodeled or policy-dependent candidates.

Broad contract coverage does **not** establish a high instrumented code-coverage percentage.
The source fingerprints cover 212 files with individually recorded inspection ranges, not
212 fully inspected files. Formal coverage still omits important runtime boundaries:
physical process death, cache startup/same-key replacement, native filesystem atomicity,
all cloud retry/identity cases, real PostgreSQL schedules, browser parser behavior and full
provider translation. No application E2E, line/branch instrumentation, cross-model refinement
or deployment qualification was performed. The full reviews make these omissions actionable
rather than converting source inspection into proof:

- [Workspace/Git: all 27 original models and seven additions](reviews/workspace-git-full.md)
- [Runtime: all 17 models and eight gap candidates](reviews/runtime-full.md)
- [Identity: all 21 original models and four repair comparisons](reviews/identity-full.md)

## Evidence

- [Workspace/UI/docs/operations/security inspection](investigations/workspace.md)
- [Runtime inspection](investigations/runtime.md)
- [Git and provider inspection](investigations/git.md)
- [Chat and identity inspection](investigations/chat-identity.md)
- [Independent review and final milestone](uplift-review.md)
- [Formal campaign and reproduction](https://github.com/G5Labs/G5-Lean/blob/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology/README.md)
- [Code gaps](../../../docs/code_gaps.md) and [policy questions](clarifications.md)

Each note states actual direct ranges; partial reads do not earn whole-file credit.
Cross-model composition uses shared identity/version/authority assumptions reviewed in
those bindings. There is no machine-checked composition or global completeness theorem.
