# Workspace and Git independent review — step 5, iteration 2

> Historical investigation: model counts, TLC outcomes and reproduction commands below
> describe [commit `38046b28`](https://github.com/G5Labs/G5-Lean/tree/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology). On October 15, 2026, the user requested
> removal of all TLA campaign artifacts from this branch. Source findings and unresolved
> gaps remain; the archived model evidence is not a current executable test suite.

Reviewed checkpoint: `aacbe97059fb57a36e93a95fe9d2a8e8a7682f52` on
`ziniuw/ontology_update_based_on_ben`. Application baseline:
`20e393c02906459e442e23e3e7ab867aced8945d`. Findings and line references describe
that checkpoint, before any parent-agent adjudication or repairs. This reviewer
started with fresh context and the review assignment, without implementation history.

Scope: the workspace/Git raw concern models and binding notes, with deeper direct
source inspection of stopped file reads, selected-file Save, flush proof and native
restore, fork ownership receipts and cleanup, and automatic PR refresh. This is a
bounded review, not renewed whole-repository inspection. No application changes,
runtime tests, canonical ontology changes, model changes or Git operations were made.
Only this report was written; three existing TLC configurations were rerun with
temporary output paths outside the repository.

## Findings

### WG-1 — P1: recovered stopped-read contract overrides the preserved human contract

**Classification:** confirmed ontology contradiction and missing code-divergence
disposition; source-bound behavior, not a runtime reproduction.

The new `g5.git.saved-read` decision at
`.g5/ontology/software_architecture/git.g5.md:276–279` requires stopped browsing to
select a saved receipt, including dirty work. The retained human rules instead say
nonowners see the most recently committed **branch** state, and stopped owners see
the most recent Git state (`.g5/ontology/features/workspaces.g5.md:103–111`, supplied
source `docs/ben_new_ontology.md:73–74`). These are different observable bytes, not
two descriptions of the same durable cut. A flush snapshot commit is not the
workspace branch tip. Neither the gap ledger nor clarification index records this
conflict at the reviewed checkpoint.

The public implementation does use the dirty receipt: the common file-read facade
only diverts a running owner to live content, then sends all other actors through
`resolve_committed_snapshot` and reads the selected OID
(`backend/g5-api-routes/src/routes/workspaces.rs:2783–2796,2802–2844`). Its reader
selects a completed, unpruned receipt for stopped/failed VMs, maps each
`snapshot_oid`, and uses that vector when saved branch observations still match
(`backend/g5-api-core/src/services/git/workspace_reader.rs:1595–1617,1638–1667`).
The file-tree consumer independently has the same behavior
(`backend/g5-api-routes/src/routes/files.rs:272–283,338–346,409–439`). Thus an
authorized nonowner can receive dirty saved content after stop despite the unchanged
branch, while all current `SavedRead` safety checks pass.

`SavedRead.tla:14–18` encodes that implemented selector and `:27` explicitly makes
the dirty receipt a positive witness. `SavedRead.asbuilt.cfg:1–3` checks whole-source
selection, freshness after publication and running behavior; none checks the
human committed-branch visibility rule. The binding is useful as-built evidence,
but its successful witness does not authorize the new D to supersede Ben.

**Minimal fix:** preserve Ben's bodies verbatim; change the agent refinement so it
does not establish dirty browsing as governing intent. Record the observed
stopped-read divergence in the existing code-gap/evidence owners. Add a separately
named property/comparator for the governing committed-read contract (including
nonowner and stopped-owner cases), keeping the existing whole-vector and recovery
checks. Do not repair the application or weaken recovery of dirty work in this uplift.

### WG-2 — P2: selected-file Save has no sufficient recovered owner, and flush completion is overbroad

**Classification:** consequential reconstruction and model-coverage gap, with an
overbroad agent-authored statement. This is not evidence that selected-file Save
itself is absent.

`g5.git.flush-proof` says **“Save completion”** requires proof of every repository's
retained branch/snapshot commits
(`.g5/ontology/software_architecture/git.g5.md:281–284`). That describes the separate
workspace flush protocol (`backend/g5-api-core/src/services/git/workspace_flush_service.rs:186–259,329–407`),
not editor Save. The document contract describes draft acknowledgment
(`.g5/ontology/features/documents.g5.md:21–24`) and mentions “selected-file Save” in
the question workflow (`:47–50`), but does not own what that operation must preserve.
`SaveAcknowledgment.tla:8–22` only models browser submitted bytes/session completion;
`FlushProof.tla:7–23` models complete-repository receipt/restoration;
`PRPublication.tla:9–13` models whole-source preparation. None covers the backend
selected-file operation. An implementation satisfying these contracts could commit
other dirty files during editor Save or lose the distinction between applied bytes
and failed publication.

The source has a clear reusable operation: `workspace_file_save.rs:316–395` performs
the observed-file/VM write, accepts an identical-content conflict retry without a
second write, and distinguishes `Applied` from `Published`. Its checkpoint groups
only selected paths with expected blob hashes, obtains the committed head, and
pushes that exact head (`:268–311`). This is called by the public editor Save
(`backend/g5-api-routes/src/routes/workspaces.rs:2881–2909`) and by question flows.
It is important shared behavior, not incidental code narration.

**Minimal fix:** scope the receipt statement explicitly to workspace flush
completion. Recover one concise owner for selected-file Save, and reference it from
draft/question consumers. Add a focused source-bound model covering submitted blob
selection, unrelated dirty-file exclusion, exact returned-head publication, applied
versus published failure and identical-content retry. Preserve the boundary that
publication can fail after the live write succeeds.

Do not infer continuous incarnation pinning from the existing precheck:
`workspace_file_save.rs:411–419` validates and drops an expected owner handle;
`workspace_runtime.rs:1001–1027,1094–1112` independently reacquires the current handle
for commit and push. The PR preparation path explicitly does retain one incarnation
(`workspace_runtime.rs:1115–1118`). Any new Save model must reflect those boundaries
or name the resulting qualification, not put the stronger conclusion in a guard.

### WG-3 — P2: fork receipt ownership is not challenged by the existing cleanup control

**Classification:** model sensitivity/coverage gap; the inspected implementation
has the relevant protection.

The recovered formal fork rule specifically says equal branch content without the
operation's ownership proof cannot be adopted or cleaned up
(`.g5/ontology/features/workspaces.g5.md:116–119`). `ForkManifest` never constructs
that case. `Init` has no destination refs (`ForkManifest.tla:8–9`), `Materialize`
writes the branch and matching proof together (`:14–17`), and `ForeignChange`
only changes a ref to `3` (`:31–33`) while admitted cut values are `1` or `2`.
Consequently every reachable nonzero ref equal to its cut also already has the
matching proof. Removing only the proof check in `Cleanup:35` would not alter its
enabled states in the checked model. The same missing state prevents checking
adoption/publication of an equal-content unowned destination.

`ForkManifest.cleanup.cfg` removes both branch/proof conditions together. Its
counterexample is a foreign hash `3` versus planned hash `1`, so a branch-hash guard
alone would detect it. This supports changed-ref preservation but not the distinct
receipt-ownership obligation. The binding note's “proof-owned cleanup” coverage
needs that qualification.

The actual owner deliberately handles the missing case: fork creation accepts only
both refs absent or both exactly matching; equal branch with no receipt is a
conflict (`backend/g5-scm/src/repo.rs:345–387`). Cleanup explicitly preserves a
branch with no receipt (`:639–675`). Service preflight also verifies the operation's
proof rather than accepting an equal destination
(`backend/g5-api-core/src/services/workspace/workspace_fork_service.rs:1510–1538`).

**Minimal fix:** represent a destination collision whose branch equals the admitted
cut but whose operation proof is absent/foreign, independently of changed-content
collisions. Check refusal/preservation with separate broken proof-only controls for
adoption and cleanup, plus the legitimate owned replay/completion witness. Retain
the existing changed-hash, frozen-vector and partial-publication checks. This needs
no application change or new production mechanism.

## Additional source challenge from parent: selected Save capture/hash separation

After the findings above were recorded, the parent requested an independent check
of a suspected selected-file checkpoint race. The parent supplied the hypothesis;
this is corroboration rather than a claim of independent discovery.

**Confirmed source-bound concurrency defect, without runtime reproduction.** In
`backend/g5-scm/src/native/member/checkpoint.rs:143–145`, `git add` captures selected
working-tree content in a private index. At `:149–166`, the function reads that
staged entry but validates the expected hash using a separate later
`hash-object --no-filters` of the live path. It never ties the checked raw bytes to
the already captured staged entry. `:249–255` then writes and commits the private
index tree. The native caller passes `expected_blobs` through unchanged
(`backend/g5d-dispatch/src/modules/builtin.rs:1122–1139`;
`checkpoint.rs:359–373`).

A regular file with no Git filters is sufficient: editor Save writes A and returns
its hash; another native/editor/shell writer changes it to B; selected `git add`
captures B; that writer changes the path back to A; the later raw hash matches the
expected A and the operation commits B. The exact returned head is then pushed by
`workspace_file_save.rs:299–311`, so precise head publication does not repair the
earlier content mismatch. The index lock is acquired only at `checkpoint.rs:175`,
after both observations, and protects Git index writers rather than ordinary file
content writers (`:63–85`). Branch/HEAD checks and CAS likewise do not exclude this
schedule.

The file-save owner explicitly describes committing exactly the observed/written
blob (`workspace_file_save.rs:25–26,268–270,316–321`). Git attributes explain why
raw hashes differ from filtered blob hashes but do not make the two observations
one capture. The proposed as-built model must retain independent staged bytes and
later checked bytes with an intervening writer. Any same-capture comparison is a
separately labeled hypothetical repair, not a guard present in the current code.
Record the divergence and proposed discriminating runtime schedule; no application
repair or new runtime test was performed in this review.

## Other reviewed dispositions and limits

- `WorkspaceAuthority` scopes its logical authority/transfer proof separately from
  physical VM termination; no new physical singleton conclusion is warranted here.
- `WorkspaceProducts` correctly retains cascade and optional review merge as
  implementation gaps rather than converting current refusal into intent.
- `ProviderImport` distinguishes atomic internal publication from incomplete
  selected inputs. The documented GIT-GAP-1 is not contradicted by atomic landing;
  this pass did not reproduce its worker schedule at runtime.
- `ExternalChecks` distinguishes admission-time observations from a stronger
  effect-time comparator. The documented GIT-GAP-2 reconciliation is appropriately
  scoped; the stronger comparator is not a shipped guarantee.
- PR refresh directly locks the PR, checks provider acceptance and rechecks source
  publication before recording completion
  (`workspace_pull_request_create.rs:804–865`). `PRRecovery` explicitly excludes
  unmanaged provider writes; no additional defect is established by this pass.
- The native restore owner keeps supplied local HEAD and dirty snapshot separate
  from newer fetched tracking state (`backend/g5-scm/src/native/restore.rs:142–234`).
  This supports the dirty-recovery refinement, independently of the browsing
  conflict in WG-1. Receipt pruning/pinning, filesystem fidelity, all Git DAG
  algorithms and actual disaster restore remain stated assurance limits.
- Raw remaining workspace/Git models were read for their guards, frames and
  controls, but their entire bound source inventories were not reread here. No
  whole-feature assurance is inferred from those model reads.

## Fresh selected execution evidence

Pinned jar SHA-256:
`936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88`.
Executed with `java -cp /tmp/g5-ben-tla2tools-1.7.4.jar tlc2.TLC -workers 1
-metadir TEMP -config CASE.cfg MODULE.tla`, in the respective concern directory.
The temporary logs are under
`/var/folders/6m/hcldwlsj4k72ml4b0s9d5fqr0000gn/T/g5-workspace-git-review-y7636fhq/`;
reviewed campaign logs were not overwritten.

| Configuration | Observed result | Generated / distinct states |
| --- | --- | --- |
| `ForkManifest.safe` | Exit 0, no error | 891 / 382 |
| `ForkManifest.cleanup` | Exit 12, exact `ForeignRefsPreserved` violation | 121 / 74 |
| `SavedRead.witness` | Exit 12, exact `NeverSavedRead` violation | 9 / 8 |

Formal-input SHA-256 receipts:

- `ForkManifest.tla`: `11862928b63c5449c39bcbf5f6274959e5b0da93b7ed569137ec9e2328908c6f`
- `ForkManifest.safe.cfg`: `12dec1af0a8a43f610127e9760f134cf23abb6f1794499a2fcec2a8a5174df1c`
- `ForkManifest.cleanup.cfg`: `9b85bce7b15af02fb258132f24365b65b15de09291cc97201ee0def21e3ab7d1`
- `SavedRead.tla`: `cd1ea80e8b720d8fdbde143055f828451b5e6e569fb375ce9d3edb829ead1125`
- `SavedRead.witness.cfg`: `f0f25a8c632eec136cc3289247728c0931011eb34c77197804815ff755e24e29`

The expected failures remain controls/reachability evidence, not safety passes or
runtime reproductions. These reruns do not close the additional properties requested
in WG-1–WG-3.

## Focused recheck of the first corrections

This addendum preserves the initial findings above. It reviews the parent’s working
changes after checkpoint `aacbe970`, not a new application revision. No broad
campaign, runtime test or application change was performed. The selected current
model/config hashes were compared with their saved execution receipts; all matched.
Validating receipts here is not an independent TLC rerun.

- **WG-1 resolved at ontology/evidence level.** The saved-read D now explicitly
  preserves the owner/nonowner branch-view rules and scopes dirty retained state to
  recovery. GIT-GAP-5 records the implementation divergence. `SavedRead.current`
  checks `CommittedBrowsing` against the as-built selector and fails as expected;
  `SavedRead.branch` checks the intended branch selector and passes. The existing
  snapshot witness remains classified as implemented reachability. No application
  fix is implied. The absent live-owner path is explicitly outside this selector
  model; the modeled fallback applies to nonowners and stopped owners.
- **WG-2 contract/scope correction resolved; one narrow coverage follow-up remains.**
  The flush statement now names VM receipt completion, and
  `g5.documents.selected-save` owns exact selected content, unrelated staged work,
  applied/publication distinction and equal-content retry. The metadata correctly
  avoids claiming a continuously held VM handle. SelectedSave covers the newly
  investigated checkpoint boundary, but its `RetryUnreachable` predicate at line
  37 is unused and its completion witness does not require `PushFailure`. It is
  equivalent to normal completion under the intended mode. Add a witness that
  requires a recorded push failure before successful continuation, or explicitly
  leave retry as inspected rather than model-verified behavior.
- **SelectedSave ABA correction accepted.** `NativeEdit`, `Stage`, and `Check` keep
  staged bytes independent of the later raw-file observation. The retained as-built
  trace stages B, restores A, admits the raw hash, and commits B; this matches the
  native source already corroborated above. The intended `VerifyCapturedBlob`
  variant is clearly hypothetical. Source-head, path-scope and unrelated-index
  controls exercise distinct assertions. Git filtering, all-writer file CAS,
  incarnation continuity and runtime reproduction remain appropriately qualified.
- **WG-3 cleanup half resolved; publication/adoption half remains open.** The new
  `ForeignSameContent` transition reaches a planned-hash branch with missing proof,
  and `ForkManifest.proof` removes only cleanup’s proof predicate. Its trace now
  detects the distinct missing-proof condition. However, `Publish:21–24` still
  assumes the proof equality while no asserted invariant records ownership of
  published refs. Removing only that conjunct would let an equal-content foreign
  branch be published while `ExactPublishedCut`, `StableMaterialization` and
  `ForeignRefsPreserved` all hold. Add a published-ownership assertion and a separate
  proof-only publication control. The implementation remains correctly protected;
  this is the remaining sensitivity gap from the original finding, not a new code
  defect.

Matched saved receipts at this recheck:

| Concern | Current executed evidence inspected |
| --- | --- |
| SavedRead | All six configurations matched input hashes; `current` fails `CommittedBrowsing` at 9 generated / 8 distinct states; `branch` passes at 37 / 16. |
| ForkManifest | All seven configurations matched input hashes; safe passes at 1,806 / 634; proof-only cleanup fails `ForeignRefsPreserved` at 209 / 115. |
| SelectedSave | All seven configurations matched input hashes; intended passes at 86 / 60; as-built fails `ExactSavedBlob` at 60 / 47; named scope, index, ref controls and completion/conflict witnesses have their expected results. |

## Final focused disposition after the two remaining controls

The parent’s subsequent corrections close both narrow model-coverage follow-ups.
This final check inspected the current affected models, configurations, complete new
traces, the investigation addendum and all 16 affected execution receipts. All
model/config input hashes match; every saved log has its specified successful exit
or exact named counterexample. No further TLC or runtime tests were run by this
reviewer during this check.

- **WG-3 resolved within the checked bounds.** `PublishedRefsOwned` is asserted in
  the positive and fair configurations. The new `publish-proof` control bypasses
  only publication’s proof condition, preserving exact branch content. Its trace
  materializes one owned member, introduces an equal-content unowned second member,
  and violates `PublishedRefsOwned` when publishing (173 generated / 103 distinct
  states). This independently checks adoption as well as the already corrected
  proof-only cleanup control. The safe case still passes at 1,806 / 634; fair
  completion keeps its explicitly collision-free environment premise.
- **WG-2 retry coverage follow-up resolved within the abstraction.** Only
  `PushFailure` sets `sawPushFailure`, and the retry witness requires that monitor
  together with a successfully published A. The retained trace explicitly traverses
  failed push then successful push without a second commit (69 / 50). It is now
  distinct from ordinary completion. This establishes finite retry reachability at
  the model’s post-write checkpoint/publication boundary, not a runtime proof of
  every public Save retry or VM replacement schedule.

WG-1 and the WG-2 contract/scope corrections remain resolved as recorded above.
The SelectedSave as-built ABA counterexample remains correctly classified and the
intended comparator remains hypothetical. No additional material issue remains in
this reviewer’s bounded ontology/model findings. **GIT-GAP-5 and
WORKSPACE-SAVE-BLOB remain application gaps**, with runtime reproductions and repairs
outside this uplift; closing this review does not close those gaps or enlarge the
model assurance beyond its stated boundaries.
