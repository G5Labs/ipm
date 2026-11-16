# Independent source-derived review and thin boundary checks

> Baseline audit clarification: P `name_order`/`needs_first` constrain an assumed output ordering and show its incompatibility; they do not transcribe `CreateLockFileForModule` writing. That member receives no inherited representation credit from those predicates. [The49-row audit](inherited-representation-audit.json) establishes a100-member inherited representation lower bound, with missing proof/native distinctions preserved.

> Correction after source/model recheck: the exact-byte-revert gap concerns failed compile/load/reload outside sync (OUT/F-SYNC-1). A failed sync with a baseline sets `SyncFailed` and is retried even when bytes match the baseline, as `sync_witness_failed_sync_is_retried` proves. The original before-review snapshot is retained as history; the canonical clause and after answers now distinguish these paths.

> Integrator note after this review: the semantic-version builder subsequently added accepted union, hyphen and inline-AND parsing, checked by 37 model verification items and 2,130 native assertions. The reviewer's earlier ExpressionSet gap below describes the earlier revision and does not independently sign off that later delta. Malformed comparators and complete diagnostics remain open. Canonical transaction rollback scope is now a settled D rule; external compensation has a separate child Q.

The independent pass derived ten held-out cases from pinned product sources before reading
consolidated model predictions. `independent-source-cases.json` records the predictions and
source hashes. A second source-only file, `independent-additional-source-cases.json`, adds
ORAS challenge handling and composition of file report output with an invalid summary format.
These records preserve predictions, including uncertainty, rather than rewriting them to match runs.
The major new artifacts are `verus/thin_surfaces.rs`, its manifest, trust inventory and checker.
All work is under `.g5`; no product, inherited model, shared guide or git index was edited.

## Checked outcome

Run from the IPM checkout:

```
python3 .g5/Verus_verification/consolidated/verus/check_thin_surfaces.py
```

Native fixtures can be rerun without resetting the container with
`python3 .g5/Verus_verification/consolidated/verus/run_thin_native.py`.
The runner saves a fresh receipt and checks that the actual passing test executed.

The final model has 22 verified checks and no errors. Its executable input-driven decisions
agree with 24 selected native fields. Eight erased-distinction mutants are rejected by
Verus; seven also contradict a selected native field. The protected-directory clearing
mutant has no matching native branch in this extension and receives no native credit.
The checker stores exact commands, source/tool/library/receipt/fixture hashes, compiler
output and native differences in `verus/results/thin_surfaces.json`.

| Source-derived case | Independent native result | Formal property and limit |
|---|---|---|
| FileCopy before/after Activate | Immediate copy survives a later failing Invoke; deferred copy is never reached. Both loads return error, leave no module row and return to transaction level 0. | `copies_before`, `copies_after`, `rollback_preserves_filesystem`; successful deferred after-hook correspondence remains open. |
| FileCopy and rollback | Successful direct copy leaves the target after `TROLLBACK`; missing source returns error with no target. A database sentinel is gone in both runs. | `file_copy_observation`, `copied_file_survives_rollback`; OS copy outcomes and partial writes remain explicit environment inputs. |
| FileCopy Clean | KeepOnUninstall preserves the target; its false neighbor removes it. | `clean_removes`; P `uninstall.rs::clean_step` already represented this distinction. Reconfirmation is not new Verus coverage. |
| Lockfile versus rollback | Real `load -lock` leaves a lockfile for the correct module after a later Activate failure rolls back its module row. A direct nonempty-root writer leaves its file across rollback too. | `write_file`, `copied_file_survives_rollback`; file path and content are separate from database state. |
| Lockfile path | First normal load has an empty stored root. The resulting lockfile appears in process cwd, not the module root. Parsed JSON name equals the just-loaded module, so this is not merely a stale file check. | `lock_in_module_root`; this upgrades the prior OBS-L3 suspicion for these native fixtures, not every load mode. |
| Lockfile write error | A directory at the lockfile path causes an error message, yet `load -dev -lock` returns success and completes Activate. A writable neighbor returns success and writes the file. | `dependency_status`, `lock_written`; exported-byte completion is distinct from command success. |
| ORAS availability | Real adapter with loopback 200, 401 with challenge, headerless 401 and 403 yields true, true, false, false. | `oras_available`; availability proves neither credentials accepted nor artifact pull success. |
| Report composition | Passing tests with JSON file output plus an unknown summary format return error after writing JSON. Valid JSON summary neighbor returns success. | `report_result`, `report_error_can_keep_output`; detailed report-tree completeness and serialization are not proved. |
| Inspection | Stored name, external alias and uppercase alias all succeed; missing name fails. Selected record identity/version remain unchanged. | `inspection_choice`, `inspection_preserves_stores`; datatype collation, query completeness and detailed formatting remain environment/open scope. |
| History writer/reader interleaving | Reader sees unfinished row, a separate JOB deletes it, then Finalize and PersistToTwin both return success. Final reader sees no row after close. | Native-only challenge selects the non-recreation branch of parent's `persist_after_delete`; it refutes resurrection on this schedule, not every interleaving. Owner policy stays open. |

`independent-native-final/` contains a complete final native run, exact helper/fixture copies,
build/architecture and input hashes. `independent-native-lockfailure/` adds the swallowed
write-failure neighbor; `independent-native-inspection-case/` adds uppercase lookup. Earlier
native directories are retained as setup history. Do not count failed setup probes: the first
helper omitted FileCopy's constructor argument; a later fixture placed files under SourcesRoot
although FileCopy reads module root; initial test classes had an invalid assertion/include and
then the wrong package path. Final runs use correctly located fixtures and actually execute a
passing test and its assertions. The shared container was never reset or restarted.

## What inherited proofs already say

P `uninstall.rs::clean_step` and `RKind::Files` already distinguish keep, protected directory,
single file and shared-directory cleanup. P `dep_resolve.rs::{name_order,needs_first,
dep_witness_lockfile_name_order}` represent the lockfile ordering defect, but have no file
location, exception propagation or rollback store. P `sync_baseline.rs::{sync_state,sync_ok,
sync_lemma_sync}` already distinguish hash commit followed by a failed test from a compile
failure before commit. The held-out sync source case confirms that relationship and adds no
new proof. P `load_general.rs::{Kind,L2}` distinguishes process and instance crashes with
IRISTEMP loss; its original premises and platform receipts remain the evidence. There was no
independent whole-instance restart here. G/P module and history stores cannot express a
filesystem or external CPF configuration effect simply by adding another source citation.

The new manifest has ten scoped member bindings. Six original native-rejected controls
support DoCopy, OnBeforePhase, OnPhase, IsAvailable, ReportTestResults and ModuleSummary.Show;
the added lockfile failure control supports LoadDependencies. Historical union gains depend
on overlap with the inherited baseline and are computed centrally. OnPhase is already
inherited. Do not credit successful OnAfterPhase native execution or CPF merge correspondence.
No ObjectScript body was mechanically proved.

## Review of precision and eligibility

`independent-precision.json` records answers to all twenty precision questions from canonical
I/D/Q rules and child links before consulting historical score/reason columns. It is a snapshot
before the parent's subsequent ontology repairs. Among ten ontology rows, four were sufficient
or explicitly open, four were partial, and two missed essential conditions: exact-byte revert
on retry and first-invalid-key termination in ordered history filters. The ten model-layer
questions need actual symbols and theorem domains; ontology prose does not establish proof.
The parent has repaired the reported owner-rule omissions and should re-answer against the
new hashes, keeping this earlier snapshot as the before measurement.

An independent read of the final manifests and parent models found several honest, material
limits that must remain visible:

- `history.GetHistory` is bound only to predicate selection; ordering, limits and SQL-error
  results are explicitly open. `programmatic_global` is narrower than a complete SQL cursor
  environment, so its namespace/cursor premise is material.
- `history_display.py` includes constructors and rendering helpers whose own output is not
  defined by merely supplying `Row.action` or by a boolean display decision. The parent was
  asked to enumerate these missing per-claim representations rather than hide them in the
  generic uncovered-member count.
- Semver `ExpressionSet` is explicitly open; parsed range/expression algebra does not prove
  lexical union/inline-operator parsing. P's exact-range resolver theorem remains conditional.
- FileCopy path normalization, overlap/symlink handling, OS permissions, directory-chain
  creation and partial failure remain environmental or unmodeled beyond the selected paths.
- CPF external changes are represented as environment freedom and exit conversion, without
  a destructive native merge. Registry availability is one adapter distinction, not registry
  publish/pull/auth completeness. Mapping's inherited two-namespace premises are preserved.

There is no independent sign-off of all 1,216 product members or all 611 playbook intent rows.
The 10 bindings are scoped behavior edges, not claims that all behavior of each member is
represented. Parent coverage outputs must retain unresolved dispositions and critical gaps.

## Residual gaps by consequence

1. Destructive history filtering and delete/finalize: ordered parser cases are now represented,
   but all interleavings, access rights and late object-save errors are not proved. The tested
   concurrent schedule demonstrates delete permanence without action failure.
2. Mixed journaling and crash persistence: inherited process/restart premises, repeated writes
   to the same key, concurrent writers and filesystem effects need separate contracts. A
   disjoint-write prefix must not be presented as a general overwrite theorem.
3. Registry pull/publish, package digest and retry: availability alone is insufficient for
   dependency provenance or publishing authorization. Native protocol fixtures should cover
   body/digest mismatches and retries before any adapter-wide credit.
4. File/CPF effects: normalized path and external process boundaries need adversarial and
   cross-platform tests; no compensation/atomic filesystem guarantee is established.
5. Reporting and inspection: selected operator outcomes are represented, but result-tree
   completeness, all option combinations and dependency/resource display queries remain open.

## Future shared-guidance proposals only

These proposals are separate from product findings. Shared guidance was not changed. They are
assessed against the five mechanism principles in the historical benchmark's mechanism page.

| Mechanism principle | Proposal and reason to retain or reject it |
|---|---|
| Fix concrete bugs and obvious issues | Retain a strict distinction between a matched native field and a merely nonempty evidence object. Empty receipt arrays must remain open. This fixes a concrete counting hazard without prescribing a new authoring workflow. |
| Preserve all good behavior | Preserve executable independent oracles, source-derived held-out predictions and wrong variants after porting to Verus. The env object-in-array miss and FileCopy composition show that passing formal proofs of a translation need independent consumers. Preserve inherited domains rather than replace every model with one new abstraction. |
| Justify value, evidence and tradeoffs | Require exact input fingerprints when an assurance claim is rebound, with a changed-input/affected-ID receipt. This adds freshness evidence; it does not determine semantic applicability by itself. Costs include larger receipts and dependency maintenance, so do not impose full-tree reruns for unrelated changes. |
| Reason about generalizability | At cross-store boundaries, report status, durable database state and external effects separately. FileCopy, lockfile and report output all show useful counterexamples. This transfers to payment/provider/file services; it should not force trivial pure helpers into a new transaction framework. |
| Convince yourself first | Pilot the cross-store distinction and native consumer controls on a held-out operation before adding blanket checklist text. This pass found the lockfile root issue and invalid-summary/file-survival composition by following source operations naturally. Evidence supports reusable outcome axes; it does not yet show that another long mandatory checklist improves the next agent. |
