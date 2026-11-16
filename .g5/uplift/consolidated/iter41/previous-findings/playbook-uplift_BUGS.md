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
<a id="summary"></a><!-- historical section alias -->
<a id="how-to-read-this-file"></a><!-- historical section alias -->
# IPM findings: P register

Product: `571e04a1`; baseline: sealed iteration 28. [Full reconciliation and commands](../.g5/uplift/consolidated/findings-audit-iter28/README.md). Decisions and historical claims are preserved in the review. Entries with Decision: Open are active; review and scope records are not active functional findings.

<a id="p-f-inst-1"></a>
### P:F-INST-1: `update` reports success without running update steps when the update lock is busy for 3 s. No message unless verbose. The steps stay due, and nothing prompt...

Severity: High · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-1)

Reproduced by: e2e reproduced. real competing update lock; command OK with old step rows; unlocked retry runs due steps.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/update.g5.md#update.steps.busy-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-inst-6"></a>
### P:F-INST-6: A class of the update package named `V<n><something>` (e.g. `V3Helper`) replaces step class `V<n>` in the step list; update then reports success without runn...

Severity: High · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-6)

Reproduced by: e2e reproduced. real V3Helper fixture suppresses V3 execution; V2/V9/V10 ordinary order neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/update.g5.md#update.steps). No product repair or maintainer approval is claimed.

<a id="p-f-unin-1"></a>
### P:F-UNIN-1: A blocked uninstall (a dependent exists, or global scope) still deletes the module's web and CSP applications and runs `Clean`/`Unconfigure` hooks, then prin...

Severity: High · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-1)

Reproduced by: e2e reproduced. blocked uninstall deletes actual test web application; module remains; ordinary global-resource uninstall neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/uninstall.g5.md#uninstall.dependents.as-built). No product repair or maintainer approval is claimed.

<a id="p-f-unin-4"></a>
### P:F-UNIN-4: Uninstall without `-force` removes a module that a dependent still needs if that dependent's dependency graph fails to build (e.g. a registry is missing): gr...

Severity: High · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-4)

Reproduced by: e2e reproduced. dependent graph error lets uninstall remove required class; resolvable dependent neighbor blocks.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/uninstall.g5.md#uninstall.dependents). No product repair or maintainer approval is claimed.

<a id="p-f-dep-7"></a>
### P:F-DEP-7: The resolver is greedy (newest first, no backtracking past one level) and fails on solvable inputs, although its comment says "with backtracking". Known upst...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-7)

Reproduced by: e2e reproduced. ordinary solvable graph install refuses; pinned solution installs all modules.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/dependencies.g5.md#deps). No product repair or maintainer approval is claimed.

<a id="p-f-inst-10"></a>
### P:F-INST-10: An update that brings in a module for the first time runs all of that module's update steps instead of seeding them as a fresh install does.

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-10)

Reproduced by: e2e reproduced. new dependency installed during update runs V1; fresh normal install seeds without running.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/update.g5.md#update.steps.fresh-seeding-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-inst-4"></a>
### P:F-INST-4: Between registries that offer the same version, the registry created first wins; `OverriddenSortOrder` has no effect. Deleting and re-adding a registry chang...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-4)

Reproduced by: e2e reproduced. two filesystem repositories with tied versions; creation-order selection and reordered neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/concepts/repositories.g5.md#repo.search-order). No product repair or maintainer approval is claimed.

<a id="p-f-inst-5"></a>
### P:F-INST-5: `update -dev` from a registry does not run in developer mode, and a plain `update` of a module stored in developer mode clears it. Only `update -dev -path` k...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-5)

Reproduced by: e2e reproduced. ordinary update -dev from filesystem registry ends normal; local -path developer neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/update.g5.md#update.dev-mode). No product repair or maintainer approval is claimed.

<a id="p-f-inst-7"></a>
### P:F-INST-7: A class of the update package whose name starts with `V` but has no digit (e.g. `Validator`) makes every update of the module fail with `<SUBSCRIPT>`, naming...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-7)

Reproduced by: e2e reproduced. real Validator fixture makes update fail; numbered-class neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/update.g5.md#update.steps). No product repair or maintainer approval is claimed.

<a id="p-f-inst-8"></a>
### P:F-INST-8: A developer-mode install (or a plain `load` of a module stored in developer mode) seeds no update steps, so the next update reruns every step from V1, includ...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-8)

Reproduced by: e2e reproduced. developer fresh install leaves zero step records; later update executes old steps; normal fresh install seeds.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/update.g5.md#update.steps.dev-seeding-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-inst-9"></a>
### P:F-INST-9: Install silently replaces a needed module that is in developer mode and clears its developer mode; the guard covers only the named module. Wiki Concepts: a d...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-9)

Reproduced by: e2e reproduced. ordinary parent install replaces developer dependency; initial developer load verifies prestate.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.dependency-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-nsp-2"></a>
### P:F-NSP-2: In a namespace that gets IPM by mapping, an `IPMVersion` requirement is checked against an empty version, so every module with such a requirement is refused ...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-2)

Reproduced by: e2e reproduced. ordinary requirement load fails in mapped namespace, succeeds in source namespace.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/concepts/namespaces.g5.md#namespace.version.mapped-reader). No product repair or maintainer approval is claimed.

<a id="p-f-nsp-4"></a>
### P:F-NSP-4: `enable -map -repos` (and `enable -community`) on a namespace with no IPM maps the code but skips the registries; a second run is needed. The repo step reads...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-4)

Reproduced by: e2e reproduced. enable -map -repos skips first repo mapping; same command second run maps it.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/concepts/namespaces.g5.md#namespace.enable.mapping-results). No product repair or maintainer approval is claimed.

<a id="p-f-nsp-6"></a>
### P:F-NSP-6: `unmap` leaves the `%IPM.*` routine mapping: the namespace still runs the source's IPM and is still reported as mapped. Argument-slot bug: the third argument...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-6)

Reproduced by: e2e reproduced. unmap removes package mapping but routine mapping remains and IPM still callable; source refusal neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/concepts/namespaces.g5.md#namespace.unmap.routine-result). No product repair or maintainer approval is claimed.

<a id="p-f-repo-5"></a>
### P:F-REPO-5: `default-modifiers` are documented per namespace but stored in `^%IPM.DefaultModifiers`, which IRIS keeps in IRISSYS: setting `-DNoTransaction=1` in one name...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-5)

Reproduced by: e2e reproduced. real namespaces read shared default modifiers; local namespace settings neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/concepts/settings.g5.md#settings.default-modifiers). No product repair or maintainer approval is claimed.

<a id="p-f-sync-1"></a>
### P:F-SYNC-1: A failed `compile`, `load -dev` or `reload` that puts edited source on the server records no baseline and no retry mark. If the file is then reverted to its ...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-sync-1)

Reproduced by: e2e reproduced. ordinary failed compile then exact-byte revert makes sync skip stale server definition; successful initial load neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/developer-tools.g5.md#devtools.sync.failed-import-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-unin-2"></a>
### P:F-UNIN-2: `uninstall -purge` deletes no table data. Nothing reads the `Purge` parameter; `%Clean` deletes classes without the extent flag `e`. (An installer can read `...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-2)

Reproduced by: e2e reproduced. ordinary persistent sentinel survives -purge and schema removal/reload; real extent-delete and installer callback neighbors.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T030017516810Z-iter27/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `iter27`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/uninstall.g5.md#uninstall.purge.as-built). No product repair or maintainer approval is claimed.

<a id="p-f-unin-5"></a>
### P:F-UNIN-5: A global resource declared only by name (`<Resource Name="X.GBL"/>`, the usual form) keeps its data after uninstall; only resources with the `Global` attribu...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-5)

Reproduced by: e2e reproduced. ordinary name-only global resource retains data; explicit Global attribute deletes it.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/uninstall.g5.md#uninstall). No product repair or maintainer approval is claimed.

<a id="p-f-unin-6"></a>
### P:F-UNIN-6: A blocked uninstall returns OK, prints `Clean SUCCESS` and logs `uninstall/success` while the module stays. The code comment at `Lifecycle/Base.cls#L276` say...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-6)

Reproduced by: e2e reproduced. blocked uninstall returns OK and logs success with module retained; successful ordinary uninstall neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/uninstall.g5.md#uninstall.dependents). No product repair or maintainer approval is claimed.

<a id="p-f-dep-1"></a>
### P:F-DEP-1: `PermitDowngrade` is half implemented: the resolver drops the "≥ installed" bound and picks an older version, then the load refuses any downgrade.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-1)

Reproduced by: e2e reproduced. PermitDowngrade relaxes resolution then load refuses; normal load bound refusal neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/dependencies.g5.md#deps). No product repair or maintainer approval is claimed.

<a id="p-f-dep-2"></a>
### P:F-DEP-2: Lock-file dependencies are written in name order, not dependency order, although `LockFile.cls#L24` promises least-dependent first. Latent (no `ci` command y...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-2)

Reproduced by: e2e reproduced. ordinary create-lockfile emits name order while load output is dependency first; downstream ci is unimplemented.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/dependencies.g5.md#deps.lockfile). No product repair or maintainer approval is claimed.

<a id="p-f-dep-3"></a>
### P:F-DEP-3: `list-dependents` with a capitalised name (as in the help examples) prints nothing.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-3)

Reproduced by: e2e reproduced. list-dependents lowercase displays parent; mixed case displays none.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/inspection.g5.md#inspect.dependents). No product repair or maintainer approval is claimed.

<a id="p-f-dep-8"></a>
### P:F-DEP-8: A failed `install -create-lockfile` leaves a new lock file behind (written before the module's own Activate, outside the transaction). PR #963's author said ...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-8)

Reproduced by: e2e reproduced. failed create-lockfile install leaves JSON while module/dependency rows disappear; successful lockfile neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/dependencies.g5.md#deps.lockfile). No product repair or maintainer approval is claimed.

<a id="p-f-dep-9"></a>
### P:F-DEP-9: A needed module loaded from a folder (no registry) makes lock-file creation throw, so no file is written; the base module's empty registry is already handled.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-9)

Reproduced by: e2e reproduced. ordinary local-loaded dependency produces lockfile warning and no changed file; registry supplier neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031657840431Z-inst/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `inst`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/dependencies.g5.md#deps.lockfile.local-supplier-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-load-4"></a>
### P:F-LOAD-4: A load can fail after its final commit: an after-install message with `{` and a `$$$` macro that does not compile throws in `%EvaluateMacro`. The command ret...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-load-4)

Reproduced by: e2e reproduced. ordinary after-install macro failure with durable module/history committed; successful message neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/load.g5.md#load). No product repair or maintainer approval is claimed.

<a id="p-f-repo-4"></a>
### P:F-REPO-4: `config delete` restores the first value ever set, not the factory default, for keys the installer does not seed (`TerminalPrompt`, `PublishTimeout`, `Histor...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-4)

Reproduced by: e2e reproduced. config delete restores first stored value; recorded changed-value prestate.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/concepts/settings.g5.md#settings). No product repair or maintainer approval is claimed.

<a id="p-f-repo-8"></a>
### P:F-REPO-8: `config` with an unknown key or a bad value prints a message and returns OK, so scripts cannot detect typos.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-8)

Reproduced by: e2e reproduced. ordinary config invalid key/value returns OK; valid config neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/concepts/settings.g5.md#settings.print). No product repair or maintainer approval is claimed.

<a id="p-f-sync-3"></a>
### P:F-SYNC-3: `reload` of a module not in developer mode (no root) loads nothing and reports SUCCESS.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-sync-3)

Reproduced by: e2e reproduced. ordinary reload of normal module succeeds without reloading edited file; root-bearing developer load neighbor.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T030308508323Z-nsp/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `nsp`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/concepts/lifecycle.g5.md#lifecycle.empty-reload-policy). No product repair or maintainer approval is claimed. This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.

<a id="p-f-unin-9"></a>
### P:F-UNIN-9: A dependency removed by `uninstall -r` keeps its stored manifest in `^IPM.StudioDoc.ModuleStreamD`; only the top module's manifest is deleted.

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-9)

Reproduced by: e2e reproduced. ordinary recursive uninstall removes dependency code/module but retains its manifest; top manifest is removed.

[Executable regression receipt](../.g5/uplift/consolidated/findings-audit-iter28/runs/20261116T031648777527Z-repo/receipt.json); [Runner](../.g5/uplift/consolidated/findings-audit-iter28/run_regressions.py) `repo`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../.g5/ontology/features/uninstall.g5.md#uninstall.recurse). No product repair or maintainer approval is claimed.

## Outside active reproduction scope

<a id="p-f-repo-1"></a>
### P:F-REPO-1 review record

Severity: Unranked

P:F-REPO-1: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-1)

<a id="p-f-repo-10"></a>
### P:F-REPO-10 review record

Severity: Unranked

P:F-REPO-10: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-10)

<a id="p-f-repo-11"></a>
### P:F-REPO-11 review record

Severity: Unranked

P:F-REPO-11: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-11)

<a id="p-f-repo-2"></a>
### P:F-REPO-2 review record

Severity: Unranked

P:F-REPO-2: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-2)

<a id="p-f-repo-7"></a>
### P:F-REPO-7 review record

Severity: Unranked

P:F-REPO-7: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-7)

<a id="p-f-unin-3"></a>
### P:F-UNIN-3 review record

Severity: Unranked

P:F-UNIN-3: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-3)

<a id="p-f-unin-8"></a>
### P:F-UNIN-8 review record

Severity: Unranked

P:F-UNIN-8: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-8)

## IDs forwarded to their owning record or review

<a id="p-f-dep-4"></a>
### P:F-DEP-4 review record

Severity: Unranked

P:F-DEP-4: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-4)

<a id="p-f-dep-5"></a>
### P:F-DEP-5 review record

Severity: Unranked

P:F-DEP-5: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-5)

<a id="p-f-dep-6"></a>
### P:F-DEP-6 review record

Severity: Unranked

P:F-DEP-6: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-dep-6)

<a id="p-f-hist-1"></a>
### P:F-HIST-1 review record

Severity: Unranked

P:F-HIST-1: alias. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-1)

<a id="p-f-hist-10"></a>
### P:F-HIST-10 review record

Severity: Unranked

P:F-HIST-10: alias. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-10)

<a id="p-f-hist-2"></a>
### P:F-HIST-2 review record

Severity: Unranked

P:F-HIST-2: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-2)

<a id="p-f-hist-3"></a>
### P:F-HIST-3 review record

Severity: Unranked

P:F-HIST-3: alias. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-3)

<a id="p-f-hist-4"></a>
### P:F-HIST-4 review record

Severity: Unranked

P:F-HIST-4: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-4)

<a id="p-f-hist-5"></a>
### P:F-HIST-5 review record

Severity: Unranked

P:F-HIST-5: alias. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-5)

<a id="p-f-hist-6"></a>
### P:F-HIST-6 review record

Severity: Unranked

P:F-HIST-6: alias. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-6)

<a id="p-f-hist-7"></a>
### P:F-HIST-7 review record

Severity: Unranked

P:F-HIST-7: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-7)

<a id="p-f-hist-8"></a>
### P:F-HIST-8 review record

Severity: Unranked

P:F-HIST-8: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-8)

<a id="p-f-hist-9"></a>
### P:F-HIST-9 review record

Severity: Unranked

P:F-HIST-9: alias. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-hist-9)

<a id="p-f-inst-11"></a>
### P:F-INST-11 review record

Severity: Unranked

P:F-INST-11: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-11)

<a id="p-f-inst-2"></a>
### P:F-INST-2 review record

Severity: Unranked

P:F-INST-2: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-2)

<a id="p-f-inst-3"></a>
### P:F-INST-3 review record

Severity: Unranked

P:F-INST-3: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-inst-3)

<a id="p-f-load-1"></a>
### P:F-LOAD-1 review record

Severity: Unranked

P:F-LOAD-1: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-load-1)

<a id="p-f-load-2"></a>
### P:F-LOAD-2 review record

Severity: Unranked

P:F-LOAD-2: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-load-2)

<a id="p-f-load-3"></a>
### P:F-LOAD-3 review record

Severity: Unranked

P:F-LOAD-3: inconclusive/execution-scope-blocked. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-load-3)

<a id="p-f-nsp-1"></a>
### P:F-NSP-1 review record

Severity: Unranked

P:F-NSP-1: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-1)

<a id="p-f-nsp-10"></a>
### P:F-NSP-10 review record

Severity: Unranked

P:F-NSP-10: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-10)

<a id="p-f-nsp-11"></a>
### P:F-NSP-11 review record

Severity: Unranked

P:F-NSP-11: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-11)

<a id="p-f-nsp-12"></a>
### P:F-NSP-12 review record

Severity: Unranked

P:F-NSP-12: alias. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-12)

<a id="p-f-nsp-3"></a>
### P:F-NSP-3 review record

Severity: Unranked

P:F-NSP-3: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-3)

<a id="p-f-nsp-5"></a>
### P:F-NSP-5 review record

Severity: Unranked

P:F-NSP-5: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-5)

<a id="p-f-nsp-7"></a>
### P:F-NSP-7 review record

Severity: Unranked

P:F-NSP-7: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-7)

<a id="p-f-nsp-8"></a>
### P:F-NSP-8 review record

Severity: Unranked

P:F-NSP-8: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-8)

<a id="p-f-nsp-9"></a>
### P:F-NSP-9 review record

Severity: Unranked

P:F-NSP-9: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-nsp-9)

<a id="p-f-repo-12"></a>
### P:F-REPO-12 review record

Severity: Unranked

P:F-REPO-12: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-12)

<a id="p-f-repo-3"></a>
### P:F-REPO-3 review record

Severity: Unranked

P:F-REPO-3: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-3)

<a id="p-f-repo-6"></a>
### P:F-REPO-6 review record

Severity: Unranked

P:F-REPO-6: refuted/unreachable-CLI-trigger. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-6)

<a id="p-f-repo-9"></a>
### P:F-REPO-9 review record

Severity: Unranked

P:F-REPO-9: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-repo-9)

<a id="p-f-res-1"></a>
### P:F-RES-1 review record

Severity: Unranked

P:F-RES-1: alias. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-1)

<a id="p-f-res-2"></a>
### P:F-RES-2 review record

Severity: Unranked

P:F-RES-2: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-2)

<a id="p-f-res-3"></a>
### P:F-RES-3 review record

Severity: Unranked

P:F-RES-3: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-3)

<a id="p-f-res-4"></a>
### P:F-RES-4 review record

Severity: Unranked

P:F-RES-4: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-4)

<a id="p-f-res-5"></a>
### P:F-RES-5 review record

Severity: Unranked

P:F-RES-5: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-5)

<a id="p-f-res-6"></a>
### P:F-RES-6 review record

Severity: Unranked

P:F-RES-6: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-6)

<a id="p-f-res-7"></a>
### P:F-RES-7 review record

Severity: Unranked

P:F-RES-7: inconclusive/execution-scope-blocked. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-7)

<a id="p-f-res-8"></a>
### P:F-RES-8 review record

Severity: Unranked

P:F-RES-8: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-res-8)

<a id="p-f-sync-2"></a>
### P:F-SYNC-2 review record

Severity: Unranked

P:F-SYNC-2: documentation-or-test-gap. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-sync-2)

<a id="p-f-unin-10"></a>
### P:F-UNIN-10 review record

Severity: Unranked

P:F-UNIN-10: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-10)

<a id="p-f-unin-11"></a>
### P:F-UNIN-11 review record

Severity: Unranked

P:F-UNIN-11: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-11)

<a id="p-f-unin-7"></a>
### P:F-UNIN-7 review record

Severity: Unranked

P:F-UNIN-7: inconclusive. [Disposition, source and remaining gate](../.g5/uplift/consolidated/findings-audit-iter28/BUG-REVIEW.md#p-f-unin-7)
