# Identity and ontology review — iteration 2

> Historical investigation: model counts, TLC outcomes and reproduction commands below
> describe [commit `38046b28`](https://github.com/G5Labs/G5-Lean/tree/38046b2854bb8bbd6f061e20ee9596d5d61102d0/.g5/TLA_verification/ben-ontology). On October 15, 2026, the user requested
> removal of all TLA campaign artifacts from this branch. Source findings and unresolved
> gaps remain; the archived model evidence is not a current executable test suite.

Fresh-context review of checkpoint `aacbe970`, against application baseline
`20e393c0`. Application code was read only. This review owns this report only;
it does not change policy or classify the findings below as reproduced runtime bugs.

## Findings

### 1. P2 — Preserve legacy approval eligibility in the recovered contract and model

**Locations:** `.g5/ontology/features/workspaces.g5.md:134–137`;
`.g5/TLA_verification/ben-ontology/chat-identity/ApprovalEligibility.tla:8–9,18–23,32`.

The recovered rule says all agent verdicts require valid matching run/artifact
evidence, and the model admits only human, v3 and v4 records. The real eligibility
owner intentionally has another supported branch: an agent status without details
returns its reviewed heads, as does a non-v3-shaped legacy record
(`approval_config_service.rs:2757–2765,2821–2835`). The checked source has a specific
`legacy_agent_eligibility_remains_reviewed_cut_based` test with schema-v2 evidence
and no artifact fields (`3250–3271`). `requirement_state:2550–2575` can count that
status toward approval. This is distinct from the already described legacy v3
publication-cut branch. Neither the ontology nor the model currently preserves it.

Consequently, a reconstruction following the new D block would invalidate retained
legacy approvals, and the finite evidence incorrectly appears to cover the whole
generation-sensitive eligibility contract. Ben's supplied statements do not require
removing this compatibility behavior. This is an extraction/model omission, not
evidence for introducing a new application restriction.

**Minimal supported fix:** qualify the evidence rule by the record's generation and
preserve the earlier reviewed-cut compatibility branch. Add a distinct legacy kind
to the model, with a reachable accepted legacy record on its reviewed cut, refusal
on another cut, and separate checks that malformed v3/v4-shaped evidence still fails
closed. Update the bound claim and rerun the affected campaign. If broader history
establishes that compatibility should be removed, record that separately rather than
silently treating the intended restriction as as-built evidence.

### 2. P2 — Add the missing completed-output preservation obligation before closing coverage

**Locations:** `.g5/TLA_verification/ben-ontology/chat-identity/ChatDelivery.tla:45–59`;
`.g5/uplift/ben-ontology/coverage.md:37–38`.

Ben's preserved `g5.chat.completed-output` requires ordered completed output to
remain visible after interruption or failure (`features/chat.g5.md:58–66`). The
only transcript-content state in the 19 chat/identity models is `finalText`:
`Finish` directly assigns the desired two-element sequence, and `Stop` frames it
unchanged. None of the five baseline invariants mentions `finalText`; it appears
only in the completion witness. There is no failing terminal outcome, independent
completed blocks, or assertion against dropping/reordering already completed output.
`ModuleLifecycle.OutcomeRetained` checks execution-outcome identity retention, not
the conversation transcript. It cannot supply this missing obligation.

The code has consequential behavior to bind: journal sequence admission and
message-final handling (`task_lifecycle.rs:1865–1980`), final/delta block reduction
and first-seen ordering (`3408–3520`), and terminal materialization/status handling
(`3554–3610`). These are not merely structural details. The earlier investigation
acknowledges incomplete transcript-content coverage, but the current coverage table
does not identify this specific required check as still unexecuted.

**Minimal supported fix:** extend the delivery concern or add a small transcript
concern with two independently completed blocks, replay, interruption/failure and
terminal projection. Check order and preservation of completed blocks, with controls
that drop or reorder a block and a successful interrupted/failed path retaining
prior output. Until executed, explicitly retain this behavioral coverage gap; do
not count the existing completion witness as the safety check. Keep the original
human statement unchanged.

### 3. P2 — Scope question idempotency to the retained receipt

**Locations:** `.g5/ontology/features/documents.g5.md:47–50`;
`.g5/TLA_verification/ben-ontology/chat-identity/QuestionPublication.tla:4–8,14–16,22–24`.

The new rule unconditionally rejects reuse of an idempotency key for different
content. Its implementation checks only the receipt currently stored on the
question (`question_set.rs:1159–1173`); a later answer replaces that receipt
(`1187–1202`). For one author, accepting key A, then key B, then key A with different
content and the now-current expected revision reaches `ReplayCheck::Fresh` on the
third command. Normal author and revision checks permit that third answer
(`question_set_service.rs:832–880`). No durable key-history ledger is consulted.
An old unchanged retry remains protected by its stale expected revision, which is
a different guarantee.

`QuestionPublication` has one answer and a Boolean receipt, so it never exercises
receipt replacement and cannot establish the unqualified key-reuse promise.
This recovered agent rule currently introduces broader behavior than the inspected
source; it should not silently require a new permanent deduplication store.

**Minimal supported fix:** describe equal/conflicting replay against the question's
retained receipt and separately preserve stale-revision refusal after later edits.
Add a bounded second-answer/receipt-replacement case to check those actual rules,
or retain that exact boundary as unmodeled pending its execution. A stronger
historical-key prohibition would need independent governing evidence and an explicit
implementation-gap disposition.

## Inspected scope and evidence

- Read all 19 chat/identity `.tla` modules, the relevant configurations/runners,
  the investigation and current coverage ledger. Inspected the canonical users,
  organizations, documents, UI, chat, workspace approval and VM execution/preview
  blocks, including their provenance and assurance notes.
- Direct source reads included question answer/replay/shape gates, approval request
  settlement and version/head/artifact eligibility, refresh resolution/minting,
  native-answer acceptance/restoration, journal input admission, assistant-output
  reduction and terminal materialization. This is a bounded correspondence review,
  not direct inspection of every source range in the earlier investigations.
- Source-preservation validator passed: all 285 human statements, stable IDs and
  human-provided lineage remain exact. Artifact validator passed: 338 blocks,
  460 generated links, 60 models and 379 retained receipts. The three chat/identity
  source manifests also match current file SHA-256 values: 28, 23 and 11 entries
  respectively, with overlap; these are fingerprint checks, not new inspection credit.
- The 135 saved chat/identity runs still match their formal input hashes. Four
  selected existing configurations were freshly executed in temporary output:
  ApprovalEligibility baseline passed (784 distinct states), its generation control
  failed `ExactCut` as expected (180), ChatDelivery baseline passed (80), and its
  completion witness reached `CompletionUnreachable` as expected (16). Logs are in
  `/var/folders/6m/hcldwlsj4k72ml4b0s9d5fqr0000gn/T/g5-identity-review-9snd3j7i`.
  These reruns confirm the current models' declared outcomes; they do not close
  the omitted behaviors above. No runtime tests or Git operations were performed.
- New blocks use readable natural sections and correct agent provenance. Existing
  source duplication is intentionally preserved. The actionable meaning problems
  are the omitted legacy exception and overbroad replay scope above; cosmetic
  rewriting of the original statements is unnecessary.

Existing invitation, viewer, UI preference and notification-setting gaps remain
properly distinguished from model passes. The RPC/native-writer composition limit,
process-local preview replay scope, preview shared-site residual, in-flight refresh
admission and optional initial browser-write principal guard remain material stated
limits. No contrary finding is asserted for those already-qualified boundaries.

## Focused independent recheck — all three findings closed

Rechecked the iteration-2 correction packet, affected canonical blocks, four affected
or new models and their bound source owners. The original findings above describe
the reviewed checkpoint and remain preserved; the dispositions below supersede their
open status. No remaining material issue was found within these three corrections.

1. **Legacy approval eligibility — closed.** The canonical approval rule now
   distinguishes absent/pre-modern details, assigned-cut current evidence and the
   publication-cut legacy generation. `ApprovalEligibility` includes the two earlier
   compatibility kinds, explicit acceptance of eligible legacy records, exact-cut
   checks and separately sensitive v3/v4 evidence controls. The model's kind and
   validity abstraction agrees with the reread branches in
   `approval_config_service.rs:2757–2840`, their requirement consumer at `2550–2580`
   and the legacy compatibility test at `3250–3271`. Field parsing and shape
   classification remain explicitly inspected boundaries, not newly claimed
   cryptographic or parser proofs.

2. **Completed-output preservation — closed within the stated finite scope.**
   `TranscriptRetention` now checks two independently completed blocks and a partial
   block against an accepted-journal oracle, preserving first-seen order and the
   first final for each explicit block. Terminal failure/interruption and replay
   have meaningful witnesses; six controls detect content loss, reordering,
   replacement/ignored finals, duplicate delta application and sequence gaps.
   Rereading journal admission, reduction, ordinary terminal materialization,
   VM-loss refresh and the canonical-row SQL updates supports the binding. The
   original human statement is unchanged and its metadata names the limited model
   scope. Legacy block-ID pairing, provider translation and browser rendering are
   correctly left outside this finite proof rather than silently counted as covered.

3. **Question receipt scope — closed.** The canonical text now promises replay and
   conflict refusal against the retained receipt, then current-revision admission
   for other commands. `QuestionReceipt` preserves the code's ordering of those
   checks and replaces the receipt after a fresh answer. Its A→B→A and stale-old-A
   witnesses exercise the missing boundary; the lifetime-history prohibition
   control fails the permissive fresh-command property. The unchanged implementation
   at `question_set.rs:1159–1205` and `question_set_service.rs:826–880` supports that
   meaning. `QuestionPublication` now clearly limits its receipt scope and qualifies
   exact publication by the separately tracked selected-file Save/native-writer gaps.

For this recheck, all **43** saved affected/new executions were checked against their
logs: five baseline passes, 24 expected broken-control failures and 14 reachability
witnesses. Exit codes, exact assertion names and recorded state counts match; all
48 formal-input fingerprints and six updated source-file fingerprints match current
files. The four chat/identity receipts contain 161 distinct configurations without
duplicate counting. Source preservation again passes for all 285 statements.
These are validated saved executions, not fresh TLC executions in this recheck.
No broad campaign, application test, application edit or Git operation was performed.
