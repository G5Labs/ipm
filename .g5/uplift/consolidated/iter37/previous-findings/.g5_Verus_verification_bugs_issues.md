<a id="catalog-rows-24-of-42-checked"></a><!-- historical section alias -->
<a id="history-commitments-17-of-17-checked"></a><!-- historical section alias -->
<a id="load-commitments-18-of-18-checked"></a><!-- historical section alias -->
<a id="appendix-b-sampling-record"></a><!-- historical section alias -->
<a id="a7-a1-conflicts-and-stale-statements"></a><!-- historical section alias -->
<a id="a6-review-findings-of-the-original-uplift"></a><!-- historical section alias -->
<a id="a5-follow-up-native-cases-and-brief-items"></a><!-- historical section alias -->
<a id="a4-other-registry-native-cases"></a><!-- historical section alias -->
<a id="a3-b1-cases-that-violate-the-final-contract-or-are-approximate"></a><!-- historical section alias -->
<a id="a2-defects-issues-model-findings-and-deviation-rows"></a><!-- historical section alias -->
<a id="a1-a2-observations"></a><!-- historical section alias -->
<a id="coverage"></a><!-- historical section alias -->
<a id="appendix-a-id-reconciliation"></a><!-- historical section alias -->
<a id="corrections-to-the-original-file"></a><!-- historical section alias -->
<a id="withdrawn-refuted-and-out-of-scope"></a><!-- historical section alias -->
<a id="suspected-code-reading-only"></a><!-- historical section alias -->
<a id="issues-without-a-governing-requirement"></a><!-- historical section alias -->
<a id="decision-dependent-conditional-findings"></a><!-- historical section alias -->
<a id="supported-defects"></a><!-- historical section alias -->
<a id="summary"></a><!-- historical section alias -->
<a id="destructive-behaviors"></a><!-- historical section alias -->
<a id="paths-and-labels"></a><!-- historical section alias -->
<a id="fields"></a><!-- historical section alias -->
<a id="ids"></a><!-- historical section alias -->
<a id="questions"></a><!-- historical section alias -->
<a id="classes"></a><!-- historical section alias -->
<a id="how-to-read-this-file"></a><!-- historical section alias -->
# IPM findings: G register

Product: `571e04a1`; baseline: sealed iteration 28. [Full reconciliation and commands](../uplift/consolidated/findings-audit-iter28/README.md). Decisions and historical claims are preserved in the review. Entries with Decision: Open are active; review and scope records are not active functional findings.

<a id="d1-a-dependencys-history-record-is-lost-when-its-parents-transaction-rolls-back"></a><!-- historical fragment alias -->
<a id="g-d1"></a>
### G:D1: A dependency's history record is lost when its parent's transaction rolls back

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d1)

Reproduced by: e2e reproduced. dependency row absent after parent rollback; successful load neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.survives-rollback). No product repair or maintainer approval is claimed.

<a id="d2-a-filter-column-with-_-drops-every-later-filter-so-history-find-widens-and-a-filtered-history-delete-can-remove-every-record"></a><!-- historical fragment alias -->
<a id="g-d2"></a>
### G:D2: A filter column with `_` drops every later filter, so find widens and a filtered delete can remove every record

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d2)

Reproduced by: e2e reproduced. history delete removes three seeded identities despite unmatched Version_Major; Package filter preserves nonmatches.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.read.filter). No product repair or maintainer approval is claimed.

<a id="d3-the-retention-purge-deletes-nothing-and-still-marks-the-days-purge-as-done"></a><!-- historical fragment alias -->
<a id="g-d3"></a>
### G:D3: The retention purge deletes nothing and marks the day's purge done

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d3)

Reproduced by: e2e reproduced. ordinary list triggers retention; old row survives with marker today; retention-off neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.retention). No product repair or maintainer approval is claimed.

<a id="d4-q-17-refused--dev-notransaction-or-nojournal-loads-keep-the-module-and-read-committed-when-system-requirements-fail"></a><!-- historical fragment alias -->
<a id="g-d4"></a>
### G:D4: Refused `-dev`, NoTransaction or NoJournal loads keep the module and read committed when system requirements fail

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d4)

Reproduced by: e2e reproduced. SYSNamespace refusal retains developer module; ordinary-mode refusal leaves none; policy remains open.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/load.g5.md#load.refusal.requirements.timing). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d6-q-14-an-invoke-that-returns-an-error-leaves-its-phase-open-in-a-finalized-record"></a><!-- historical fragment alias -->
<a id="g-d6"></a>
### G:D6: An Invoke that returns an error leaves its phase open

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d6)

Reproduced by: e2e reproduced. ordinary returned-error Activate Invoke leaves last phase OPEN; successful C01 phase is OK; intent remains Q-14.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031958567291Z-returns/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `returns`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.phases.returned-errors). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d9-a-refused-load-records-an-empty-package-although-the-manifest-named-the-module"></a><!-- historical fragment alias -->
<a id="g-d9"></a>
### G:D9: A refused load records an empty package

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d9)

Reproduced by: e2e reproduced. ordinary downgrade refusal followed by history details; named successful history neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.fields). No product repair or maintainer approval is claimed.

<a id="d11-manifest-refusals-under--dnotransaction1-or--dnojournal1-keep-module-data"></a><!-- historical fragment alias -->
<a id="g-d11"></a>
### G:D11: Manifest refusals under NoTransaction or NoJournal keep module data

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d11)

Reproduced by: e2e reproduced. NoJournal manifest refusal leaves the imported ZPM document at 2.0.0; successful NoJournal neighbor; broader cases remain in starting evidence.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031958567291Z-returns/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `returns`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history). No product repair or maintainer approval is claimed.

<a id="d12-q-31-a-notransaction-or-nojournal-load-refused-after-the-no-transaction-marker-reads-committed1-with-nothing-kept"></a><!-- historical fragment alias -->
<a id="g-d12"></a>
### G:D12: Refusals after the no-transaction marker read `Committed=1` with nothing kept

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d12)

Reproduced by: e2e reproduced. no-module-definition refusal under NoTransaction keeps no module and reports committed=1; ordinary successful NoJournal neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031958567291Z-returns/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `returns`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d13-a-load-that-committed-reports-invalid-oref-when-the-temporary-modules-repository-could-not-be-created"></a><!-- historical fragment alias -->
<a id="g-d13"></a>
### G:D13: A load that committed reports `<INVALID OREF>` when the `.modules` repository cannot be created

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d13)

Reproduced by: e2e reproduced. ordinary directory load hits failed temporary repository creation after commit; module remains; valid .modules neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history). No product repair or maintainer approval is claimed.

<a id="d18-q-18-namespace-default-modifiers-become-history-filters"></a><!-- historical fragment alias -->
<a id="g-d18"></a>
### G:D18: Namespace default modifiers become `history` filters

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d18)

Reproduced by: e2e reproduced. stored NoTransaction modifier makes ordinary history find hide seeded row while returning OK; deleting default restores the same row.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033221686336Z-readers/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `readers`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.read.filter.default-modifiers). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d20-the-globalrecords-query-returns-the-current-namespaces-rows-once-per-namespace"></a><!-- historical fragment alias -->
<a id="g-d20"></a>
### G:D20: `GlobalRecords` returns the current namespace's rows once per namespace

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d20)

Reproduced by: e2e reproduced. public GlobalRecords repeats USER history and misses seeded other namespace; local Records returns exactly one correct row.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033221686336Z-readers/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `readers`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.read.api). No product repair or maintainer approval is claimed.

<a id="d21-new-q-22-q-25-a-failed-normal-mode-load-can-leave-an-installed-module-without-its-dependency"></a><!-- historical fragment alias -->
<a id="g-d21"></a>
### G:D21: A failed load can leave an installed module without its dependency

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d21)

Reproduced by: e2e reproduced. ordinary parent failure leaves NoJournal middle dependency while its own child is gone; successful NESTS neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-closure). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d22-new-q-22-q-26-the-top-record-reads-committed0-although-a-dependencys-nojournal-changes-were-kept"></a><!-- historical fragment alias -->
<a id="g-d22"></a>
### G:D22: The top record reads `Committed=0` although a dependency's changes were kept

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d22)

Reproduced by: e2e reproduced. same parent failure records committed=0 while middle dependency survives; successful parent neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.committed.escaped-dependency). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d23-new-q-27-the-global-history-delete-hides-every-error-not-only-permission-errors"></a><!-- historical fragment alias -->
<a id="g-d23"></a>
### G:D23: The global history delete hides every error, not only permission errors

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d23)

Reproduced by: e2e reproduced. ordinary global delete with unknown SQL column reports OK and retains sentinel; valid Package-filtered global delete removes it; nonpermission-error policy remains Q-27.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033221686336Z-readers/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `readers`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.delete.global-errors). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d24-history-find-shows-an-empty-log-with-an-ok-status-when-its-query-fails"></a><!-- historical fragment alias -->
<a id="g-d24"></a>
### G:D24: `history find` shows an empty log with an OK status when its query fails

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d24)

Reproduced by: e2e reproduced. misspelled history column returns OK; valid Success query neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.read.filter.invalid-column). No product repair or maintainer approval is claimed.

<a id="d25-q-05q-06b-a-callers-rollback-removes-the-record-of-a-load-it-wrapped"></a><!-- historical fragment alias -->
<a id="g-d25"></a>
### G:D25: A caller's rollback removes the record of a load it wrapped

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d25)

Reproduced by: e2e reproduced. supported caller transaction rollback removes the row/module; the ordinary no-caller successful return retains a finalized row; policy remains Q-05.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031958567291Z-returns/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `returns`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.committed.nested). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d28-q-23-a-dependencys-own-defaults-notransaction-does-not-keep-it-out-of-its-parents-transaction"></a><!-- historical fragment alias -->
<a id="g-d28"></a>
### G:D28: A dependency's own `NoTransaction` default does not keep it out of the parent's transaction

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d28)

Reproduced by: e2e reproduced. own NoTransaction dependency still rolls back with parent; successful Q23S neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-no-transaction). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d29-new-q-09-refused-update-reinstall-and-uninstall-attempts-leave-no-record"></a><!-- historical fragment alias -->
<a id="g-d29"></a>
### G:D29: Refused `update`, `reinstall` and `uninstall` attempts leave no record

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d29)

Reproduced by: e2e reproduced. ordinary update/reinstall/uninstall argument refusals allocate no history ID; successful/failed named load neighbors elsewhere; intent remains Q-09.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033221686336Z-readers/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `readers`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.record.unlogged-refusals). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="i3-q-03-q-24-loading-a-parent-replaces-a-dependency-that-is-in-developer-mode-and-clears-its-developer-mode"></a><!-- historical fragment alias -->
<a id="g-i3"></a>
### G:I3: Loading a parent replaces a dependency in developer mode and clears its developer mode

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-i3)

Reproduced by: e2e reproduced. ordinary parent load upgrades developer dependency and clears developer mode; same-version neighbor retains it.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.dependency-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

## Outside active reproduction scope

<a id="g-oos-1"></a>
### G:OOS-1 review record

Severity: Unranked

G:OOS-1: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-oos-1)

## IDs forwarded to their owning record or review

<a id="d5-config-delete-historyretain-restores-the-first-value-ever-set-not-the-default-0"></a><!-- historical fragment alias -->
<a id="g-d5"></a>
### G:D5 review record

Severity: Unranked

G:D5: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d5)

<a id="d10-the-durable-record-of-an-action-that-never-ended-keeps-only-its-start-values"></a><!-- historical fragment alias -->
<a id="g-d10"></a>
### G:D10 review record

Severity: Unranked

G:D10: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d10)

<a id="d14-q-22-a-dependency-whose-own-defaults-set-nojournal-escapes-its-parents-normal-mode-rollback"></a><!-- historical fragment alias -->
<a id="g-d14"></a>
### G:D14 review record

Severity: Unranked

G:D14: policy-only/as-built-observed. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d14)

<a id="d15-load-refuses-targz-archives"></a><!-- historical fragment alias -->
<a id="g-d15"></a>
### G:D15 review record

Severity: Unranked

G:D15: inconclusive/execution-scope-blocked. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d15)

<a id="d16-update--path-with-another-modules-archive-crashes-instead-of-giving-the-mismatch-refusal"></a><!-- historical fragment alias -->
<a id="g-d16"></a>
### G:D16 review record

Severity: Unranked

G:D16: inconclusive/execution-scope-blocked. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d16)

<a id="d17-load-accepts-an-existing-relative-path"></a><!-- historical fragment alias -->
<a id="g-d17"></a>
### G:D17 review record

Severity: Unranked

G:D17: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d17)

<a id="d19-namespace-default-modifiers-are-shared-by-every-namespace"></a><!-- historical fragment alias -->
<a id="g-d19"></a>
### G:D19 review record

Severity: Unranked

G:D19: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d19)

<a id="d26-q-08-the-record-of-an-action-that-never-ended-reads-committed0-although-its-changes-were-kept"></a><!-- historical fragment alias -->
<a id="g-d26"></a>
### G:D26 review record

Severity: Unranked

G:D26: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d26)

<a id="d27-q-21-a-terminate-finalizes-locks-the-record-of-an-action-that-never-ended"></a><!-- historical fragment alias -->
<a id="g-d27"></a>
### G:D27 review record

Severity: Unranked

G:D27: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d27)

<a id="d30-new-uninstall--purge-sets-a-key-nothing-reads"></a><!-- historical fragment alias -->
<a id="g-d30"></a>
### G:D30 review record

Severity: Unranked

G:D30: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d30)

<a id="d31-new-q-03-q-24-install-replaces-a-dependency-in-developer-mode-without-the-overwrite-guard"></a><!-- historical fragment alias -->
<a id="g-d31"></a>
### G:D31 review record

Severity: Unranked

G:D31: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d31)

<a id="i1-an-unfinished-phase-is-shown-as-error-00-no-error-description"></a><!-- historical fragment alias -->
<a id="g-i1"></a>
### G:I1 review record

Severity: Unranked

G:I1: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-i1)

<a id="i2-a-load-ended-mid-way-leaves-its-temporary-modules-repository-registered-at-first-priority"></a><!-- historical fragment alias -->
<a id="g-i2"></a>
### G:I2 review record

Severity: Unranked

G:I2: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-i2)

<a id="i4-new-the-history-class-queries-return-no-id-and-no-namespace"></a><!-- historical fragment alias -->
<a id="g-i4"></a>
### G:I4 review record

Severity: Unranked

G:I4: policy-only/source-reviewed. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-i4)

<a id="i5-compile-ignores-a-resource-processors-error-status"></a><!-- historical fragment alias -->
<a id="g-i5"></a>
### G:I5 review record

Severity: Unranked

G:I5: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-i5)

<a id="i6-files-a-load-writes-outside-the-database-outlive-its-rollback"></a><!-- historical fragment alias -->
<a id="g-i6"></a>
### G:I6 review record

Severity: Unranked

G:I6: policy-only/source-reviewed. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-i6)

<a id="i7-permitdowngrade-reaches-the-dependency-graph-and-one-guard-but-not-the-downgrade-refusal"></a><!-- historical fragment alias -->
<a id="g-i7"></a>
### G:I7 review record

Severity: Unranked

G:I7: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-i7)

<a id="i8-new--dnolock1-turns-compile-locking-on"></a><!-- historical fragment alias -->
<a id="g-i8"></a>
### G:I8 review record

Severity: Unranked

G:I8: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-i8)

<a id="obs-l3-a-plain-load-does-not-save-root-and-repository-so-the-first-lock-file-goes-to-the-wrong-folder"></a><!-- historical fragment alias -->
<a id="g-obs-l3"></a>
### G:OBS-L3 review record

Severity: Unranked

G:OBS-L3: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-l3)

<a id="obs-l4-update--dev-from-a-registry-ends-in-normal-mode"></a><!-- historical fragment alias -->
<a id="g-obs-l4"></a>
### G:OBS-L4 review record

Severity: Unranked

G:OBS-L4: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-l4)

<a id="obs-l5-a-dependency-first-installed-by-update-runs-all-its-update-steps"></a><!-- historical fragment alias -->
<a id="g-obs-l5"></a>
### G:OBS-L5 review record

Severity: Unranked

G:OBS-L5: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-l5)

<a id="obs-l9-update-steps-are-skipped-silently-when-the-update-lock-times-out"></a><!-- historical fragment alias -->
<a id="g-obs-l9"></a>
### G:OBS-L9 review record

Severity: Unranked

G:OBS-L9: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-l9)

<a id="obs-l16-a-failed-legacy-xml-install-reads-committed0-although-it-kept-changes"></a><!-- historical fragment alias -->
<a id="g-obs-l16"></a>
### G:OBS-L16 review record

Severity: Unranked

G:OBS-L16: inconclusive/execution-scope-blocked. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-l16)

<a id="obs-l17-uninstalling-a-module-others-require-reports-success"></a><!-- historical fragment alias -->
<a id="g-obs-l17"></a>
### G:OBS-L17 review record

Severity: Unranked

G:OBS-L17: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-l17)

<a id="obs-h7-q-15-dependency-records-are-labeled-load-keep-a-source-and-list-it-as-na"></a><!-- historical fragment alias -->
<a id="g-obs-h7"></a>
### G:OBS-H7 review record

Severity: Unranked

G:OBS-H7: policy-only/source-reviewed. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-h7)

<a id="obs-h12-q-13-a-purge-failure-stops-the-users-command"></a><!-- historical fragment alias -->
<a id="g-obs-h12"></a>
### G:OBS-H12 review record

Severity: Unranked

G:OBS-H12: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-h12)

<a id="obs-h13-q-16-ipm-self-uninstall-with-full-cleanup-deletes-other-namespaces-in-progress-records"></a><!-- historical fragment alias -->
<a id="g-obs-h13"></a>
### G:OBS-H13 review record

Severity: Unranked

G:OBS-H13: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-h13)

<a id="obs-h15-self-uninstall-records-success-before-it-runs-and-exits-the-shell-even-when-skipped"></a><!-- historical fragment alias -->
<a id="g-obs-h15"></a>
### G:OBS-H15 review record

Severity: Unranked

G:OBS-H15: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-h15)

<a id="g-d7"></a>
### G:D7 review record

Severity: Unranked

G:D7: withdrawn/source-settled. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d7)

<a id="g-obs-h9"></a>
### G:OBS-H9 review record

Severity: Unranked

G:OBS-H9: withdrawn/source-settled. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-h9)

<a id="g-obs-h10"></a>
### G:OBS-H10 review record

Severity: Unranked

G:OBS-H10: refuted. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-h10)

<a id="g-obs-l15"></a>
### G:OBS-L15 review record

Severity: Unranked

G:OBS-L15: outside-functional-contract. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-obs-l15)

<a id="g-oos-2"></a>
### G:OOS-2 review record

Severity: Unranked

G:OOS-2: inconclusive/execution-scope-blocked. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-oos-2)

<a id="g-oos-3"></a>
### G:OOS-3 review record

Severity: Unranked

G:OOS-3: outside-functional-contract. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-oos-3)
