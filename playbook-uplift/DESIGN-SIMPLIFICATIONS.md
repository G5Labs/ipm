# Historical register references

[Open the current register](../.g5/Verus_verification/design_simplification.md).

<a id="overview"></a>
[overview](../.g5/Verus_verification/design_simplification.md#overview)

<a id="d1-durable-history-writes-only-at-tlevel-0-reconcile-at-the-next-command"></a>
[d1-durable-history-writes-only-at-tlevel-0-reconcile-at-the-next-command](../.g5/Verus_verification/design_simplification.md#d1-durable-history-writes-only-at-tlevel-0-reconcile-at-the-next-command)

<a id="p-d1"></a>
[p-d1](../.g5/Verus_verification/design_simplification.md#p-d1)

<a id="d2-never-drop-a-status"></a>
[d2-never-drop-a-status](../.g5/Verus_verification/design_simplification.md#d2-never-drop-a-status)

<a id="p-d2"></a>
[p-d2](../.g5/Verus_verification/design_simplification.md#p-d2)

<a id="d3-check-before-acting-in-clean-skipped-is-its-own-outcome-fail-closed"></a>
[d3-check-before-acting-in-clean-skipped-is-its-own-outcome-fail-closed](../.g5/Verus_verification/design_simplification.md#d3-check-before-acting-in-clean-skipped-is-its-own-outcome-fail-closed)

<a id="p-d3"></a>
[p-d3](../.g5/Verus_verification/design_simplification.md#p-d3)

<a id="d4-identify-ipm-by-resolved-module-not-by-typed-name-normalise-names-once"></a>
[d4-identify-ipm-by-resolved-module-not-by-typed-name-normalise-names-once](../.g5/Verus_verification/design_simplification.md#d4-identify-ipm-by-resolved-module-not-by-typed-name-normalise-names-once)

<a id="p-d4"></a>
[p-d4](../.g5/Verus_verification/design_simplification.md#p-d4)

<a id="d5-one-ordered-registry-selection-used-by-every-command"></a>
[d5-one-ordered-registry-selection-used-by-every-command](../.g5/Verus_verification/design_simplification.md#d5-one-ordered-registry-selection-used-by-every-command)

<a id="p-d5"></a>
[p-d5](../.g5/Verus_verification/design_simplification.md#p-d5)

<a id="d6-select-update-step-classes-by-exact-name-vdigits"></a>
[d6-select-update-step-classes-by-exact-name-vdigits](../.g5/Verus_verification/design_simplification.md#d6-select-update-step-classes-by-exact-name-vdigits)

<a id="p-d6"></a>
[p-d6](../.g5/Verus_verification/design_simplification.md#p-d6)

<a id="d7-resolve-developer-mode-once-at-the-command-boundary"></a>
[d7-resolve-developer-mode-once-at-the-command-boundary](../.g5/Verus_verification/design_simplification.md#d7-resolve-developer-mode-once-at-the-command-boundary)

<a id="p-d7"></a>
[p-d7](../.g5/Verus_verification/design_simplification.md#p-d7)

<a id="d8-stage-file-system-effects-and-apply-them-after-the-database-outcome"></a>
[d8-stage-file-system-effects-and-apply-them-after-the-database-outcome](../.g5/Verus_verification/design_simplification.md#d8-stage-file-system-effects-and-apply-them-after-the-database-outcome)

<a id="p-d8"></a>
[p-d8](../.g5/Verus_verification/design_simplification.md#p-d8)

<a id="d9-one-mapping-helper-with-one-signature-decide-from-the-planned-state"></a>
[d9-one-mapping-helper-with-one-signature-decide-from-the-planned-state](../.g5/Verus_verification/design_simplification.md#d9-one-mapping-helper-with-one-signature-decide-from-the-planned-state)

<a id="p-d9"></a>
[p-d9](../.g5/Verus_verification/design_simplification.md#p-d9)

<a id="d10-record-the-sync-baseline-wherever-source-reaches-the-server"></a>
[d10-record-the-sync-baseline-wherever-source-reaches-the-server](../.g5/Verus_verification/design_simplification.md#d10-record-the-sync-baseline-wherever-source-reaches-the-server)

<a id="p-d10"></a>
[p-d10](../.g5/Verus_verification/design_simplification.md#p-d10)

<a id="d11-remove-or-implement-half-built-options"></a>
[d11-remove-or-implement-half-built-options](../.g5/Verus_verification/design_simplification.md#d11-remove-or-implement-half-built-options)

<a id="p-d11"></a>
[p-d11](../.g5/Verus_verification/design_simplification.md#p-d11)

<a id="d12-reuse-existing-helpers-instead-of-near-duplicates"></a>
[d12-reuse-existing-helpers-instead-of-near-duplicates](../.g5/Verus_verification/design_simplification.md#d12-reuse-existing-helpers-instead-of-near-duplicates)

<a id="p-d12"></a>
[p-d12](../.g5/Verus_verification/design_simplification.md#p-d12)

<a id="d13-small-local-fixes-with-no-shared-pattern"></a>
[d13-small-local-fixes-with-no-shared-pattern](../.g5/Verus_verification/design_simplification.md#d13-small-local-fixes-with-no-shared-pattern)

<a id="p-d13"></a>
[p-d13](../.g5/Verus_verification/design_simplification.md#p-d13)

<a id="d14-nothing-that-can-fail-runs-after-the-final-commit"></a>
[d14-nothing-that-can-fail-runs-after-the-final-commit](../.g5/Verus_verification/design_simplification.md#d14-nothing-that-can-fail-runs-after-the-final-commit)

<a id="p-d14"></a>
[p-d14](../.g5/Verus_verification/design_simplification.md#p-d14)

<a id="d15-check-every-requirement-before-any-commit-in-every-mode"></a>
[d15-check-every-requirement-before-any-commit-in-every-mode](../.g5/Verus_verification/design_simplification.md#d15-check-every-requirement-before-any-commit-in-every-mode)

<a id="p-d15"></a>
[p-d15](../.g5/Verus_verification/design_simplification.md#p-d15)

<a id="d16-use-the-backtracking-resolver"></a>
[d16-use-the-backtracking-resolver](../.g5/Verus_verification/design_simplification.md#d16-use-the-backtracking-resolver)

<a id="p-d16"></a>
[p-d16](../.g5/Verus_verification/design_simplification.md#p-d16)

<a id="d17-run-the-legacy-record-migration-wherever-ipm-is-enabled"></a>
[d17-run-the-legacy-record-migration-wherever-ipm-is-enabled](../.g5/Verus_verification/design_simplification.md#d17-run-the-legacy-record-migration-wherever-ipm-is-enabled)

<a id="p-d17"></a>
[p-d17](../.g5/Verus_verification/design_simplification.md#p-d17)

