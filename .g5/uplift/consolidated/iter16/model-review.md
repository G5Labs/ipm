# Qualified filesystem supplier and final readers

This lane follows an ordinary qualified fresh filesystem install from the requested repository through the selected package, the saved module and successful history receipt, and the `show`, repository-filtered list and history readers. It does not claim a new product defect. The missing relation in the inherited foundation was selection-to-source causality: `history.rs::set_source` preserved arbitrary supplied source bytes, while the release ORAS model deliberately had one repository.

The pinned product remains `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. Only `iter16/provenance.rs`, `provenance-cases.json`, this note and model-lane artifacts were written by this lane. No product, canonical, harness, native, Docker or Git changes were made. No evaluator-private material was inspected. Native execution and integration belong to the parent.

## Predictions and corrections retained

The pre-source alpha/beta neighbors had the same module and full version but distinct callable payloads. Qualified alpha should install A's payload and record alpha/filesystem/rootA; beta should install B's payload and record beta/filesystem/rootB. The existing model could preserve those source fields only after they were supplied; it did not derive them from the configured supplier.

An earlier proposed shared manifest `Repository=https://project.example/pkg` and uncertainty about `show` were corrected before native execution. `General.ModuleInfo.Repository` is `XMLPROJECTION=NONE` and is documented as the repository from which the module was loaded. `LoadNewModule` writes the configured supplier name into that field; `ModuleSummary.GetLines` prints it. A filesystem XML declaration is not a producer of catalog project metadata. Separately, Remote/ORAS catalog rows can carry a project repository URL, and `search -r` explicitly documents a GitHub repository column. That distinction is source-inspected here; catalog metadata is not claimed as newly modeled or natively qualified by this lane.

The initial model prototype used an abstract depth-zero `Root + '*'` details representation. It was not credited as a native result. The actual source expression also appends `module.xml`, and the parent selected concrete depth-one fixtures. The final model therefore derives the exact strings `/tmp/iter16/repos/{a,b,c}/*/module.xml`, with independent literal assertions to prevent a formatter error from passing by comparing the same wrong helper on both sides.

## Model domain and APIs

`World` contains configured repositories, valid fixture packages at roots, the installed module projection, callable-code projection and successful install/local-load receipts. `select` checks the qualified repository's presence and enabled flag, follows its configured filesystem root and checks the exact package name/version. `apply_install` describes the load writer algebra; only the assessed fresh-install slice derives a product outcome from it. Existing-installed/no-op handling is not the whole `Main.Install` algorithm and is unassessed. The selected root is a logical provenance field: the actual `QualifiedModuleInfo` carries `ServerName`, and the client obtained by that name holds the filesystem root. Stable configuration within one successful install is required; the model does not establish concurrent search/configuration/client-cache behavior.

All three concrete roots contain one valid module folder and use `Depth=1`. `root_text` represents the normalized literal roots, and `details` transcribes the depth-one `Definition.Details` expression. Generic path normalization, directory scanning, XML decoding, cache/index correctness, class loading/compilation, saves and object-close persistence remain provider/correspondence boundaries. Actual Root, Depth, Details, stored fields and callable values must be read independently in native evidence.

`qualified_receipt` reuses the unchanged inherited `set_source`, `finalize` and action-gated source reader. The new `persist` step transcribes `HistoryTemp.PersistToTwin` setting Finalized and copying the receipt. `history_detail` and `history_name` read saved fields rather than the live repository definitions. `show_repository` omits an empty saved supplier; `listed_for_repository` compares the saved module column without a live-definition join.

The named worlds follow the parent's native order:

| State | Current installed projection | Preserved history projection |
| --- | --- | --- |
| `after_a()` | supplier alpha, rootA, payload1 | A: alpha/filesystem/detailsA |
| `after_alpha_delete()` | still alpha/rootA/payload1; show and filter still identify alpha | A unchanged despite no current definition |
| `after_alpha_recreate()` | still alpha/rootA/payload1; current definition now rootC | A still detailsA |
| `after_a_uninstall()` | module/code absent, establishing fresh C prestate | A unchanged |
| `after_c()` | alpha/rootC/payload3 | A unchanged; C records detailsC |
| `after_c_uninstall()` | module/code absent, establishing fresh B prestate | A and C unchanged |
| `after_beta()` | beta/rootB/payload2 | A and C unchanged; B records beta/detailsB |
| `after_local()` | empty supplier, rootA, payload1; show omits Repository and alpha/beta filters exclude it | A/C/B unchanged; new local Action=load, empty source triple, reader `n/a` |

The named witnesses now explicitly use `after_a_uninstall()` before C and `after_c_uninstall()` before B. `successful_uninstall` is a conditional projection that clears the selected module/code state and preserves the selected source receipts; its caller must establish actual module-row and selected payload absence after the real uninstall. A successful status alone is not that premise. Uninstall receipts and general removal/compensation mechanics remain omitted, not newly proved. The records' logical IDs A=0, C=1, B=2, Local=3 map to separately captured permanent native IDs. They are not raw database IDs, temp IDs or an assertion that ID counters agree. Old installation history describes that past action; it is not a current-origin guarantee after a local reload.

`missing()` refuses a missing qualified repository in a fresh state before module/code installation. `disabled()` checks the inspected enabled-repository filter in that same fresh domain. `missing_after_a()` and every other existing-installed/no-op case are explicitly unassessed, rather than an asserted refusal or overwrite. `Result.assessed` gates the product interpretation of `ok` and `world`; those are placeholders when assessed is false. Successful install applicability requires both module and selected code absent, and its symbolic theorem requires this premise. History allocated for refused attempts, its source/error shape and its numeric ID effects are unassessed. The functions are scoped to parsed nonempty qualified exact-name/version requests; an unqualified all-repository search is outside this domain, not asserted to be refused. Other refusal guards and partial failure effects are not generalized from successful fixtures.

## Source roles

| Modeled role | Scoped source correspondence |
| --- | --- |
| qualified request | `Main.Install:2558-2563,2591-2596`, splitting the qualifier and filling SearchCriteria |
| configured/enabled supplier selection | `Repo.Manager.SearchRepositoriesForModule:65-71,81-114,175-177`; this domain has one qualified supplier and one exact version, so it does not add a general ordering proof |
| qualified identity | `Storage.QualifiedModuleInfo.%OnNew:15-25`; ServerName and selected name/version fields |
| filesystem root/client and selected bytes | `Repo.Filesystem.Definition.GetPackageService:51-55`; `PackageService.GetModuleDirectory:48-54`; `Utils.Module.LoadQualifiedReference:31-33` and `LoadModuleReference:87-99,128-140` |
| exact Details producer | `Repo.Filesystem.Definition.Details:12`, normalized Root plus depth pattern and module.xml; depth-one fixture only |
| install source snapshot | `Main.Install:2631-2633` and `General.AbstractHistory.SetSource:86-96`; the latter opens the selected definition and saves its name/MONIKER/Details |
| installed supplier writer | `Utils.Module.LoadNewModule:1290-1296` plus its successful save/lifecycle; `General.ModuleInfo.Repository:29-31` documents the field and XML exclusion |
| intervening configuration | ordinary delete/recreate changes definition/cache state; installed Repository and saved History have no current-definition lookup in the scoped readers |
| successful history persistence | `Main.Install:2666-2670`; `General.HistoryTemp.PersistToTwin:35-55`, especially copied source fields at46-48 |
| current installed reader | `Main` show dispatch at1101; `General.ModuleSummary.Show:11-29`, `GetLines:66`, `AddField:345-346`; `Main.GetListModules:1379-1381` |
| historical supplier reader | `Main.History:4146-4149`; `History.DisplayOneRecord:319-325`, `ShowColumns:360-372`, `SourceToString:457-470` |
| local reload | `LoadNewModule` empty pRepository writer at1296 and ordinary load history; `SourceToString` returns n/a for every non-install action, including details |

These locators bind the listed decision/writer/reader slices. They do not verify whole ObjectScript members. Filesystem parsing, the service-cache lifetime and all lifecycle resource actions are not covered merely because their callers appear in the chain.

## Proofs, controls and retained attempts

The final direct baseline verifies23 units with zero errors. An imported-only baseline verifies14 units from the unchanged inherited history model; the delta is nine new proof units. Twenty-five controls reject semantically: twenty-three local variants and two imported dependency variants. They cover qualified supplier/root, enabled and missing-repository outcomes, stored supplier/root/payload, source name/type/details, delete/recreate frames, late live lookups by history/show/list readers, local reset/action, finalization, the literal Details format, inherited source copying and inherited action gating. No compile failure, timeout or resource limit is counted as a semantic rejection.

Authoritative direct receipt: `iter16/model-lane/controls/receipt.json`. The direct suite took41.751 seconds in aggregate. Its model SHA256 is `ad7ce5b21047a847db88018437293ebdaccd234ce70e1f78a46120108509fef0`; imported source/tool hashes and individual commands/logs are in the receipt. `provenance-cases.json` proposes71 replay fields, with named-state preludes; parent integration adapts the labels and adds native setup/status qualifications.

Two failed generic receipt proofs and exact source snapshots remain as `model-lane/baseline-1.*` and `baseline-2.*`. The first omitted the domain's nonempty qualified-name premise, so an arbitrary map containing an empty supplier key produced the real reader's NotSet branch instead of Supplier. Revealing more definitions in attempt2 did not fix that counterexample. The final theorem explicitly requires a nonempty qualified request, consistent with the already stated domain. No assumption, admission, external body or new trusted declaration was introduced. Direct ObjectScript implementation proofs remain zero.

## Native handoff and remaining limits

Capture the largest permanent History ID before each action, then query rows above that watermark with exact Action, Package and CommandString and require exactly one match for the dependency-free fixture. Read ID, version components, source triple, TimeEnd, Success, Committed and Finalized. Map the captured permanent IDs to A/C/B/Local, then run actual `history details <ID>` and `show <module>` independently of raw stored-field comparisons. Root/Depth/Details shape and callable payload must also be checked independently of a successful status. Never infer a current supplier solely from old install history.

After A, deleting alpha and then recreating the same name at rootC must leave historyA's saved detailsA intact. The current definition must independently report rootC/detailsC. Ordinary Shell/cache lifetimes should decide whether the later install really uses rootC; no special cache clearing should force the prediction. A naturally retained stale client would refute this model's next-command producer correspondence and must be retained and modeled. This lane has not run that native schedule.

Available catalog project metadata is source-inspected only. Configurations changing during an install, arbitrary version ranges/order, malformed manifests, cache refresh failures, provider partial writes, failed-load rollback and general origin reporting remain outside this model. The earlier filesystem duplicate first-admission/provenance workflow remains deferred and meaningful, not dismissed as trivial. Archive-path behavior is explicitly UNASSESSED/UNPURSUED by user direction; this lane performs no source investigation, representation, reproduction or probe of that boundary. No repository-wide convergence claim follows from this batch.

## Final native literal alignment

Only the three literal definitions were changed to the actual native aliases `alpha`, `beta` and module `iter16-origin`. All model rules and case-control mutations are unchanged. The complete prior source/config/report and proof/control outputs remain in `model-lane/pre-literal-alignment/`; `model-lane/literal-alignment.json` records the paired hashes and exact three changed lines. The full final baseline/imported-baseline and all21 controls were rerun successfully before refreezing. Parent-reported ordinary same-Shell execution supplied C after alpha recreation without special cache clearing; final native evidence and CLI reader qualification remain parent-owned.

## Fresh-install witness correction

Parent integration identified that the earlier `after_c` and `after_beta` witnesses applied the generic assignment algebra while a module/code projection was still present, whereas the actual native sequence first uninstalled A and C. That source/trace correspondence gap is retained in `model-lane/pre-fresh-install-alignment/`, including the exact source, config, report and prior full receipts. The correction adds explicit observed-success uninstall projections, requires fresh state for assessed installs, and marks existing-installed/no-op cases unassessed. It does not infer that a same-version install overwrites or that every successful uninstall fully compensates effects. Four new controls challenge the clearing/history frame and applicability tag; all earlier21 controls were rerun. `model-lane/fresh-install-alignment.json` records paired hashes and the final23-unit/25-control receipt. The added native replay proposals check module/code absence and preserved source history after each actual uninstall.
