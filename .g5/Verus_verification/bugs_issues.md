# IPM bugs and issues

Each entry explains a recorded problem, its impact and the evidence. Findings that depend on an unresolved policy say so in their descriptions.

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

<a id="d1-a-dependencys-history-record-is-lost-when-its-parents-transaction-rolls-back"></a><!-- historical fragment alias -->
<a id="g-d1"></a>
### G:D1: A dependency's history record is lost when its parent's transaction rolls back

Severity: Critical · Decision: Open

Description: When loading a dependency inside its parent's transaction, a later parent failure can erase the dependency's history record. The audit trail then loses evidence of work that was attempted.

Severity basis: A parent rollback removes a dependency's durable audit record entirely. The through-product witness establishes audit-data loss, which the G5 severity convention rates Critical.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d1)

Reproduced by: e2e reproduced. dependency row absent after parent rollback; successful load neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.survives-rollback). No product repair or maintainer approval is claimed.

<a id="d2-a-filter-column-with-_-drops-every-later-filter-so-history-find-widens-and-a-filtered-history-delete-can-remove-every-record"></a><!-- historical fragment alias -->
<a id="g-d2"></a>
### G:D2: A filter column with `_` drops every later filter, so find widens and a filtered delete can remove every record

Severity: Critical · Decision: Open

Description: A history filter whose column name contains an underscore causes later filters to be ignored. A search can return too much history, and a filtered deletion can remove records the caller did not select.

Severity basis: A documented version-part filter can turn a filtered delete into deletion of the entire history. The witness establishes unintended loss of unrelated persisted audit data.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d2)

Reproduced by: e2e reproduced. history delete removes three seeded identities despite unmatched Version_Major; Package filter preserves nonmatches.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.read.filter). No product repair or maintainer approval is claimed.

<a id="d25-q-05q-06b-a-callers-rollback-removes-the-record-of-a-load-it-wrapped"></a><!-- historical fragment alias -->
<a id="g-d25"></a>
### G:D25: A caller's rollback removes the record of a load it wrapped

Severity: Critical · Decision: Open

Description: If a caller wraps a load in its own transaction and rolls back, the load's history record disappears with the module changes. The intended audit behavior for this case still needs a human decision.

Severity basis: A caller rollback removes the action's durable history. Critical is the conditional audit-data-loss impact if caller-survival is required; Q-05 remains unresolved.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d25)

Reproduced by: e2e reproduced. supported caller transaction rollback removes the row/module; the ordinary no-caller successful return retains a finalized row; policy remains Q-05.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031958567291Z-returns/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `returns`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.committed.nested). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="c-d6"></a>
### C:D6: An operator on a partial version is dropped. `<2` means `2.x` (`>=2.0.0 <3.0.0-0`), `>=1.2` and `>1.2` mean `1.2.x`, `<=1` means `1.x`. A dependency declared...

Severity: High · Decision: Open

Description: Version requirements such as less than 2 lose their comparison operator when the version is abbreviated. Dependency resolution can install a version outside the range the module requested.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d6)

Reproduced by: e2e reproduced. ordinary dependency load installs 2.5.0 for <2; valid dependency neighbor Dok.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d39"></a>
### C:D39: A filesystem repository install picks a prerelease over its own release and orders prerelease numbers as text

Severity: High · Decision: Open

Description: A filesystem repository can choose a prerelease over the corresponding stable release and sort numbered prereleases incorrectly. Installing without an exact version can select an older or less stable package than expected.

Run: Uplift 2026-11-16

Found by: iteration34 exploration (`iter34/attempts/explore1.log`) refuting the source prediction that install follows the version index, then source review of `Repo/Utils.cls::moduleSqlToList`, and [iteration34 model](consolidated/iter34/versions.rs) `release_versus_own_prerelease` and `fixture_neighbors` with controls `sql-release-first` and `sql-prerelease-semantic` ([proof receipt](../uplift/consolidated/iter34/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration34 native receipt](../uplift/consolidated/iter34/native/20261116T172512256458Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`, filesystem repositories created by `repo -fs` (which stores prereleases on). With `i34d` offered at 2.0.0 and 2.0.0-beta.1, `install i34d` installs 2.0.0-beta.1 (case d1) while the repository's own cache query lists 2.0.0 first (q5). With `i34c` at 0.9.0, 1.0.0-rc.2 and 1.0.0-rc.10, `install i34c` installs rc.2 (c1) while the cache query lists rc.10 first (q3). Passing neighbours: `install i34d 2.0.0` installs the release (d2), and with releases only, `install i34b 1.x` installs 1.10.0 over 1.9.0 (a3).

`moduleSqlToList` orders the cache rows by major, minor and patch, then by the prerelease column as text in descending order, and keeps the first row per module. That text order puts a release, whose prerelease is empty, after its own prereleases, and `rc.2` before `rc.10`. The repository manager then sees one offer per module, so Main.Install installs it. The method's comment says the query "returns items in correct order", and Main.Install says "the latest version will always be first". The [version order](../ontology/concepts/versions.g5.md#version.order) and the [search rule](../ontology/concepts/repositories.g5.md#repo.search-order) require the release and rc.10. A repair would keep the index order (`m.%id`) instead of the explicit version columns. It was not checked.

<a id="c-d40"></a>
### C:D40: Uninstalling a Copy resource deletes classes that Overwrite="0" kept and the module never wrote

Severity: High · Decision: Open

Description: A Copy resource can preserve a pre-existing class during installation but delete it during uninstall. Removing the module can therefore destroy a class that the module never installed.

Run: Uplift 2026-11-16

Found by: iteration37 native exploration (`iter37/attempts/explore5.log`), then source review of `Lifecycle/Base.cls::%Clean` child resolution against `ResourceProcessor/Copy.cls::OnAfterPhase`, and [iteration37 model](consolidated/iter37/copy.rs) `uninstall_deletes_kept_item` with control `uninstall-only-copied` ([proof receipt](../uplift/consolidated/iter37/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration37 native receipt](../uplift/consolidated/iter37/native/20261116T173211781373Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. The installing namespace already holds `CopyB.D1` (returns 4) before module `cpb` copies package `CopyB` from namespace CPSRC with `Overwrite="0"`. The load keeps D1 at 4 and copies only D2 (o2). `uninstall cpb` then deletes both D1 and D2 (u2). Passing neighbours: `CopyD.Owned`, listed by another installed module, survives `uninstall cpd` while the copied `CopyD.Free` is removed (u4). With the default `Overwrite`, the replaced pre-existing `CopyA.C2` is also removed by `uninstall cpa` (u1).

The Copy load writes only the classes it exports from the source, but uninstall resolves the package in the installing namespace and deletes every class found there that no other module owns. Code a user kept by choosing `Overwrite="0"` is therefore lost on uninstall, contrary to the [Copy statement](../ontology/features/resources.g5.md#module.processor.copy) and to uninstall removing the module's own resources. A repair would record the classes each load actually copied and delete only those. It was not checked.

<a id="c-d60"></a>
### C:D60: Uninstalling a CSPApplication without DeployPath deletes the module's source folder

Severity: High · Decision: Open

Description: Uninstalling a web application without a separate deployment path deletes files from the module's source folder. A developer can lose source files that were never copied into an installed deployment.

Run: Uplift 2026-11-16

Found by: source review of `ResourceProcessor/CSPApplication.cls::DeleteCSPApp` against the default directory recorded in [module.csp.directory](../ontology/features/web-applications.g5.md#module.csp.directory), then [iteration57 model](consolidated/iter57/cspapp.rs) `no_deploy_path_uninstall_deletes_source` with control `purge-skips-source-folder` ([proof receipt](../uplift/consolidated/iter57/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration57 native receipt](../uplift/consolidated/iter57/native/20261116T182314549001Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. After `load m57b`, whose `<CSPApplication>` has no DeployPath, `uninstall m57b` removes the module's own source folder `/tmp/iter57/m57b/web/` and its `index.html` (case b2). Passing neighbour: with DeployPath, uninstall purges only the deployed copy and the source survives (a3).

Uninstall destroys files outside the namespace that the module never installed, which a developer may not have elsewhere, contrary to the [CSP lifecycle statement](../ontology/features/web-applications.g5.md#module.csp.lifecycle) and uninstall removing only the module's resources. A repair would never purge a directory inside the module's source root, or purge only a DeployPath. It was not checked.

<a id="p-f-inst-1"></a>
### P:F-INST-1: `update` reports success without running update steps when the update lock is busy for 3 s. No message unless verbose. The steps stay due, and nothing prompt...

Severity: High · Decision: Open

Description: When another process holds the update lock, update can return success without running any pending steps or giving a normal warning. Whether it should fail or report a skipped update remains an open policy decision.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-1)

Reproduced by: e2e reproduced. real competing update lock; command OK with old step rows; unlocked retry runs due steps.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#update.steps.busy-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-inst-6"></a>
### P:F-INST-6: A class of the update package named `V<n><something>` (e.g. `V3Helper`) replaces step class `V<n>` in the step list; update then reports success without runn...

Severity: High · Decision: Open

Description: A helper class whose name starts like a numbered update step can replace the real step in the execution list. Update reports success while leaving required migration work undone.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-6)

Reproduced by: e2e reproduced. real V3Helper fixture suppresses V3 execution; V2/V9/V10 ordinary order neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/update.g5.md#update.steps). No product repair or maintainer approval is claimed.

<a id="p-f-unin-1"></a>
### P:F-UNIN-1: A blocked uninstall (a dependent exists, or global scope) still deletes the module's web and CSP applications and runs `Clean`/`Unconfigure` hooks, then prin...

Severity: High · Decision: Open

Description: An uninstall that should be blocked can still remove web applications and run cleanup hooks before leaving the module installed. The remaining module can be damaged even though removal was refused.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-1)

Reproduced by: e2e reproduced. blocked uninstall deletes actual test web application; module remains; ordinary global-resource uninstall neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.dependents.as-built). No product repair or maintainer approval is claimed.

<a id="p-f-unin-4"></a>
### P:F-UNIN-4: Uninstall without `-force` removes a module that a dependent still needs if that dependent's dependency graph fails to build (e.g. a registry is missing): gr...

Severity: High · Decision: Open

Description: If IPM cannot build a dependent module's dependency graph, uninstall ignores that failure and removes a required module. Other installed modules can be left without a dependency they still need.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-4)

Reproduced by: e2e reproduced. dependent graph error lets uninstall remove required class; resolvable dependent neighbor blocks.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.dependents). No product repair or maintainer approval is claimed.

<a id="c-d32"></a>
### C:D32: Cache a valid1.0.0 directory, then replace its manifest/code with2.0.0 without rebuilding the cache. Search/API stilloffer1, but ordinary explicit `install t...

Severity: High · Decision: Open

Description: A filesystem repository's cached version can disagree with the files currently stored in its directory. An explicit install of the cached version can succeed while installing different code and a different version.

Severity basis: An explicit requested-version install succeeds with another version's callable bytes. This is a wrong installation result, not merely stale listing text.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d32)

Reproduced by: e2e reproduced. ordinary explicit-version install reads changed directory bytes; stable/rebuilt/refusal neighbors.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030008965338Z-iter21/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter21`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/repositories.g5.md#repo.install.version-request). No product repair or maintainer approval is claimed.

<a id="d3-the-retention-purge-deletes-nothing-and-still-marks-the-days-purge-as-done"></a><!-- historical fragment alias -->
<a id="g-d3"></a>
### G:D3: The retention purge deletes nothing and marks the day's purge done

Severity: Medium · Decision: Open

Description: The history-retention job marks its daily cleanup complete without deleting expired records. Old history keeps accumulating while the status suggests that retention has been enforced.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d3)

Reproduced by: e2e reproduced. ordinary list triggers retention; old row survives with marker today; retention-off neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/data-policy.g5.md#history.retention). No product repair or maintainer approval is claimed.

<a id="d4-q-17-refused--dev-notransaction-or-nojournal-loads-keep-the-module-and-read-committed-when-system-requirements-fail"></a><!-- historical fragment alias -->
<a id="g-d4"></a>
### G:D4: Refused `-dev`, NoTransaction or NoJournal loads keep the module and read committed when system requirements fail

Severity: Medium · Decision: Open

Description: A load refused for unmet system requirements can leave module data behind in developer or nontransactional modes. The history can also mark it committed; the intended refusal policy still needs a decision.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d4)

Reproduced by: e2e reproduced. SYSNamespace refusal retains developer module; ordinary-mode refusal leaves none; policy remains open.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#load.refusal.requirements.timing). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d6-q-14-an-invoke-that-returns-an-error-leaves-its-phase-open-in-a-finalized-record"></a><!-- historical fragment alias -->
<a id="g-d6"></a>
### G:D6: An Invoke that returns an error leaves its phase open

Severity: Medium · Decision: Open

Description: When an invoked lifecycle hook returns an error, its history phase can remain marked open. The record looks unfinished rather than failed, and the intended final status is still awaiting clarification.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d6)

Reproduced by: e2e reproduced. ordinary returned-error Activate Invoke leaves last phase OPEN; successful C01 phase is OK; intent remains Q-14.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031958567291Z-returns/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `returns`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.phases.returned-errors). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d9-a-refused-load-records-an-empty-package-although-the-manifest-named-the-module"></a><!-- historical fragment alias -->
<a id="g-d9"></a>
### G:D9: A refused load records an empty package

Severity: Medium · Decision: Open

Description: A load refused before installation can record an empty package name in history. Operators cannot identify the affected module from the normal audit fields.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d9)

Reproduced by: e2e reproduced. ordinary downgrade refusal followed by history details; named successful history neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.fields). No product repair or maintainer approval is claimed.

<a id="d11-manifest-refusals-under--dnotransaction1-or--dnojournal1-keep-module-data"></a><!-- historical fragment alias -->
<a id="g-d11"></a>
### G:D11: Manifest refusals under NoTransaction or NoJournal keep module data

Severity: Medium · Decision: Open

Description: A load that rejects its manifest can retain imported module data when transaction or journaling options are disabled. The failure result does not by itself mean that no state was kept.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d11)

Reproduced by: e2e reproduced. NoJournal manifest refusal leaves the imported ZPM document at 2.0.0; successful NoJournal neighbor; broader cases remain in starting evidence.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031958567291Z-returns/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `returns`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history). No product repair or maintainer approval is claimed.

<a id="d12-q-31-a-notransaction-or-nojournal-load-refused-after-the-no-transaction-marker-reads-committed1-with-nothing-kept"></a><!-- historical fragment alias -->
<a id="g-d12"></a>
### G:D12: Refusals after the no-transaction marker read `Committed=1` with nothing kept

Severity: Medium · Decision: Open

Description: A refused nontransactional load can be marked committed even when it kept no module changes. What that flag should mean for this case remains an open policy question.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d12)

Reproduced by: e2e reproduced. no-module-definition refusal under NoTransaction keeps no module and reports committed=1; ordinary successful NoJournal neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031958567291Z-returns/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `returns`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d13-a-load-that-committed-reports-invalid-oref-when-the-temporary-modules-repository-could-not-be-created"></a><!-- historical fragment alias -->
<a id="g-d13"></a>
### G:D13: A load that committed reports `<INVALID OREF>` when the `.modules` repository cannot be created

Severity: Medium · Decision: Open

Description: A load can commit successfully and then fail while creating its temporary local repository. The caller receives an internal error even though the module remains installed.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d13)

Reproduced by: e2e reproduced. ordinary directory load hits failed temporary repository creation after commit; module remains; valid .modules neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history). No product repair or maintainer approval is claimed.

<a id="d18-q-18-namespace-default-modifiers-become-history-filters"></a><!-- historical fragment alias -->
<a id="g-d18"></a>
### G:D18: Namespace default modifiers become `history` filters

Severity: Medium · Decision: Open

Description: Default command modifiers are reused as history filters, which can hide records from an ordinary history search. Whether those defaults should affect history queries remains an open policy decision.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d18)

Reproduced by: e2e reproduced. stored NoTransaction modifier makes ordinary history find hide seeded row while returning OK; deleting default restores the same row.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033221686336Z-readers/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `readers`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.read.filter.default-modifiers). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d20-the-globalrecords-query-returns-the-current-namespaces-rows-once-per-namespace"></a><!-- historical fragment alias -->
<a id="g-d20"></a>
### G:D20: `GlobalRecords` returns the current namespace's rows once per namespace

Severity: Medium · Decision: Open

Description: The global history API repeats the current namespace's records instead of reading each namespace's own records. Its result contains duplicates and omits activity from other namespaces.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d20)

Reproduced by: e2e reproduced. public GlobalRecords repeats USER history and misses seeded other namespace; local Records returns exactly one correct row.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033221686336Z-readers/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `readers`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.read.api). No product repair or maintainer approval is claimed.

<a id="d21-new-q-22-q-25-a-failed-normal-mode-load-can-leave-an-installed-module-without-its-dependency"></a><!-- historical fragment alias -->
<a id="g-d21"></a>
### G:D21: A failed load can leave an installed module without its dependency

Severity: Medium · Decision: Open

Description: A failed parent load can keep a dependency installed while rolling back that dependency's own child. This leaves an incomplete installation; the intended transaction policy for these mixed modes remains open.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d21)

Reproduced by: e2e reproduced. ordinary parent failure leaves NoJournal middle dependency while its own child is gone; successful NESTS neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#transaction.flag-sources.dependency-closure). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d22-new-q-22-q-26-the-top-record-reads-committed0-although-a-dependencys-nojournal-changes-were-kept"></a><!-- historical fragment alias -->
<a id="g-d22"></a>
### G:D22: The top record reads `Committed=0` although a dependency's changes were kept

Severity: Medium · Decision: Open

Description: A parent operation can be marked uncommitted even though a dependency's changes survived. The meaning of the parent's committed flag for partial persistence still needs a human decision.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d22)

Reproduced by: e2e reproduced. same parent failure records committed=0 while middle dependency survives; successful parent neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.committed.escaped-dependency). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d23-new-q-27-the-global-history-delete-hides-every-error-not-only-permission-errors"></a><!-- historical fragment alias -->
<a id="g-d23"></a>
### G:D23: The global history delete hides every error, not only permission errors

Severity: Medium · Decision: Open

Description: Global history deletion suppresses all query errors, not just permission failures. It can report success without deleting anything; the intended handling of other errors remains under discussion.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d23)

Reproduced by: e2e reproduced. ordinary global delete with unknown SQL column reports OK and retains sentinel; valid Package-filtered global delete removes it; nonpermission-error policy remains Q-27.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033221686336Z-readers/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `readers`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.delete.global-errors). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d24-history-find-shows-an-empty-log-with-an-ok-status-when-its-query-fails"></a><!-- historical fragment alias -->
<a id="g-d24"></a>
### G:D24: `history find` shows an empty log with an OK status when its query fails

Severity: Medium · Decision: Open

Description: A failed history query is presented as an empty log with a successful status. Operators cannot distinguish no matching records from a bad filter or query failure; the reporting policy remains open.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d24)

Reproduced by: e2e reproduced. misspelled history column returns OK; valid Success query neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.read.filter.invalid-column). No product repair or maintainer approval is claimed.

<a id="d28-q-23-a-dependencys-own-defaults-notransaction-does-not-keep-it-out-of-its-parents-transaction"></a><!-- historical fragment alias -->
<a id="g-d28"></a>
### G:D28: A dependency's own `NoTransaction` default does not keep it out of the parent's transaction

Severity: Medium · Decision: Open

Description: A dependency's own NoTransaction default does not prevent it from joining its parent's transaction. A parent rollback removes it anyway, and the intended precedence of these settings still needs a decision.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d28)

Reproduced by: e2e reproduced. own NoTransaction dependency still rolls back with parent; successful Q23S neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#transaction.flag-sources.dependency-no-transaction). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="d29-new-q-09-refused-update-reinstall-and-uninstall-attempts-leave-no-record"></a><!-- historical fragment alias -->
<a id="g-d29"></a>
### G:D29: Refused `update`, `reinstall` and `uninstall` attempts leave no record

Severity: Medium · Decision: Open

Description: Some refused update, reinstall and uninstall commands create no history record. Whether the audit should include these early refusals remains an open policy question.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-d29)

Reproduced by: e2e reproduced. ordinary update/reinstall/uninstall argument refusals allocate no history ID; successful/failed named load neighbors elsewhere; intent remains Q-09.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033221686336Z-readers/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `readers`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#history.record.unlogged-refusals). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="i3-q-03-q-24-loading-a-parent-replaces-a-dependency-that-is-in-developer-mode-and-clears-its-developer-mode"></a><!-- historical fragment alias -->
<a id="g-i3"></a>
### G:I3: Loading a parent replaces a dependency in developer mode and clears its developer mode

Severity: Medium · Decision: Open

Description: Loading a parent can replace a dependency that is in developer mode and clear that mode. Protection of developer dependencies still needs a policy decision, so the observed replacement is not a settled defect verdict.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#g-i3)

Reproduced by: e2e reproduced. ordinary parent load upgrades developer dependency and clears developer mode; same-version neighbor retains it.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T032112831789Z-nested/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nested`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#dev-mode.overwrite-guard.dependency-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="c-d4"></a>
### C:D4: A parameter value that contains `$` cannot be used. `%RegExReplace` passes the value as ICU replacement text and escapes only backslashes, so `$5` or an unfi...

Severity: Medium · Decision: Open

Description: A module parameter containing a dollar sign is interpreted as replacement syntax rather than literal text. Files, messages or hooks using that parameter can fail during evaluation.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d4)

Reproduced by: e2e reproduced. literal dollar Default makes load fail; five succeeds.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/manifest.g5.md#module.parameter). No product repair or maintainer approval is claimed.

<a id="c-d7"></a>
### C:D7: `^1.2` (a caret on a two-part version) means `1.2.x` instead of `>=1.2.0 <2.0.0-0`. The branch written for it is unreachable, because a two-part version is a...

Severity: Medium · Decision: Open

Description: The version-range parser treats a two-part caret requirement as a much narrower range. This is confirmed at the component level; the full installation impact has not been reproduced for this entry.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d7)

Reproduced by: partial reproduced. real range parser witness and valid caret neighbors; ordinary resolver caller with ^1.2 not re-executed.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d8"></a>
### C:D8: Combining two requirements depends on their order when two bounds differ only by a prerelease part: the first is kept. `^1.0.0` AND `<2.0.0` gives `<2.0.0-0`...

Severity: Medium · Decision: Open

Description: Combining dependency version constraints can give a different result depending on their order. Renaming parent modules can make the same satisfiable dependency set fail resolution.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d8)

Reproduced by: e2e reproduced. named diamond load refusal with source-selected dependency bounds; successful neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d14"></a>
### C:D14: An OS environment value used in an `-env` file through `${VAR}` is inserted as ICU replacement text: `a$1b` becomes `aCONS_DOLLARb` (the variable's name), `a...

Severity: Medium · Decision: Open

Description: Environment values containing dollar signs or backslashes can be changed or rejected when inserted into a configuration file. Credentials and file paths may no longer contain the values supplied by the operator.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d14)

Reproduced by: e2e reproduced. ordinary -env load feeds altered dollar value to its real Invoke reader; five neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/manifest.g5.md#module.env). No product repair or maintainer approval is claimed.

<a id="c-d15"></a>
### C:D15: The downgrade refusal misses prereleases of different versions: with `2.0.0-beta` installed, `load` of `1.0.0-rc` succeeds and replaces it. `Follows` treats ...

Severity: Medium · Decision: Open

Description: The downgrade check mishandles prereleases from different base versions. Loading an older prerelease can replace a newer installed version that should have been protected.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d15)

Reproduced by: e2e reproduced. ordinary prerelease downgrade accepted; release and mixed-release downgrades refused.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/load.g5.md#load.refusal.downgrade). No product repair or maintainer approval is claimed.

<a id="c-d19"></a>
### C:D19: Document export overwrites the deployed-item helper's error before its error guard. A controlled failing override followed by a real source export returns OK...

Severity: Medium · Decision: Open

Description: A component test shows that document export can overwrite an earlier helper error with a later successful result. Export may report success even though one part failed; the failing helper was supplied by a controlled override.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d19)

Reproduced by: partial reproduced (historical helper witness; not rerun). existing allowed deployed-helper override shows local status suppression; no ordinary default-helper failure/full package entry established; no new package/archive run.

[Governing owner](../ontology/features/packaging-deployed-code.g5.md#build.package.deployed-status). No product repair or maintainer approval is claimed.

<a id="c-d16"></a>
### C:D16: First plain `load -lock` uses an empty stored Root and writes relative `module-lock.json` in process cwd. A developer-mode or direct nonempty-Root neighbor w...

Severity: Medium · Decision: Open

Description: A first non-developer load can write its lock file into the process's working directory instead of the module folder. The file can also remain after a later database rollback.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d16)

Reproduced by: e2e reproduced. first ordinary load -lock writes valid audit-lock JSON in process cwd instead of source root; Root is populated by return, so the final record is not evidence of its earlier availability to the writer; developer-mode neighbor writes at source.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T033609225862Z-lock/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `lock`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/dependencies.g5.md#deps.lockfile). No product repair or maintainer approval is claimed.

<a id="c-d24"></a>
### C:D24: The `$zpm("<command>")` function form raises `<COMMAND> Function must return a value` after running the command, pass or fail, so scripts cannot read the sta...

Severity: Medium · Decision: Open

Description: The function form of the package-manager command runs the command and then raises an error instead of returning its status. Scripts cannot safely use that form to tell whether the operation succeeded.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d24)

Reproduced by: e2e reproduced. $zpm(version) throws after command; Shell(version) returns success.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/package-commands.g5.md#ipm.shell.forms). No product repair or maintainer approval is claimed.

<a id="c-d27"></a>
### C:D27: A supported processor returns an error from OnResolveChildren. Package refuses, but uninstall discards the status, deletes module/manifest metadata and leave...

Severity: Medium · Decision: Open

Description: Uninstall ignores an error while resolving a resource's children, removes module metadata, and leaves the class installed. The package manager can lose track of code that is still callable.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d27)

Reproduced by: e2e reproduced. supported error-returning resource extension; uninstall deletes metadata while class remains callable; successful extension neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030635655399Z-error/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `error`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.child-error). No product repair or maintainer approval is claimed.

<a id="c-d28"></a>
### C:D28: A supported editing lock makes OBJ.Delete refuse and keep a class. Uninstall warns, returns success and deletes module/manifest metadata; class remains calla...

Severity: Medium · Decision: Open

Description: A class held by an editing lock can survive uninstall while its module metadata is deleted. IPM reports success with a warning, but the supposedly removed code remains callable.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d28)

Reproduced by: e2e reproduced. real class edit lock; uninstall succeeds with callable class retained; unlocked uninstall removes it.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030643833060Z-iter11/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter11`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.class-delete-error). No product repair or maintainer approval is claimed.

<a id="c-d29"></a>
### C:D29: FileCopy Overlay=0 loads new files but retains unrelated old entries instead of replacing the directory. Direct CopyDir flags0/1 both keep a sentinel and rep...

Severity: Medium · Decision: Open

Description: A FileCopy resource configured to replace a directory retains unrelated old files. Installation can leave stale contents alongside the new files instead of producing a clean target directory.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d29)

Reproduced by: e2e reproduced. ordinary FileCopy Overlay=0 retains unrelated sentinel and writes new payload; real Unix filesystem provider.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/resources.g5.md#module.processor.file-copy.unix-overlay). No product repair or maintainer approval is claimed.

<a id="c-d31"></a>
### C:D31: A failing outer test runs a passing child through ordinary Shell. Outer device/status identify the failed outer run, but the outer JSON output file contains ...

Severity: Medium · Decision: Open

Description: A failing test that runs a passing child test can write the child's results into its own JSON report. The report can look successful even though the outer test failed.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d31)

Reproduced by: e2e reproduced. actual outer/child Shell test runs, final file/device identities and alone neighbors.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030004292651Z-iter17/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter17`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/testing-policy.g5.md#build.test-output.run-identity). No product repair or maintainer approval is claimed.

<a id="c-d33"></a>
### C:D33: Install a parent with exact dependency `1.0.0+snapshot`, change that dependency's source payload, then ordinarily reinstall the parent. The forced refresh ad...

Severity: Medium · Decision: Open

Description: Reinstalling a parent with an exact snapshot dependency can fail while combining compatible version constraints. The dependency's updated source is not installed, leaving the previous code in use.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d33)

Reproduced by: e2e reproduced. default snapshot reinstall refuses; shallow/release/explicit-permission neighbors.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030013086002Z-iter24/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter24`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version.range.snapshot-intersection). No product repair or maintainer approval is claimed.

<a id="c-d34"></a>
### C:D34: `-only` with two or more phase names fails before any phase runs

Severity: Medium · Decision: Open

Description: Using the only option with multiple lifecycle phases generates an invalid method call. None of the requested phases runs, even though each phase works when requested alone.

Run: Uplift 2026-11-16

Found by: source review of `Lifecycle/Base.cls::MatchSinglePhase` and `Storage/Module.cls::ExecutePhases`, then [iteration30 model](consolidated/iter30/phases.rs) `only_multiple_names_fail` and its control `only-runs-each-name` ([proof receipt](../uplift/consolidated/iter30/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration30 native receipt](../uplift/consolidated/iter30/native/20261116T170433591502Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`, developer-mode directory module `phasecase`. `phasecase compile activate -only` (case c3) and `mycustom compile -only` (c10) return an error status with `<METHOD DOES NOT EXIST> *%Compile,Activate` and `*%Mycustom,Compile`. Neither the After-Compile nor the After-Activate hook runs, and the compiled class keeps its old value. Passing neighbours: `phasecase compile -only` (c1), `compile phasecase -only` (c9) and `phasecase activate -only` (c11) each run exactly the named phase.

`ModuleAction` builds a phase list and, under `-only`, `ExecutePhases` passes `$listtostring` of the whole list to `MatchSinglePhase`. The comma-joined name matches neither a standard nor a custom phase, so the lifecycle method call fails. The module-action help says `-only` "Only runs the specified phase(s)", and the [governing statement](../ontology/architecture/lifecycle.g5.md#lifecycle.only) says it runs just the named phases. A repair would match each name separately and keep the given order. A distinguishing check is c3 running Compile then Activate with no Initialize or Reload. The proposed repair was not checked.

<a id="c-d35"></a>
### C:D35: A phase list silently drops custom and unknown names listed beside a standard phase

Severity: Medium · Decision: Open

Description: A lifecycle phase list silently drops custom or unknown names when they appear beside a standard phase. The command can report success without running all the work the caller requested.

Run: Uplift 2026-11-16

Found by: source review of `Lifecycle/Base.cls::GetCompletePhases`, then [iteration30 model](consolidated/iter30/phases.rs) `custom_and_unknown_neighbors` and its control `custom-beside-standard-runs` ([proof receipt](../uplift/consolidated/iter30/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration30 native receipt](../uplift/consolidated/iter30/native/20261116T170433591502Z/observations.json). `phasecase reload mycustom` (case c4) succeeds after Initialize and Reload, and the module's `MyCustom` Invoke counter does not change. `phasecase compile bogus` (c6) succeeds as a plain compile chain. Passing and contrasting neighbours: `phasecase MYCUSTOM` (c5) runs the custom hook once, and `phasecase bogus` (c7) fails.

In complete mode `GetCompletePhases` keeps a non-standard name only when it is the sole name. Otherwise each name contributes its standard chain, and a custom or unknown name contributes none. A requested custom phase is therefore skipped without notice, and a mistyped phase name beside a valid one is ignored while the command reports success. The [custom-phase statement](../ontology/architecture/lifecycle.g5.md#lifecycle.custom) says a custom phase runs when requested by name. Where it should run in a mixed list, and whether an unknown name must refuse the command, is the open [phase-list policy](open_questions.md#lifecycle.phase-list-policy). No repair was checked.

<a id="c-d36"></a>
### C:D36: A module whose classes sit directly under SourcesRoot loads, but sync never sees its edits or deletions

Severity: Medium · Decision: Open

Description: A developer module can load classes directly from its source root, but sync does not scan that same layout. Later edits and deletions are ignored while sync reports that there is nothing to do.

Run: Uplift 2026-11-16

Found by: native exploration of the iteration32 sync workflow, then source review of `ResourceProcessor/Default/Document.cls::GetSyncDirectory` against its Reload directory list, and [iteration32 model](consolidated/iter32/sync.rs) `flat_schedule` with control `scan-uses-load-fallback` ([proof receipt](../uplift/consolidated/iter32/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration32 native receipt](../uplift/consolidated/iter32/native/20261116T171504430334Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. Developer-mode module `syncf` declares `SyncF.PKG` with files at `src/SyncF/*.cls`; `load -dev` loads them. After editing `SyncF.A` to return 2, `sync syncf` prints "Nothing to sync." and the class still returns 1 (case f1). After deleting `B.cls`, `sync syncf -delete` again prints "Nothing to sync." and `SyncF.B` stays (f2). `load syncf -dev` then picks up the edit (f3). The passing neighbour is module `syncc` with files at `src/cls/...`, whose edits and deletions sync (s1-s5).

Reload looks for a resource in `{SourcesRoot}/{Directory}` and, when Directory is not declared, also directly in `{SourcesRoot}`. Sync scans and tracks only `{SourcesRoot}/{Directory}` (Directory defaults to `cls` for classes), so it never sees files the loader found through the fallback. Sync then reports success while the namespace keeps stale code, contrary to the [sync statement](../ontology/features/developer-tools.g5.md#devtools.sync). A repair would derive the sync directory from the directory Reload actually used, for example the developer-mode `^Sources` entry, or warn when a resource's sync directory does not exist. Neither repair was checked.

<a id="c-d38"></a>
### C:D38: `list-dependents -t` leaves out modules at the top of a dependency graph

Severity: Medium · Decision: Open

Description: The tree form of list-dependents omits modules at the top of the dependency graph. An operator sees an incomplete picture of which installed modules rely on the target.

Run: Uplift 2026-11-16

Found by: native graph dump during iteration33 exploration (`iter33/attempts/explore2-tree-dump.log`), then source review of `Utils/Module.cls::InvertDependencies` and `GetDependentsAsTree`, and [iteration33 model](consolidated/iter33/dependents.rs) `tree_omits_roots` with control `tree-includes-roots` ([proof receipt](../uplift/consolidated/iter33/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration33 native receipt](../uplift/consolidated/iter33/native/20261116T171635420146Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. On one installed graph the flat `list-dependents i33a` lists b, c, d and g, while `list-dependents -t i33a` prints only a, b and c, and `-t i33b` leaves out g. The passing neighbours are the flat report on the same graph and `-t` on a module with no dependents.

The tree is built by inverting each installed module's dependency graph. That graph records parent links for nested dependencies but none from the top module to its direct dependencies, so the inversion never reaches a graph's top module. A user checking dependents before removing a module sees an incomplete tree, contrary to the [list-dependents statement](../ontology/features/inspection.g5.md#inspect.list-dependents). A repair would seed the inversion with each graph's top module as the parent of its direct dependencies. It was not checked.

<a id="c-d41"></a>
### C:D41: The installer class does not receive `<Default>` values

Severity: Medium · Decision: Open

Description: During installation, a module's installer receives explicit parameters but not values declared as defaults. The same setting can therefore be available to later lifecycle steps while missing from configuration code.

Run: Uplift 2026-11-16

Found by: source review of `Lifecycle/Base.cls::%Configure`, whose `GetCustomParameters` result is unused, and [iteration36 model](consolidated/iter36/installer.rs) `default_versus_parameter` with control `default-reaches-installer-vars` ([proof receipt](../uplift/consolidated/iter36/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration36 native receipt](../uplift/consolidated/iter36/native/20261116T173156335552Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. Module m36a declares `<Default Name="Greeting" Value="hello"/>` and `<Parameter Name="Mode">param</Parameter>`. On load, `OnConfigureComponent` receives Mode=param but no Greeting (case l1), while the module's After-Activate Invoke receives `${Greeting}`=hello. Passing neighbour: with `-DGreeting=bye`, both receive bye (l3).

`%Configure` calls `GetCustomParameters` into `customParams`, then builds the installer variables only from the phase parameters, which hold `<Parameter>` and `-D` values. This contradicts [module defaults](../ontology/features/manifest.g5.md#module.parameter), which says the module receives both kinds of default, and the [installer parameters statement](../ontology/architecture/lifecycle.g5.md#lifecycle.installer.parameters). A repair would merge `customParams` into the variables before `OnConfigureComponent`, keeping `-D` precedence. It was not checked.

<a id="c-d44"></a>
### C:D44: A refused module document save empties the stored document text

Severity: Medium · Decision: Open

Description: Saving malformed module XML correctly reports an error, but also empties the previously stored document. The module record survives, leaving it out of sync with the document an administrator can open and edit.

Run: Uplift 2026-11-16

Found by: iteration39 native exploration (`iter39/attempts/explore2.log`), then source review of `StudioDocument/Module.cls::ImportStream`, and [iteration39 model](consolidated/iter39/document.rs) `refused_save_erases_text` with control `stream-clear-rolled-back` ([proof receipt](../uplift/consolidated/iter39/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration39 native receipt](../uplift/consolidated/iter39/native/20261116T174319930857Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. After a valid edit to `m39.ZPM` (version 1.1.0, e1), saving malformed XML through `%RoutineMgr` is refused with an XSLT parse error (e2). The module record still reads 1.1.0 with its resource, but the stored `m39.ZPM` text is now empty. Passing neighbour: the valid save in e1 stores both the text and the record.

`ImportStream` clears the stored stream and writes the new text before its transaction starts. When the record update fails, the rollback discards the written text but not the earlier clear. An editor that reopens the document then sees an empty manifest, and saving that empty document resets the version (see the [document statement](../ontology/features/manifest.g5.md#module.identity.document)). A repair would clear and write the stream inside the transaction. It was not checked.

<a id="c-d46"></a>
### C:D46: A preload class that fails to compile does not fail the load

Severity: Medium · Decision: Open

Description: A module can report a successful load even when one of its preload classes fails to compile. The module is then marked installed although code it needs is unavailable.

Run: Uplift 2026-11-16

Found by: source review of `Lifecycle/Base.cls::%Initialize`, which keeps the import status in a local and returns its own OK status, then [iteration42 model](consolidated/iter42/initialize.rs) `preload_error_swallowed` with control `preload-error-returned` ([proof receipt](../uplift/consolidated/iter42/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration42 native receipt](../uplift/consolidated/iter42/native/20261116T174706181264Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. `load m42b`, whose `preload/cls/M42B/Broken.cls` uses an undefined property type, reports Initialize SUCCESS and succeeds; the module is installed and `M42B.Broken` is not compiled (case b1). Passing neighbour: the same class error in an ordinary `src` resource fails `load m42c` and installs nothing (c1).

Preload code usually bootstraps what later phases need, so a broken preload passes silently and fails later or not at all, contrary to the [Initialize statement](../ontology/architecture/lifecycle.g5.md#lifecycle.initialize) and to a load failing on compile errors. A repair would return the ImportDir/compile status from `%Initialize`. It was not checked.

<a id="c-d49"></a>
### C:D49: `arrange` fails in a namespace without a source-control class

Severity: Medium · Decision: Open

Description: The arrange command fails in a namespace that has no source-control class configured, even for a simple manifest. Users must configure source control before they can rearrange that document.

Run: Uplift 2026-11-16

Found by: iteration44 native exploration, then source review of `Utils/Module.cls::ExportDocumentForObject`, which throws the `%Studio.SourceControl.Interface` status, and [iteration44 model](consolidated/iter44/comments.rs) `without_source_control_nothing_changes` with control `no-source-control-ok` ([proof receipt](../uplift/consolidated/iter44/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration44 native receipt](../uplift/consolidated/iter44/native/20261116T175351554424Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. In a fresh namespace, `arrange c44` and `arrange n44` (a module without comments) fail with error #5889 "Not logged into source control system" and leave the stored manifests unchanged (cases a1, n1). Passing neighbour: after configuring `%Studio.SourceControl.Base`, both succeed (s1, s3).

The [authoring statement](../ontology/features/developer-tools.g5.md#devtools.authoring) offers `arrange` as an ordinary authoring command, and no product test covers it. A repair would treat an absent source-control class as "not under source control, editable". It was not checked.

<a id="c-d58"></a>
### C:D58: A module with a `.LOC` resource cannot be loaded from a read-only folder

Severity: Medium · Decision: Open

Description: Loading localization resources attempts to write inside the module's source folder. A module containing a .LOC resource therefore fails to install from a read-only folder, while an ordinary module can load from the same kind of location.

Run: Uplift 2026-11-16

Found by: source review of `ResourceProcessor/LocalizationExport.cls::OnBeforePhase` (Activate export into the module root), then [iteration56 model](consolidated/iter56/locmsgs.rs) `readonly_root_fails` with control `readonly-tolerated` ([proof receipt](../uplift/consolidated/iter56/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration56 native receipt](../uplift/consolidated/iter56/native/20261116T182256528947Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. Loading `loc56ro` from a read-only copy fails with `Cannot open file '<root>/localize/D56RMsgs.xml'` and installs nothing (case r1). Passing neighbour: a read-only module without a `.LOC` resource loads (n1).

The module asks for no export, and installing should not need to write into its source folder, contrary to the [localization statement](../ontology/features/resources.g5.md#module.processor.localized-messages). A repair would skip or tolerate the export when the root is not writable, or write it only on request. It was not checked.

<a id="c-d65"></a>
### C:D65: Uninstall leaves the package mappings IPM created, overriding an administrator's mapping

Severity: Medium · Decision: Open

Description: Uninstall removes a module but leaves the package mappings that IPM added for it. A leftover mapping can continue to override an administrator's broader mapping and send later package lookups to the wrong database.

Run: Uplift 2026-11-16

Found by: source review (no removal caller for mappings added by `OnConfigureMappings`), then [iteration59 model](consolidated/iter59/mappings.rs) `mapping_outlives_uninstall` with control `mapping-removed-on-uninstall` ([proof receipt](../uplift/consolidated/iter59/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration59 native receipt](../uplift/consolidated/iter59/native/20261116T182911128903Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. After `uninstall m59a`, the USER mapping `%ZI59 -> USER` remains (case a2). With an administrator mapping `P59 -> DB59`, `load m59b` adds `P59.Sub -> USER`, which remains after `uninstall m59b`, so `P59.Sub` still resolves to USER rather than DB59 (b2). Passing neighbour: an ordinary package gets no mapping (d1).

Namespace configuration changed by an install persists after the module is gone, contrary to the [placement statement](../ontology/features/resources.g5.md#module.placement). A repair would record the mappings a load created and remove them on uninstall unless another module needs them. It was not checked.

<a id="c-d66"></a>
### C:D66: Uninstall of a NoMapping module leaves its code outside the namespace

Severity: Medium · Decision: Open

Description: A module loaded with NoMapping can put code in the system database. Ordinary uninstall removes its registration but leaves that shared code accessible from other namespaces; the recorded global uninstall removes it.

Run: Uplift 2026-11-16

Found by: native exploration (case c2) and source review of `Lifecycle/Base.cls::%Clean` (mapped classes skipped unless `Clean.GlobalScope`), then [iteration59 model](consolidated/iter59/mappings.rs) `nomapping_code_orphaned` with control `mapped-code-deleted` ([proof receipt](../uplift/consolidated/iter59/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration59 native receipt](../uplift/consolidated/iter59/native/20261116T182911128903Z/observations.json). `load m59c -DNoMapping=1` compiles `%ZI59C.Main` into IRISSYS. `uninstall m59c` prints "Skipping mapped resource", removes the module record and leaves the class visible from `%SYS` and every namespace (c2). Passing neighbour: `uninstall m59c -global` removes it (c4).

The module record disappears while its code remains shared by every namespace, contrary to the [placement statement](../ontology/features/resources.g5.md#module.placement). A repair would delete classes the module itself installed wherever they resolve, or refuse NoMapping placement outside the namespace database. It was not checked.

<a id="c-d67"></a>
### C:D67: Uninstall leaves a module's declared `<Mapping>` entries in the namespace

Severity: Medium · Decision: Open

Description: Uninstall leaves the namespace mappings declared by a module, including when purge is requested. The module disappears from the installed list, but its changes to where classes or data are looked up remain active.

Run: Uplift 2026-11-16

Found by: source search (no uninstall caller removes the mappings declared in `Module.Mappings`), then [iteration63 model](consolidated/iter63/cleanup.rs) `reload_side_effects_outlive_uninstall` with control `mapping-removed` ([proof receipt](../uplift/consolidated/iter63/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration63 native receipt](../uplift/consolidated/iter63/native/20261116T183505649094Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. After `load /tmp/iter63/u63` adds `U63M.GBL -> U63DB` and `U63MP.PKG -> U63DB`, both mappings remain after `uninstall u63` (case x1) and after `uninstall u63 -purge` (x3). Passing neighbour: every code resource, the record and the non-kept FileCopy targets are removed in the same runs.

Namespace configuration made for the module outlives it, contrary to the [cleanup statement](../ontology/features/uninstall.g5.md#uninstall.cleanup). This is the declared-mapping counterpart of C:D65. A repair would remove the module's declared mappings in Clean unless another installed module declares the same mapping. It was not checked.

<a id="p-f-dep-7"></a>
### P:F-DEP-7: The resolver is greedy (newest first, no backtracking past one level) and fails on solvable inputs, although its comment says "with backtracking". Known upst...

Severity: Medium · Decision: Open

Description: When packages have competing version requirements, the resolver can stop at a conflict even though a compatible combination exists. It does not fully reconsider earlier choices, so a valid installation can be refused.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-7)

Reproduced by: e2e reproduced. ordinary solvable graph install refuses; pinned solution installs all modules.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/dependencies.g5.md#deps). No product repair or maintainer approval is claimed.

<a id="p-f-inst-10"></a>
### P:F-INST-10: An update that brings in a module for the first time runs all of that module's update steps instead of seeding them as a fresh install does.

Severity: Medium · Decision: Open

Description: When an update introduces a dependency for the first time, IPM runs that dependency's old migration steps as though it were already installed. A fresh install normally records those steps as already accounted for.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-10)

Reproduced by: e2e reproduced. new dependency installed during update runs V1; fresh normal install seeds without running.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#update.steps.fresh-seeding-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-inst-4"></a>
### P:F-INST-4: Between registries that offer the same version, the registry created first wins; `OverriddenSortOrder` has no effect. Deleting and re-adding a registry chang...

Severity: Medium · Decision: Open

Description: If two registries provide the same package version, their creation order decides which copy installs. Configuring their priority does not change that choice, and deleting and re-adding a registry can unexpectedly change the source.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-4)

Reproduced by: e2e reproduced. two filesystem repositories with tied versions; creation-order selection and reordered neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/repositories.g5.md#repo.search-order). No product repair or maintainer approval is claimed.

<a id="p-f-inst-5"></a>
### P:F-INST-5: `update -dev` from a registry does not run in developer mode, and a plain `update` of a module stored in developer mode clears it. Only `update -dev -path` k...

Severity: Medium · Decision: Open

Description: Updating from a registry does not honor the requested developer mode, and a normal update clears an existing developer-mode setting. A module can therefore stop being treated as a local development checkout after an update.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-5)

Reproduced by: e2e reproduced. ordinary update -dev from filesystem registry ends normal; local -path developer neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/update.g5.md#update.dev-mode). No product repair or maintainer approval is claimed.

<a id="p-f-inst-7"></a>
### P:F-INST-7: A class of the update package whose name starts with `V` but has no digit (e.g. `Validator`) makes every update of the module fail with `<SUBSCRIPT>`, naming...

Severity: Medium · Decision: Open

Description: An update helper class named Validator, or another name starting with V but no version number, crashes update discovery. Every update of that module fails with a low-level error that does not identify the offending class.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-7)

Reproduced by: e2e reproduced. real Validator fixture makes update fail; numbered-class neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/update.g5.md#update.steps). No product repair or maintainer approval is claimed.

<a id="p-f-inst-8"></a>
### P:F-INST-8: A developer-mode install (or a plain `load` of a module stored in developer mode) seeds no update steps, so the next update reruns every step from V1, includ...

Severity: Medium · Decision: Open

Description: A developer-mode install does not record the update steps belonging to the installed version. The next update can rerun old migrations, including steps for versions already present.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-8)

Reproduced by: e2e reproduced. developer fresh install leaves zero step records; later update executes old steps; normal fresh install seeds.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#update.steps.dev-seeding-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-inst-9"></a>
### P:F-INST-9: Install silently replaces a needed module that is in developer mode and clears its developer mode; the guard covers only the named module. Wiki Concepts: a d...

Severity: Medium · Decision: Open

Description: Installing a package can overwrite one of its dependencies even when that dependency is in developer mode. The protection applies only to the package named in the command, so dependency updates can replace a developer's working copy.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-9)

Reproduced by: e2e reproduced. ordinary parent install replaces developer dependency; initial developer load verifies prestate.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#dev-mode.overwrite-guard.dependency-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-nsp-2"></a>
### P:F-NSP-2: In a namespace that gets IPM by mapping, an `IPMVersion` requirement is checked against an empty version, so every module with such a requirement is refused ...

Severity: Medium · Decision: Open

Description: In a namespace that uses a mapped copy of IPM, the version check reads an empty IPM version. Packages that declare an IPMVersion requirement are refused even when the mapped installation is new enough.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-2)

Reproduced by: e2e reproduced. ordinary requirement load fails in mapped namespace, succeeds in source namespace.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/operations/namespaces.g5.md#namespace.version.mapped-reader). No product repair or maintainer approval is claimed.

<a id="p-f-nsp-4"></a>
### P:F-NSP-4: `enable -map -repos` (and `enable -community`) on a namespace with no IPM maps the code but skips the registries; a second run is needed. The repo step reads...

Severity: Medium · Decision: Open

Description: The first enable command maps IPM into a fresh namespace but skips the requested registries. A second run is needed before the namespace has both the package manager and its registry configuration.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-4)

Reproduced by: e2e reproduced. enable -map -repos skips first repo mapping; same command second run maps it.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/operations/namespaces.g5.md#namespace.enable.mapping-results). No product repair or maintainer approval is claimed.

<a id="p-f-nsp-6"></a>
### P:F-NSP-6: `unmap` leaves the `%IPM.*` routine mapping: the namespace still runs the source's IPM and is still reported as mapped. Argument-slot bug: the third argument...

Severity: Medium · Decision: Open

Description: Unmapping IPM leaves a routine mapping behind. The namespace can still execute the source namespace's IPM and still appears mapped, so the command has not fully detached it.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-6)

Reproduced by: e2e reproduced. unmap removes package mapping but routine mapping remains and IPM still callable; source refusal neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/operations/namespaces.g5.md#namespace.unmap.routine-result). No product repair or maintainer approval is claimed.

<a id="p-f-repo-5"></a>
### P:F-REPO-5: `default-modifiers` are documented per namespace but stored in `^%IPM.DefaultModifiers`, which IRIS keeps in IRISSYS: setting `-DNoTransaction=1` in one name...

Severity: Medium · Decision: Open

Description: Default command modifiers are documented as namespace-specific but are stored globally. Changing a default in one namespace, such as disabling transactions, changes how commands behave in other namespaces too.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-5)

Reproduced by: e2e reproduced. real namespaces read shared default modifiers; local namespace settings neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/operations/settings.g5.md#settings.default-modifiers). No product repair or maintainer approval is claimed.

<a id="p-f-sync-1"></a>
### P:F-SYNC-1: A failed `compile`, `load -dev` or `reload` that puts edited source on the server records no baseline and no retry mark. If the file is then reverted to its ...

Severity: Medium · Decision: Open

Description: After edited source fails to compile on the server, reverting the local file to its earlier contents can leave sync reporting nothing to do. The server still holds the broken source because the failed operation did not record that a retry was needed.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-sync-1)

Reproduced by: e2e reproduced. ordinary failed compile then exact-byte revert makes sync skip stale server definition; successful initial load neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#devtools.sync.failed-import-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-unin-2"></a>
### P:F-UNIN-2: `uninstall -purge` deletes no table data. Nothing reads the `Purge` parameter; `%Clean` deletes classes without the extent flag `e`. (An installer can read `...

Severity: Medium · Decision: Open

Description: Uninstall with purge removes classes without deleting their table data through the normal cleanup path. Users requesting data removal can be left with the old records unless a custom installer handles the flag itself.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-2)

Reproduced by: e2e reproduced. ordinary persistent sentinel survives -purge and schema removal/reload; real extent-delete and installer callback neighbors.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030017516810Z-iter27/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter27`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.purge.as-built). No product repair or maintainer approval is claimed.

<a id="p-f-unin-5"></a>
### P:F-UNIN-5: A global resource declared only by name (`<Resource Name="X.GBL"/>`, the usual form) keeps its data after uninstall; only resources with the `Global` attribu...

Severity: Medium · Decision: Open

Description: A global declared using the usual name-only resource form keeps its data after uninstall. Cleanup deletes the data only for the alternative form with an explicit Global attribute, so equivalent-looking declarations behave differently.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-5)

Reproduced by: e2e reproduced. ordinary name-only global resource retains data; explicit Global attribute deletes it.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall). No product repair or maintainer approval is claimed.

<a id="p-f-unin-6"></a>
### P:F-UNIN-6: A blocked uninstall returns OK, prints `Clean SUCCESS` and logs `uninstall/success` while the module stays. The code comment at `Lifecycle/Base.cls#L276` say...

Severity: Medium · Decision: Open

Description: When dependents prevent uninstall, the module correctly stays installed but the command reports and records success. Automation and operators cannot tell from that result that the requested removal was blocked.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-6)

Reproduced by: e2e reproduced. blocked uninstall returns OK and logs success with module retained; successful ordinary uninstall neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.dependents). No product repair or maintainer approval is claimed.

<a id="p-f-load-4"></a>
### P:F-LOAD-4: A load can fail after its final commit: an after-install message with `{` and a `$$$` macro that does not compile throws in `%EvaluateMacro`. The command ret...

Severity: Medium · Decision: Open

Description: A malformed macro in the final installation message can make load report failure after the module and classes have already been committed. The failed history entry therefore does not mean the installation was rolled back.

Severity basis: An ordinary load returns failure after its changes commit, leaving installed code and a failed command result. This can misdirect scripts and recovery even though the trigger is an after-install message.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-load-4)

Reproduced by: e2e reproduced. ordinary after-install macro failure with durable module/history committed; successful message neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/load.g5.md#load). No product repair or maintainer approval is claimed.

<a id="p-f-repo-4"></a>
### P:F-REPO-4: `config delete` restores the first value ever set, not the factory default, for keys the installer does not seed (`TerminalPrompt`, `PublishTimeout`, `Histor...

Severity: Medium · Decision: Open

Description: Deleting a configuration override can restore the first custom value ever assigned instead of the factory default. Settings not initialized by the installer can remain customized after a user tries to reset them.

Severity basis: Config delete restores a prior assigned value instead of the documented default, changing effective configuration. This is administrator-repairable behavior, not merely cosmetic text.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-4)

Reproduced by: e2e reproduced. config delete restores first stored value; recorded changed-value prestate.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

The #281 candidate adds the `NotaryExecutable` key with an explicit reset to empty, as `TestReportFormat` has, so this defect does not apply to it ([iteration 70, R2](../uplift/consolidated/iter70/HANDOFF.md#findings-and-repairs)).

Fresh reproduction on Linux x86-64 IRIS 2026.1: [iteration41 receipt](../uplift/consolidated/iter41/native/20261116T174828407055Z/observations.json), with the [iteration41 model](consolidated/iter41/settings.rs) replaying the same outcome.

[Governing owner](../ontology/operations/settings.g5.md#settings). No product repair or maintainer approval is claimed.

<a id="c-d2"></a>
### C:D2: When IPM cannot create a load's temporary `.modules` repository, it prints "Failed to clean up repository 'ipm-temp-modules-N'. You may need to manually dele...

Severity: Low · Decision: Open

Description: If creation of a temporary load repository fails, cleanup tells the user to delete that repository manually even though it was already rolled back. The warning sends the operator looking for a repository that does not exist.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d2)

Reproduced by: e2e reproduced. TR7 missing-row cleanup warning; TR1 succeeds.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/load.g5.md#load.dependencies.local-repo). No product repair or maintainer approval is claimed.

<a id="c-d3"></a>
### C:D3: Parameters that refer to each other in a loop are never reported. The 20-level guard that throws "Circular reference or too many levels in placeholders" is u...

Severity: Low · Decision: Open

Description: Parameters that refer to each other in a loop do not produce the intended circular-reference error. Resolution stops with an unfilled placeholder, which can later fail with an unrelated regular-expression error.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d3)

Reproduced by: e2e reproduced. cyclic Default reaches ordinary load error; acyclic neighbor succeeds.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/manifest.g5.md#module.parameter). No product repair or maintainer approval is claimed.

<a id="c-d5"></a>
### C:D5: A parameter name is placed into a regular expression unescaped, so a dotted name such as `a.b` also fills `{$axb}`. Dotted names are the documented way to ne...

Severity: Low · Decision: Open

Description: A dotted parameter name is treated as a regular-expression pattern during substitution. Setting a.b can also replace a different parameter such as axb, giving the wrong value to module configuration.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d5)

Reproduced by: e2e reproduced. a.b Default changes {$axb} in the command output; exact ordinary value neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/manifest.g5.md#module.parameter). No product repair or maintainer approval is claimed.

<a id="c-d9"></a>
### C:D9: Build metadata containing a hyphen is parsed as a prerelease: `1.2.3+build-x` becomes `1.2.3-x+build-x`, which sorts before `1.2.3` and matches only itself a...

Severity: Low · Decision: Open

Description: A hyphen in version build metadata is mistaken for a prerelease marker. A release such as 1.2.3+build-x is then ordered and matched as a prerelease, which can change dependency selection.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d9)

Reproduced by: e2e reproduced. ordinary module load stores the malformed parsed build version; ordinary 1.0.0 neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version). No product repair or maintainer approval is claimed.

<a id="c-d10"></a>
### C:D10: The `&&` conflict error prints literal placeholders: "No overlapping versions in && operation for %1 and %2". `$$$ERROR($$$GeneralError, ...)` takes one argu...

Severity: Low · Decision: Open

Description: When version requirements conflict, the error can show literal placeholders instead of the two incompatible ranges. Users are told resolution failed without the details needed to identify and fix the conflict.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d10)

Reproduced by: partial reproduced. real AND helper error text; full conflicting-dependency command message path remains.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d12"></a>
### C:D12: `history find` shows the result of an action that has not ended (still running or killed) as `ERROR #00: (no error description)`, so an operator reads it as ...

Severity: Low · Decision: Open

Description: History displays a still-running or interrupted action as an unexplained error. An operator cannot distinguish an unfinished command from one that completed and actually reported failure.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d12)

Reproduced by: e2e reproduced. history find displays ERROR #00 for an unfinished record; completed history neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/history.g5.md#history.record). No product repair or maintainer approval is claimed.

<a id="c-d13"></a>
### C:D13: `module-version <module>` with no version, or with the current version, fails with "Cannot set version to X as it is not strictly higher than the current ver...

Severity: Low · Decision: Open

Description: Asking module-version to retain the current version is rejected instead of being treated as an unchanged value. This also blocks a build-metadata-only change unless force is used.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d13)

Reproduced by: e2e reproduced. module-version unchanged refusal; patch bump succeeds.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/developer-tools.g5.md#devtools.authoring.module-version). No product repair or maintainer approval is claimed.

<a id="c-d21"></a>
### C:D21: The test path converter strips the resource folder as raw leading text. A path outside the folder keeps its folders in the class name, although the method co...

Severity: Low · Decision: Open

Description: The test-path helper removes a folder name as a text prefix rather than checking folder boundaries. Files outside the test folder can be converted into incorrect test class names instead of being excluded.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#c-d21)

Reproduced by: partial reproduced. real private converter exposed by subclass and valid inside-path neighbor; sync routing reachability remains untraced.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/developer-tools.g5.md#devtools.sync.test-paths). No product repair or maintainer approval is claimed.

<a id="c-d37"></a>
### C:D37: Whole-namespace sync ignores dependencies that pass through a module not in developer mode

Severity: Low · Decision: Open

Description: Whole-namespace sync loses dependency ordering when the connection passes through a module outside developer mode. A developer module can be synced before another developer module it indirectly depends on.

Run: Uplift 2026-11-16

Found by: source review of `Utils/Module.cls::GetDevModeModulesInDependencyOrder`, then [iteration32 model](consolidated/iter32/sync.rs) `order_neighbors` with controls `dev-edge-filter-removed` and `row-order-only` ([proof receipt](../uplift/consolidated/iter32/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration32 native receipt](../uplift/consolidated/iter32/native/20261116T171504430334Z/observations.json). Developer-mode `tx` depends on `tz`, which is not in developer mode and depends on developer-mode `ty`. With `tx` earlier in row order, `sync` with no module runs Sync for `tx` (position 3) before `ty` (position 5). The passing neighbour is a direct developer-mode dependency: `ox` depends on `oy`, and `oy` (4) runs before `ox` (6) although `ox` comes first in row order.

The helper keeps only dependency edges whose both ends are in developer mode and never follows edges through other modules. The [sync statement](../ontology/features/developer-tools.g5.md#devtools.sync) promises dependency order so that each module recompiles against dependencies already synced in the run. Here `tx` recompiles before `ty` is synced. A repair would order by the transitive closure of dependencies restricted to the developer-mode set. It was not checked.

<a id="c-d42"></a>
### C:D42: `list-installed -s version` orders prereleases of one version as text

Severity: Low · Decision: Open

Description: The installed-package list sorts prerelease numbers as text. Versions such as rc.10 can appear before rc.2, giving users a misleading version order even though normal release numbers sort correctly.

Run: Uplift 2026-11-16

Found by: source review of `Main.cls::SortList`, then [iteration35 model](consolidated/iter35/listing.rs) `sort_neighbors` with control `version-prerelease-semantic` ([proof receipt](../uplift/consolidated/iter35/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration35 native receipt](../uplift/consolidated/iter35/native/20261116T173038793473Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. `list-installed -s v` lists i35pb `1.0.0-rc.10` at position 2 and i35pa `1.0.0-rc.2` at position 3. The passing neighbour is i35y `1.9.0` listed before i35x `1.10.0`, so numeric parts are compared as numbers.

The [version order](../ontology/concepts/versions.g5.md#version.order) compares numeric prerelease identifiers as numbers, so rc.2 comes before rc.10, and the [listing statement](../ontology/features/inspection.g5.md#inspect.list-installed) promises a version sort. The same text comparison drives the install choice in C:D39, but through different code. A repair would compare prerelease identifiers in semantic-version order. It was not checked.

<a id="c-d43"></a>
### C:D43: History shows a module whose real version is 0.0.0-0 as `<unset>`

Severity: Low · Decision: Open

Description: History treats the valid version 0.0.0-0 as if no version was recorded. An installed module with that version is shown as unset, obscuring what the command actually operated on.

Run: Uplift 2026-11-16

Found by: source review of `General/History.cls::VersionToString`, then [iteration38 model](consolidated/iter38/render.rs) `known_placeholder_indistinguishable` with control `placeholder-only-when-unknown` ([proof receipt](../uplift/consolidated/iter38/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration38 native receipt](../uplift/consolidated/iter38/native/20261116T173827930786Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. `load m38z`, whose manifest declares version 0.0.0-0, succeeds with a known package, yet `history find` and `history details` show `<unset>` for that record (record z). Passing neighbours: m38a 1.2.3 shows `v1.2.3`, and the record of an invalid manifest (record b) shows `<unset>` as intended.

The renderer compares the stored version text with the placeholder, so a real `0.0.0-0` cannot be told apart from a version that was never known, contrary to the [Version field statement](../ontology/features/history.g5.md#history.fields). A repair would show the placeholder only when the version was never set, for example with an explicit flag. It was not checked.

<a id="c-d45"></a>
### C:D45: A module document that names a different module is saved under the old name

Severity: Low · Decision: Open

Description: Editing a stored module document to name a different module succeeds without renaming its module record. The document and installed record then disagree about which module they describe.

Run: Uplift 2026-11-16

Found by: iteration39 native exploration (`iter39/attempts/explore1.log`, `explore3.log`), then source review of `StudioDocument/Module.cls::UpdatePersistentFromStream`, and [iteration39 model](consolidated/iter39/document.rs) `other_name_accepted_under_old_name` with control `name-guard-effective` ([proof receipt](../uplift/consolidated/iter39/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration39 native receipt](../uplift/consolidated/iter39/native/20261116T174319930857Z/observations.json). Saving `m39.ZPM` with `<Name>m39other</Name>` and version 1.2.0 succeeds (e3). The record stays named m39 and takes version 1.2.0, no `m39other` record exists, and the stored document text says m39other. Passing neighbour: a save that keeps the name (e1) updates the record and the text consistently.

`UpdatePersistentFromStream` refuses a name that differs from the document name, but importing onto the existing record does not replace its Name, so the comparison always passes. The document and the record then disagree, contrary to the [document statement](../ontology/features/manifest.g5.md#module.identity.document). A repair would compare the parsed manifest name before importing it onto the record. It was not checked.

<a id="c-d47"></a>
### C:D47: Module actions never refresh a developer-mode module's preload code

Severity: Low · Decision: Open

Description: Ordinary actions on a developer-mode module do not find its preload folder. Editing preload code and then initializing or compiling the module leaves the old code running unless the root is supplied explicitly or the module is loaded again.

Run: Uplift 2026-11-16

Found by: source review of `Lifecycle/Base.cls::%Initialize` (preload root from the RootDirectory parameter only) against `%Reload` (developer-mode root fallback), then [iteration42 model](consolidated/iter42/initialize.rs) `module_action_skips_preload` with control `module-action-uses-module-root` ([proof receipt](../uplift/consolidated/iter42/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration42 native receipt](../uplift/consolidated/iter42/native/20261116T174706181264Z/observations.json). After editing `preload/cls/M42P/Boot.cls` from 1 to 2, `m42 initialize -only` and `m42 compile` both print "Skipping preload - directory does not exist" and Boot stays 1 (cases i1, i2). Passing neighbours: `load <dir> -dev` (i3) and `m42 initialize -only -DRootDirectory=<dir>` (i4) refresh it.

Reload falls back to the stored root for developer-mode modules, but Initialize does not, so the chain run by `compile` reloads `src` but not `preload`. The [Initialize statement](../ontology/architecture/lifecycle.g5.md#lifecycle.initialize) and CHANGELOG `#702` place preload in the complete chain. A repair would use the same root fallback as Reload. It was not checked.

<a id="c-d48"></a>
### C:D48: The test phase keeps test classes loaded in a module that is not in developer mode

Severity: Low · Decision: Open

Description: Running tests for a non-development module leaves its test classes in the namespace afterward. It also loads verification-only test classes that were not run, leaving extra code until uninstall.

Run: Uplift 2026-11-16

Found by: source review of `ResourceProcessor/Test.cls::OnPhase`, whose existence check runs after `LoadTestDirectory` has loaded the classes, and [iteration40 model](consolidated/iter40/unittests.rs) `test_leaves_test_code` with control `delete-checked-before-load` ([proof receipt](../uplift/consolidated/iter40/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration40 native receipt](../uplift/consolidated/iter40/native/20261116T174722753053Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. After `load t40` (not developer mode) and `test t40 -only`, `T40.Tests.Basic` and the never-run verify-only `T40.VTests.Basic` are both defined, and the former is compiled (case n1). Passing neighbours: the plain load has no test classes (n0), and `uninstall` removes them (n2).

The processor means to delete loaded test code outside developer mode, but the check that should protect pre-existing classes always sees the classes it just loaded, so `/nodelete` is always passed. This contradicts the [test statement](../ontology/features/build-and-publish.g5.md#build.test) that test-scoped resources load for this phase only. A repair would take the existence check before loading the test directory. It was not checked.

<a id="c-d50"></a>
### C:D50: `arrange` drops comments placed before resources and a module's last comment

Severity: Low · Decision: Open

Description: Rearranging a module manifest silently deletes comments before resources and at the end of the module. Explanations written alongside those declarations are lost even though other comments survive.

Run: Uplift 2026-11-16

Found by: iteration44 exploration, then source review of `Utils/XMLCommentHandler.cls` (`GetPosition`, `endElement`, `Reapply`), and [iteration44 model](consolidated/iter44/comments.rs) `resource_comments_dropped` and `trailing_comment_dropped` with controls `resource-path-unchanged` and `trailing-attached-to-parent` ([proof receipt](../uplift/consolidated/iter44/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration44 native receipt](../uplift/consolidated/iter44/native/20261116T175351554424Z/observations.json). With a source-control class configured, `arrange c44` keeps the comments before `Name` and inside a dependency, but drops the comments before each resource and the module's last comment (case s1). A second `arrange` gives the same result. Passing neighbours: comments C1 to C3 are kept.

The export wraps resources in a new `<Resources>` element, so the recorded resource comment paths no longer match, and a comment with no following sibling is discarded. This contradicts the handler's purpose of carrying comments across the rewrite. A repair would map resource comments by resource name onto the exported path and keep trailing comments as the parent's last child. It was not checked.

<a id="c-d51"></a>
### C:D51: A source-control extension is never told that a module's root changed

Severity: Low · Decision: Open

Description: Moving a module's source root updates IPM's stored path without notifying its source-control extension. The extension can keep using the previous location because the promised root-change callback never arrives.

Run: Uplift 2026-11-16

Found by: iteration43 native exploration (explore4) refuting the source prediction, then source review of `Lifecycle/Base.cls::%Reload` against `Storage/Module.cls::ExecutePhases` (Root assigned before the comparison), and [iteration43 model](consolidated/iter43/schooks.rs) `root_hook_not_reached` with control `root-hook-fires` ([proof receipt](../uplift/consolidated/iter43/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration43 native receipt](../uplift/consolidated/iter43/native/20261116T175445272137Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. With the IPM-aware test extension as the composite's primary, loading m43 from `/tmp/iter43/m43copy` after `/tmp/iter43/m43` changes the stored root but records no OnChangeModuleRoot call (p4). Reloading with `-DRootDirectory` back to the original directory also changes the root without the call (p4b). Passing neighbours: the before/after-phase and resource-change calls in the same runs (p1, p4, p4b).

The interface comment says the call is "invoked when an existing module is reloaded from a different directory than its known module root". `%Reload` compares against a root that was already updated, so extensions that track module locations never learn of the move, contrary to the [source-control statement](../ontology/architecture/lifecycle.g5.md#lifecycle.source-control). A repair would compare against the stored root before it is overwritten. It was not checked.

<a id="c-d52"></a>
### C:D52: `show` omits a `Health="false"` system requirement

Severity: Low · Decision: Open

Description: The show command hides a system requirement explicitly set to Health=false. Users inspecting the module cannot see that declared compatibility restriction in the displayed requirements.

Run: Uplift 2026-11-16

Found by: source review of `General/ModuleSummary.cls::GetSystemRequirements` (Health listed only when true) against `Storage/SystemRequirements.cls::CheckHealth` (Health=0 refuses Health instances), and [iteration46 model](consolidated/iter46/summary.rs) `health_false_hidden` with control `health-false-shown` ([proof receipt](../uplift/consolidated/iter46/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration46 native receipt](../uplift/consolidated/iter46/native/20261116T175856608516Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. `show s46d`, whose only stored requirement is `Health="false"`, prints no System requirements section (case d). `show s46a` lists IRIS and Interoperability but no Health entry (case a). Passing neighbours: s46h, which has no requirements, also shows none, and the other set requirements are listed.

A reader of `show` cannot tell that the module refuses IRIS for Health, contrary to the [show summary statement](../ontology/features/inspection.g5.md#inspect.show), which says the summary prints the module's system requirements. A repair would list "not supported on IRIS for Health" when Health is explicitly false. It was not checked.

<a id="c-d53"></a>
### C:D53: YAML and TOON test reports change assertion message text

Severity: Low · Decision: Open

Description: YAML and TOON test reports alter some assertion messages, including line endings; YAML also changes leading spaces and adds a newline. The reported message can differ from the actual failure text, while JSON preserves it.

Run: Uplift 2026-11-16

Found by: iteration47 exploration with byte-exact file reads, then source review of `Test/YamlOutput.cls::BuildYamlRows` (block scalars that re-indent only LF) and `Test/ToonOutput.cls::BuildToonRows`, and [iteration47 model](consolidated/iter47/formats.rs) `fixture_rendering` with controls `toon-keeps-cr`, `yaml-no-trailing-newline` and `yaml-keeps-leading-spaces` ([proof receipt](../uplift/consolidated/iter47/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration47 native receipt](../uplift/consolidated/iter47/native/20261116T180053540899Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. For an assertion message `"one"_$c(13,10)_"two"`, the JSON report file keeps `one\r\ntwo` while the YAML and TOON files read back `one\ntwo`. `"  leading spaces"` reads back from YAML as `leading spaces\n`, and every YAML description gains a trailing newline. Passing neighbours: JSON is exact for every message, TOON is exact apart from CR, and all formats agree on counts.

A CI tool reading the YAML or TOON report gets assertion text that differs from the JSON report for the same run, although the [test output statement](../ontology/features/build-and-publish.g5.md#build.test-output) offers these formats as alternative renderings of the same full results. A repair would write YAML descriptions as escaped double-quoted scalars and escape CR in TOON quoted fields. It was not checked.

<a id="c-d54"></a>
### C:D54: `generate -author` stores the license and copyright answers in each other's fields

Severity: Low · Decision: Open

Description: Interactive module generation swaps the license and copyright-date answers. The generated manifest can contain a year as its license and a license name as its copyright date.

Run: Uplift 2026-11-16

Found by: source review of `Main.cls::GenerateModuleXML` against `Storage/ModuleTemplate.cls::SetAuthorProps`, and [iteration53 model](consolidated/iter53/generator.rs) `author_swapped` with control `author-args-in-order` ([proof receipt](../uplift/consolidated/iter53/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration53 native receipt](../uplift/consolidated/iter53/native/20261116T180748958256Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. Answering `Apache-2.0` for the license and `2024` for the copyright date writes `<License>2024</License>` and `<CopyrightDate>Apache-2.0</CopyrightDate>` (case t2). Passing neighbour: the `-t` template's own License MIT and CopyrightDate year are in the right fields (t1).

The command passes the two answers to `SetAuthorProps` in the wrong order, contrary to the [generation statement](../ontology/features/developer-tools.g5.md#devtools.authoring.generate). A repair would pass them in parameter order. It was not checked.

<a id="c-d55"></a>
### C:D55: `generate -export` ignores the ignore list for routines and matches routine masks anywhere in the name

Severity: Low · Decision: Open

Description: Module export ignores routine exclusions and matches routine names too broadly. A generated package can include routines the user excluded or unrelated routines whose names merely contain the requested prefix.

Run: Uplift 2026-11-16

Found by: source review of `Storage/ModuleTemplate.cls::ExportOneResource`, then [iteration53 model](consolidated/iter53/generator.rs) `export_neighbors` with controls `routines-honour-ignore` and `routines-prefix-mask` ([proof receipt](../uplift/consolidated/iter53/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration53 native receipt](../uplift/consolidated/iter53/native/20261116T180748958256Z/observations.json). With `-export 00000,G53E,G53E.Skip`, class `G53E.Skip.Gone` is not exported, but routine `G53E.Skip.R` and the unrelated `ZZG53E` are exported and listed as resources (cases e1, t3). Passing neighbours: `G53E.Keep` and `G53E.Rout` are exported.

This is the `generate` authoring export of namespace source into the module folder, not package or artifact export. Ignored or unrelated routines end up in a new module, contrary to the [generation statement](../ontology/features/developer-tools.g5.md#devtools.authoring.generate). A repair would apply the same prefix mask and ignore test to routines as to classes. It was not checked.

<a id="c-d56"></a>
### C:D56: The `exec` help example that changes the namespace has no effect

Severity: Low · Decision: Open

Description: The documented exec example for changing namespaces reports success but leaves the session in its original namespace. Users following that example can run subsequent commands in a different namespace than they intended.

Run: Uplift 2026-11-16

Found by: source review of `Main.cls::ExecuteCOS` (`new $namespace`), then [iteration55 model](consolidated/iter55/nsinit.rs) `exec_neighbors` with control `exec-keeps-namespace-change` ([proof receipt](../uplift/consolidated/iter55/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration55 native receipt](../uplift/consolidated/iter55/native/20261116T181741769046Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. `exec set $namespace="%SYS"` returns OK and the session stays in USER (case e2). Passing neighbour: `exec set ^IPMCONT55=1` persists (e1).

The command help's own example is `cos set $Namespace = "MYAPP"`, contrary to the [exec statement](../ontology/features/developer-tools.g5.md#devtools.exec). A repair would either drop the namespace save or change the help example. It was not checked.

<a id="c-d57"></a>
### C:D57: `init -noprompt` reports enabling the package manager extension but leaves a plain source-control class active

Severity: Low · Decision: Open

Description: Initialization says the package-manager source-control extension was enabled even when a plain source-control class remains active. The message gives users a false indication that the integration is ready.

Run: Uplift 2026-11-16

Found by: iteration55 exploration refuting the source prediction, then source review of `Main.cls::Init` (`SetExtensionClasses` stores the primary without setting the namespace class), and [iteration55 model](consolidated/iter55/nsinit.rs) `init_neighbors` with control `init-activates-composite` ([proof receipt](../uplift/consolidated/iter55/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration55 native receipt](../uplift/consolidated/iter55/native/20261116T181741769046Z/observations.json). With `%Studio.SourceControl.Base` configured, `init -noprompt` prints "Enabling package manager extension if present... done" and lists Base as "(primary)", but the namespace class stays Base (case i2). Passing neighbour: `init -noprompt -extension %Studio.SourceControl.Base` activates the composite (i3).

Module lifecycle hooks therefore never reach the extension, contrary to the [init statement](../ontology/features/developer-tools.g5.md#devtools.init). A repair would also set the namespace class to the composite when wrapping, or word the message as a stored preference. It was not checked.

<a id="c-d59"></a>
### C:D59: Uninstall leaves a module's localized messages in the message dictionary

Severity: Low · Decision: Open

Description: Uninstall removes a localization module's document and registration but leaves its translated messages in the shared dictionary. Text supplied by an uninstalled module can remain available to applications.

Run: Uplift 2026-11-16

Found by: iteration56 native case u1 against the Clean phase, which deletes only the document, and [iteration56 model](consolidated/iter56/locmsgs.rs) `uninstall_keeps_messages` with control `uninstall-clears-messages` ([proof receipt](../uplift/consolidated/iter56/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration56 native receipt](../uplift/consolidated/iter56/native/20261116T182256528947Z/observations.json). After `uninstall loc56`, `^IRIS.Msg("D56Dom","en")` still holds messages A and C, while the document and module record are gone (u1). Passing neighbour: load and reload replace the domain correctly (l1, l2).

This contradicts Clean removing the module's resources, as stated in the [localization statement](../ontology/features/resources.g5.md#module.processor.localized-messages). A repair would delete the document's domains on Clean unless another module owns them. It was not checked.

<a id="c-d61"></a>
### C:D61: A CSPApplication load resets an existing application's undeclared properties

Severity: Low · Decision: Open

Description: Loading a CSPApplication resource resets existing application settings that the manifest did not specify. An administrator's timeout, description and authentication settings can be replaced while installing the module.

Run: Uplift 2026-11-16

Found by: source review of `ResourceProcessor/CSPApplication.cls::CreateOrUpdateCSPApp` (every known property written), then [iteration57 model](consolidated/iter57/cspapp.rs) `takeover_resets_undeclared` with control `takeover-keeps-undeclared` ([proof receipt](../uplift/consolidated/iter57/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration57 native receipt](../uplift/consolidated/iter57/native/20261116T182314549001Z/observations.json). An administrator's `/ipmcont57c` loses its description, its timeout of 1234 and its password authentication after `load m57c` (cases c0, c1). Passing neighbour: `WebApplication` keeps undeclared properties (iteration5).

This contradicts the [CSP lifecycle statement](../ontology/features/web-applications.g5.md#module.csp.lifecycle) for a deprecated but supported processor. A repair would write only declared attributes when the application exists. It was not checked.

<a id="c-d62"></a>
### C:D62: On Linux, a wildcard DFI resource skips `.dfi` and `.XML` files

Severity: Low · Decision: Open

Description: On Linux, wildcard discovery of dashboard resources accepts only some filename-capitalization variants. Files ending in .dfi or .XML are silently skipped even though equivalent .DFI and .xml files load.

Run: Uplift 2026-11-16

Found by: source review of `ResourceProcessor/Default/DeepSeeItem.cls::OnPhase`, whose UNIX branch appends the extra patterns without a `;`, then [iteration58 model](consolidated/iter58/dfiitems.rs) `wildcard_extension_neighbors` with control `separator-present` ([proof receipt](../uplift/consolidated/iter58/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration58 native receipt](../uplift/consolidated/iter58/native/20261116T182720911100Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. W2.dashboard.dfi and W3.dashboard.XML are not loaded while W1 (`.xml`) and W4 (`.DFI`) are (case d1). Passing neighbour: explicit `.xml` items load (d1, n1).

The skipped files produce no warning, contrary to the [DeepSee item statement](../ontology/features/resources.g5.md#module.processor.deepsee). A repair would add the missing separator. It was not checked.

<a id="c-d63"></a>
### C:D63: Uninstall leaves DFI items loaded through a wildcard resource

Severity: Low · Decision: Open

Description: Uninstall removes explicitly named dashboard items but leaves items discovered through a wildcard resource. The package is removed while some of its dashboards remain installed.

Run: Uplift 2026-11-16

Found by: native case u1, then [iteration58 model](consolidated/iter58/dfiitems.rs) `leftover_after_uninstall` with control `wildcard-cleaned` ([proof receipt](../uplift/consolidated/iter58/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration58 native receipt](../uplift/consolidated/iter58/native/20261116T182720911100Z/observations.json). After `uninstall m58`, the wildcard-loaded items W1 and W4 remain while the explicitly named A and B are removed (u1).

Uninstall therefore leaves module content behind, contrary to the [DeepSee item statement](../ontology/features/resources.g5.md#module.processor.deepsee). A repair would resolve wildcard children at Clean from the namespace items matching the pattern. It was not checked.

<a id="c-d64"></a>
### C:D64: Uninstall leaves the includes generated by a multi-entry LegacyLocalizedMessages resource

Severity: Low · Decision: Open

Description: A legacy localization resource containing several include-file mappings is parsed incorrectly for cleanup. Uninstall leaves the generated include files behind, although the single-file form is removed correctly.

Run: Uplift 2026-11-16

Found by: iteration60 exploration refuting the source prediction, then source review of `ResourceProcessor/LegacyLocalizedMessages.cls::OnResolveChildren`, and [iteration60 model](consolidated/iter60/legacymsgs.rs) `mapped_includes_leak` with control `children-per-entry` ([proof receipt](../uplift/consolidated/iter60/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration60 native receipt](../uplift/consolidated/iter60/native/20261116T182833808281Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. For `includeFiles="L60DomA:L60IncA,L60DomB:L60IncB"`, the resource's only resolved child is `L60IncA,L60DomB.INC` (case p2), and after `uninstall lm60` both `L60IncA.INC` and `L60IncB.INC` remain (u1). Passing neighbour: the single-include form resolves `L60Inc1.INC`, and uninstall deletes it (p1, u2).

The include name is read from the whole attribute instead of from each entry, contrary to the [legacy localization statement](../ontology/features/resources.g5.md#module.processor.localized-messages.legacy). A repair would take the include name from each list entry. It was not checked.

<a id="c-d68"></a>
### C:D68: Uninstall leaves a developer-mode module's `^Sources` entries

Severity: Low · Decision: Open

Description: Uninstalling a developer-mode module leaves source-location entries pointing to its old folders. The installed module and classes are gone, but source tooling can still find stale mappings for them.

Run: Uplift 2026-11-16

Found by: source search (no `^Sources` removal for module entries), then [iteration63 model](consolidated/iter63/cleanup.rs) `reload_side_effects_outlive_uninstall` with control `sources-cleared` ([proof receipt](../uplift/consolidated/iter63/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration63 native receipt](../uplift/consolidated/iter63/native/20261116T183505649094Z/observations.json). After `load u63d -dev` and `uninstall u63d`, `^Sources("MODULE","u63d")` and `^Sources("cls","U63D.")` remain and point at the old folders (case x2). Passing neighbour: the module's record, class and sync baseline rows are removed.

A later document in that package is still mapped to the old folder for source-control export, contrary to the [cleanup statement](../ontology/features/uninstall.g5.md#uninstall.cleanup). A repair would remove the module's `^Sources` entries in Clean. It was not checked.

<a id="c-d69"></a>
### C:D69: `document` regenerates no API documentation unless a module adds its own Document hooks

Severity: Low · Decision: Open

Description: The document action reports success without generating API documentation unless the module supplies its own hooks. Users expecting the command itself to regenerate documentation receive no output and no explanation.

Run: Uplift 2026-11-16

Found by: source search (no consumer of the Document phase besides `%Document`), then [iteration66 model](consolidated/iter66/docphase.rs) `phase_generates_nothing` with control `phase-writes-own-mark` ([proof receipt](../uplift/consolidated/iter66/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration66 native receipt](../uplift/consolidated/iter66/native/20261116T184640249526Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. `n66 document` succeeds and does nothing (case c3), and `m66 document` only runs the modules' own Document hooks, dependency first (c1). Passing neighbour: the hooks run in the documented order and roll back on failure (c1, c4).

This is documentation drift against the module-action help, recorded under the [Document phase statement](../ontology/architecture/lifecycle.g5.md#lifecycle.document). A repair would generate documentation or reword the help to "runs Document-phase hooks for the module and its dependencies". It was not checked.

<a id="c-d70"></a>
### C:D70: The composite's read-only check ignores other extensions

Severity: Low · Decision: Open

Description: When several source-control extensions are combined, the read-only check asks only the primary extension. Another extension can mark a file read-only while this check still permits editing, disagreeing with the reported file status.

Run: Uplift 2026-11-16

Found by: source review of `ExtensionBase/Composite.cls::IsReadOnly`, then [iteration67 model](consolidated/iter67/composite.rs) `other_extension_inconsistency` with control `readonly-all-extensions` ([proof receipt](../uplift/consolidated/iter67/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration67 native receipt](../uplift/consolidated/iter67/native/20261116T184753560177Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. With the primary not read-only and the other extension read-only, `IsReadOnly("M67.Main.cls")` returns 0 and the other extension is never asked (case r1), while `GetStatus` for the same configuration reports the item not editable (g1). Passing neighbour: a read-only primary gives 1 (r2).

The two checks disagree, contrary to the [editor statement](../ontology/architecture/lifecycle.g5.md#lifecycle.source-control.editor). A repair would consult the other extensions after a false primary answer. It was not checked.

<a id="c-d71"></a>
### C:D71: Configuring a nonexistent composite extension succeeds and then breaks source control in the namespace

Severity: Low · Decision: Open

Description: Source-control configuration accepts an extension class that does not exist. Creating the combined extension then fails, leaving source control unusable in that namespace until the configuration is corrected.

Run: Uplift 2026-11-16

Found by: source review of `ExtensionBase/CompositeMethodOverrides.cls::SetSubExtensionClasses` (validated list discarded, raw list stored) and `%OnNew`, then [iteration67 model](consolidated/iter67/composite.rs) `misconfigured_other_breaks_creation` with control `subs-filtered` ([proof receipt](../uplift/consolidated/iter67/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration67 native receipt](../uplift/consolidated/iter67/native/20261116T184753560177Z/observations.json). `SetExtensionClasses("IPMCont.P67",$lb("IPMCont.O67","No.Such67"))` returns OK and stores `No.Such67`, after which creating the composite fails (case s1). Passing neighbour: the same call without the bad name creates the composite (setup).

The trigger is a direct API call rather than an ordinary command, which keeps this Low. It contradicts the [editor statement](../ontology/architecture/lifecycle.g5.md#lifecycle.source-control.editor). A repair would store the validated list or refuse unknown class names. It was not checked.

<a id="p-f-dep-1"></a>
### P:F-DEP-1: `PermitDowngrade` is half implemented: the resolver drops the "≥ installed" bound and picks an older version, then the load refuses any downgrade.

Severity: Low · Decision: Open

Description: Allowing a downgrade lets dependency resolution select an older version, but the loader still refuses to install it. The option can produce a plan that IPM itself cannot execute.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-1)

Reproduced by: e2e reproduced. PermitDowngrade relaxes resolution then load refuses; normal load bound refusal neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/dependencies.g5.md#deps). No product repair or maintainer approval is claimed.

<a id="p-f-dep-2"></a>
### P:F-DEP-2: Lock-file dependencies are written in name order, not dependency order, although `LockFile.cls#L24` promises least-dependent first. Latent (no `ci` command y...

Severity: Low · Decision: Open

Description: Lock files list packages alphabetically instead of putting dependencies before their consumers. This is a recorded format-contract gap; the audit did not establish a current installation failure because lock-file replay was not implemented.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-2)

Reproduced by: e2e reproduced. ordinary create-lockfile emits name order while load output is dependency first; downstream ci is unimplemented.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/dependencies.g5.md#deps.lockfile). No product repair or maintainer approval is claimed.

<a id="p-f-dep-3"></a>
### P:F-DEP-3: `list-dependents` with a capitalised name (as in the help examples) prints nothing.

Severity: Low · Decision: Open

Description: The command that lists packages depending on a module returns no results when its name is capitalized. A valid module can appear to have no dependents when users follow the capitalization shown in help.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-3)

Reproduced by: e2e reproduced. list-dependents lowercase displays parent; mixed case displays none.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/inspection.g5.md#inspect.dependents). No product repair or maintainer approval is claimed.

<a id="p-f-dep-8"></a>
### P:F-DEP-8: A failed `install -create-lockfile` leaves a new lock file behind (written before the module's own Activate, outside the transaction). PR #963's author said ...

Severity: Low · Decision: Open

Description: An installation that later fails can leave a newly written lock file behind. That file describes a dependency selection from an installation that did not complete, which can mislead later tooling or users.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-8)

Reproduced by: e2e reproduced. failed create-lockfile install leaves JSON while module/dependency rows disappear; successful lockfile neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/dependencies.g5.md#deps.lockfile). No product repair or maintainer approval is claimed.

<a id="p-f-dep-9"></a>
### P:F-DEP-9: A needed module loaded from a folder (no registry) makes lock-file creation throw, so no file is written; the base module's empty registry is already handled.

Severity: Low · Decision: Open

Description: Lock-file creation fails when a dependency was loaded from a local folder and has no registry. A valid local dependency therefore prevents the installation from producing its requested lock file.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-9)

Reproduced by: e2e reproduced. ordinary local-loaded dependency produces lockfile warning and no changed file; registry supplier neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#deps.lockfile.local-supplier-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-repo-8"></a>
### P:F-REPO-8: `config` with an unknown key or a bad value prints a message and returns OK, so scripts cannot detect typos.

Severity: Low · Decision: Open

Description: Invalid configuration names and values print a warning but return success. Scripts cannot reliably detect a typo or rejected setting from the command result.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-8)

Reproduced by: e2e reproduced. ordinary config invalid key/value returns OK; valid config neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

Fresh reproduction on Linux x86-64 IRIS 2026.1: [iteration41 receipt](../uplift/consolidated/iter41/native/20261116T174828407055Z/observations.json), with the [iteration41 model](consolidated/iter41/settings.rs) replaying the same outcome.

[Governing owner](../ontology/operations/settings.g5.md#settings.print). No product repair or maintainer approval is claimed.

<a id="p-f-sync-3"></a>
### P:F-SYNC-3: `reload` of a module not in developer mode (no root) loads nothing and reports SUCCESS.

Severity: Low · Decision: Open

Description: Reloading a module without a developer source root reports success while loading nothing. The result suggests the server code was refreshed even though no source was available to reload.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-sync-3)

Reproduced by: e2e reproduced. ordinary reload of normal module succeeds without reloading edited file; root-bearing developer load neighbor.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](open_questions.md#lifecycle.empty-reload-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-unin-9"></a>
### P:F-UNIN-9: A dependency removed by `uninstall -r` keeps its stored manifest in `^IPM.StudioDoc.ModuleStreamD`; only the top module's manifest is deleted.

Severity: Low · Decision: Open

Description: Recursive uninstall deletes a removed dependency's module record but keeps its stored manifest text. The top-level module is cleaned up correctly, leaving inconsistent cleanup for its dependencies.

Found by: [Starting entry and evidence](../uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-9)

Reproduced by: e2e reproduced. ordinary recursive uninstall removes dependency code/module but retains its manifest; top manifest is removed.

[Executable regression receipt](../uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../ontology/features/uninstall.g5.md#uninstall.recurse). No product repair or maintainer approval is claimed.
