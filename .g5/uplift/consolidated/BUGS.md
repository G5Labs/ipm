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
# IPM findings: C register

Product: `571e04a1`; baseline: sealed iteration 28. [Full reconciliation and commands](findings-audit-iter28/README.md). Decisions and historical claims are preserved in the review. Entries with Decision: Open are active; review and scope records are not active functional findings.

<a id="c-d6"></a>
### C:D6: An operator on a partial version is dropped. `<2` means `2.x` (`>=2.0.0 <3.0.0-0`), `>=1.2` and `>1.2` mean `1.2.x`, `<=1` means `1.x`. A dependency declared...

Severity: High · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d6)

Reproduced by: e2e reproduced. ordinary dependency load installs 2.5.0 for <2; valid dependency neighbor Dok.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d39"></a>
### C:D39: A filesystem repository install picks a prerelease over its own release and orders prerelease numbers as text

Severity: High · Decision: Open

Run: Uplift 2026-11-16

Found by: iteration34 exploration (`iter34/attempts/explore1.log`) refuting the source prediction that install follows the version index, then source review of `Repo/Utils.cls::moduleSqlToList`, and [iteration34 model](../../Verus_verification/consolidated/iter34/versions.rs) `release_versus_own_prerelease` and `fixture_neighbors` with controls `sql-release-first` and `sql-prerelease-semantic` ([proof receipt](iter34/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration34 native receipt](iter34/native/20261116T172512256458Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`, filesystem repositories created by `repo -fs` (which stores prereleases on). With `i34d` offered at 2.0.0 and 2.0.0-beta.1, `install i34d` installs 2.0.0-beta.1 (case d1) while the repository's own cache query lists 2.0.0 first (q5). With `i34c` at 0.9.0, 1.0.0-rc.2 and 1.0.0-rc.10, `install i34c` installs rc.2 (c1) while the cache query lists rc.10 first (q3). Passing neighbours: `install i34d 2.0.0` installs the release (d2), and with releases only, `install i34b 1.x` installs 1.10.0 over 1.9.0 (a3).

`moduleSqlToList` orders the cache rows by major, minor and patch, then by the prerelease column as text in descending order, and keeps the first row per module. That text order puts a release, whose prerelease is empty, after its own prereleases, and `rc.2` before `rc.10`. The repository manager then sees one offer per module, so Main.Install installs it. The method's comment says the query "returns items in correct order", and Main.Install says "the latest version will always be first". The [version order](../../ontology/concepts/versions.g5.md#version.order) and the [search rule](../../ontology/concepts/repositories.g5.md#repo.search-order) require the release and rc.10. A repair would keep the index order (`m.%id`) instead of the explicit version columns. It was not checked.

<a id="c-d40"></a>
### C:D40: Uninstalling a Copy resource deletes classes that Overwrite="0" kept and the module never wrote

Severity: High · Decision: Open

Run: Uplift 2026-11-16

Found by: iteration37 native exploration (`iter37/attempts/explore5.log`), then source review of `Lifecycle/Base.cls::%Clean` child resolution against `ResourceProcessor/Copy.cls::OnAfterPhase`, and [iteration37 model](../../Verus_verification/consolidated/iter37/copy.rs) `uninstall_deletes_kept_item` with control `uninstall-only-copied` ([proof receipt](iter37/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration37 native receipt](iter37/native/20261116T173211781373Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. The installing namespace already holds `CopyB.D1` (returns 4) before module `cpb` copies package `CopyB` from namespace CPSRC with `Overwrite="0"`. The load keeps D1 at 4 and copies only D2 (o2). `uninstall cpb` then deletes both D1 and D2 (u2). Passing neighbours: `CopyD.Owned`, listed by another installed module, survives `uninstall cpd` while the copied `CopyD.Free` is removed (u4). With the default `Overwrite`, the replaced pre-existing `CopyA.C2` is also removed by `uninstall cpa` (u1).

The Copy load writes only the classes it exports from the source, but uninstall resolves the package in the installing namespace and deletes every class found there that no other module owns. Code a user kept by choosing `Overwrite="0"` is therefore lost on uninstall, contrary to the [Copy statement](../../ontology/concepts/modules.g5.md#module.processor.copy) and to uninstall removing the module's own resources. A repair would record the classes each load actually copied and delete only those. It was not checked.

<a id="c-d4"></a>
### C:D4: A parameter value that contains `$` cannot be used. `%RegExReplace` passes the value as ICU replacement text and escapes only backslashes, so `$5` or an unfi...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d4)

Reproduced by: e2e reproduced. literal dollar Default makes load fail; five succeeds.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/modules.g5.md#module.parameter). No product repair or maintainer approval is claimed.

<a id="c-d7"></a>
### C:D7: `^1.2` (a caret on a two-part version) means `1.2.x` instead of `>=1.2.0 <2.0.0-0`. The branch written for it is unreachable, because a two-part version is a...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d7)

Reproduced by: partial reproduced. real range parser witness and valid caret neighbors; ordinary resolver caller with ^1.2 not re-executed.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d8"></a>
### C:D8: Combining two requirements depends on their order when two bounds differ only by a prerelease part: the first is kept. `^1.0.0` AND `<2.0.0` gives `<2.0.0-0`...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d8)

Reproduced by: e2e reproduced. named diamond load refusal with source-selected dependency bounds; successful neighbor.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d14"></a>
### C:D14: An OS environment value used in an `-env` file through `${VAR}` is inserted as ICU replacement text: `a$1b` becomes `aCONS_DOLLARb` (the variable's name), `a...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d14)

Reproduced by: e2e reproduced. ordinary -env load feeds altered dollar value to its real Invoke reader; five neighbor.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/modules.g5.md#module.env). No product repair or maintainer approval is claimed.

<a id="c-d15"></a>
### C:D15: The downgrade refusal misses prereleases of different versions: with `2.0.0-beta` installed, `load` of `1.0.0-rc` succeeds and replaces it. `Follows` treats ...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d15)

Reproduced by: e2e reproduced. ordinary prerelease downgrade accepted; release and mixed-release downgrades refused.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/features/load.g5.md#load.refusal.downgrade). No product repair or maintainer approval is claimed.

<a id="c-d19"></a>
### C:D19: Document export overwrites the deployed-item helper's error before its error guard. A controlled failing override followed by a real source export returns OK...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d19)

Reproduced by: partial reproduced (historical helper witness; not rerun). existing allowed deployed-helper override shows local status suppression; no ordinary default-helper failure/full package entry established; no new package/archive run.

[Governing owner](../../ontology/features/build-and-publish.g5.md#build.package.deployed-status). No product repair or maintainer approval is claimed.

<a id="c-d16"></a>
### C:D16: First plain `load -lock` uses an empty stored Root and writes relative `module-lock.json` in process cwd. A developer-mode or direct nonempty-Root neighbor w...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d16)

Reproduced by: e2e reproduced. first ordinary load -lock writes valid audit-lock JSON in process cwd instead of source root; Root is populated by return, so the final record is not evidence of its earlier availability to the writer; developer-mode neighbor writes at source.

[Executable regression receipt](findings-audit-iter28/runs/20261116T033609225862Z-lock/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `lock`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/features/dependencies.g5.md#deps.lockfile). No product repair or maintainer approval is claimed.

<a id="c-d24"></a>
### C:D24: The `$zpm("<command>")` function form raises `<COMMAND> Function must return a value` after running the command, pass or fail, so scripts cannot read the sta...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d24)

Reproduced by: e2e reproduced. $zpm(version) throws after command; Shell(version) returns success.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/root.g5.md#ipm.shell.forms). No product repair or maintainer approval is claimed.

<a id="c-d27"></a>
### C:D27: A supported processor returns an error from OnResolveChildren. Package refuses, but uninstall discards the status, deletes module/manifest metadata and leave...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d27)

Reproduced by: e2e reproduced. supported error-returning resource extension; uninstall deletes metadata while class remains callable; successful extension neighbor.

[Executable regression receipt](findings-audit-iter28/runs/20261116T030635655399Z-error/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `error`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/features/uninstall.g5.md#uninstall.child-error). No product repair or maintainer approval is claimed.

<a id="c-d28"></a>
### C:D28: A supported editing lock makes OBJ.Delete refuse and keep a class. Uninstall warns, returns success and deletes module/manifest metadata; class remains calla...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d28)

Reproduced by: e2e reproduced. real class edit lock; uninstall succeeds with callable class retained; unlocked uninstall removes it.

[Executable regression receipt](findings-audit-iter28/runs/20261116T030643833060Z-iter11/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `iter11`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/features/uninstall.g5.md#uninstall.class-delete-error). No product repair or maintainer approval is claimed.

<a id="c-d29"></a>
### C:D29: FileCopy Overlay=0 loads new files but retains unrelated old entries instead of replacing the directory. Direct CopyDir flags0/1 both keep a sentinel and rep...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d29)

Reproduced by: e2e reproduced. ordinary FileCopy Overlay=0 retains unrelated sentinel and writes new payload; real Unix filesystem provider.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/modules.g5.md#module.processor.file-copy.unix-overlay). No product repair or maintainer approval is claimed.

<a id="c-d31"></a>
### C:D31: A failing outer test runs a passing child through ordinary Shell. Outer device/status identify the failed outer run, but the outer JSON output file contains ...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d31)

Reproduced by: e2e reproduced. actual outer/child Shell test runs, final file/device identities and alone neighbors.

[Executable regression receipt](findings-audit-iter28/runs/20261116T030004292651Z-iter17/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `iter17`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/features/build-and-publish.g5.md#build.test-output.run-identity). No product repair or maintainer approval is claimed.

<a id="c-d32"></a>
### C:D32: Cache a valid1.0.0 directory, then replace its manifest/code with2.0.0 without rebuilding the cache. Search/API stilloffer1, but ordinary explicit `install t...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d32)

Reproduced by: e2e reproduced. ordinary explicit-version install reads changed directory bytes; stable/rebuilt/refusal neighbors.

[Executable regression receipt](findings-audit-iter28/runs/20261116T030008965338Z-iter21/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `iter21`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/repositories.g5.md#repo.install.version-request). No product repair or maintainer approval is claimed.

<a id="c-d33"></a>
### C:D33: Install a parent with exact dependency `1.0.0+snapshot`, change that dependency's source payload, then ordinarily reinstall the parent. The forced refresh ad...

Severity: Medium · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d33)

Reproduced by: e2e reproduced. default snapshot reinstall refuses; shallow/release/explicit-permission neighbors.

[Executable regression receipt](findings-audit-iter28/runs/20261116T030013086002Z-iter24/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `iter24`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/versions.g5.md#version.range.snapshot-intersection). No product repair or maintainer approval is claimed.

<a id="c-d34"></a>
### C:D34: `-only` with two or more phase names fails before any phase runs

Severity: Medium · Decision: Open

Run: Uplift 2026-11-16

Found by: source review of `Lifecycle/Base.cls::MatchSinglePhase` and `Storage/Module.cls::ExecutePhases`, then [iteration30 model](../../Verus_verification/consolidated/iter30/phases.rs) `only_multiple_names_fail` and its control `only-runs-each-name` ([proof receipt](iter30/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration30 native receipt](iter30/native/20261116T170433591502Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`, developer-mode directory module `phasecase`. `phasecase compile activate -only` (case c3) and `mycustom compile -only` (c10) return an error status with `<METHOD DOES NOT EXIST> *%Compile,Activate` and `*%Mycustom,Compile`. Neither the After-Compile nor the After-Activate hook runs, and the compiled class keeps its old value. Passing neighbours: `phasecase compile -only` (c1), `compile phasecase -only` (c9) and `phasecase activate -only` (c11) each run exactly the named phase.

`ModuleAction` builds a phase list and, under `-only`, `ExecutePhases` passes `$listtostring` of the whole list to `MatchSinglePhase`. The comma-joined name matches neither a standard nor a custom phase, so the lifecycle method call fails. The module-action help says `-only` "Only runs the specified phase(s)", and the [governing statement](../../ontology/concepts/lifecycle.g5.md#lifecycle.only) says it runs just the named phases. A repair would match each name separately and keep the given order. A distinguishing check is c3 running Compile then Activate with no Initialize or Reload. The proposed repair was not checked.

<a id="c-d35"></a>
### C:D35: A phase list silently drops custom and unknown names listed beside a standard phase

Severity: Medium · Decision: Open

Run: Uplift 2026-11-16

Found by: source review of `Lifecycle/Base.cls::GetCompletePhases`, then [iteration30 model](../../Verus_verification/consolidated/iter30/phases.rs) `custom_and_unknown_neighbors` and its control `custom-beside-standard-runs` ([proof receipt](iter30/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration30 native receipt](iter30/native/20261116T170433591502Z/observations.json). `phasecase reload mycustom` (case c4) succeeds after Initialize and Reload, and the module's `MyCustom` Invoke counter does not change. `phasecase compile bogus` (c6) succeeds as a plain compile chain. Passing and contrasting neighbours: `phasecase MYCUSTOM` (c5) runs the custom hook once, and `phasecase bogus` (c7) fails.

In complete mode `GetCompletePhases` keeps a non-standard name only when it is the sole name. Otherwise each name contributes its standard chain, and a custom or unknown name contributes none. A requested custom phase is therefore skipped without notice, and a mistyped phase name beside a valid one is ignored while the command reports success. The [custom-phase statement](../../ontology/concepts/lifecycle.g5.md#lifecycle.custom) says a custom phase runs when requested by name. Where it should run in a mixed list, and whether an unknown name must refuse the command, is the open [phase-list policy](../../ontology/concepts/lifecycle.g5.md#lifecycle.phase-list-policy). No repair was checked.

<a id="c-d36"></a>
### C:D36: A module whose classes sit directly under SourcesRoot loads, but sync never sees its edits or deletions

Severity: Medium · Decision: Open

Run: Uplift 2026-11-16

Found by: native exploration of the iteration32 sync workflow, then source review of `ResourceProcessor/Default/Document.cls::GetSyncDirectory` against its Reload directory list, and [iteration32 model](../../Verus_verification/consolidated/iter32/sync.rs) `flat_schedule` with control `scan-uses-load-fallback` ([proof receipt](iter32/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration32 native receipt](iter32/native/20261116T171504430334Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. Developer-mode module `syncf` declares `SyncF.PKG` with files at `src/SyncF/*.cls`; `load -dev` loads them. After editing `SyncF.A` to return 2, `sync syncf` prints "Nothing to sync." and the class still returns 1 (case f1). After deleting `B.cls`, `sync syncf -delete` again prints "Nothing to sync." and `SyncF.B` stays (f2). `load syncf -dev` then picks up the edit (f3). The passing neighbour is module `syncc` with files at `src/cls/...`, whose edits and deletions sync (s1-s5).

Reload looks for a resource in `{SourcesRoot}/{Directory}` and, when Directory is not declared, also directly in `{SourcesRoot}`. Sync scans and tracks only `{SourcesRoot}/{Directory}` (Directory defaults to `cls` for classes), so it never sees files the loader found through the fallback. Sync then reports success while the namespace keeps stale code, contrary to the [sync statement](../../ontology/features/developer-tools.g5.md#devtools.sync). A repair would derive the sync directory from the directory Reload actually used, for example the developer-mode `^Sources` entry, or warn when a resource's sync directory does not exist. Neither repair was checked.

<a id="c-d38"></a>
### C:D38: `list-dependents -t` leaves out modules at the top of a dependency graph

Severity: Medium · Decision: Open

Run: Uplift 2026-11-16

Found by: native graph dump during iteration33 exploration (`iter33/attempts/explore2-tree-dump.log`), then source review of `Utils/Module.cls::InvertDependencies` and `GetDependentsAsTree`, and [iteration33 model](../../Verus_verification/consolidated/iter33/dependents.rs) `tree_omits_roots` with control `tree-includes-roots` ([proof receipt](iter33/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration33 native receipt](iter33/native/20261116T171635420146Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. On one installed graph the flat `list-dependents i33a` lists b, c, d and g, while `list-dependents -t i33a` prints only a, b and c, and `-t i33b` leaves out g. The passing neighbours are the flat report on the same graph and `-t` on a module with no dependents.

The tree is built by inverting each installed module's dependency graph. That graph records parent links for nested dependencies but none from the top module to its direct dependencies, so the inversion never reaches a graph's top module. A user checking dependents before removing a module sees an incomplete tree, contrary to the [list-dependents statement](../../ontology/features/inspection.g5.md#inspect.list-dependents). A repair would seed the inversion with each graph's top module as the parent of its direct dependencies. It was not checked.

<a id="c-d41"></a>
### C:D41: The installer class does not receive `<Default>` values

Severity: Medium · Decision: Open

Run: Uplift 2026-11-16

Found by: source review of `Lifecycle/Base.cls::%Configure`, whose `GetCustomParameters` result is unused, and [iteration36 model](../../Verus_verification/consolidated/iter36/installer.rs) `default_versus_parameter` with control `default-reaches-installer-vars` ([proof receipt](iter36/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration36 native receipt](iter36/native/20261116T173156335552Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. Module m36a declares `<Default Name="Greeting" Value="hello"/>` and `<Parameter Name="Mode">param</Parameter>`. On load, `OnConfigureComponent` receives Mode=param but no Greeting (case l1), while the module's After-Activate Invoke receives `${Greeting}`=hello. Passing neighbour: with `-DGreeting=bye`, both receive bye (l3).

`%Configure` calls `GetCustomParameters` into `customParams`, then builds the installer variables only from the phase parameters, which hold `<Parameter>` and `-D` values. This contradicts [module defaults](../../ontology/concepts/modules.g5.md#module.parameter), which says the module receives both kinds of default, and the [installer parameters statement](../../ontology/concepts/lifecycle.g5.md#lifecycle.installer.parameters). A repair would merge `customParams` into the variables before `OnConfigureComponent`, keeping `-D` precedence. It was not checked.

<a id="c-d44"></a>
### C:D44: A refused module document save empties the stored document text

Severity: Medium · Decision: Open

Run: Uplift 2026-11-16

Found by: iteration39 native exploration (`iter39/attempts/explore2.log`), then source review of `StudioDocument/Module.cls::ImportStream`, and [iteration39 model](../../Verus_verification/consolidated/iter39/document.rs) `refused_save_erases_text` with control `stream-clear-rolled-back` ([proof receipt](iter39/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration39 native receipt](iter39/native/20261116T174319930857Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. After a valid edit to `m39.ZPM` (version 1.1.0, e1), saving malformed XML through `%RoutineMgr` is refused with an XSLT parse error (e2). The module record still reads 1.1.0 with its resource, but the stored `m39.ZPM` text is now empty. Passing neighbour: the valid save in e1 stores both the text and the record.

`ImportStream` clears the stored stream and writes the new text before its transaction starts. When the record update fails, the rollback discards the written text but not the earlier clear. An editor that reopens the document then sees an empty manifest, and saving that empty document resets the version (see the [document statement](../../ontology/concepts/modules.g5.md#module.identity.document)). A repair would clear and write the stream inside the transaction. It was not checked.

<a id="c-d46"></a>
### C:D46: A preload class that fails to compile does not fail the load

Severity: Medium · Decision: Open

Run: Uplift 2026-11-16

Found by: source review of `Lifecycle/Base.cls::%Initialize`, which keeps the import status in a local and returns its own OK status, then [iteration42 model](../../Verus_verification/consolidated/iter42/initialize.rs) `preload_error_swallowed` with control `preload-error-returned` ([proof receipt](iter42/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration42 native receipt](iter42/native/20261116T174706181264Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. `load m42b`, whose `preload/cls/M42B/Broken.cls` uses an undefined property type, reports Initialize SUCCESS and succeeds; the module is installed and `M42B.Broken` is not compiled (case b1). Passing neighbour: the same class error in an ordinary `src` resource fails `load m42c` and installs nothing (c1).

Preload code usually bootstraps what later phases need, so a broken preload passes silently and fails later or not at all, contrary to the [Initialize statement](../../ontology/concepts/lifecycle.g5.md#lifecycle.initialize) and to a load failing on compile errors. A repair would return the ImportDir/compile status from `%Initialize`. It was not checked.

<a id="c-d49"></a>
### C:D49: `arrange` fails in a namespace without a source-control class

Severity: Medium · Decision: Open

Run: Uplift 2026-11-16

Found by: iteration44 native exploration, then source review of `Utils/Module.cls::ExportDocumentForObject`, which throws the `%Studio.SourceControl.Interface` status, and [iteration44 model](../../Verus_verification/consolidated/iter44/comments.rs) `without_source_control_nothing_changes` with control `no-source-control-ok` ([proof receipt](iter44/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration44 native receipt](iter44/native/20261116T175351554424Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. In a fresh namespace, `arrange c44` and `arrange n44` (a module without comments) fail with error #5889 "Not logged into source control system" and leave the stored manifests unchanged (cases a1, n1). Passing neighbour: after configuring `%Studio.SourceControl.Base`, both succeed (s1, s3).

The [authoring statement](../../ontology/features/developer-tools.g5.md#devtools.authoring) offers `arrange` as an ordinary authoring command, and no product test covers it. A repair would treat an absent source-control class as "not under source control, editable". It was not checked.

<a id="c-d2"></a>
### C:D2: When IPM cannot create a load's temporary `.modules` repository, it prints "Failed to clean up repository 'ipm-temp-modules-N'. You may need to manually dele...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d2)

Reproduced by: e2e reproduced. TR7 missing-row cleanup warning; TR1 succeeds.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/features/load.g5.md#load.dependencies.local-repo). No product repair or maintainer approval is claimed.

<a id="c-d3"></a>
### C:D3: Parameters that refer to each other in a loop are never reported. The 20-level guard that throws "Circular reference or too many levels in placeholders" is u...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d3)

Reproduced by: e2e reproduced. cyclic Default reaches ordinary load error; acyclic neighbor succeeds.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/modules.g5.md#module.parameter). No product repair or maintainer approval is claimed.

<a id="c-d5"></a>
### C:D5: A parameter name is placed into a regular expression unescaped, so a dotted name such as `a.b` also fills `{$axb}`. Dotted names are the documented way to ne...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d5)

Reproduced by: e2e reproduced. a.b Default changes {$axb} in the command output; exact ordinary value neighbor.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/modules.g5.md#module.parameter). No product repair or maintainer approval is claimed.

<a id="c-d9"></a>
### C:D9: Build metadata containing a hyphen is parsed as a prerelease: `1.2.3+build-x` becomes `1.2.3-x+build-x`, which sorts before `1.2.3` and matches only itself a...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d9)

Reproduced by: e2e reproduced. ordinary module load stores the malformed parsed build version; ordinary 1.0.0 neighbor.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/versions.g5.md#version). No product repair or maintainer approval is claimed.

<a id="c-d10"></a>
### C:D10: The `&&` conflict error prints literal placeholders: "No overlapping versions in && operation for %1 and %2". `$$$ERROR($$$GeneralError, ...)` takes one argu...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d10)

Reproduced by: partial reproduced. real AND helper error text; full conflicting-dependency command message path remains.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/concepts/versions.g5.md#version.range). No product repair or maintainer approval is claimed.

<a id="c-d12"></a>
### C:D12: `history find` shows the result of an action that has not ended (still running or killed) as `ERROR #00: (no error description)`, so an operator reads it as ...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d12)

Reproduced by: e2e reproduced. history find displays ERROR #00 for an unfinished record; completed history neighbor.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031710701394Z-misc/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `misc`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/features/history.g5.md#history.record). No product repair or maintainer approval is claimed.

<a id="c-d13"></a>
### C:D13: `module-version <module>` with no version, or with the current version, fails with "Cannot set version to X as it is not strictly higher than the current ver...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d13)

Reproduced by: e2e reproduced. module-version unchanged refusal; patch bump succeeds.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031715006352Z-early/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `early`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/features/developer-tools.g5.md#devtools.authoring.module-version). No product repair or maintainer approval is claimed.

<a id="c-d21"></a>
### C:D21: The test path converter strips the resource folder as raw leading text. A path outside the folder keeps its folders in the class name, although the method co...

Severity: Low · Decision: Open

Found by: [Starting entry and evidence](findings-audit-iter28/BUG-REVIEW.md#c-d21)

Reproduced by: partial reproduced. real private converter exposed by subclass and valid inside-path neighbor; sync routing reachability remains untraced.

[Executable regression receipt](findings-audit-iter28/runs/20261116T031653939391Z-params/receipt.json); [Runner](findings-audit-iter28/run_regressions.py) `params`. The receipt records commands, expected observations, actual readers and provider qualifications.

[Governing owner](../../ontology/features/developer-tools.g5.md#devtools.sync.test-paths). No product repair or maintainer approval is claimed.

<a id="c-d37"></a>
### C:D37: Whole-namespace sync ignores dependencies that pass through a module not in developer mode

Severity: Low · Decision: Open

Run: Uplift 2026-11-16

Found by: source review of `Utils/Module.cls::GetDevModeModulesInDependencyOrder`, then [iteration32 model](../../Verus_verification/consolidated/iter32/sync.rs) `order_neighbors` with controls `dev-edge-filter-removed` and `row-order-only` ([proof receipt](iter32/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration32 native receipt](iter32/native/20261116T171504430334Z/observations.json). Developer-mode `tx` depends on `tz`, which is not in developer mode and depends on developer-mode `ty`. With `tx` earlier in row order, `sync` with no module runs Sync for `tx` (position 3) before `ty` (position 5). The passing neighbour is a direct developer-mode dependency: `ox` depends on `oy`, and `oy` (4) runs before `ox` (6) although `ox` comes first in row order.

The helper keeps only dependency edges whose both ends are in developer mode and never follows edges through other modules. The [sync statement](../../ontology/features/developer-tools.g5.md#devtools.sync) promises dependency order so that each module recompiles against dependencies already synced in the run. Here `tx` recompiles before `ty` is synced. A repair would order by the transitive closure of dependencies restricted to the developer-mode set. It was not checked.

<a id="c-d42"></a>
### C:D42: `list-installed -s version` orders prereleases of one version as text

Severity: Low · Decision: Open

Run: Uplift 2026-11-16

Found by: source review of `Main.cls::SortList`, then [iteration35 model](../../Verus_verification/consolidated/iter35/listing.rs) `sort_neighbors` with control `version-prerelease-semantic` ([proof receipt](iter35/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration35 native receipt](iter35/native/20261116T173038793473Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. `list-installed -s v` lists i35pb `1.0.0-rc.10` at position 2 and i35pa `1.0.0-rc.2` at position 3. The passing neighbour is i35y `1.9.0` listed before i35x `1.10.0`, so numeric parts are compared as numbers.

The [version order](../../ontology/concepts/versions.g5.md#version.order) compares numeric prerelease identifiers as numbers, so rc.2 comes before rc.10, and the [listing statement](../../ontology/features/inspection.g5.md#inspect.list-installed) promises a version sort. The same text comparison drives the install choice in C:D39, but through different code. A repair would compare prerelease identifiers in semantic-version order. It was not checked.

<a id="c-d43"></a>
### C:D43: History shows a module whose real version is 0.0.0-0 as `<unset>`

Severity: Low · Decision: Open

Run: Uplift 2026-11-16

Found by: source review of `General/History.cls::VersionToString`, then [iteration38 model](../../Verus_verification/consolidated/iter38/render.rs) `known_placeholder_indistinguishable` with control `placeholder-only-when-unknown` ([proof receipt](iter38/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration38 native receipt](iter38/native/20261116T173827930786Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. `load m38z`, whose manifest declares version 0.0.0-0, succeeds with a known package, yet `history find` and `history details` show `<unset>` for that record (record z). Passing neighbours: m38a 1.2.3 shows `v1.2.3`, and the record of an invalid manifest (record b) shows `<unset>` as intended.

The renderer compares the stored version text with the placeholder, so a real `0.0.0-0` cannot be told apart from a version that was never known, contrary to the [Version field statement](../../ontology/features/history.g5.md#history.fields). A repair would show the placeholder only when the version was never set, for example with an explicit flag. It was not checked.

<a id="c-d45"></a>
### C:D45: A module document that names a different module is saved under the old name

Severity: Low · Decision: Open

Run: Uplift 2026-11-16

Found by: iteration39 native exploration (`iter39/attempts/explore1.log`, `explore3.log`), then source review of `StudioDocument/Module.cls::UpdatePersistentFromStream`, and [iteration39 model](../../Verus_verification/consolidated/iter39/document.rs) `other_name_accepted_under_old_name` with control `name-guard-effective` ([proof receipt](iter39/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration39 native receipt](iter39/native/20261116T174319930857Z/observations.json). Saving `m39.ZPM` with `<Name>m39other</Name>` and version 1.2.0 succeeds (e3). The record stays named m39 and takes version 1.2.0, no `m39other` record exists, and the stored document text says m39other. Passing neighbour: a save that keeps the name (e1) updates the record and the text consistently.

`UpdatePersistentFromStream` refuses a name that differs from the document name, but importing onto the existing record does not replace its Name, so the comparison always passes. The document and the record then disagree, contrary to the [document statement](../../ontology/concepts/modules.g5.md#module.identity.document). A repair would compare the parsed manifest name before importing it onto the record. It was not checked.

<a id="c-d47"></a>
### C:D47: Module actions never refresh a developer-mode module's preload code

Severity: Low · Decision: Open

Run: Uplift 2026-11-16

Found by: source review of `Lifecycle/Base.cls::%Initialize` (preload root from the RootDirectory parameter only) against `%Reload` (developer-mode root fallback), then [iteration42 model](../../Verus_verification/consolidated/iter42/initialize.rs) `module_action_skips_preload` with control `module-action-uses-module-root` ([proof receipt](iter42/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration42 native receipt](iter42/native/20261116T174706181264Z/observations.json). After editing `preload/cls/M42P/Boot.cls` from 1 to 2, `m42 initialize -only` and `m42 compile` both print "Skipping preload - directory does not exist" and Boot stays 1 (cases i1, i2). Passing neighbours: `load <dir> -dev` (i3) and `m42 initialize -only -DRootDirectory=<dir>` (i4) refresh it.

Reload falls back to the stored root for developer-mode modules, but Initialize does not, so the chain run by `compile` reloads `src` but not `preload`. The [Initialize statement](../../ontology/concepts/lifecycle.g5.md#lifecycle.initialize) and CHANGELOG `#702` place preload in the complete chain. A repair would use the same root fallback as Reload. It was not checked.

<a id="c-d48"></a>
### C:D48: The test phase keeps test classes loaded in a module that is not in developer mode

Severity: Low · Decision: Open

Run: Uplift 2026-11-16

Found by: source review of `ResourceProcessor/Test.cls::OnPhase`, whose existence check runs after `LoadTestDirectory` has loaded the classes, and [iteration40 model](../../Verus_verification/consolidated/iter40/unittests.rs) `test_leaves_test_code` with control `delete-checked-before-load` ([proof receipt](iter40/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration40 native receipt](iter40/native/20261116T174722753053Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. After `load t40` (not developer mode) and `test t40 -only`, `T40.Tests.Basic` and the never-run verify-only `T40.VTests.Basic` are both defined, and the former is compiled (case n1). Passing neighbours: the plain load has no test classes (n0), and `uninstall` removes them (n2).

The processor means to delete loaded test code outside developer mode, but the check that should protect pre-existing classes always sees the classes it just loaded, so `/nodelete` is always passed. This contradicts the [test statement](../../ontology/features/build-and-publish.g5.md#build.test) that test-scoped resources load for this phase only. A repair would take the existence check before loading the test directory. It was not checked.

<a id="c-d50"></a>
### C:D50: `arrange` drops comments placed before resources and a module's last comment

Severity: Low · Decision: Open

Run: Uplift 2026-11-16

Found by: iteration44 exploration, then source review of `Utils/XMLCommentHandler.cls` (`GetPosition`, `endElement`, `Reapply`), and [iteration44 model](../../Verus_verification/consolidated/iter44/comments.rs) `resource_comments_dropped` and `trailing_comment_dropped` with controls `resource-path-unchanged` and `trailing-attached-to-parent` ([proof receipt](iter44/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration44 native receipt](iter44/native/20261116T175351554424Z/observations.json). With a source-control class configured, `arrange c44` keeps the comments before `Name` and inside a dependency, but drops the comments before each resource and the module's last comment (case s1). A second `arrange` gives the same result. Passing neighbours: comments C1 to C3 are kept.

The export wraps resources in a new `<Resources>` element, so the recorded resource comment paths no longer match, and a comment with no following sibling is discarded. This contradicts the handler's purpose of carrying comments across the rewrite. A repair would map resource comments by resource name onto the exported path and keep trailing comments as the parent's last child. It was not checked.

<a id="c-d51"></a>
### C:D51: A source-control extension is never told that a module's root changed

Severity: Low · Decision: Open

Run: Uplift 2026-11-16

Found by: iteration43 native exploration (explore4) refuting the source prediction, then source review of `Lifecycle/Base.cls::%Reload` against `Storage/Module.cls::ExecutePhases` (Root assigned before the comparison), and [iteration43 model](../../Verus_verification/consolidated/iter43/schooks.rs) `root_hook_not_reached` with control `root-hook-fires` ([proof receipt](iter43/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration43 native receipt](iter43/native/20261116T175445272137Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. With the IPM-aware test extension as the composite's primary, loading m43 from `/tmp/iter43/m43copy` after `/tmp/iter43/m43` changes the stored root but records no OnChangeModuleRoot call (p4). Reloading with `-DRootDirectory` back to the original directory also changes the root without the call (p4b). Passing neighbours: the before/after-phase and resource-change calls in the same runs (p1, p4, p4b).

The interface comment says the call is "invoked when an existing module is reloaded from a different directory than its known module root". `%Reload` compares against a root that was already updated, so extensions that track module locations never learn of the move, contrary to the [source-control statement](../../ontology/concepts/lifecycle.g5.md#lifecycle.source-control). A repair would compare against the stored root before it is overwritten. It was not checked.

<a id="c-d52"></a>
### C:D52: `show` omits a `Health="false"` system requirement

Severity: Low · Decision: Open

Run: Uplift 2026-11-16

Found by: source review of `General/ModuleSummary.cls::GetSystemRequirements` (Health listed only when true) against `Storage/SystemRequirements.cls::CheckHealth` (Health=0 refuses Health instances), and [iteration46 model](../../Verus_verification/consolidated/iter46/summary.rs) `health_false_hidden` with control `health-false-shown` ([proof receipt](iter46/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration46 native receipt](iter46/native/20261116T175856608516Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. `show s46d`, whose only stored requirement is `Health="false"`, prints no System requirements section (case d). `show s46a` lists IRIS and Interoperability but no Health entry (case a). Passing neighbours: s46h, which has no requirements, also shows none, and the other set requirements are listed.

A reader of `show` cannot tell that the module refuses IRIS for Health, contrary to the [inspection statement](../../ontology/features/inspection.g5.md#inspect) that `show` shows the module's details. A repair would list "not supported on IRIS for Health" when Health is explicitly false. It was not checked.

<a id="c-d53"></a>
### C:D53: YAML and TOON test reports change assertion message text

Severity: Low · Decision: Open

Run: Uplift 2026-11-16

Found by: iteration47 exploration with byte-exact file reads, then source review of `Test/YamlOutput.cls::BuildYamlRows` (block scalars that re-indent only LF) and `Test/ToonOutput.cls::BuildToonRows`, and [iteration47 model](../../Verus_verification/consolidated/iter47/formats.rs) `fixture_rendering` with controls `toon-keeps-cr`, `yaml-no-trailing-newline` and `yaml-keeps-leading-spaces` ([proof receipt](iter47/proofs/models.json)).

Reproduced by: E2E reproduced. [Iteration47 native receipt](iter47/native/20261116T180053540899Z/observations.json) on IRIS 2026.1 x86-64 at `571e04a1`. For an assertion message `"one"_$c(13,10)_"two"`, the JSON report file keeps `one\r\ntwo` while the YAML and TOON files read back `one\ntwo`. `"  leading spaces"` reads back from YAML as `leading spaces\n`, and every YAML description gains a trailing newline. Passing neighbours: JSON is exact for every message, TOON is exact apart from CR, and all formats agree on counts.

A CI tool reading the YAML or TOON report gets assertion text that differs from the JSON report for the same run, although the [test output statement](../../ontology/features/build-and-publish.g5.md#build.test-output) offers these formats as alternative renderings of the same full results. A repair would write YAML descriptions as escaped double-quoted scalars and escape CR in TOON quoted fields. It was not checked.

## Outside active reproduction scope

<a id="c-d17"></a>
### C:D17 review record

Severity: Unranked

C:D17: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](findings-audit-iter28/BUG-REVIEW.md#c-d17)

<a id="c-d18"></a>
### C:D18 review record

Severity: Unranked

C:D18: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](findings-audit-iter28/BUG-REVIEW.md#c-d18)

<a id="c-d20"></a>
### C:D20 review record

Severity: Unranked

C:D20: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](findings-audit-iter28/BUG-REVIEW.md#c-d20)

<a id="c-d22"></a>
### C:D22 review record

Severity: Unranked

C:D22: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](findings-audit-iter28/BUG-REVIEW.md#c-d22)

<a id="c-d23"></a>
### C:D23 review record

Severity: Unranked

C:D23: defensive source/existing-evidence review only; outside active reproduction scope. [Preserved claim and scope](findings-audit-iter28/BUG-REVIEW.md#c-d23)

## IDs forwarded to their owning record or review

<a id="c-d25"></a>
### C:D25 review record

Severity: Unranked

C:D25: alias. [Disposition, source and remaining gate](findings-audit-iter28/BUG-REVIEW.md#c-d25)

<a id="c-d26"></a>
### C:D26 review record

Severity: Unranked

C:D26: inconclusive. [Disposition, source and remaining gate](findings-audit-iter28/BUG-REVIEW.md#c-d26)

<a id="c-d30"></a>
### C:D30 review record

Severity: Unranked

C:D30: inconclusive. [Disposition, source and remaining gate](findings-audit-iter28/BUG-REVIEW.md#c-d30)
