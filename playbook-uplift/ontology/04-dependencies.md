# 04 — Dependencies

Module root: DEP-0

Scope: dependency resolution (`%IPM.Storage.Module:BuildDependencyGraph`), phase-scoped requirements, checks against
installed modules, coordinated (diamond) updates, lock files, `list-dependents` and `dependency-analyzer`. The commands
that trigger resolution (`install`, `update`) are in `03-install-update.md`. Baseline: ipm 571e04a1. Paths in Source are
relative to the IPM checkout (`ipm/`). An `X` tag means a test asserts it, not that it holds here. `O:nc-dep-*` and
`O:nc-inst-*` tags come from the native checks in `intents/03-04-seed-challenge.md` ("Native check results"), and `T`
tags from IPM tests run here (`validation/logs/64-inst-ipm-tests.log`), all on IRIS 2026.1.

## Glossary
| Term | Defined by |
|---|---|
| requirement | DEP-D1 |
| version range | DEP-D2 |
| exact requirement | DEP-D3 |
| dependency graph | DEP-D4 |
| scope | DEP-D5 |
| clash | DEP-D6 |
| dependent | DEP-D7 |
| lock file | DEP-D8 |

## Statements
| ID | Parent | Kind | Statement | Evidence | Source | Notes |
|---|---|---|---|---|---|---|
| DEP-0 | SYS-0 | R | This part covers how IPM finds and checks the modules that a module needs. | I | ipm/src/cls/IPM/Storage/Module.cls#L898 | |
| DEP-D1 | DEP-0 | D | A **requirement** is one entry in a manifest. It names a needed module and the versions it may have. | I | ipm/src/cls/IPM/Storage/ModuleReference.cls#L10 | `<ModuleReference>` with `Name`, `Version`, optional `Scope`. Names are stored in lower case (`NameSet`). |
| DEP-D2 | DEP-0 | D | A **version range** is a rule for which versions are allowed, like ^1.2.0 or >=2.0. | I | ipm/src/cls/IPM/General/SemanticVersionExpression.cls | Ranges are joined with AND when several requirements name one module (`And`, L120 ff). |
| DEP-D3 | DEP-0 | D | An **exact requirement** allows just one version, like 1.2.0. | I | ipm/src/cls/IPM/Storage/Module.cls#L968 | `SemanticVersion.IsValid(dep.VersionString)` decides exact versus fuzzy. |
| DEP-D4 | DEP-0 | D | The **dependency graph** is the set of all modules a module needs. It also holds what those modules need, and so on. | I | ipm/src/cls/IPM/Storage/Module.cls#L857 | Built breadth first; each node keeps depth, registry, version, scope and which modules asked for it. |
| DEP-D5 | DEP-0 | D | A requirement can have a **scope**. Then it counts only for one phase, test or verify. | I | ipm/src/cls/IPM/Storage/ModuleReference.cls#L17 | `Scope` VALUELIST ",test,verify". |
| DEP-D6 | DEP-0 | D | A **clash** is when the requirements on one module allow no version in common. | I | ipm/src/cls/IPM/General/SemanticVersionExpression.cls | "No overlapping versions in AND operation ... for module <m> with constraints: ..." |
| DEP-D7 | DEP-0 | D | A **dependent** of a module is a module that needs it, directly or through others. | I | ipm/src/cls/IPM/Main.cls#L577 | |
| DEP-D8 | DEP-0 | D | A **lock file** records the exact versions and registries that one install used. | I | ipm/src/cls/IPM/General/LockFile.cls#L1 | `%IPM.General.LockFile`; issue #549 (npm-style lock file). |
| DEP-B1 | DEP-0 | B | Before it loads any needed module, IPM builds the dependency graph. If that fails, it loads none of them. | I; O:nc-dep-11-greedy-resolution; O:nc-dep-19-blocked-update | ipm/src/cls/IPM/Utils/Module.cls#L1379; ipm/src/cls/IPM/Utils/Module.cls#L1381 | The named module's own manifest is already loaded by then; it is rolled back unless developer mode committed it (see INST-B14). |
| DEP-B2 | DEP-B1 | B | A requirement that gives no version allows any version. | X:tests/integration_tests/Test/PM/Integration/DepNoVersion.cls#L21; T:Test.PM.Integration.DepNoVersion | ipm/src/cls/IPM/Storage/ModuleReference.cls#L22 | Stored as "*". Changelog 0.10.7 #408. |
| DEP-B3 | DEP-B1 | B | For a version range, IPM picks the newest version that fits. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L414; T:Test.PM.Integration.BuildDependencyGraph | ipm/src/cls/IPM/Storage/Module.cls#L1218 | Repository results come newest first. |
| DEP-B4 | DEP-B1 | B | For an exact requirement, IPM uses that version. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L430; T:Test.PM.Integration.BuildDependencyGraph | ipm/src/cls/IPM/Storage/Module.cls#L1189 | |
| DEP-B5 | DEP-B1 | B | If the installed version already fits, IPM keeps it. It fetches nothing for that module. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L56; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L57; T:Test.PM.Integration.BuildDependencyGraph | ipm/src/cls/IPM/Storage/Module.cls#L1146 | Except an installed +snapshot version when snapshot refresh is on (reinstall default). |
| DEP-B6 | DEP-B1 | B | A module needed on many paths is resolved once. All its requirements must agree. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L313; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L464; T:Test.PM.Integration.BuildDependencyGraph; P:dep_refines; M:dep-explore-soundness | ipm/src/cls/IPM/Storage/Module.cls#L1096 | Requirements from every path are joined with AND. Proof: dep_refines (a resolution that ends Done meets every active requirement of the root and of each chosen module, one version per module; premises PU, PR (models/verus/update_steps.TRUST.md)). Premises as declared in dep_resolve.rs. |
| DEP-B7 | DEP-B6 | B | If requirements clash, IPM stops. The error names the module and the requirements that clash. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L352; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L353; O:nc-dep-11-greedy-resolution; O:nc-dep-17-scoped-clash; T:Test.PM.Integration.BuildDependencyGraph | ipm/src/cls/IPM/Storage/Module.cls#L1106; ipm/CHANGELOG.md#L87 | 0.10.8 #1192 (clashes were silently dropped before) and #1077 (error names the sources); commit dfdda468. |
| DEP-B8 | DEP-B7 | B | A clash between a direct requirement and a deeper one is caught too. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L363; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L482; T:Test.PM.Integration.BuildDependencyGraph | ipm/src/cls/IPM/Storage/Module.cls#L1096 | |
| DEP-B9 | DEP-B7 | B | An exact requirement that falls outside a range is a clash. | X:tests/unit_tests/Test/PM/Unit/SemVer/Expressions.cls#L318 | ipm/src/cls/IPM/General/SemanticVersionExpression/Range.cls#L251 | Fixed in 0.10.8 #1192. A stale test note still calls this a "known Intersect edge case" (finding F-DEP-5). |
| DEP-B10 | DEP-B1 | B | If no registry has a version that fits, IPM stops. The error lists the requirements. | I; X:tests/integration_tests/Test/PM/Integration/OrasDeployedPlatformFilter.cls#L143 | ipm/src/cls/IPM/Storage/Module.cls#L1183 | "Could not find satisfactory version of %1 in any repositories. Required by: %2". |
| DEP-B11 | DEP-B1 | B | Needs that form a loop are an error. The error shows the loop. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L383; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L397; T:Test.PM.Integration.BuildDependencyGraph | ipm/src/cls/IPM/Storage/Module.cls#L963 | "Cyclic dependency detected in module '%1': a -> b -> a". |
| DEP-B12 | DEP-B1 | B | When a range fits many versions, IPM tries them newest first. It takes the first one whose own needs fit what it has chosen so far. | I; O:nc-dep-11-greedy-resolution; M:dep-conf-nc-dep-11-greedy-resolution | ipm/src/cls/IPM/Storage/Module.cls#L1219; ipm/src/cls/IPM/Storage/Module.cls#L1248 | The check looks only at the candidate's direct requirements against modules already in the graph. Model: fuzzy_pick in models/verus/dep_resolve.rs; both models replay nc-dep-11. |
| DEP-B13 | DEP-B12 | B | If some choice of versions meets every requirement, IPM finds one. | I; F:F-DEP-7; O!:nc-dep-11-greedy-resolution | ipm/src/cls/IPM/Storage/Module.cls#L867; ipm/src/cls/IPM/Storage/Module.cls#L1284 | Intent: the method comment says the graph is built "with backtracking" (#L867); Confirmed by issue #1299 (bug) and #1301; fix PR #1303 open, not merged at 571e04a1 (F-DEP-7). Code: no backtracking past one level; when no candidate passes the one-level check the last (oldest) one is kept and the clash surfaces one level down (#L1248-L1291). IRIS: `install nc-dep-root` fails with a clash on nc-dep-c (nc-dep-11-greedy-resolution) although a, b and c at 1.0.0 meet every requirement (nc-dep-11b-solvable-with-pin installs them once a is pinned). Witness of the defect (not a proof of this statement): dep_witness_greedy_rejects_solvable (a sound choice exists and the deterministic run fails; dep_deterministic). dep-explore-completeness fails on 2,040 of 70,656 instances; both models replay nc-dep-11 and nc-dep-11b (dep-conf-*). Intent: DEC-DEP-1 resolved (evidence), decisions/research/inst.md. |
| DEP-B14 | DEP-B1 | B | IPM loads the needed modules so that each one comes after the modules it needs. | I; O:nc-dep-13-lockfile-order; M:dep-conf-nc-dep-13-lockfile-order | ipm/src/cls/IPM/Utils/Module.cls#L1457 | Repeatedly takes a module with no remaining needs; ties go by name. Modules already installed are skipped (empty registry, L1407). IRIS: nc-dep-zz-base was loaded before nc-dep-aa-mid. Model: the fine model's install order (GetFlatDependencyListFromInvertedDependencyGraph) replays nc-dep-13. |
| DEP-B54 | DEP-B1 | B | IPM stops resolving only when needs form a loop or a requirement has no version that fits. | P:dep_refines; M:dep-explore-failure-justified | ipm/src/cls/IPM/Storage/Module.cls#L950-L953; ipm/src/cls/IPM/Storage/Module.cls#L1183; ipm/src/cls/IPM/Storage/Module.cls#L1284 | Reviewer finding 1, resolver side: L1 is two-way. Done needs a sound choice (DEP-B6); Failed needs a stated reason (ghost `why`): an active requirement of a module that closes a chain of recorded requirements back onto it (a loop), or an active requirement of a module in the graph with no offered or installed version allowed by it, by the requirements already recorded on its module, by installed modules and by the no-downgrade floor. Those recorded requirements include the resolver's own earlier choices, so this does not say that no solution exists (DEP-B13, DEC-DEP-1). Proof: dep_refines; premises PU, PR. Controls spurious-cycle, exact-requirement-fails and finished-reported-failed are killed in their step lemmas; dep-explore-failure-justified: every one of 10,880 failed explored runs names a valid reason. Premises as declared in dep_resolve.rs. |
| DEP-B15 | DEP-0 | B | A requirement with a scope is left out unless its phase runs. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L175; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L176; O:nc-dep-17-scoped-clash; T:Test.PM.Integration.BuildDependencyGraph; M:dep-conf-nc-dep-17-scoped-clash | ipm/src/cls/IPM/Storage/Module.cls#L834 | `HasScope`. Model: both models replay nc-dep-17. |
| DEP-B16 | DEP-B15 | B | Install leaves out every requirement that has a scope. | I; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L175; O:nc-dep-17-scoped-clash; T:Test.PM.Integration.BuildDependencyGraph; M:dep-conf-nc-dep-17-scoped-clash | ipm/src/cls/IPM/Utils/Module.cls#L1328 | Called with no phases. The parameter comment (Module.cls L892) says "Empty=all", which the code and test contradict (finding F-DEP-4). Model: load with no phase leaves the scoped requirement out (nc-dep-17 replayed). |
| DEP-B17 | DEP-B15 | B | When the test phase runs, IPM adds the test requirements. It loads the ones that are missing. | I; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L195; T:Test.PM.Integration.BuildDependencyGraph | ipm/src/cls/IPM/Lifecycle/Base.cls#L1261 | Changelog 0.10.6 #1072. The test checks the graph, not the load. |
| DEP-B18 | DEP-B15 | B | The verify phase does the same for its own requirements. | I; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L199; T:Test.PM.Integration.BuildDependencyGraph | ipm/src/cls/IPM/Lifecycle/Base.cls#L1557 | |
| DEP-B19 | DEP-B15 | B | A requirement with a scope can bring a clash. The clash shows only when its phase runs. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L216; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L240; O:nc-dep-17-scoped-clash; T:Test.PM.Integration.BuildDependencyGraph; M:dep-conf-nc-dep-17-scoped-clash | ipm/src/cls/IPM/Storage/Module.cls#L954 | Install succeeds; `test` later fails in resolution. Model: the clash appears only with the test phase (nc-dep-17 replayed). |
| DEP-B20 | DEP-B15 | B | When its phase runs, the needs of that module come with it. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L257; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L264; T:Test.PM.Integration.BuildDependencyGraph; P:dep_refines; M:dep-explore-soundness | ipm/src/cls/IPM/Storage/Module.cls#L957 | Nested scope filtering against the root's phases needs `pCheckNestedScoped` (asserted L282); lifecycle callers do not pass it. Proof: dep_refines (with the phase on, the active requirements of every chosen module, at any depth, are met; premises PU, PR (models/verus/update_steps.TRUST.md)). Premises as declared in dep_resolve.rs. |
| DEP-B21 | DEP-0 | B | IPM also obeys what installed modules require. | I; P:dep_refines; M:dep-explore-soundness | ipm/src/cls/IPM/Storage/Module.cls#L1122 | `VersionRequirements` query over `%IPM_Storage.ModuleItem_Dependencies`. Proof: dep_refines (installed_met: the requirements of every installed module that is not in the graph, not a known dependency and not co-updating are met; the excluded ones: DEP-B26). Premises PU, PR (models/verus/update_steps.TRUST.md D6, D7). Premises as declared in dep_resolve.rs. |
| DEP-B22 | DEP-B21 | B | IPM will not pick a version that another installed module does not allow. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L104; X:tests/integration_tests/Test/PM/Integration/Update.cls#L769; O:nc-dep-19-blocked-update; T:Test.PM.Integration.BuildDependencyGraph; T:Test.PM.Integration.Update#TestDependencyVersionsUpdated; P:dep_refines; M:dep-explore-soundness | ipm/src/cls/IPM/Storage/Module.cls#L1127 | Proof: dep_refines (installed_met). Premises PU, PR (models/verus/update_steps.TRUST.md D6, D7). Premises as declared in dep_resolve.rs. |
| DEP-B23 | DEP-B22 | B | This holds for the named module too. If its version does not fit, IPM stops and names who needs another version. | I; O:nc-dep-18-installed-requirements | ipm/src/cls/IPM/Utils/Module.cls#L116; ipm/src/cls/IPM/Utils/Module.cls#L124 | "Requested version (%1 %2) does not satisfy the requirements of other modules installed in the current namespace (%3)." No test asserts this message. IRIS printed the message word for word (nc-dep-18). |
| DEP-B24 | DEP-B21 | B | IPM will not move an installed needed module to an older version. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L130; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L76; O:nc-dep-12-permit-downgrade; T:Test.PM.Integration.BuildDependencyGraph; P:dep_refines; M:dep-explore-soundness; M:dep-conf-nc-dep-12-permit-downgrade | ipm/src/cls/IPM/Storage/Module.cls#L1157 | Adds ">=installed (currently installed, downgrade not permitted)" to the range. Proof: dep_refines (no_downgrade unless PermitDowngrade). Premises PU, PR (models/verus/update_steps.TRUST.md D6, D7). Premises as declared in dep_resolve.rs. |
| DEP-B25 | DEP-B24 | N | With the PermitDowngrade option, IPM may move a needed module to an older version. | F:F-DEP-1; O!:nc-dep-12-permit-downgrade | ipm/src/cls/IPM/Storage/Module.cls#L894; ipm/src/cls/IPM/Utils/Module.cls#L1214 | Intent from the parameter comment. The resolver drops the lower bound, but the load then refuses the downgrade unconditionally. Witness of the defect: dep_witness_permit_downgrade_refused (F-DEP-1). |
| DEP-B26 | DEP-B21 | B | Modules this module already needs do not add their old requirements. Their new manifests count instead. | I | ipm/src/cls/IPM/Storage/Module.cls#L1112; ipm/src/cls/IPM/Storage/Module.cls#L1300 | `GetKnownDependencies` walks the installed tree; those names are excluded from the installed-requirements query. |
| DEP-B27 | DEP-0 | B | One update can move several modules. Then IPM checks requirements against the new versions. | X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L155; X:tests/integration_tests/Test/PM/Integration/BuildDependencyGraph.cls#L157; O:nc-dep-19c-coordinated-update; T:Test.PM.Integration.BuildDependencyGraph | ipm/src/cls/IPM/Utils/Module.cls#L1402; ipm/src/cls/IPM/Utils/Module.cls#L384 | Changelog 0.10.7 #1119 and #430; `CoUpdatingModules` added by commit 2aba9825 (PR #1136). |
| DEP-B28 | DEP-B27 | B | So two modules that need the same base can move up together with it. | X:tests/integration_tests/Test/PM/Integration/Update.cls#L657; X:tests/integration_tests/Test/PM/Integration/Update.cls#L663; T:Test.PM.Integration.Update#TestSharedTransitiveDependencyUpdate | ipm/src/cls/IPM/Storage/Module.cls#L1112 | Simple diamond, lock-step exact requirements. Before the fix: "Exact version matches do not agree". |
| DEP-B29 | DEP-B27 | B | This also works when the shared base sits at two depths. | X:tests/integration_tests/Test/PM/Integration/Update.cls#L692; X:tests/integration_tests/Test/PM/Integration/Update.cls#L698; T:Test.PM.Integration.Update#TestOffsetDiamondUpdate | ipm/src/cls/IPM/Storage/Module.cls#L1112 | Offset diamond. Both diamond tests update from a folder with `-path`, not from a registry. |
| DEP-B30 | DEP-B27 | B | An installed module outside the update still blocks it if it needs an old version. | X:tests/integration_tests/Test/PM/Integration/Update.cls#L769; O:nc-dep-19-blocked-update; T:Test.PM.Integration.Update#TestDependencyVersionsUpdated | ipm/src/cls/IPM/Storage/Module.cls#L1127 | module-d requires module-c ^5.3.0; the update needs 6.2.0. |
| DEP-B31 | DEP-B27 | B | If such an update fails, the modules it moved go back. This is not so in developer mode. | I; O:nc-dep-19d-rollback-after-dep-moved; O:nc-dep-19e-dev-path-update-dep-kept | ipm/src/cls/IPM/Utils/Module.cls#L1195; ipm/src/cls/IPM/Utils/Module.cls#L1338 | Nested loads share the outer transaction. Not asserted: Update.cls L769 checks only the error status. IRIS: a dependency moved, then the parent failed: it went back (19d); with update -dev -path it stayed (19e). From a filesystem registry update -dev rolls back too (F-INST-5). |
| DEP-B32 | DEP-0 | B | With -create-lockfile, install writes a lock file for the module it was asked for. | X:tests/integration_tests/Test/PM/Integration/LockFile.cls#L348; X:tests/integration_tests/Test/PM/Integration/LockFile.cls#L354; O:nc-dep-13-lockfile-order; T:Test.PM.Integration.LockFile#Test01Module0Dependencies; T:Test.PM.Integration.LockFile#Test10ComplexNestedDependencies | ipm/src/cls/IPM/Utils/Module.cls#L1416 | Issue #961, PR #963 (0.10.5). Compared as whole JSON with expected files. |
| DEP-B33 | DEP-B32 | B | The file is named module-lock.json. It sits in the root folder of the module. | I; O:nc-dep-13-lockfile-order | ipm/src/cls/IPM/General/LockFile.cls#L7; ipm/src/cls/IPM/General/LockFile.cls#L82 | `module.Root`: for a registry install, `<ModuleRoot>/<name>/<version>/`. |
| DEP-B34 | DEP-B32 | B | It gives the module name, version and registry. For each needed module, it gives the version, registry and its own requirements. | X:tests/integration_tests/Test/PM/Integration/LockFile.cls#L354; T:Test.PM.Integration.LockFile#Test01Module0Dependencies; T:Test.PM.Integration.LockFile#Test10ComplexNestedDependencies | ipm/src/cls/IPM/General/LockFile.cls#L10; ipm/src/cls/IPM/General/LockFile/Dependency.cls | Expected files under `_data/lock-test/expected-files/`. |
| DEP-B35 | DEP-B32 | B | It says how to reach each registry used. It never holds passwords or tokens. | I | ipm/src/cls/IPM/Repo/Remote/Definition.cls#L141; ipm/src/cls/IPM/Repo/Oras/Definition.cls#L162; ipm/src/cls/IPM/Repo/Filesystem/Definition.cls#L361 | `LockFileMapping` exports type, sort order, read-only, URL or root, ORAS namespace only. |
| DEP-B36 | DEP-B32 | B | Only the named module gets a lock file. Needed modules do not. | I | ipm/src/cls/IPM/Utils/Module.cls#L1325 | `LockFileModule` is set once at the top. |
| DEP-B37 | DEP-B32 | B | Update with -create-lockfile writes the lock file too. | I | ipm/src/cls/IPM/Main.cls#L185 | Same code path. No test. |
| DEP-B38 | DEP-B32 | B | If IPM cannot write the lock file, it prints an error. The install still goes on. | I; O:nc-dep-14-lockfile-folder-dependency | ipm/src/cls/IPM/Utils/Module.cls#L1417; ipm/src/cls/IPM/Utils/Module.cls#L1420 | "Error creating lock file for %1 - %2". |
| DEP-B39 | DEP-B32 | B | The lock file lists each needed module after the modules it needs. | F:F-DEP-2; O!:nc-dep-13-lockfile-order | ipm/src/cls/IPM/General/LockFile.cls#L24; ipm/tests/integration_tests/Test/PM/Integration/LockFile.cls#L360 | Intent: property comment and test comment (for a future `ci`). Code stores them in an array keyed by name, so JSON order is by name. Witness of the defect: dep_witness_lockfile_name_order (F-DEP-2). |
| DEP-B40 | DEP-B32 | B | IPM writes the lock file only after the whole install works. A failed install leaves no new lock file. | I; F:F-DEP-8; O!:nc-dep-15-lockfile-failed-install | ipm/src/cls/IPM/Utils/Module.cls#L1328; ipm/src/cls/IPM/Utils/Module.cls#L1415-L1421; ipm/src/cls/IPM/Utils/Module.cls#L1346 | Code writes it in LoadDependencies (#L1415-L1421, called at #L1328), before the module's own Activate (#L1346) and outside the transaction, so it stays when Activate fails (F-DEP-8). Intent: PR #963 review, 'I put the CreateLockFile check at the end of Install() so if it fails then will do so prior to creating the lock file'; commit 49fc412e3 later moved the call. Intent: DEC-DEP-2 resolved (evidence), decisions/research/inst.md. |
| DEP-B41 | DEP-B32 | B | Say a needed module came from a folder, with no registry. The lock file still lists it, with no registry named. | I; F:F-DEP-9; O!:nc-dep-14-lockfile-folder-dependency | ipm/src/cls/IPM/General/LockFile.cls#L75; ipm/src/cls/IPM/General/LockFile.cls#L115 | Code: an empty registry is skipped for the named module only (LockFile.cls#L74-L78); for a needed module `GetRepo("")` throws (#L72, #L115), which DEP-B38 turns into a message, and no file is written (F-DEP-9). Intent: the base-module rule in the same method; `load -create-lockfile` is supported (Main.cls#L314). Intent: DEC-DEP-3 resolved (assumption), decisions/research/inst.md. |
| DEP-B42 | DEP-B32 | B | If the dependency graph cannot be built, no lock file is written. | I | ipm/src/cls/IPM/Utils/Module.cls#L1381; ipm/src/cls/IPM/Utils/Module.cls#L1416 | |
| DEP-B43 | DEP-0 | B | An operator can install a module and its needs from a lock file. | N | ipm/src/cls/IPM/General/LockFile.cls#L2; ipm/tests/integration_tests/Test/PM/Integration/LockFile.cls#L316 | Issue #962 is open. No `ci` command in Main.cls. Test08 has an empty body with a TODO, so it is not evidence. |
| DEP-B44 | DEP-0 | B | The list-dependents command shows the installed modules that need a given module. This counts needs through other modules too. | I; O:nc-dep-16-list-dependents-case | ipm/src/cls/IPM/Main.cls#L3150; ipm/src/cls/IPM/Utils/Module.cls#L460 | Builds a full graph for every installed module. No test covers this command. |
| DEP-B45 | DEP-B44 | B | If a version is given, it shows only dependents that use that exact version. | I | ipm/src/cls/IPM/Utils/Module.cls#L484 | String compare with the resolved version, not a range. |
| DEP-B46 | DEP-B44 | B | With -repos, it looks at the modules in those registries instead. | I | ipm/src/cls/IPM/Main.cls#L599; ipm/src/cls/IPM/Utils/Module.cls#L632 | Disabled registries are skipped; installed modules are ignored while resolving. |
| DEP-B47 | DEP-B44 | B | With -tree, it shows the result as a tree. | I | ipm/src/cls/IPM/Main.cls#L598; ipm/src/cls/IPM/Utils/Module.cls#L506 | "TODO: Handle cyclic dependencies?" (L539). |
| DEP-B48 | DEP-B44 | B | If a graph fails for one module, it warns and goes on. | I | ipm/src/cls/IPM/Utils/Module.cls#L619; ipm/src/cls/IPM/Main.cls#L3168 | "WARNING: some errors occurred." |
| DEP-B49 | DEP-B44 | B | The module name may be typed in capital or small letters. | F:F-DEP-3; O!:nc-dep-16-list-dependents-case | ipm/src/cls/IPM/Main.cls#L584; ipm/src/cls/IPM/Utils/Module.cls#L482 | Help example uses `HS.JSON`; graph keys are lower case and the lookup is case-sensitive. Witness of the defect: dep_witness_list_dependents_case (F-DEP-3). |
| DEP-B50 | DEP-0 | B | The dependency-analyzer command looks at code. It finds uses of classes that are not in the module or what it needs. | I | ipm/src/cls/IPM/Main.cls#L651 | Writes findings to the current device. |
| DEP-B51 | DEP-B50 | B | It works only when you add the analyzer package first. | I | ipm/src/cls/IPM/Main.cls#L2862 | Needs `%pkg.isc.dependencyAnalyzer.Utils`. |
| DEP-B52 | DEP-B50 | B | With -direct-deps-only, it uses only what the module needs itself. Needs of needs do not count. | I | ipm/src/cls/IPM/Main.cls#L669 | |
| DEP-B53 | DEP-B50 | B | It reads only class and include files. | I | ipm/src/cls/IPM/Main.cls#L660 | |

## States
| ID | State |
|---|---|
| S1 | The module has no requirements. |
| S2 | Every requirement can be met from the registries. No installed module touches them. |
| S3 | A needed module is already installed at a version that fits. |
| S4 | A needed module is installed at a version newer than a requirement allows. |
| S5 | An installed module outside the command needs a version that clashes with a requirement. |
| S6 | Two requirements in the tree clash, maybe only for a requirement with a scope. |
| S7 | The requirements form a loop. |

## Commands
| ID | Command |
|---|---|
| K1 | `install <module>` (resolution at install or load) |
| K2 | `update <module>` that moves several modules |
| K3 | `<module> test` (runs the test phase) |
| K4 | `install -create-lockfile <module>` |
| K5 | `list-dependents <needed module>` |
| K6 | install from a lock file (`ci`) |

## Cells
| Cell | State | Command | Outcome | Decision |
|---|---|---|---|---|
| DEP-C1 | S1 | K1 | DEP-B1 | |
| DEP-C2 | S1 | K2 | DEP-B27, INST-B44 | |
| DEP-C3 | S1 | K3 | DEP-B17 | |
| DEP-C4 | S1 | K4 | DEP-B32, DEP-B34 | |
| DEP-C5 | S1 | K5 | DEP-B44 | |
| DEP-C6 | S1 | K6 | DEP-B43 | |
| DEP-C7 | S2 | K1 | DEP-B1, DEP-B3, DEP-B4, DEP-B12, DEP-B14, DEP-B13 |  |
| DEP-C8 | S2 | K2 | DEP-B27, DEP-B28, DEP-B29, DEP-B13 |  |
| DEP-C9 | S2 | K3 | DEP-B17, DEP-B20 | |
| DEP-C10 | S2 | K4 | DEP-B32, DEP-B34, DEP-B35, DEP-B39, DEP-B40 |  |
| DEP-C11 | S2 | K5 | DEP-B44, DEP-B45, DEP-B49 | |
| DEP-C12 | S2 | K6 | DEP-B43 | |
| DEP-C13 | S3 | K1 | DEP-B5 | |
| DEP-C14 | S3 | K2 | DEP-B26, DEP-B27 | |
| DEP-C15 | S3 | K3 | DEP-B5, DEP-B17 | |
| DEP-C16 | S3 | K4 | DEP-B34, DEP-B41 |  |
| DEP-C17 | S3 | K5 | DEP-B44 | |
| DEP-C18 | S3 | K6 | DEP-B43 | |
| DEP-C19 | S4 | K1 | DEP-B24, DEP-B10 | |
| DEP-C20 | S4 | K2 | DEP-B24 | |
| DEP-C21 | S4 | K3 | DEP-B24, DEP-B17 | |
| DEP-C22 | S4 | K4 | DEP-B24, DEP-B42 | |
| DEP-C23 | S4 | K5 | DEP-B44 | |
| DEP-C24 | S4 | K6 | DEP-B43 | |
| DEP-C25 | S5 | K1 | DEP-B22, DEP-B23 | |
| DEP-C26 | S5 | K2 | DEP-B30, DEP-B31 | |
| DEP-C27 | S5 | K3 | DEP-B22, DEP-B17 | |
| DEP-C28 | S5 | K4 | DEP-B22, DEP-B42 | |
| DEP-C29 | S5 | K5 | DEP-B44 | |
| DEP-C30 | S5 | K6 | DEP-B43 | |
| DEP-C31 | S6 | K1 | DEP-B7, DEP-B16, DEP-B19 | |
| DEP-C32 | S6 | K2 | DEP-B7, DEP-B31 | |
| DEP-C33 | S6 | K3 | DEP-B19 | |
| DEP-C34 | S6 | K4 | DEP-B7, DEP-B42 | |
| DEP-C35 | S6 | K5 | DEP-B44, DEP-B48 | |
| DEP-C36 | S6 | K6 | DEP-B43 | |
| DEP-C37 | S7 | K1 | DEP-B11 | |
| DEP-C38 | S7 | K2 | DEP-B11 | |
| DEP-C39 | S7 | K3 | DEP-B11 | |
| DEP-C40 | S7 | K4 | DEP-B11, DEP-B42 | |
| DEP-C41 | S7 | K5 | DEP-B48 | |
| DEP-C42 | S7 | K6 | DEP-B43 | |

K5 does not depend on the state of a pending install: it reads what is installed (or, with `-repos`, what registries
offer). Its cells point at the listing rules and, where a graph cannot be built, at the warning rule.
