# G arm coverage baseline: notes

This file explains `G-baseline.tsv`. That table grades which of the 1,216 members in `../coverage/members.tsv` the G arm's existing models genuinely model, under the consolidation plan's four-part evidence rule. Product source is IPM at `571e04a1`, under `src/cls/IPM/`. `git diff 571e04a1 HEAD -- src/` is empty, so the checked-out source is the pinned source. Only the two baseline files were written. Nothing was rerun. Receipts were read, not re-executed.

The G arm is everything under `.g5/` except `.g5/uplift/consolidated/` and `.g5/Verus_verification/consolidated/`. Graded citations come from the listed locations: the crate `crates/ipm-load-history/`, including `src/bin/b1.rs`; `followup/` (models, MODELS.md, PROJECTIONS.md, replay, native, results); `coverage-map.md`; `premises.md`; `README.md`; `cases/cases.json`; `native/`; and `results/index.jsonl`.

In the TSV, `model_ref`, `assertion` and `binding` paths are relative to `.g5/Verus_verification/`. Member ids and product line citations are relative to `src/cls/IPM/`.

## Result

| Verdict | Members |
|---|---:|
| `yes` (meets rules 1–4) | **23** (1.89% of 1,216) |
| `ambiguous` | 8 |
| `no` (cited, fails at least one rule) | 24 |
| Cited members graded | 55 (58 rows; LoadNewModule and LoadModuleReference also carry exclusion rows, and LoadNewModule an observation-only row) |

If every `ambiguous` member were accepted, the upper bound would be 31 (2.55%).

These are the 23 qualifying members:

- `Main.cls::Load`, `Main.cls::LoadInternal`, `Main.cls::History`
- `General/AbstractHistory.cls::Init`, `General/AbstractHistory.cls::Finalize`
- `General/HistoryTemp.cls::Init`, `General/HistoryTemp.cls::PersistToTwin`, `General/HistoryTemp.cls::%OnClose`
- `General/History.cls::PreventSQLUpdates`, `ConstructSQLWhere`, `GetHistory`, `DeleteHistory`, `DeleteHistoryGlobally`, `GetHistoryGlobally`, `GlobalRecordsFetch`, `AutoPurgeOldHistory`
- `Utils/JournalManager.cls::%OnClose`
- `Utils/Module.cls::LoadNewModule`, `LoadModuleReference`, `LoadModuleFromDirectory`, `LoadDependencies`
- `StudioDocument/Module.cls::ImportStream`
- `Storage/Module.cls::ExecutePhases`

## How the rule was applied

1. **Source slice.** The citation must give a `file:line` range inside the member's start..end. A range that spans several members was split by overlap, as instructed. A citation by method name alone, or by lines that belong to a caller, fails rule 1. One example is "`InitPhase` 570-572", where the lines are ExecutePhases lines.
2. **Model action.** The model must have a step that represents this member's own effect or decision. Two cases fail this rule:
   - A forwarder. LoadInit, GlobalRecordsExecute and GetModuleObjectFromPath's parse path forward to another member.
   - An input oracle, where the model takes the member's whole decision as an input bit and so cannot disagree with it. This covers `sysreq` for CheckSystemRequirements, `act` for %Activate, `cleanup_fails` for TempLocalRepoManager.CleanUp, `mf == 1` for GetModuleObject*, and `found` for BuildDependencyGraph. The assertions that use such a bit distinguish how the caller handles the outcome.

   A dispatcher earns credit only for the branches it models. Its reason column names those branches.
3. **Assertion.** These forms were accepted:
   - A Verus theorem or lemma in `proofs.rs`. The final receipt is `results/verus/20261113T222933Z-asbuilt-proofs-r3`: 263 verified, 0 errors.
   - A case lemma in `cases.rs`.
   - An explorer run in `src/bin/b1.rs` with an `expect` value. In the final batch `20261113T223451Z`..`224441Z`, every run matched its expectation, controls 01–04 and 06–13 were detected, and control 05 survived as designed.
   - A Python check with a stated expected verdict: the `loadtx.py` correspondence rows (exit code, 12/12 hold) and the `history_readers.py` valid neighbor and archived correspondence (receipt `expected`).
   - Clause-violation counts from `explore()` in `history_readers.py`/`loadtx.py`, recorded in the receipts and stated as results in MODELS.md. These were treated as explorer properties.
   - `cases.json` case verdicts whose clause sets the corrected replay evaluator confirmed against native observations (`followup/results/20261114T002646Z-final-replay-reeval`: 0 verdicts changed, 59/59 same clauses).

   Native field agreement on its own, from `l2_compare` on a field that no model check reads, was not accepted as a rule-3 assertion.
4. **Correspondence.** Either a native receipt that exercises the member, or a `file:line` static binding (BINDINGS.md tables or the model docstrings' line bindings). The untested premise is named in the row's `binding` or `reason` column.

Edges follow the task's definitions. An exclusion row is written where a citation states that the cited behavior is not modelled. An observation-only row is written where the citation only explains a native observation or a proxy.

## How each citation was resolved

### BINDINGS.md (bare numbers take the file in the column header, unless a file is named earlier in the cell)

- **Stores table.**
  - `General/History.cls:4-22` overlaps only `PreventSQLUpdates` (15-22). Lines 4-14 are class header, indexes and a property.
  - `General/HistoryTemp.cls:3-130` overlaps Init, PersistToTwin and %OnClose. Each of the three also has a step-level slice.
  - `Utils/JournalManager.cls:9-75` overlaps %OnNew, Enabled, Enable, Disable and %OnClose. Only %OnClose has a step-level slice elsewhere (MODELS.md `:22-25`).
- **Main:Load table.**
  - `Main.cls:2420`, `:2427`, `:2430` resolve to Main.Load.
  - `AbstractHistory.cls:55-61` and "%Save at 61" resolve to Init.
  - "SetVersion saves at 109" is relative to AbstractHistory.cls and resolves to SetVersion.
  - `HistoryTemp.cls:14-24`, 21 and 24 resolve to HistoryTemp.Init.
  - `Main.cls:2447-2452`, `:2529`, `2529-2531`, 2530 and 2531 resolve to LoadInternal.
  - `AbstractHistory.cls:127-134` resolves to Finalize.
  - `HistoryTemp.cls:62-67` and `:70` resolve to %OnClose. `:30-59` is relative to HistoryTemp.cls and resolves to PersistToTwin.
  - `History.cls:15-22` resolves to PreventSQLUpdates.
  - "ShellInternal returns" is a name-only citation of ShellInternal.
- **LoadNewModule table (`Utils/Module.cls`).**
  - Every bare range from 1162 to 1363 resolves to LoadNewModule.
  - `341-347` resolves to GetModuleObjectFromPath and `359-362` to GetModuleObjectFromStream.
  - `StudioDocument/Module.cls:149-202` and 184, 185, 190, 194 and 198-200 (relative to that file) resolve to ImportStream. Line 190 is labelled "UpdatePersistentFromStream", but the line belongs to ImportStream, so UpdatePersistentFromStream has a name-only citation.
  - `Storage/Module.cls:337-354` resolves to CheckSystemRequirements.
  - `1368-1423`, `1380-1381`, `1407-1409` and `1411-1412` resolve to LoadDependencies. The labels "BuildDependencyGraph" and "LoadModuleReference" on call lines are name-only citations of those members.
  - `Storage/Module.cls:447-720` with 570-572, 608-610, 669-671, 692-697 and 709-711 resolves to ExecutePhases. InitPhase and FinalizePhase are named at those call lines, so their citations are name-only.
  - `Lifecycle/Base.cls:615` resolves to %Reload.
  - `ResourceProcessor/Default/Document.cls:350`/`378` resolves to OnLoad.
  - `Lifecycle/Module.cls:95-132` resolves to %Activate.
- **LoadModuleReference table.**
  - 63-68, 69-70, 128-140 and 207-209 resolve to LoadModuleReference. LoadInit, SetSource, SetVersion and Finalize are named at its call lines.
  - 293-330, 309-310 and 322 resolve to LoadModuleFromDirectory.
- **Abstractions.**
  - `Utils/Module.cls:84-126` is a LoadModuleReference exclusion.
  - `Utils/Module.cls:1354` is a LoadNewModule exclusion slice.
  - WriteAfterInstallMessage, MarkResourceForUpdate and ApplyUpdateSteps are name-only exclusions.
  - These concepts carry no member or line and were not resolved: archive and git sources, ModuleRoot copies, `.modules` registration, lock file, pip, CPF, `LoadedResource`, analytics, read-only repository forcing, legacy XML installs and version conflicts.

### Crate sources and stage-B notes in Verus_verification

- **`model.rs`.** `Utils/Module.cls:1158-1307`, `1213-1223` and `1258-1260` resolve to LoadNewModule. Every other reference is a name: GetModuleObjectFromPath, ImportStream, UpdatePersistentFromStream, LoadModuleFromDirectory, BuildDependencyGraph, LoadDependencies, AbstractHistory:Finalize, AbstractHistory:SetCommitted, HistoryTemp:Init, PersistToTwin, %OnClose and CleanUp.
- **`contract.rs`, `proofs.rs`, `cases.rs`, `explore.rs`.** Names only.
- **`src/bin/b1.rs`.** `Utils/Module.cls:1226-1228` resolves to LoadNewModule.
- **`README.md`.** `Utils/Module.cls:1312-1314` resolves to LoadNewModule.
- **`premises.md`.** `StudioDocument/Module.cls:184-200` resolves to ImportStream.
- **`coverage-map.md`.** Gives no product line citations. It names GlobalRecords (D20) as "Not modeled" in the crate, which the follow-up model later superseded.
- **`cases/cases.json` and `results/index.jsonl`.** No line citations.

### followup/

- **`models/MODELS.md`.**
  - `Utils/Module.cls:1165-1172` and the relative `:1191-1196`, `:1224-1227`, `:1309-1351` and `:1357-1362` resolve to LoadNewModule.
  - `:58-67` resolves to LoadModuleReference.
  - `AbstractHistory.cls:127-133` resolves to Finalize.
  - `HistoryTemp.cls:30-70` is split between PersistToTwin (30-59) and %OnClose (62-70).
  - `Storage/Module.cls:1139-1154` resolves to ProcessSingleDependencyIterative.
  - `JournalManager.cls:14` falls in no member: it is the `InitialValue` property initializer. `:22-25` resolves to JournalManager.%OnClose.
  - `History.cls:93-126` resolves to ConstructSQLWhere, `:132-147` to GetHistory (member 132-148), `:151-159` to DeleteHistory, `History.cls:199-230` to DeleteHistoryGlobally and `:162-181` to AutoPurgeOldHistory.
  - `:233-317` is split across GlobalRecords, GlobalRecordsExecute, GlobalRecordsFetch, GlobalRecordsClose and GetHistoryGlobally.
  - `Main.cls:4161-4173` resolves to Main.History, `Main.cls:1067-1070` to ShellInternal and `CLI.cls:815-827` to SetData (809-828).
- **`models/history_readers.py`.**
  - The docstring sets the default file: "src/cls/IPM/General/History.cls unless stated". That default, not the nearest named file, governs bare ranges. Under the nearest-file rule, `:93-126` and `:162-181` would resolve into Main.cls.
  - Bare ranges therefore resolve to History.cls: ConstructSQLWhere, GetHistory, DeleteHistory, DeleteHistoryGlobally, GetHistoryGlobally (`:295-317`) and AutoPurgeOldHistory.
  - `:233-293` is split across the GlobalRecords query and its three methods.
  - "Main.History (:4161-4173)" resolves by its method qualifier to Main.cls. `Main.cls:4136` resolves to Main.History and `Main.cls:862` to Shell.
  - DisplayOneRecord is cited by name only.
- **`models/loadtx.py`.**
  - `Utils/Module.cls:1165-1169` resolves to LoadNewModule. `:1328`, labelled "LoadDependencies at", is the call line inside LoadNewModule.
  - `:306-311` and `:307-311` resolve to LoadModuleFromDirectory and `:130-139` to LoadModuleReference.
  - `:1191-1193`, `:1194-1196`, `:1224-1227`, `:1309-1319`, `:1335-1339`, `:1347-1351` and `:1357-1362` resolve to LoadNewModule.
  - `Main.cls:2417-2431` resolves to Main.Load, `Utils/Module.cls:58-67` to LoadModuleReference and `AbstractHistory.cls:51-62` to Init.
  - `AbstractHistory.cls:127-133` resolves to Finalize. `HistoryTemp.cls:30-70` is split as in MODELS.md.
  - `(:1368-1405)` follows `HistoryTemp.cls:30-70`, and HistoryTemp.cls has only 132 lines. The method context ("LoadDependencies of the top module") resolves it to Utils/Module.cls LoadDependencies.
  - `Storage/Module.cls:1139-1154` resolves to ProcessSingleDependencyIterative. `JournalManager.cls:14`/`:22-25` resolve as in MODELS.md.
- **`models/prefix_cases.py`, `models/breakdown.py`, `replay/*.py`.** These cite `contract.rs`/`model.rs` only.
- **`replay/PROJECTION.md` and `PROJECTIONS.md`.** `Storage/Module.cls:1879-1882` resolves to %OnBeforeSave. PROJECTIONS also cites `Utils/Module.cls:1165-1172` (LoadNewModule), `1368-1405` (LoadDependencies) and `Storage/Module.cls:1139-1154`.
- **`native/make_sessions.py` and `native/cases/expected-r04.json`/`expected-r05.json`.** These are native expectations, written before the runs.
  - LoadNewModule: `Utils/Module.cls:1172`, `1194-1196`, `:1357-1362`, `1165-1169`, `:1323` and `:1224-1227`.
  - LoadDependencies: `1368-1405` and `1381-1395`.
  - LoadModuleFromDirectory: `307-311`. LoadModuleReference: `:109`, the install-only guard inside the 84-126 exclusion.
  - ProcessSingleDependencyIterative: `Storage/Module.cls:1139-1163`, `1157-1163` and `1180-1182`.
  - GetHistory: `History.cls:132-147`. AutoPurgeOldHistory: `:162-181`.
- **`native/make_fixtures.py`.** `Utils/Module.cls:134-139` resolves to LoadModuleReference.

### native/ (B2 harness)

- **`cases/expected-*.json` and `make_stagec*.py`.**
  - HistoryTemp: `HistoryTemp.cls:21` resolves to Init, `:53` to PersistToTwin and `:70` to %OnClose.
  - Main.cls: `2416-2431` resolves to Load, `2445-2453` and `2452` to LoadInternal, and `4165-4167` to History.
  - LoadNewModule: `Utils/Module.cls:1166-1169`, `1255-1264`, `1256`, `1260`, `1269-1275`, `1272` and `1358-1362`. The update `-path` lines, 1269-1275, became the LoadNewModule observation-only row.
  - Lines that fall in no member: `HistoryTemp.cls:1`, `AbstractHistory.cls:6-7`, `17` and `38` (class and property declarations); `Main.cls:183` and `280-282` (command help); `Utils/Module.cls:1033-1040` (a doc comment between members).
- **`scripts/B2Native/*.cls`, `cos/*.cos`, `repro/*.cos`.** These call product members directly. Those calls were used only as correspondence evidence. A call in a test driver is not a model citation, so members that appear only there have no row: `History.GetHistoryLastPurge`, `Main.GetNamespaceDefaultModifiers` and `TempLocalRepoManager.%OnNew`.

### Citations elsewhere in the G arm (read, not graded)

- `uplift/ipm-verus-20261113/B/model-notes.md` adds no new member. `Utils/Module.cls:172-185`, the legacy XML install path, falls inside LoadModuleReference and is consistent with its exclusion row.
- `B/native-report.md` and `B/native-replay.md` cite defect explanations: History.cls `:103-105`, `:173`, `:176` (SetHistoryLastPurge), `:420` (ShowPhases) and `:478-480` (SuccessToString); `UniversalSettings.cls:101`; and several Main.cls and Utils/Module.cls lines. These are observations, not model citations.
- The `uplift/ipm-quality-followup/` review and findings files, and the `ontology/` files, carry many audit and intent citations. None of them is a model citation.

## Ambiguous matches

**Resolution ambiguities.**

- **Bare ranges in `history_readers.py`.** The docstring's default file was used, not the nearest-file rule. The two rules disagree for `:93-126`, `:162-181` and `:4161-4173`.
- **`loadtx.py` `(:1368-1405)`.** Resolved by method context, because the nearest named file (HistoryTemp.cls) is too short.
- **Labels on call lines.** Labels such as "`BuildDependencyGraph` 1380-1381", "`InitPhase` 570-572", "`UpdatePersistentFromStream` 190", "`LoadDependencies` at :1328" and "SetSource, SetVersion 69-70" name one member at a line that belongs to another. The line was credited to the member that contains it. The named member got a name-only citation.
- **"ApplyUpdateSteps".** Resolved to `Lifecycle/Base.cls::%ApplyUpdateSteps`. The name is also the phase string passed to ExecutePhases.
- **Multi-member ranges `:233-317` and `:233-293`.** These split across the GlobalRecords query, its three methods and GetHistoryGlobally. The D20 behavior (rows fetched in the caller's namespace) is located in GlobalRecordsFetch 259-263; DisplayArray, by contrast, does switch namespace. That location was derived by reading the source, not from a citation.

**Members graded `ambiguous`, and what would settle each.**

| Member | Open point | Settles it |
|---|---|---|
| `AbstractHistory.cls::SetVersion` | Rules 1, 2 and 4 hold (slice `:109`). No model check reads the version. Only native `l2_compare` field agreement does | Accept native field agreement as an assertion → yes; otherwise no |
| `JournalManager.cls::%OnNew`, `Disable`, `Enable` | The only in-member slice comes from splitting the whole-class Stores row `9-75`. The step binding cites the call site `Utils/Module.cls:1191-1193`. The rest is strong: `lnm_begin`/`lnm_exit`, `loadtx.py` M5 and N-P19 rows, the M-JOURNAL-NOT-RESTORED mutant (Enable) and native N-P07 | Treat a whole-class store row as a slice → yes; otherwise no |
| `Storage/Module.cls::ProcessSingleDependencyIterative` | `dep_satisfied` models the outcome (installed at the requested version → skipped), but BINDINGS attributes it to LoadDependencies. G cites `1139-1154` for a reduction premise, and version satisfaction is reduced to equal ordinals | Decide whether the split skip decision credits this member |
| `Lifecycle/Base.cls::%Reload` | One `res` flag stands for the resource work of Reload and Compile. The cited `615` is the declaration line. `followup/replay/PROJECTION.md` names Compile as the producer of the observed field | Decide whether a coarse aggregate effect is "own effect" |
| `Main.cls::ShellInternal` | Only the default-modifier merge (`1067-1070`) is represented, in a single `as_built` line shared with Main.History:4136 and CLI.SetData and annotated "native H2" | Decide whether a joint, observation-derived step credits each contributor |
| `General/History.cls::GlobalRecords` (query) | The row shape (no namespace column) is encoded and checked by H10. A declaration-only member, with its slice only from multi-member ranges | Decide whether a query declaration can carry a modeled effect |

## Re-judgment of the prior pass's ten G members

All ten still qualify. The thinnest are these:

- **GlobalRecordsFetch.** Its slice comes only from multi-member ranges. H10 is its only native evidence, and its defect is partly a property of the query's ROWSPEC.
- **AutoPurgeOldHistory.** "Ran today" is an input, so only its quoting and retain decisions are distinguished.
- **LoadDependencies.** No Verus lemma covers it. Its check is the case verdicts (B1-C35/C36/C40), the `loadtx.py` reduction and the native runs.

## Members this pass adds

Thirteen more members qualify than in the prior pass:

- LoadInternal and Main.History
- AbstractHistory.Init
- HistoryTemp.Init and PersistToTwin
- History.PreventSQLUpdates, GetHistory, GetHistoryGlobally and DeleteHistoryGlobally
- JournalManager.%OnClose
- LoadModuleFromDirectory, ImportStream and ExecutePhases

Most were missed because the counting rule did not read BINDINGS.md, or did not resolve relative and short-filename ranges.

## Known facts checked

All of these were confirmed in the files:

- BINDINGS.md binds AbstractHistory.Init and Finalize; HistoryTemp.Init, PersistToTwin and %OnClose; the History update trigger (PreventSQLUpdates); ImportStream; CheckSystemRequirements; and LoadModuleFromDirectory.
- The follow-up models cite GetHistory, DeleteHistory, DeleteHistoryGlobally, GetHistoryGlobally, GlobalRecords, AutoPurgeOldHistory and JournalManager.%OnClose by relative, short-filename or multi-member forms.

Being bound is not enough: CheckSystemRequirements is bound but graded `no`, because its whole decision is the input bit `sysreq`.

## Most doubtful judgments

1. **DeleteHistoryGlobally = yes.**
   - Rule 3 rests on recorded explorer counts (delete-global 52 STATUS, 12 DELETED-UNSELECTED) that sit outside the receipt's stated expectation.
   - The branch that distinguishes it (every error ignored) has only a member-range static binding and was never run natively. MODELS.md says so.
2. **GlobalRecordsFetch = yes.** Its slice comes only from multi-member ranges, and the D20 outcome is equally a property of the query's ROWSPEC.
3. **CheckSystemRequirements = no.**
   - It is bound in BINDINGS. `case_c23` and native C22–C24 exercise it.
   - It is graded `no` under the input-oracle rule. A reader who credits the bound "refusal → fail" step would grade it `yes`.
4. **JournalManager %OnNew, Disable, Enable = ambiguous.** Everything holds except an in-member slice beyond the whole-class Stores row.
5. **SetCommitted = no.** The only gap is a method-name-only citation. Controls 01–03 and `theorem_commit_sound` are otherwise strong, but they remove LoadNewModule's call sites, not this body.

The next most doubtful are LoadModuleFromDirectory (`yes`, with no Verus lemma) and %Reload (`ambiguous`).
