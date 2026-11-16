# IPM design simplifications

The G, C and P prefixes retain the original register identities. [Iteration 28 reconciliation](../uplift/consolidated/findings-audit-iter28/README.md).

<a id="summary"></a><!-- historical section alias -->
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
<a id="overview"></a><!-- historical section alias -->

<a id="d1-durable-history-writes-only-at-tlevel-0-reconcile-at-the-next-command"></a><!-- historical fragment alias -->
<a id="p-d1"></a>
### P:D1: Durable history writes only at `$TLEVEL` 0; reconcile at the next command

Severity: Critical · Decision: Open

Severity basis: Priority follows the most severe scoped consequence addressed: G:D1, P:F-HIST-2, P:F-HIST-8. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.

Per-action success versus kept database writes, rollback-surviving history and the named caller/crash boundary.

The same 10 declared L2 fields occur on both sides (including configuration and four ghost fields, so this is not an independent semantic-state count). The proposed configuration activates the pending Seq<Row> and enables the Reconcile action: one additional active history collection and one additional recovery transition. No field is eliminated.

Durability is paid for by extra durable writes, a pending dependency collection and reconciliation/version lookup. The instance-loss-of-pending theorem remains; a durable pending marker is not modeled. Caller transactions, terminal commit and commit-window assumptions are not removed. The fresh full-file run has 37 verified and 2 resource-limit failures, so this audit does not claim a complete green re-verification.

[Actual model pair, proof result and source](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d1). Retained as a correctness repair; no net simplification is established.

Governing owner: [history.survives-rollback](../ontology/features/history.g5.md#history.survives-rollback). The repair remains a proposal.

<a id="c-s17"></a>
### C:S17: Run the dependent and global-scope check before any processor's `OnBeforePhase("Clean")`, and return an error naming the dependents.

Severity: High · Decision: Open

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-UNIN-1, P:F-UNIN-4. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.

Unblocked and forced uninstall and the app state of valid neighbors. A blocked command intentionally changes to an error with app retained.

3 CleanResult fields on each side, with the same Apps map. The dependent check moves before app deletion; no persistent field is eliminated.

The authorization/ownership check must precede all cleanup hooks, not just the modeled web-app deletion. The abstract pair does not prove full hook ordering, general compensation or recursive cleanup.

[Actual model pair, proof result and source](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s17). Retained as a correctness repair; no net simplification is established.

Governing owner: [uninstall.dependents](../ontology/features/uninstall.g5.md#uninstall.dependents). The repair remains a proposal.

<a id="c-s13"></a>
### C:S13: Preserve the deployed-item helper's returned status before testing handled/error or falling back to source.

Severity: Medium · Decision: Open

Severity basis: Priority follows the most severe scoped consequence addressed: C:D19. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.

Successful deployed handling, legitimate source fallback and source-export errors.

0 mutable model fields on both sides; 4 input parameters and 3 result fields on both sides. The proposal adds one failed-helper decision before fallback; no state field or interface disappears.

The caller must retain deployed_ok and prove that the new guard precedes fallback. export_repair_preserves_success and export_repair_stops_failed_helper are two distinct obligations. Decoder, provider and whole-package behavior stay outside this comparison.

[Actual model pair, proof result and source](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s13). Retained as a correctness repair; no net simplification is established.

Governing owner: [build.package.deployed-status](../ontology/features/build-and-publish.g5.md#build.package.deployed-status). The repair remains a proposal.

<a id="c-s20"></a>
### C:S20: Capture ResolveChildren's returned status in Clean, propagate its error and do not delete module metadata after that error.

Severity: Medium · Decision: Open

Severity basis: Priority follows the most severe scoped consequence addressed: C:D27. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.

Successful child enumeration and ordinary class cleanup; the error path must retain metadata and report failure.

3 State fields on both sides (module, manifest, code) and 2 Outcome fields on both sides. One returned-status guard is added; no state field or transition obligation is eliminated.

The dispatcher now owns the returned-status check. Earlier resource deletions and extension side effects are still not compensated. returned_error_neighbors proves the successful neighbor equality and the changed failure outcome.

[Actual model pair, proof result and source](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s20). Retained as a correctness repair; no net simplification is established.

Governing owner: [uninstall.child-error](../ontology/features/uninstall.g5.md#uninstall.child-error). The repair remains a proposal.

<a id="c-s21"></a>
### C:S21: Forward the already-selected testIndex into the ToFile call, using the same run identity as device output.

Severity: Medium · Decision: Open

Severity basis: Priority follows the most severe scoped consequence addressed: C:D31. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

Comparison: none. Verus-only comparison; no TLC or reachable-state reduction.

Single-run results, outer command status, nested child rows and file-before-fatal-check ordering.

2 State fields on both sides (db, tracked_ids), and 5 Report fields on both sides. The selected ID is forwarded to an existing API; the optional default selector and generic API still exist. Zero stored fields are removed.

Every report caller must forward the correct ID; selection responsibility moves to that caller. explicit_forwarding_preserves_reader adds an equality obligation. Stream/serialization failure and full nested lifetimes remain provider/caller premises.

[Actual model pair, proof result and source](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s21). Retained as a correctness repair; no net simplification is established.

Governing owner: [build.test-output.run-identity](../ontology/features/build-and-publish.g5.md#build.test-output.run-identity). The repair remains a proposal.

## Review records

These ratings describe the retained consequence or review priority. Aliases do not add defects; closed, withdrawn, excluded and unconfirmed records retain their existing dispositions.

<a id="s1-validate-the-whole-load-before-the-first-write"></a><!-- historical fragment alias -->
<a id="g-s1"></a>
### G:S1 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: G:D4, G:D9, G:D11, G:D12, G:D16. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#g-s1).

<a id="s2-one-durable-history-row-per-action"></a><!-- historical fragment alias -->
<a id="g-s2"></a>
### G:S2 comparison review

Severity: Critical

Severity basis: Priority follows the most severe scoped consequence addressed: G:D1, G:D25, G:D10. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#g-s2).

<a id="s3-mark-committed-only-where-a-write-is-kept"></a><!-- historical fragment alias -->
<a id="g-s3"></a>
### G:S3 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: G:D12, G:D22. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#g-s3).

<a id="s4-structured-history-filters"></a><!-- historical fragment alias -->
<a id="g-s4"></a>
### G:S4 comparison review

Severity: Critical

Severity basis: Priority follows the most severe scoped consequence addressed: G:D2, G:D3, G:D24. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#g-s4).

<a id="s5-one-exit-path-per-phase"></a><!-- historical fragment alias -->
<a id="g-s5"></a>
### G:S5 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: G:D6. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#g-s5).

<a id="s6-declared-setting-defaults"></a><!-- historical fragment alias -->
<a id="g-s6"></a>
### G:S6 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-REPO-4. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#g-s6).

<a id="s7-a-temporary-repository-that-cannot-outlive-or-fail-its-load"></a><!-- historical fragment alias -->
<a id="g-s7"></a>
### G:S7 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: G:D13. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#g-s7).

<a id="s8-one-transaction-plan-per-load"></a><!-- historical fragment alias -->
<a id="g-s8"></a>
### G:S8 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: G:D21, G:D22, G:D28. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#g-s8).

<a id="s9-namespace-aware-history-readers-and-setting-scope"></a><!-- historical fragment alias -->
<a id="g-s9"></a>
### G:S9 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: G:D20, P:F-REPO-5. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#g-s9).

<a id="s10-stage-files-outside-the-database-until-the-commit"></a><!-- historical fragment alias -->
<a id="g-s10"></a>
### G:S10 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-LOAD-3, G:I6. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#g-s10).

<a id="c-s1"></a>
### C:S1 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: G:D13, C:D2. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s1).

<a id="c-s2"></a>
### C:S2 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: C:D4, C:D5. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s2).

<a id="c-s3"></a>
### C:S3 comparison review

Severity: Low

Severity basis: Priority follows the most severe scoped consequence addressed: C:D3. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s3).

<a id="c-s4"></a>
### C:S4 comparison review

Severity: High

Severity basis: Priority follows the most severe scoped consequence addressed: C:D6, C:D7. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s4).

<a id="c-s5"></a>
### C:S5 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: C:D8. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s5).

<a id="c-s6"></a>
### C:S6 comparison review

Severity: Low

Severity basis: Priority follows the most severe scoped consequence addressed: C:D13. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s6).

<a id="c-s7"></a>
### C:S7 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: G:D13. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s7).

<a id="c-s8"></a>
### C:S8 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: C:D4, C:D5, C:D14. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s8).

<a id="c-s9"></a>
### C:S9 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: C:D15. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s9).

<a id="c-s11"></a>
### C:S11 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: C:D17. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s11).

<a id="c-s12"></a>
### C:S12 comparison review

Severity: High

Severity basis: Priority follows the most severe scoped consequence addressed: C:D18. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s12).

<a id="c-s14"></a>
### C:S14 comparison review

Severity: High

Severity basis: Priority follows the most severe scoped consequence addressed: C:D20. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s14).

<a id="c-s15"></a>
### C:S15 comparison review

Severity: Low

Severity basis: Priority follows the most severe scoped consequence addressed: C:D21. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s15).

<a id="c-s16"></a>
### C:S16 comparison review

Severity: Low

Severity basis: Priority follows the most severe scoped consequence addressed: C:D22. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s16).

<a id="c-s14b"></a>
### C:S14b comparison review

Severity: High

Severity basis: Priority follows the most severe scoped consequence addressed: C:D20. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s14b).

<a id="c-s18"></a>
### C:S18 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: C:D24. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s18).

<a id="c-s19"></a>
### C:S19 comparison review

Severity: Low

Severity basis: Priority follows the most severe scoped consequence addressed: C:D23. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s19).

<a id="c-s10"></a>
### C:S10 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: C:D16. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#c-s10).

<a id="d2-never-drop-a-status"></a><!-- historical fragment alias -->
<a id="p-d2"></a>
### P:D2 comparison review

Severity: High

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-INST-1, P:F-NSP-1, G:D6. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d2).

<a id="d3-check-before-acting-in-clean-skipped-is-its-own-outcome-fail-closed"></a><!-- historical fragment alias -->
<a id="p-d3"></a>
### P:D3 comparison review

Severity: High

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-UNIN-1, P:F-UNIN-4, P:F-UNIN-6. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d3).

<a id="d4-identify-ipm-by-resolved-module-not-by-typed-name-normalise-names-once"></a><!-- historical fragment alias -->
<a id="p-d4"></a>
### P:D4 comparison review

Severity: High

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-UNIN-3, P:F-UNIN-8, P:F-DEP-3. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d4).

<a id="d5-one-ordered-registry-selection-used-by-every-command"></a><!-- historical fragment alias -->
<a id="p-d5"></a>
### P:D5 comparison review

Severity: High

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-INST-4, P:F-REPO-7, P:F-REPO-10. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d5).

<a id="d6-select-update-step-classes-by-exact-name-vdigits"></a><!-- historical fragment alias -->
<a id="p-d6"></a>
### P:D6 comparison review

Severity: High

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-INST-6, P:F-INST-7. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d6).

<a id="d7-resolve-developer-mode-once-at-the-command-boundary"></a><!-- historical fragment alias -->
<a id="p-d7"></a>
### P:D7 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-INST-5, P:F-INST-8, P:F-INST-9, P:F-INST-10. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d7).

<a id="d8-stage-file-system-effects-and-apply-them-after-the-database-outcome"></a><!-- historical fragment alias -->
<a id="p-d8"></a>
### P:D8 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-LOAD-2, P:F-LOAD-3, P:F-DEP-8. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d8).

<a id="d9-one-mapping-helper-with-one-signature-decide-from-the-planned-state"></a><!-- historical fragment alias -->
<a id="p-d9"></a>
### P:D9 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-NSP-4, P:F-NSP-6, P:F-NSP-7, P:F-NSP-8. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d9).

<a id="d10-record-the-sync-baseline-wherever-source-reaches-the-server"></a><!-- historical fragment alias -->
<a id="p-d10"></a>
### P:D10 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-SYNC-1. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d10).

<a id="d11-remove-or-implement-half-built-options"></a><!-- historical fragment alias -->
<a id="p-d11"></a>
### P:D11 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-UNIN-2, P:F-REPO-1, P:F-REPO-2, P:F-REPO-9, P:F-DEP-1, P:F-RES-2. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d11).

<a id="d12-reuse-existing-helpers-instead-of-near-duplicates"></a><!-- historical fragment alias -->
<a id="p-d12"></a>
### P:D12 comparison review

Severity: High

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-NSP-2, P:F-UNIN-10, G:D3, P:F-REPO-11. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d12).

<a id="d13-small-local-fixes-with-no-shared-pattern"></a><!-- historical fragment alias -->
<a id="p-d13"></a>
### P:D13 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-REPO-4, P:F-REPO-5, P:F-REPO-12, P:F-RES-3, P:F-RES-7, P:F-UNIN-5, P:F-DEP-2, P:F-DEP-9. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d13).

<a id="d14-nothing-that-can-fail-runs-after-the-final-commit"></a><!-- historical fragment alias -->
<a id="p-d14"></a>
### P:D14 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-LOAD-4. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d14).

<a id="d15-check-every-requirement-before-any-commit-in-every-mode"></a><!-- historical fragment alias -->
<a id="p-d15"></a>
### P:D15 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: G:D4. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d15).

<a id="d16-use-the-backtracking-resolver"></a><!-- historical fragment alias -->
<a id="p-d16"></a>
### P:D16 comparison review

Severity: Medium

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-DEP-7. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d16).

<a id="d17-run-the-legacy-record-migration-wherever-ipm-is-enabled"></a><!-- historical fragment alias -->
<a id="p-d17"></a>
### P:D17 comparison review

Severity: Low

Severity basis: Priority follows the most severe scoped consequence addressed: P:F-NSP-10. This does not accept the proposal, establish a state reduction or change its withdrawn/conditional status.

The simplification claim is withdrawn. [Preserved behavior, proposal and remaining obligations](../uplift/consolidated/findings-audit-iter28/SIMPLIFICATION-REVIEW.md#p-d17).
