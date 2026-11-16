# IPM bugs and issues

The G, C and P prefixes retain the original register identities. [Iteration 28 reconciliation](../uplift/consolidated/findings-audit-iter28/README.md).

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
<a id="iteration-28-scripting-sequence-and-final-setting-reader"></a><!-- historical section alias -->
<a id="iteration-27-inherited-uninstall-purgedata-reader"></a><!-- historical section alias -->
<a id="iteration-26-reconciliation"></a><!-- historical section alias -->
<a id="iteration-25-reconciliation"></a><!-- historical section alias -->
<a id="iteration-24-findings"></a><!-- historical section alias -->
<a id="iteration-23-reconciliation"></a><!-- historical section alias -->
<a id="iteration-22-reconciliation"></a><!-- historical section alias -->
<a id="iteration-21-findings"></a><!-- historical section alias -->
<a id="iteration-20-reconciliation"></a><!-- historical section alias -->
<a id="iteration-19-reconciliation"></a><!-- historical section alias -->
<a id="iteration-18-reconciliation"></a><!-- historical section alias -->
<a id="iteration-17-findings"></a><!-- historical section alias -->
<a id="iteration-16-reconciliation"></a><!-- historical section alias -->
<a id="iteration-15-reconciliation"></a><!-- historical section alias -->
<a id="iteration-14-dispositions"></a><!-- historical section alias -->
<a id="iteration-13-reconciliation"></a><!-- historical section alias -->
<a id="iteration-12-reconciliation"></a><!-- historical section alias -->
<a id="iteration-11-dispositions"></a><!-- historical section alias -->
<a id="iteration-10-findings"></a><!-- historical section alias -->
<a id="iteration-9-dispositions"></a><!-- historical section alias -->
<a id="iteration-8-finding-reconciliation"></a><!-- historical section alias -->
<a id="iteration-7-finding-reconciliation"></a><!-- historical section alias -->
<a id="iteration-6-finding-reconciliation"></a><!-- historical section alias -->
<a id="alias-check-for-iteration-5-claims"></a><!-- historical section alias -->
<a id="iteration-5-new-evidence-for-iteration-4-findings"></a><!-- historical section alias -->
<a id="iteration-5-findings"></a><!-- historical section alias -->
<a id="alias-check-for-iteration-4-claims"></a><!-- historical section alias -->
<a id="iteration-4-new-evidence-for-inherited-findings"></a><!-- historical section alias -->
<a id="iteration-4-findings"></a><!-- historical section alias -->
<a id="reconciled-iteration-2-outcomes"></a><!-- historical section alias -->
<a id="verus-extension-confirmation"></a><!-- historical section alias -->
<a id="new-evidence-for-inherited-defects"></a><!-- historical section alias -->
<a id="not-established"></a><!-- historical section alias -->
<a id="ipm-test-suite-issues-found-while-running-it"></a><!-- historical section alias -->
<a id="09-developer-sync"></a><!-- historical section alias -->
<a id="08-resources-and-packaging"></a><!-- historical section alias -->
<a id="07-namespace-ownership"></a><!-- historical section alias -->
<a id="06-registries-and-settings"></a><!-- historical section alias -->
<a id="05-uninstall"></a><!-- historical section alias -->
<a id="04-dependencies"></a><!-- historical section alias -->
<a id="03-install-and-update"></a><!-- historical section alias -->
<a id="02-transaction-history"></a><!-- historical section alias -->
<a id="01-package-loading"></a><!-- historical section alias -->
<a id="shared-root-causes"></a><!-- historical section alias -->
<a id="the-six-high-defects"></a><!-- historical section alias -->
<a id="p-summary"></a><!-- historical section alias -->
<a id="p-how-to-read-this-file"></a><!-- historical section alias -->

<a id="c-d6"></a>
### C:D6: An operator on a partial version is dropped. `<2` means `2.x` (`>=2.0.0 <3.0.0-0`), `>=1.2` and `>1.2` mean `1.2.x`, `<=1` means `1.x`. A dependency declared...

Severity: High · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d6)

Reproduced by: e2e reproduced. ordinary dependency load installs 2.5.0 for <2; valid dependency neighbor Dok.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="p-f-inst-1"></a>
### P:F-INST-1: `update` reports success without running update steps when the update lock is busy for 3 s. No message unless verbose. The steps stay due, and nothing prompt...

Severity: High · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-1)

Reproduced by: e2e reproduced. real competing update lock; command OK with old step rows; unlocked retry runs due steps.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#update.steps.busy-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-inst-6"></a>
### P:F-INST-6: A class of the update package named `V<n><something>` (e.g. `V3Helper`) replaces step class `V<n>` in the step list; update then reports success without runn...

Severity: High · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-6)

Reproduced by: e2e reproduced. real V3Helper fixture suppresses V3 execution; V2/V9/V10 ordinary order neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/update.g5.md#update.steps). No product repair or maintainer approval is claimed.

<a id="p-f-unin-1"></a>
### P:F-UNIN-1: A blocked uninstall (a dependent exists, or global scope) still deletes the module's web and CSP applications and runs `Clean`/`Unconfigure` hooks, then prin...

Severity: High · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-1)

Reproduced by: e2e reproduced. blocked uninstall deletes actual test web application; module remains; ordinary global-resource uninstall neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.dependents.as-built). No product repair or maintainer approval is claimed.

<a id="p-f-unin-4"></a>
### P:F-UNIN-4: Uninstall without `-force` removes a module that a dependent still needs if that dependent's dependency graph fails to build (e.g. a registry is missing): gr...

Severity: High · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-4)

Reproduced by: e2e reproduced. dependent graph error lets uninstall remove required class; resolvable dependent neighbor blocks.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.dependents). No product repair or maintainer approval is claimed.

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

[Governing owner](open_questions.md#load.refusal.requirements.timing). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d6-q-14-an-invoke-that-returns-an-error-leaves-its-phase-open-in-a-finalized-record"></a><!-- historical fragment alias -->
<a id="g-d6"></a>
### G:D6: An Invoke that returns an error leaves its phase open

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d6)

Reproduced by: e2e reproduced. ordinary returned-error Activate Invoke leaves last phase OPEN; successful C01 phase is OK; intent remains Q-14.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031958567291Z-returns/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `returns`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.phases.returned-errors). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

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

[Governing owner](open_questions.md#history.read.filter.default-modifiers). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

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

[Governing owner](open_questions.md#transaction.flag-sources.dependency-closure). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d22-new-q-22-q-26-the-top-record-reads-committed0-although-a-dependencys-nojournal-changes-were-kept"></a><!-- historical fragment alias -->
<a id="g-d22"></a>
### G:D22: The top record reads `Committed=0` although a dependency's changes were kept

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d22)

Reproduced by: e2e reproduced. same parent failure records committed=0 while middle dependency survives; successful parent neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.committed.escaped-dependency). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d23-new-q-27-the-global-history-delete-hides-every-error-not-only-permission-errors"></a><!-- historical fragment alias -->
<a id="g-d23"></a>
### G:D23: The global history delete hides every error, not only permission errors

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d23)

Reproduced by: e2e reproduced. ordinary global delete with unknown SQL column reports OK and retains sentinel; valid Package-filtered global delete removes it; nonpermission-error policy remains Q-27.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033221686336Z-readers/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `readers`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.delete.global-errors). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d24-history-find-shows-an-empty-log-with-an-ok-status-when-its-query-fails"></a><!-- historical fragment alias -->
<a id="g-d24"></a>
### G:D24: `history find` shows an empty log with an OK status when its query fails

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d24)

Reproduced by: e2e reproduced. misspelled history column returns OK; valid Success query neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.read.filter.invalid-column). No product repair or maintainer approval is claimed.

<a id="d25-q-05q-06b-a-callers-rollback-removes-the-record-of-a-load-it-wrapped"></a><!-- historical fragment alias -->
<a id="g-d25"></a>
### G:D25: A caller's rollback removes the record of a load it wrapped

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d25)

Reproduced by: e2e reproduced. supported caller transaction rollback removes the row/module; the ordinary no-caller successful return retains a finalized row; policy remains Q-05.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031958567291Z-returns/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `returns`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.committed.nested). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d28-q-23-a-dependencys-own-defaults-notransaction-does-not-keep-it-out-of-its-parents-transaction"></a><!-- historical fragment alias -->
<a id="g-d28"></a>
### G:D28: A dependency's own `NoTransaction` default does not keep it out of the parent's transaction

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d28)

Reproduced by: e2e reproduced. own NoTransaction dependency still rolls back with parent; successful Q23S neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#transaction.flag-sources.dependency-no-transaction). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d29-new-q-09-refused-update-reinstall-and-uninstall-attempts-leave-no-record"></a><!-- historical fragment alias -->
<a id="g-d29"></a>
### G:D29: Refused `update`, `reinstall` and `uninstall` attempts leave no record

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d29)

Reproduced by: e2e reproduced. ordinary update/reinstall/uninstall argument refusals allocate no history ID; successful/failed named load neighbors elsewhere; intent remains Q-09.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033221686336Z-readers/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `readers`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.record.unlogged-refusals). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="i3-q-03-q-24-loading-a-parent-replaces-a-dependency-that-is-in-developer-mode-and-clears-its-developer-mode"></a><!-- historical fragment alias -->
<a id="g-i3"></a>
### G:I3: Loading a parent replaces a dependency in developer mode and clears its developer mode

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-i3)

Reproduced by: e2e reproduced. ordinary parent load upgrades developer dependency and clears developer mode; same-version neighbor retains it.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#dev-mode.overwrite-guard.dependency-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="c-d4"></a>
### C:D4: A parameter value that contains `$` cannot be used. `%RegExReplace` passes the value as ICU replacement text and escapes only backslashes, so `$5` or an unfi...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d4)

Reproduced by: e2e reproduced. literal dollar Default makes load fail; five succeeds.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/modules.g5.md#module.parameter). No product repair or maintainer approval is claimed.

<a id="c-d7"></a>
### C:D7: `^1.2` (a caret on a two-part version) means `1.2.x` instead of `>=1.2.0 <2.0.0-0`. The branch written for it is unreachable, because a two-part version is a...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d7)

Reproduced by: partial reproduced. real range parser witness and valid caret neighbors; ordinary resolver caller with ^1.2 not re-executed.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d8"></a>
### C:D8: Combining two requirements depends on their order when two bounds differ only by a prerelease part: the first is kept. `^1.0.0` AND `<2.0.0` gives `<2.0.0-0`...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d8)

Reproduced by: e2e reproduced. named diamond load refusal with source-selected dependency bounds; successful neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d14"></a>
### C:D14: An OS environment value used in an `-env` file through `${VAR}` is inserted as ICU replacement text: `a$1b` becomes `aCONS_DOLLARb` (the variable's name), `a...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d14)

Reproduced by: e2e reproduced. ordinary -env load feeds altered dollar value to its real Invoke reader; five neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/modules.g5.md#module.env). No product repair or maintainer approval is claimed.

<a id="c-d15"></a>
### C:D15: The downgrade refusal misses prereleases of different versions: with `2.0.0-beta` installed, `load` of `1.0.0-rc` succeeds and replaces it. `Follows` treats ...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d15)

Reproduced by: e2e reproduced. ordinary prerelease downgrade accepted; release and mixed-release downgrades refused.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/load.g5.md#load.refusal.downgrade). No product repair or maintainer approval is claimed.

<a id="c-d19"></a>
### C:D19: Document export overwrites the deployed-item helper's error before its error guard. A controlled failing override followed by a real source export returns OK...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d19)

Reproduced by: partial reproduced (historical helper witness; not rerun). existing allowed deployed-helper override shows local status suppression; no ordinary default-helper failure/full package entry established; no new package/archive run.

[Governing owner](../ontology/features/build-and-publish.g5.md#build.package.deployed-status). No product repair or maintainer approval is claimed.

<a id="c-d16"></a>
### C:D16: First plain `load -lock` uses an empty stored Root and writes relative `module-lock.json` in process cwd. A developer-mode or direct nonempty-Root neighbor w...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d16)

Reproduced by: e2e reproduced. first ordinary load -lock writes valid audit-lock JSON in process cwd instead of source root; Root is populated by return, so the final record is not evidence of its earlier availability to the writer; developer-mode neighbor writes at source.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033609225862Z-lock/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `lock`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/dependencies.g5.md#deps.lockfile). No product repair or maintainer approval is claimed.

<a id="c-d24"></a>
### C:D24: The `$zpm("<command>")` function form raises `<COMMAND> Function must return a value` after running the command, pass or fail, so scripts cannot read the sta...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d24)

Reproduced by: e2e reproduced. $zpm(version) throws after command; Shell(version) returns success.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/root.g5.md#ipm.shell.forms). No product repair or maintainer approval is claimed.

<a id="c-d27"></a>
### C:D27: A supported processor returns an error from OnResolveChildren. Package refuses, but uninstall discards the status, deletes module/manifest metadata and leave...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d27)

Reproduced by: e2e reproduced. supported error-returning resource extension; uninstall deletes metadata while class remains callable; successful extension neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030635655399Z-error/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `error`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.child-error). No product repair or maintainer approval is claimed.

<a id="c-d28"></a>
### C:D28: A supported editing lock makes OBJ.Delete refuse and keep a class. Uninstall warns, returns success and deletes module/manifest metadata; class remains calla...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d28)

Reproduced by: e2e reproduced. real class edit lock; uninstall succeeds with callable class retained; unlocked uninstall removes it.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030643833060Z-iter11/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter11`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.class-delete-error). No product repair or maintainer approval is claimed.

<a id="c-d29"></a>
### C:D29: FileCopy Overlay=0 loads new files but retains unrelated old entries instead of replacing the directory. Direct CopyDir flags0/1 both keep a sentinel and rep...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d29)

Reproduced by: e2e reproduced. ordinary FileCopy Overlay=0 retains unrelated sentinel and writes new payload; real Unix filesystem provider.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/modules.g5.md#module.processor.file-copy.unix-overlay). No product repair or maintainer approval is claimed.

<a id="c-d31"></a>
### C:D31: A failing outer test runs a passing child through ordinary Shell. Outer device/status identify the failed outer run, but the outer JSON output file contains ...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d31)

Reproduced by: e2e reproduced. actual outer/child Shell test runs, final file/device identities and alone neighbors.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030004292651Z-iter17/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter17`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/build-and-publish.g5.md#build.test-output.run-identity). No product repair or maintainer approval is claimed.

<a id="c-d32"></a>
### C:D32: Cache a valid1.0.0 directory, then replace its manifest/code with2.0.0 without rebuilding the cache. Search/API stilloffer1, but ordinary explicit `install t...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d32)

Reproduced by: e2e reproduced. ordinary explicit-version install reads changed directory bytes; stable/rebuilt/refusal neighbors.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030008965338Z-iter21/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter21`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/repositories.g5.md#repo.install.version-request). No product repair or maintainer approval is claimed.

<a id="c-d33"></a>
### C:D33: Install a parent with exact dependency `1.0.0+snapshot`, change that dependency's source payload, then ordinarily reinstall the parent. The forced refresh ad...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d33)

Reproduced by: e2e reproduced. default snapshot reinstall refuses; shallow/release/explicit-permission neighbors.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030013086002Z-iter24/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter24`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version.range.snapshot-intersection). No product repair or maintainer approval is claimed.

<a id="p-f-dep-7"></a>
### P:F-DEP-7: The resolver is greedy (newest first, no backtracking past one level) and fails on solvable inputs, although its comment says "with backtracking". Known upst...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-7)

Reproduced by: e2e reproduced. ordinary solvable graph install refuses; pinned solution installs all modules.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/dependencies.g5.md#deps). No product repair or maintainer approval is claimed.

<a id="p-f-inst-10"></a>
### P:F-INST-10: An update that brings in a module for the first time runs all of that module's update steps instead of seeding them as a fresh install does.

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-10)

Reproduced by: e2e reproduced. new dependency installed during update runs V1; fresh normal install seeds without running.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#update.steps.fresh-seeding-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-inst-4"></a>
### P:F-INST-4: Between registries that offer the same version, the registry created first wins; `OverriddenSortOrder` has no effect. Deleting and re-adding a registry chang...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-4)

Reproduced by: e2e reproduced. two filesystem repositories with tied versions; creation-order selection and reordered neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/repositories.g5.md#repo.search-order). No product repair or maintainer approval is claimed.

<a id="p-f-inst-5"></a>
### P:F-INST-5: `update -dev` from a registry does not run in developer mode, and a plain `update` of a module stored in developer mode clears it. Only `update -dev -path` k...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-5)

Reproduced by: e2e reproduced. ordinary update -dev from filesystem registry ends normal; local -path developer neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/update.g5.md#update.dev-mode). No product repair or maintainer approval is claimed.

<a id="p-f-inst-7"></a>
### P:F-INST-7: A class of the update package whose name starts with `V` but has no digit (e.g. `Validator`) makes every update of the module fail with `<SUBSCRIPT>`, naming...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-7)

Reproduced by: e2e reproduced. real Validator fixture makes update fail; numbered-class neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/update.g5.md#update.steps). No product repair or maintainer approval is claimed.

<a id="p-f-inst-8"></a>
### P:F-INST-8: A developer-mode install (or a plain `load` of a module stored in developer mode) seeds no update steps, so the next update reruns every step from V1, includ...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-8)

Reproduced by: e2e reproduced. developer fresh install leaves zero step records; later update executes old steps; normal fresh install seeds.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#update.steps.dev-seeding-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-inst-9"></a>
### P:F-INST-9: Install silently replaces a needed module that is in developer mode and clears its developer mode; the guard covers only the named module. Wiki Concepts: a d...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-9)

Reproduced by: e2e reproduced. ordinary parent install replaces developer dependency; initial developer load verifies prestate.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#dev-mode.overwrite-guard.dependency-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-nsp-2"></a>
### P:F-NSP-2: In a namespace that gets IPM by mapping, an `IPMVersion` requirement is checked against an empty version, so every module with such a requirement is refused ...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-2)

Reproduced by: e2e reproduced. ordinary requirement load fails in mapped namespace, succeeds in source namespace.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/namespaces.g5.md#namespace.version.mapped-reader). No product repair or maintainer approval is claimed.

<a id="p-f-nsp-4"></a>
### P:F-NSP-4: `enable -map -repos` (and `enable -community`) on a namespace with no IPM maps the code but skips the registries; a second run is needed. The repo step reads...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-4)

Reproduced by: e2e reproduced. enable -map -repos skips first repo mapping; same command second run maps it.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/namespaces.g5.md#namespace.enable.mapping-results). No product repair or maintainer approval is claimed.

<a id="p-f-nsp-6"></a>
### P:F-NSP-6: `unmap` leaves the `%IPM.*` routine mapping: the namespace still runs the source's IPM and is still reported as mapped. Argument-slot bug: the third argument...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-6)

Reproduced by: e2e reproduced. unmap removes package mapping but routine mapping remains and IPM still callable; source refusal neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/namespaces.g5.md#namespace.unmap.routine-result). No product repair or maintainer approval is claimed.

<a id="p-f-repo-5"></a>
### P:F-REPO-5: `default-modifiers` are documented per namespace but stored in `^%IPM.DefaultModifiers`, which IRIS keeps in IRISSYS: setting `-DNoTransaction=1` in one name...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-5)

Reproduced by: e2e reproduced. real namespaces read shared default modifiers; local namespace settings neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/settings.g5.md#settings.default-modifiers). No product repair or maintainer approval is claimed.

<a id="p-f-sync-1"></a>
### P:F-SYNC-1: A failed `compile`, `load -dev` or `reload` that puts edited source on the server records no baseline and no retry mark. If the file is then reverted to its ...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-sync-1)

Reproduced by: e2e reproduced. ordinary failed compile then exact-byte revert makes sync skip stale server definition; successful initial load neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#devtools.sync.failed-import-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-unin-2"></a>
### P:F-UNIN-2: `uninstall -purge` deletes no table data. Nothing reads the `Purge` parameter; `%Clean` deletes classes without the extent flag `e`. (An installer can read `...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-2)

Reproduced by: e2e reproduced. ordinary persistent sentinel survives -purge and schema removal/reload; real extent-delete and installer callback neighbors.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030017516810Z-iter27/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter27`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.purge.as-built). No product repair or maintainer approval is claimed.

<a id="p-f-unin-5"></a>
### P:F-UNIN-5: A global resource declared only by name (`<Resource Name="X.GBL"/>`, the usual form) keeps its data after uninstall; only resources with the `Global` attribu...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-5)

Reproduced by: e2e reproduced. ordinary name-only global resource retains data; explicit Global attribute deletes it.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall). No product repair or maintainer approval is claimed.

<a id="p-f-unin-6"></a>
### P:F-UNIN-6: A blocked uninstall returns OK, prints `Clean SUCCESS` and logs `uninstall/success` while the module stays. The code comment at `Lifecycle/Base.cls#L276` say...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-6)

Reproduced by: e2e reproduced. blocked uninstall returns OK and logs success with module retained; successful ordinary uninstall neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.dependents). No product repair or maintainer approval is claimed.

<a id="c-d2"></a>
### C:D2: When IPM cannot create a load's temporary `.modules` repository, it prints "Failed to clean up repository 'ipm-temp-modules-N'. You may need to manually dele...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d2)

Reproduced by: e2e reproduced. TR7 missing-row cleanup warning; TR1 succeeds.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/load.g5.md#load.dependencies.local-repo). No product repair or maintainer approval is claimed.

<a id="c-d3"></a>
### C:D3: Parameters that refer to each other in a loop are never reported. The 20-level guard that throws "Circular reference or too many levels in placeholders" is u...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d3)

Reproduced by: e2e reproduced. cyclic Default reaches ordinary load error; acyclic neighbor succeeds.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/modules.g5.md#module.parameter). No product repair or maintainer approval is claimed.

<a id="c-d5"></a>
### C:D5: A parameter name is placed into a regular expression unescaped, so a dotted name such as `a.b` also fills `{$axb}`. Dotted names are the documented way to ne...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d5)

Reproduced by: e2e reproduced. a.b Default changes {$axb} in the command output; exact ordinary value neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/modules.g5.md#module.parameter). No product repair or maintainer approval is claimed.

<a id="c-d9"></a>
### C:D9: Build metadata containing a hyphen is parsed as a prerelease: `1.2.3+build-x` becomes `1.2.3-x+build-x`, which sorts before `1.2.3` and matches only itself a...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d9)

Reproduced by: e2e reproduced. ordinary module load stores the malformed parsed build version; ordinary 1.0.0 neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version). No product repair or maintainer approval is claimed.

<a id="c-d10"></a>
### C:D10: The `&&` conflict error prints literal placeholders: "No overlapping versions in && operation for %1 and %2". `$$$ERROR($$$GeneralError, ...)` takes one argu...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d10)

Reproduced by: partial reproduced. real AND helper error text; full conflicting-dependency command message path remains.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d12"></a>
### C:D12: `history find` shows the result of an action that has not ended (still running or killed) as `ERROR #00: (no error description)`, so an operator reads it as ...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d12)

Reproduced by: e2e reproduced. history find displays ERROR #00 for an unfinished record; completed history neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.record). No product repair or maintainer approval is claimed.

<a id="c-d13"></a>
### C:D13: `module-version <module>` with no version, or with the current version, fails with "Cannot set version to X as it is not strictly higher than the current ver...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d13)

Reproduced by: e2e reproduced. module-version unchanged refusal; patch bump succeeds.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/developer-tools.g5.md#devtools.authoring.module-version). No product repair or maintainer approval is claimed.

<a id="c-d21"></a>
### C:D21: The test path converter strips the resource folder as raw leading text. A path outside the folder keeps its folders in the class name, although the method co...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d21)

Reproduced by: partial reproduced. real private converter exposed by subclass and valid inside-path neighbor; sync routing reachability remains untraced.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/developer-tools.g5.md#devtools.sync.test-paths). No product repair or maintainer approval is claimed.

<a id="p-f-dep-1"></a>
### P:F-DEP-1: `PermitDowngrade` is half implemented: the resolver drops the "≥ installed" bound and picks an older version, then the load refuses any downgrade.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-1)

Reproduced by: e2e reproduced. PermitDowngrade relaxes resolution then load refuses; normal load bound refusal neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/dependencies.g5.md#deps). No product repair or maintainer approval is claimed.

<a id="p-f-dep-2"></a>
### P:F-DEP-2: Lock-file dependencies are written in name order, not dependency order, although `LockFile.cls#L24` promises least-dependent first. Latent (no `ci` command y...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-2)

Reproduced by: e2e reproduced. ordinary create-lockfile emits name order while load output is dependency first; downstream ci is unimplemented.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/dependencies.g5.md#deps.lockfile). No product repair or maintainer approval is claimed.

<a id="p-f-dep-3"></a>
### P:F-DEP-3: `list-dependents` with a capitalised name (as in the help examples) prints nothing.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-3)

Reproduced by: e2e reproduced. list-dependents lowercase displays parent; mixed case displays none.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/inspection.g5.md#inspect.dependents). No product repair or maintainer approval is claimed.

<a id="p-f-dep-8"></a>
### P:F-DEP-8: A failed `install -create-lockfile` leaves a new lock file behind (written before the module's own Activate, outside the transaction). PR #963's author said ...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-8)

Reproduced by: e2e reproduced. failed create-lockfile install leaves JSON while module/dependency rows disappear; successful lockfile neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/dependencies.g5.md#deps.lockfile). No product repair or maintainer approval is claimed.

<a id="p-f-dep-9"></a>
### P:F-DEP-9: A needed module loaded from a folder (no registry) makes lock-file creation throw, so no file is written; the base module's empty registry is already handled.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-9)

Reproduced by: e2e reproduced. ordinary local-loaded dependency produces lockfile warning and no changed file; registry supplier neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#deps.lockfile.local-supplier-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-load-4"></a>
### P:F-LOAD-4: A load can fail after its final commit: an after-install message with `{` and a `$$$` macro that does not compile throws in `%EvaluateMacro`. The command ret...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-load-4)

Reproduced by: e2e reproduced. ordinary after-install macro failure with durable module/history committed; successful message neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/load.g5.md#load). No product repair or maintainer approval is claimed.

<a id="p-f-repo-4"></a>
### P:F-REPO-4: `config delete` restores the first value ever set, not the factory default, for keys the installer does not seed (`TerminalPrompt`, `PublishTimeout`, `Histor...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-4)

Reproduced by: e2e reproduced. config delete restores first stored value; recorded changed-value prestate.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/settings.g5.md#settings). No product repair or maintainer approval is claimed.

<a id="p-f-repo-8"></a>
### P:F-REPO-8: `config` with an unknown key or a bad value prints a message and returns OK, so scripts cannot detect typos.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-8)

Reproduced by: e2e reproduced. ordinary config invalid key/value returns OK; valid config neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/settings.g5.md#settings.print). No product repair or maintainer approval is claimed.

<a id="p-f-sync-3"></a>
### P:F-SYNC-3: `reload` of a module not in developer mode (no root) loads nothing and reports SUCCESS.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-sync-3)

Reproduced by: e2e reproduced. ordinary reload of normal module succeeds without reloading edited file; root-bearing developer load neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#lifecycle.empty-reload-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-unin-9"></a>
### P:F-UNIN-9: A dependency removed by `uninstall -r` keeps its stored manifest in `^IPM.StudioDoc.ModuleStreamD`; only the top module's manifest is deleted.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-9)

Reproduced by: e2e reproduced. ordinary recursive uninstall removes dependency code/module but retains its manifest; top manifest is removed.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.recurse). No product repair or maintainer approval is claimed.

<a id="g-oos-1"></a>
### G:OOS-1 review record

Severity: Unranked

G:OOS-1: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-oos-1)

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

<a id="c-d17"></a>
### C:D17 review record

Severity: Unranked

C:D17: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d17)

<a id="c-d18"></a>
### C:D18 review record

Severity: Unranked

C:D18: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d18)

<a id="c-d20"></a>
### C:D20 review record

Severity: Unranked

C:D20: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d20)

<a id="c-d22"></a>
### C:D22 review record

Severity: Unranked

C:D22: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d22)

<a id="c-d23"></a>
### C:D23 review record

Severity: Unranked

C:D23: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d23)

<a id="c-d25"></a>
### C:D25 review record

Severity: Unranked

C:D25: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d25)

<a id="c-d26"></a>
### C:D26 review record

Severity: Unranked

C:D26: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d26)

<a id="c-d30"></a>
### C:D30 review record

Severity: Unranked

C:D30: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d30)

<a id="p-f-repo-1"></a>
### P:F-REPO-1 review record

Severity: Unranked

P:F-REPO-1: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-1)

<a id="p-f-repo-10"></a>
### P:F-REPO-10 review record

Severity: Unranked

P:F-REPO-10: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-10)

<a id="p-f-repo-11"></a>
### P:F-REPO-11 review record

Severity: Unranked

P:F-REPO-11: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-11)

<a id="p-f-repo-2"></a>
### P:F-REPO-2 review record

Severity: Unranked

P:F-REPO-2: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-2)

<a id="p-f-repo-7"></a>
### P:F-REPO-7 review record

Severity: Unranked

P:F-REPO-7: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-7)

<a id="p-f-unin-3"></a>
### P:F-UNIN-3 review record

Severity: Unranked

P:F-UNIN-3: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-3)

<a id="p-f-unin-8"></a>
### P:F-UNIN-8 review record

Severity: Unranked

P:F-UNIN-8: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-8)

<a id="p-f-dep-4"></a>
### P:F-DEP-4 review record

Severity: Unranked

P:F-DEP-4: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-4)

<a id="p-f-dep-5"></a>
### P:F-DEP-5 review record

Severity: Unranked

P:F-DEP-5: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-5)

<a id="p-f-dep-6"></a>
### P:F-DEP-6 review record

Severity: Unranked

P:F-DEP-6: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-6)

<a id="p-f-hist-1"></a>
### P:F-HIST-1 review record

Severity: Unranked

P:F-HIST-1: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-1)

<a id="p-f-hist-10"></a>
### P:F-HIST-10 review record

Severity: Unranked

P:F-HIST-10: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-10)

<a id="p-f-hist-2"></a>
### P:F-HIST-2 review record

Severity: Unranked

P:F-HIST-2: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-2)

<a id="p-f-hist-3"></a>
### P:F-HIST-3 review record

Severity: Unranked

P:F-HIST-3: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-3)

<a id="p-f-hist-4"></a>
### P:F-HIST-4 review record

Severity: Unranked

P:F-HIST-4: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-4)

<a id="p-f-hist-5"></a>
### P:F-HIST-5 review record

Severity: Unranked

P:F-HIST-5: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-5)

<a id="p-f-hist-6"></a>
### P:F-HIST-6 review record

Severity: Unranked

P:F-HIST-6: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-6)

<a id="p-f-hist-7"></a>
### P:F-HIST-7 review record

Severity: Unranked

P:F-HIST-7: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-7)

<a id="p-f-hist-8"></a>
### P:F-HIST-8 review record

Severity: Unranked

P:F-HIST-8: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-8)

<a id="p-f-hist-9"></a>
### P:F-HIST-9 review record

Severity: Unranked

P:F-HIST-9: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-9)

<a id="p-f-inst-11"></a>
### P:F-INST-11 review record

Severity: Unranked

P:F-INST-11: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-11)

<a id="p-f-inst-2"></a>
### P:F-INST-2 review record

Severity: Unranked

P:F-INST-2: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-2)

<a id="p-f-inst-3"></a>
### P:F-INST-3 review record

Severity: Unranked

P:F-INST-3: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-3)

<a id="p-f-load-1"></a>
### P:F-LOAD-1 review record

Severity: Unranked

P:F-LOAD-1: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-load-1)

<a id="p-f-load-2"></a>
### P:F-LOAD-2 review record

Severity: Unranked

P:F-LOAD-2: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-load-2)

<a id="p-f-load-3"></a>
### P:F-LOAD-3 review record

Severity: Unranked

P:F-LOAD-3: inconclusive/execution-scope-blocked. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-load-3)

<a id="p-f-nsp-1"></a>
### P:F-NSP-1 review record

Severity: Unranked

P:F-NSP-1: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-1)

<a id="p-f-nsp-10"></a>
### P:F-NSP-10 review record

Severity: Unranked

P:F-NSP-10: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-10)

<a id="p-f-nsp-11"></a>
### P:F-NSP-11 review record

Severity: Unranked

P:F-NSP-11: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-11)

<a id="p-f-nsp-12"></a>
### P:F-NSP-12 review record

Severity: Unranked

P:F-NSP-12: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-12)

<a id="p-f-nsp-3"></a>
### P:F-NSP-3 review record

Severity: Unranked

P:F-NSP-3: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-3)

<a id="p-f-nsp-5"></a>
### P:F-NSP-5 review record

Severity: Unranked

P:F-NSP-5: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-5)

<a id="p-f-nsp-7"></a>
### P:F-NSP-7 review record

Severity: Unranked

P:F-NSP-7: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-7)

<a id="p-f-nsp-8"></a>
### P:F-NSP-8 review record

Severity: Unranked

P:F-NSP-8: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-8)

<a id="p-f-nsp-9"></a>
### P:F-NSP-9 review record

Severity: Unranked

P:F-NSP-9: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-9)

<a id="p-f-repo-12"></a>
### P:F-REPO-12 review record

Severity: Unranked

P:F-REPO-12: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-12)

<a id="p-f-repo-3"></a>
### P:F-REPO-3 review record

Severity: Unranked

P:F-REPO-3: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-3)

<a id="p-f-repo-6"></a>
### P:F-REPO-6 review record

Severity: Unranked

P:F-REPO-6: refuted/unreachable-CLI-trigger. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-6)

<a id="p-f-repo-9"></a>
### P:F-REPO-9 review record

Severity: Unranked

P:F-REPO-9: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-9)

<a id="p-f-res-1"></a>
### P:F-RES-1 review record

Severity: Unranked

P:F-RES-1: alias. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-1)

<a id="p-f-res-2"></a>
### P:F-RES-2 review record

Severity: Unranked

P:F-RES-2: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-2)

<a id="p-f-res-3"></a>
### P:F-RES-3 review record

Severity: Unranked

P:F-RES-3: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-3)

<a id="p-f-res-4"></a>
### P:F-RES-4 review record

Severity: Unranked

P:F-RES-4: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-4)

<a id="p-f-res-5"></a>
### P:F-RES-5 review record

Severity: Unranked

P:F-RES-5: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-5)

<a id="p-f-res-6"></a>
### P:F-RES-6 review record

Severity: Unranked

P:F-RES-6: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-6)

<a id="p-f-res-7"></a>
### P:F-RES-7 review record

Severity: Unranked

P:F-RES-7: inconclusive/execution-scope-blocked. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-7)

<a id="p-f-res-8"></a>
### P:F-RES-8 review record

Severity: Unranked

P:F-RES-8: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-8)

<a id="p-f-sync-2"></a>
### P:F-SYNC-2 review record

Severity: Unranked

P:F-SYNC-2: documentation-or-test-gap. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-sync-2)

<a id="p-f-unin-10"></a>
### P:F-UNIN-10 review record

Severity: Unranked

P:F-UNIN-10: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-10)

<a id="p-f-unin-11"></a>
### P:F-UNIN-11 review record

Severity: Unranked

P:F-UNIN-11: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-11)

<a id="p-f-unin-7"></a>
### P:F-UNIN-7 review record

Severity: Unranked

P:F-UNIN-7: inconclusive. [Disposition, source and remaining gate](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-7)
