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
