<a id="overview"></a><!-- historical section alias -->
# IPM simplification register: P

All starting P IDs are reconciled below. No product redesign is accepted or implemented. [Before/after evidence and cost review](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md) records preserved behavior, assumptions, public interfaces and maintenance obligations.

## Scoped repair comparisons

<a id="d1-durable-history-writes-only-at-tlevel-0-reconcile-at-the-next-command"></a><!-- historical fragment alias -->
<a id="p-d1"></a>
### P:D1: Durable history writes only at `$TLEVEL` 0; reconcile at the next command

Severity: Medium · Decision: Open

Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.

Per-action success versus kept database writes, rollback-surviving history and the named caller/crash boundary.

The same 10 declared L2 fields occur on both sides (including configuration and four ghost fields, so this is not an independent semantic-state count). The proposed configuration activates the pending Seq<Row> and enables the Reconcile action: one additional active history collection and one additional recovery transition. No field is eliminated.

Durability is paid for by extra durable writes, a pending dependency collection and reconciliation/version lookup. The instance-loss-of-pending theorem remains; a durable pending marker is not modeled. Caller transactions, terminal commit and commit-window assumptions are not removed. The fresh full-file run has 37 verified and 2 resource-limit failures, so this audit does not claim a complete green re-verification.

[Actual model pair, proof result and source](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d1). Retained as a correctness repair; no net simplification is established.

Governing owner: [history.survives-rollback](../.g5/ontology/features/history.g5.md#history.survives-rollback). The repair remains a proposal.

## Withdrawn simplification claims

<a id="d2-never-drop-a-status"></a><!-- historical fragment alias -->
<a id="p-d2"></a>
### P:D2 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d2).

<a id="d3-check-before-acting-in-clean-skipped-is-its-own-outcome-fail-closed"></a><!-- historical fragment alias -->
<a id="p-d3"></a>
### P:D3 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d3).

<a id="d4-identify-ipm-by-resolved-module-not-by-typed-name-normalise-names-once"></a><!-- historical fragment alias -->
<a id="p-d4"></a>
### P:D4 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d4).

<a id="d5-one-ordered-registry-selection-used-by-every-command"></a><!-- historical fragment alias -->
<a id="p-d5"></a>
### P:D5 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d5).

<a id="d6-select-update-step-classes-by-exact-name-vdigits"></a><!-- historical fragment alias -->
<a id="p-d6"></a>
### P:D6 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d6).

<a id="d7-resolve-developer-mode-once-at-the-command-boundary"></a><!-- historical fragment alias -->
<a id="p-d7"></a>
### P:D7 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d7).

<a id="d8-stage-file-system-effects-and-apply-them-after-the-database-outcome"></a><!-- historical fragment alias -->
<a id="p-d8"></a>
### P:D8 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d8).

<a id="d9-one-mapping-helper-with-one-signature-decide-from-the-planned-state"></a><!-- historical fragment alias -->
<a id="p-d9"></a>
### P:D9 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d9).

<a id="d10-record-the-sync-baseline-wherever-source-reaches-the-server"></a><!-- historical fragment alias -->
<a id="p-d10"></a>
### P:D10 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d10).

<a id="d11-remove-or-implement-half-built-options"></a><!-- historical fragment alias -->
<a id="p-d11"></a>
### P:D11 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d11).

<a id="d12-reuse-existing-helpers-instead-of-near-duplicates"></a><!-- historical fragment alias -->
<a id="p-d12"></a>
### P:D12 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d12).

<a id="d13-small-local-fixes-with-no-shared-pattern"></a><!-- historical fragment alias -->
<a id="p-d13"></a>
### P:D13 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d13).

<a id="d14-nothing-that-can-fail-runs-after-the-final-commit"></a><!-- historical fragment alias -->
<a id="p-d14"></a>
### P:D14 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d14).

<a id="d15-check-every-requirement-before-any-commit-in-every-mode"></a><!-- historical fragment alias -->
<a id="p-d15"></a>
### P:D15 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d15).

<a id="d16-use-the-backtracking-resolver"></a><!-- historical fragment alias -->
<a id="p-d16"></a>
### P:D16 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d16).

<a id="d17-run-the-legacy-record-migration-wherever-ipm-is-enabled"></a><!-- historical fragment alias -->
<a id="p-d17"></a>
### P:D17 comparison review

Severity: Unranked

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../.g5/uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d17).
