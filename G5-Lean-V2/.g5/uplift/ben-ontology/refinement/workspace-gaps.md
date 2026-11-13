# Workspace coverage refinement: gap proposals

Baseline `832651d0bb3b1754eb9511d1cc5ccde7c2aafde4`. This file supplies proposals to the owner of `docs/code_gaps.md`; it does not change production code. Final campaign: 40 manifest cases executed with their expected outcomes; saved evidence revalidation is run separately. These are bounded property/witness/control results, not a coverage percentage.

## WORKSPACE-REVIEW-SETTLEMENT — applied review can remain rejectable after transaction failure

Proposed governing owner: `g5.workspace.review-acceptance`. `routes/workspace_reviews.rs:1396–1429` takes a transaction-scoped decision lock; `1671–1707` writes the guest file then checkpoints it; `1720–1807` records the accepted status and commits the SQL transaction. VM/file/Git effects do not roll back when this transaction fails or the API process dies. The still-open row can subsequently be rejected (`2222–2335`) or withdrawn; `workspace_review_service.rs:4466–4537` checks the SQL status only. Normal checkpoint errors are handled correctly by recording accepted plus pending; the gap is interruption/database failure before that record becomes durable. An explicit retry of Accept can recognize already-applied content but is not a fence against a competing Reject/Withdraw.

Required outcome: recover prior application before permitting a contrary decision; preserve applied/accepted versus unpublished separately. Do not claim a specific journal or recovery mechanism is already implemented. Exact-byte publication also inherits existing `WORKSPACE-SAVE-BLOB` and native-writer TOCTOU limits; those are not new findings.

## Conditional authority and source questions (not newly proven code defects)

- **Ancestor access.** Preserve `g5.workspace.ancestor-read` exactly. The direct human audience predicate in `db/scoped.rs:642–720` has no originating-child parameter. That is insufficient to prove that every ancestor read path violates the child authority, or that every human reader of a child automatically gains independent parent access. The model makes child-origin authority explicit and treats its omission as a conditional boundary control, not a demonstrated whole-system defect.
- **Viewer proposals.** `publish_file_suggestions` uses a reader scope, like replies; the existing `g5.organization.viewer-collaboration-question` must cover proposals as well as replies/votes. “Eligible reader” does not silently decide that question.
- **Read source.** The reader-proposal route uses the actor's authorized file reader, which can choose a retained flush in the current implementation. Canonical committed-branch selection remains binding; inherit the existing durable-read code gap rather than permit flush content here.
- **Read-only scope.** App/content read-only is not physical filesystem immutability or a Git-ref lock. Detailed class exceptions and drain behavior are recovered agent design with medium confidence; not all callers/admission paths have been audited. No automatic restart of a lost drain worker is promised.

## Executed counterexample and refinement

`review-asbuilt-crash` fails the exact `NoFalseRejection` predicate after a reachable VM application, transaction loss and later contrary SQL decision. The [trace](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/results/review-asbuilt-crash.log) is an as-built divergence, distinct from the hypothetical `doubleApply` control. The intended model passes applied-truth/no-double-application invariants; `review-recovery-witness` requires a new Accept Start after crash with the edit already applied, already-applied recognition, successful checkpoint and SQL Commit, while another edit remains open, and `review-pending-witness` reaches accepted-but-unpublished state. This demonstrates the necessary recovery condition but does not implement it.

The reader observation boundary and durable freezing after lost worker also have reached witnesses. They refine the contract's limits rather than create speculative implementation gaps. Full source/action/atomicity bindings and the unmodeled limits are in [workspace model notes](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md).

## Independent read-only platform model review

Reviewed root's PhaseState, DeploymentSecrets and BrowserTickets modules, manifests/configs, bindings, README and selected retained traces against the bound source on this baseline. Phase updates/events share a SQL transaction; fork phase capture is retained across delayed finish; managed secret bootstrap serializes creation and retries directory durability before export; the new-Org-subscription counterexample follows the actual claims-only branch. No substantive correction to those bounded conclusions was found. This is an independent source/model check, not runtime execution or broad subsystem verification.

Two concrete evidence corrections were sent to root and acknowledged as applied:

- `BrowserTickets.PresentOther` enumerates non-WS token purposes with WS audience values. Its purpose-control trace is a **synthetic signed-claim boundary probe**, not evidence those claim combinations are issued by current production constructors or a demonstrated runtime exploit. Root corrected the model comment and README qualification. The separate as-built Org admission trace uses an ordinary session-minted general ticket and is unaffected.
- The JWT binding ended at 220 even though the WS validator uses `preview_validation`, which inherits expiration settings via `validation.clone()` at 226–228. Root directly read 220–230 and extended the binding, then began a complete platform rerun. No platform files were edited by this worker; root owns the updated run result.

## Independent workspace witness correction

The identity reviewer found that the original recovery witness could take `Write → Checkpoint → Crash → Close`, which recovered the intended decision but did not demonstrate an Accept retry. Added historical `retryProgress` instrumentation and strengthened `NoRecoveredCompletion` to require explicit post-crash Accept, recognition, successful checkpoint and Commit. This is a model-evidence correction, not another production defect; the as-built `NoFalseRejection` divergence is unchanged. Also directly read provider lifecycle 615–685 and bound the dispatch from existing provider row to recorded/prior paths, as recommended. All workspace configurations and evidence receipts are rerun/revalidated after these changes. Root's integrated workspace proposals and relative-link repairs were left untouched.

The same cross-review identified an unexercised capability: ordinary read-only → read-write after a completed transition. Added the separate `RestoreEditing` action and `policy-restore-witness`, preserving the distinction from freeze cancellation/crash recovery. This extends bounded evidence for existing supported intent and is not a new source defect. Final campaign: 40 cases.
