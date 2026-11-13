# Workspace, review, Git and documents: bounded quantitative audit

Baseline: `072a12d3e99032ee3a7356837542c5edeb825984`. Only this report and [workspace.json](workspace.json) are produced by this worker. No source, ontology, model, receipt, skill or code-gap changes; no runtime tests or TLC runs.

**Denominator: 17 selected consequential behavior trajectories**, chosen from actual boundary owners and recovery paths. Within that sample, 14 have adequate current intent, one has a partial contract, one is missing a recovered capability, and one needs an authority decision. Nine selected paths were traced to the stated boundary; eight were only partly traced. These are inventories, not whole-system coverage percentages or test/proof scores. A body may adequately govern behavior that the code violates.

The baseline advanced from rebased `0b50e8d8` to `072a12d3` during inspection; that diff changes documentation/reference validation, not the backend sources or canonical bodies assessed here. A concurrent task is archiving the earlier formal harness. Formal entries below refer to immutable historical inputs/results available through Git at `072a12d3`; they do not assert a runnable checkout or current evidence freshness. The parent assessment owns source/model freshness.

## Selected obligation inventory

“Traced” means the named guards, effect boundary and relevant handoff/recovery branch were read sufficiently for the narrow conclusion. It does not mean every callee, competing writer or runtime interleaving was verified. Exact source ranges, source hashes, IDs, formal predicates and exclusions are in the JSON.

| ID | Selected behavior | Intent | Enforcement inspection | Governing owner / important limit |
|---|---|---|---|---|
| WG01 | Workspace read audience and current-owner audience administration | adequate | partial | audience + ancestor-read; direct-user predicate is not the whole inherited capability |
| WG02 | Reversible content read-only transition, drain admission and final quiescence | adequate | partial | content-policy + transition; all task producers not reread |
| WG03 | Fork retries retain one captured source operation and publish only materialized member refs | adequate | partial | fork-state.formal; fresh capture/cleanup internals remain partial |
| WG04 | Owner deletion stops compute and removes a workspace with governing no-orphan cascade semantics | adequate | traced | delete; live-child refusal is an existing code mismatch, not missing intent |
| WG05 | Review starts in a reader-owned child, can resume that child, and retains a created copy if compute start fails | adequate | partial | review + fork + start; unique reuse is not promoted as required policy |
| WG06 | Successive review rounds supersede open snapshot work while preserving valid proposal history | partial | traced | review-decisions lacks cross-round identity/supersession — WG-P01 |
| WG07 | Reviewer can discard selected previewed changes from their own copy without overwriting later work or changing the source | missing | traced | review + file mutation do not require selected discard — WG-P02 |
| WG08 | A pushed Approve review verdict can settle listed formal approvals, including a requestless path | question | partial | approval authority lacks resolution of alternate requestless writer — WG-P03 |
| WG09 | Eligible readers publish additive file suggestions with detected-stale confirmation and no source write | adequate | traced | reader-suggestions; observed staleness only, viewer Q and branch-read gap retained |
| WG10 | Selected review acceptance preserves applied truth across failed publication and interrupted settlement | adequate | traced | review-acceptance; known guest-write/SQL-settlement gap retained |
| WG11 | Update keeps the G5 PR identity; known terminal provider PR replacement requires explicit Create replacement | adequate | traced | pr-write-identity; recorded merge recovery differs from explicit replacement |
| WG12 | Provider merge recovery imports accepted pinned source history without silently accepting a changed target history | adequate | partial | pr-recovery-cut; downstream/alternate import writers not retraced |
| WG13 | Issue creation handles unknown provider outcomes and separate parent attachment without losing an already-created issue | adequate | traced | creation/recovery/parent already present after rebase |
| WG14 | Document identity and references use one captured repository view | adequate | partial | captured-references; capture/handoff checked, resolver not recursively verified |
| WG15 | Projection convergence repairs missed hints without checkpointing incomplete or stale canonical content | adequate | traced | projection-recovery; checkpoint/failure boundary traced |
| WG16 | Question answers preserve revision/receipt identity and distinguish applied answers from published files | adequate | partial | question-decisions + selected-save; author transition helper not reread |
| WG17 | Reader question votes remain personal advisory records bound to the resolved question revision | adequate | traced | question-votes; existing viewer Q remains, no latest-at-commit promise |

## Proposed missing intent only

These three proposals are not applied. The first two recover agent design supported by original merged records; they are not recast as supplied human I. The third deliberately leaves authority unresolved.

### WG-P01: g5.workspace.review-rounds

**D addition or concise extension; medium.** Owner: `.g5/ontology/features/workspaces.g5.md`. Confidence: medium-high recovered agent design; no new human policy inferred.

review-decisions retains history across displays and prevents stale reopening of one suggestion. It does not define identity across later round rows, retirement of old snapshot work, or unchanged-versus-changed proposal carry-forward. Reader suggestions owns additive annotations, not replacing copy rounds. A replacement can satisfy both existing blocks while repush loses or wrongly duplicates decisions.

> Review copy pushes record committed work as successive rounds. A new round supersedes the same reviewer’s earlier open changes and findings from the same kind of review, while standalone annotations and persistent comments/artifacts remain. Continue discussion without merging distinct proposals; carry a decision and unchanged note only to the same proposal at its evidenced occurrence and content. Changed proposals require a fresh decision.

Evidence:
- `.g5/ontology-plan-comments.md`:205–213, 242–249, 285–295: Saved agent design explicitly recovers threads carrying over rounds from PR170 and identifies supersession notification semantics.
- Original commit `2ba49163`: Unchanged proposals retain decision rather than reopen.
- Original commit `a8596de1`: Same occurrence is required, not merely equal before/after text.
- Original commit `ac3ea1d2`: Exact predecessor reservation avoids duplicate edit stealing another decision.
- Original commit `1469f0ea`: Changed artifact bytes require a new decision while thread continues.

Direct current source: `backend/g5-api-routes/src/routes/workspace_reviews.rs`:675–955,2966–3100; `backend/g5-api-core/src/services/workspace/workspace_review_service.rs`:1513–1658,2227–2515,2964–3145.

Limits: Exact anchor algorithms, round-number SQL and specific kind enums are mechanisms, not proposed ontology. Do not convert ordinary review into compulsory merge or sealed result workflow.

### WG-P02: g5.workspace.review-discard

**D addition; medium.** Owner: `.g5/ontology/features/workspaces.g5.md`. Confidence: medium recovered product behavior from original merged records.

Review/fork isolation says changes do not alter the parent and file mutation governs safe writes. Neither requires the reviewer’s selected change-discard capability or its preview-bound selection/partial outcome. A replacement may preserve all current contracts while leaving rejected changes permanently in the copy.

> A reviewer may discard selected previewed changes from their own review copy without changing the source. Refuse stale selections and preserve files changed since the preview instead of reversing at a guessed location. Report completed and skipped changes separately; one failure does not conceal changes already discarded. File mutation remains governed by the shared VM mutation rules.

Evidence:
- Original commit `11c2310d`: Original rationale: rejected changes otherwise stay forever in Changes to push; adds Discard from my copy and changes nothing on source W.
- Original commit `f43da016`: Discard must refuse files changed since the saved preview.
- Original commit `9851d021`: Artifact content belongs to change identity; changed artifact needs a fresh preview.

Direct current source: `backend/g5-api-routes/src/routes/workspace_reviews.rs`:3290–3691.

Limits: Low-level hash checks are already governed; no new physical immutability or whole-request atomicity promise. Original commits are recovered agent design, not quoted human instruction; current supplied review intent remains authoritative.

### WG-P03: g5.workspace.approval-review-verdict-question

**Q addition; high decision needed before recovery as D.** Owner: `.g5/ontology/features/workspaces.g5.md`. Confidence: high that the authority boundary is unresolved in inspected records; no conclusion which policy to choose.

The human-authority block grants addressed pending-request settlement, and cut eligibility defines which evidence counts. Neither authorizes this alternate requestless writer. Earlier issue278 human H2 retains owner-only requestless direct decisions, while its reviews explicitly leave review-sync exception and withdrawal unresolved. Adding a broad D based on the existing service would silently choose policy.

> May a listed reviewer’s Approve verdict on their own review round record a formal approval without an eligible pending request, and how may that reviewer withdraw it? If this separate path is retained, define its exact reviewed-cut/current-read authority and per-phase partial settlement; request-changes/comment verdicts do not implicitly reject formal approval, and a sync failure does not undo the published round.

Evidence:
- `.g5/ontology-review-issue-278.inputs.md`:7–22: Recorded human H1/H2 distinguish addressed requests from requestless direct decisions.
- `.g5/ontology-review-issue-278.r3.md`:25–25, 89–101: Prior review explicitly asks to decide sync exception and withdrawal; not itself human resolution.
- Original commit `11c2310d`: Initial Approve-to-formal-approval sync rationale.
- Original commit `f43da016`: Exact current source cut and per-gate settlement.
- Original commit `598b13ba`: Copy must include source and an invalidated request is not reused.

Direct current source: `backend/g5-api-routes/src/routes/workspace_reviews.rs`:915–946; `backend/g5-api-core/src/services/workspace/review_approval_sync.rs`:1–460.

Limits: No newer human resolution was found, including check with identity worker. The full shared settlement writer and Undo consumer were not re-audited; question is about authority, not a new verified exploit.

## Historical formal associations

Only five selected trajectories map to properties I checked in the earlier workspace campaign. That campaign had 40 expected outcomes at its earlier baseline, including intended safety, discriminating negative controls, reachability witnesses and one as-built divergence. It does not cover the new review-round/discard/approval-authority proposals. No model is credited merely because a source symbol appears in a binding.

| Trajectory | Exact historical model/property | Material scope |
|---|---|---|
| WG01 | `WorkspaceAudience.NoDisclosure` (`audience-safety`); `WorkspaceAudience.OwnerAudienceControl` (`audience-safety`); `WorkspaceAudience.AncestorAvailable` (`audience-ancestor-boundary`) | Child-origin authority is an explicit capability assumption, not proof of every parent read consumer. |
| WG02 | `ContentPolicy.NoLostWork` (`policy-safety`); `ContentPolicy.QuiescentReadOnly` (`policy-safety`); `ContentPolicy.NoRestoreWitness` (`policy-restore-witness`) | Two admitted jobs and one draft; all producers/physical files remain outside. |
| WG09 | `ReaderSuggestions.NoSourceWrite` (`reader-safety`); `ReaderSuggestions.Additive` (`reader-safety`); `ReaderSuggestions.DetectedStaleConfirmed` (`reader-safety`); `ReaderSuggestions.NoReadCommitRaceWitness` (`reader-observation-boundary`) | Two pushes and source versions; stale observation is not latest-at-commit atomicity. |
| WG10 | `ReviewAcceptance.NoFalseRejection` (`review-asbuilt-crash`); `ReviewAcceptance.AppliedAtMostOnce` (`review-intended`); `ReviewAcceptance.NoRecoveredCompletion` (`review-recovery-witness`) | Applied-write/SQL crash divergence plus explicit Accept retry witness; not hunk/Save-byte proof. |
| WG11 | `PullRequestIdentity.UpdateNeverCreates` (`pr-safety`); `PullRequestIdentity.NoSilentReplacement` (`pr-safety`); `PullRequestIdentity.NoInitialRecoveryWitness` (`pr-first-provider-witness`) | One repository; excludes provider receipt loss, accepted-merge recovery and automatic after-push. |

## Rejected candidates and known mismatches

- **Private audience, content read-only, reader suggestions, review acceptance interruption, composer draft loss and explicit provider replacement.** Already present in current canonical bodies; no duplicate additions.
- **Deletion refuses children instead of cascading.** Known code disagreement with supplied delete I; do not recover the refusal as intent.
- **Direct-user access predicate as the entire ancestor-read contract.** Would weaken supplied child ancestor authority. Direct parent-comparison helper is only one consumer; retain conditional gap rather than inventing a universal grant/denial.
- **Review copy cannot merge/transfer or be nested.** Code restrictions alone cannot override supplied ordinary-review/merge intent; no recovered prohibition proposed.
- **Discard low-level compare-and-swap as its own safety owner.** Existing VM mutation contract already owns it. WG-P02 adds selected review behavior only.
- **Freshest-at-commit stale checks for reader proposals or question votes.** Code checks a captured authorized observation; no original instruction found requiring an atomic latest-head transaction.
- **Issue exactly-once create or new receipt ledger.** Current canonical creation recovery explicitly accepts bounded-lookup duplicates and requires stateless markers.
- **Unique review copy or exact branch-name scheme.** Reuse/name identity is a mechanism under generic fork/review recovery unless a consequential user commitment is recovered.
- **Formal approval sync is simply missing D.** Existing original records leave its requestless exception/withdrawal open; preserve as Q rather than silently choose authority.

The accepted-file interruption gap, exact-source Save/native-writer limits, dirty-flush public-reader disagreement and partial accepted-provider import are enforcement concerns against existing bodies. This pass does not invent replacement intent to make those code paths conform. Existing source gaps are not newly reproduced runtime failures.

## Read ledger and remaining scope

Canonical bodies were read before searching implementations: workspaces1–187, documents1–89, repositories1–37, issues1–128, Git1–331; the affected organization viewer question and VM file-mutation body were then checked. Current-body search found no duplicate owner for round carry-forward, selected review-copy discard or requestless review-verdict settlement. Footers retain historical evidence, not substitute governing rules.

The JSON `inspected_ranges` is the exact newly read backend ledger, with current source-file hashes. These ranges include route entry points, service writers, publication consumers and recovery boundaries; mere `rg` function inventories are not counted as body reads. Original-record reads and commit-message evidence are separately identified.

| Directly read backend owner | Ranges |
|---|---|
| `backend/g5-api-core/src/db/scoped.rs` | 610–725 |
| `backend/g5-api-routes/src/routes/workspaces.rs` | 2110–2225, 3200–3350, 5368–5485 |
| `backend/g5-api-routes/src/routes/workspace_reviews.rs` | 513–955, 966–1180, 1396–1455, 1660–1807, 2222–2280, 2434–2480, 2966–3100, 3288–3691 |
| `backend/g5-api-core/src/services/workspace/workspace_review_service.rs` | 1513–1658, 2227–2515, 2964–3145 |
| `backend/g5-api-core/src/services/workspace/review_approval_sync.rs` | 1–460 |
| `backend/g5-api-core/src/services/workspace/workspace_fork_service.rs` | 463–515, 1124–1280, 1749–1865, 1930–2073 |
| `backend/g5-api-core/src/services/workspace/workspace_service.rs` | 2249–2540, 2610–2825, 2990–3105 |
| `backend/g5-api-core/src/services/identity/workspace_mutation_policy.rs` | 1–114, 247–341 |
| `backend/g5-api-core/src/services/git/workspace_pull_request_create.rs` | 395–558 |
| `backend/g5-api-core/src/services/git/workspace_pull_request_provider_lifecycle.rs` | 1–467, 615–715, 1045–1125 |
| `backend/g5-api-routes/src/routes/external_issues.rs` | 714–756, 788–1025 |
| `backend/g5-api-core/src/services/workspace/doc_service.rs` | 4372–4502, 5920–6130 |
| `backend/g5-api-core/src/services/workspace/projection_convergence.rs` | 1–290 |
| `backend/g5-api-routes/src/routes/questions.rs` | 296–321, 382–430, 450–515 |
| `backend/g5-api-core/src/services/workspace/question_set_service.rs` | 743–1120 |
| `backend/g5-api-core/src/services/workspace/question_vote_service.rs` | 187–365 |
| `backend/g5-api-core/src/services/git/workspace_repositories.rs` | 51–90 |

Important areas outside this bounded pass:

- Repository onboarding/removal implementation, remote deletion/GC/object closure and restore/backups.
- Full transfer implementation, all fork fresh-admission/capture/cleanup writers and all workspace ACL/RLS consumers.
- All automatic follow-push paths, external checks, PR conversation ledgers, remote binding and alternate accepted-merge import writers.
- Issue search/comment/link adapters and provider internals outside selected issue-creation path.
- Legacy Project Doc creation/command ledgers, all grounding/intent-history routes, parsers and ingestion consumers.
- Review source-favoring Update/rebase, every in-place agent round, artifact delivery/rendering, full edit composition and cross-file crash recovery.
- UI/browser behavior, runtime tests and new TLC execution.
