<a id="iteration-28-reconciliation"></a><!-- historical section alias -->
<a id="iteration-27-reconciliation"></a><!-- historical section alias -->
<a id="iteration-26-reconciliation"></a><!-- historical section alias -->
<a id="iteration-25-reconciliation"></a><!-- historical section alias -->
<a id="iteration-24-reconciliation"></a><!-- historical section alias -->
<a id="iteration-23-reconciliation"></a><!-- historical section alias -->
<a id="iteration-22-reconciliation"></a><!-- historical section alias -->
<a id="iteration-21-reconciliation"></a><!-- historical section alias -->
<a id="iteration-20-reconciliation"></a><!-- historical section alias -->
<a id="iteration-19-reconciliation"></a><!-- historical section alias -->
<a id="iteration-18-reconciliation"></a><!-- historical section alias -->
<a id="iteration-17-proposal"></a><!-- historical section alias -->
<a id="iteration-16-reconciliation"></a><!-- historical section alias -->
<a id="iteration-15-reconciliation"></a><!-- historical section alias -->
<a id="iteration-14-reconciliation"></a><!-- historical section alias -->
<a id="iteration-13-reconciliation"></a><!-- historical section alias -->
<a id="iteration-12-disposition"></a><!-- historical section alias -->
<a id="iteration-11-comparisons-not-applied"></a><!-- historical section alias -->
<a id="iteration-10-proposal"></a><!-- historical section alias -->
<a id="iteration-9-disposition"></a><!-- historical section alias -->
<a id="iteration-8-disposition"></a><!-- historical section alias -->
<a id="iteration-7-disposition"></a><!-- historical section alias -->
<a id="iteration-6-disposition"></a><!-- historical section alias -->
<a id="iteration-5-proposals-not-applied"></a><!-- historical section alias -->
<a id="iteration-4-proposals-not-applied"></a><!-- historical section alias -->
<a id="boundary-proposals-carried-from-iteration-2"></a><!-- historical section alias -->
<a id="extension-proposals-not-applied"></a><!-- historical section alias -->
# IPM simplification register: C

All starting C IDs are reconciled below. No product redesign is accepted or implemented. [Before/after evidence and cost review](findings-audit-iter28/SIMPLIFICATION-REVIEW.md) records preserved behavior, assumptions, public interfaces and maintenance obligations.

## Scoped repair comparisons

<a id="c-s13"></a>
### C:S13: Preserve the deployed-item helper's returned status before testing handled/error or falling back to source.

Severity: Medium · Decision: Open

Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.

Successful deployed handling, legitimate source fallback and source-export errors.

0 mutable model fields on both sides; 4 input parameters and 3 result fields on both sides. The proposal adds one failed-helper decision before fallback; no state field or interface disappears.

The caller must retain deployed_ok and prove that the new guard precedes fallback. export_repair_preserves_success and export_repair_stops_failed_helper are two distinct obligations. Decoder, provider and whole-package behavior stay outside this comparison.

[Actual model pair, proof result and source](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s13). Retained as a correctness repair; no net simplification is established.

Governing owner: [build.package.deployed-status](../../ontology/features/build-and-publish.g5.md#build.package.deployed-status). The repair remains a proposal.

<a id="c-s17"></a>
### C:S17: Run the dependent and global-scope check before any processor's `OnBeforePhase("Clean")`, and return an error naming the dependents.

Severity: Medium · Decision: Open

Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.

Unblocked and forced uninstall and the app state of valid neighbors. A blocked command intentionally changes to an error with app retained.

3 CleanResult fields on each side, with the same Apps map. The dependent check moves before app deletion; no persistent field is eliminated.

The authorization/ownership check must precede all cleanup hooks, not just the modeled web-app deletion. The abstract pair does not prove full hook ordering, general compensation or recursive cleanup.

[Actual model pair, proof result and source](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s17). Retained as a correctness repair; no net simplification is established.

Governing owner: [uninstall.dependents](../../ontology/features/uninstall.g5.md#uninstall.dependents). The repair remains a proposal.

<a id="c-s20"></a>
### C:S20: Capture ResolveChildren's returned status in Clean, propagate its error and do not delete module metadata after that error.

Severity: Medium · Decision: Open

Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.

Successful child enumeration and ordinary class cleanup; the error path must retain metadata and report failure.

3 State fields on both sides (module, manifest, code) and 2 Outcome fields on both sides. One returned-status guard is added; no state field or transition obligation is eliminated.

The dispatcher now owns the returned-status check. Earlier resource deletions and extension side effects are still not compensated. returned_error_neighbors proves the successful neighbor equality and the changed failure outcome.

[Actual model pair, proof result and source](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s20). Retained as a correctness repair; no net simplification is established.

Governing owner: [uninstall.child-error](../../ontology/features/uninstall.g5.md#uninstall.child-error). The repair remains a proposal.

<a id="c-s21"></a>
### C:S21: Forward the already-selected testIndex into the ToFile call, using the same run identity as device output.

Severity: Medium · Decision: Open

Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.

Single-run results, outer command status, nested child rows and file-before-fatal-check ordering.

2 State fields on both sides (db, tracked_ids), and 5 Report fields on both sides. The selected ID is forwarded to an existing API; the optional default selector and generic API still exist. Zero stored fields are removed.

Every report caller must forward the correct ID; selection responsibility moves to that caller. explicit_forwarding_preserves_reader adds an equality obligation. Stream/serialization failure and full nested lifetimes remain provider/caller premises.

[Actual model pair, proof result and source](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s21). Retained as a correctness repair; no net simplification is established.

Governing owner: [build.test-output.run-identity](../../ontology/features/build-and-publish.g5.md#build.test-output.run-identity). The repair remains a proposal.

## Withdrawn simplification claims

<a id="c-s1"></a>
### C:S1 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s1).

<a id="c-s2"></a>
### C:S2 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s2).

<a id="c-s3"></a>
### C:S3 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s3).

<a id="c-s4"></a>
### C:S4 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s4).

<a id="c-s5"></a>
### C:S5 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s5).

<a id="c-s6"></a>
### C:S6 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s6).

<a id="c-s7"></a>
### C:S7 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s7).

<a id="c-s8"></a>
### C:S8 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s8).

<a id="c-s9"></a>
### C:S9 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s9).

<a id="c-s11"></a>
### C:S11 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s11).

<a id="c-s12"></a>
### C:S12 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s12).

<a id="c-s14"></a>
### C:S14 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s14).

<a id="c-s15"></a>
### C:S15 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s15).

<a id="c-s16"></a>
### C:S16 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s16).

<a id="c-s14b"></a>
### C:S14b comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s14b).

<a id="c-s18"></a>
### C:S18 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s18).

<a id="c-s19"></a>
### C:S19 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s19).

<a id="c-s10"></a>
### C:S10 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s10).
