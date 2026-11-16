# Same-version qualified supplier reload

This lane extends the existing supplier workflow to an ordinary install of an already installed plain `1.0.0`. It supports the existing `load.same-version` commitment: the selected source is loaded again. It does not identify a new defect. Iteration16 remains unchanged and correctly leaves nonfresh installs unassessed.

Product pin: `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. Parent baseline18: `bcb548b3c55944083148494c3501e8de8d72bfd2acd30997367bcf57f7b714d1`; prior17 seal clean-verified. This lane writes only its authorized model, cases, model-lane evidence and this note. Parent owns native execution, canonical owners, bindings and integration. No product, native, Docker, harness or Git edits/jobs were performed here, and no evaluator-private data was inspected. Archive-path investigation remains explicitly **UNASSESSED/UNPURSUED**, including reproduction and probing.

## Prediction ledger

Before the deciding implementation read in the preceding survey, the canonical `load.same-version` commitment predicted that a qualified beta install over alpha at the same full version would succeed and actually load beta's different callable payload, update current supplier/root, and create a new beta install receipt. A missing qualified repository should refuse before loading and preserve the current module/code projection. The inherited model could not make this prediction: `iter16::install` deliberately returns `assessed=false` for nonfresh states. That tag was never evidence that the product was a no-op.

Source then showed unconditional selected loading in `Main.Install:2631-2633` and only strict downgrade/guarded strict upgrade refusals in `Utils.Module.LoadNewModule:1213-1222`. Equal plain versions reach the loader. The negative neighbor is executed after beta, so its preserved state is B rather than the survey's original A prestate. A final qualified alpha install checks that the same-version writer can run again without an uninstall, special cache reset, `-dev` or `-force`.

Parent native trial `20261115T205443672919Z`, actual image `sha256:8db58b93cf76245f6cc9cfc16aaa9627391535eab6350134ecc1bbf8bcf8b82f`, observes148 fields: A1/alpha/rootA, then B2/beta/rootB, missing qualifier refusal preserving B, then A1/alpha/rootA. Final native `20261115T205851829719Z` adds two direct `Follows` calls using the installed A version and separately parsed candidate `1.0.0`: both directions return0, making150 fields. Exact `1.0.0` is supplied to every install. Actual `show`, repository-filtered list and permanent history detail output agree with the independently read stored fields and callable values. This is parent-run evidence, not a direct job by this lane.

The permanent selected successful history IDs are A2, B3, A-again5. The failed attempt occupies gap4. Model logical IDs0,1,2 count only selected successful receipts; they are not DB/temp IDs or a model of the allocator. Refused-attempt history shape is unassessed. A watermark plus exact command/package/action query identifies each actual row; old A/B rows and their rendered source text are checked after later commands.

No product outcome miss arose in this lane. The initial formal source omitted crate-level `main`; its exact source and compile log are retained as `model-lane/baseline-1.*`. The inherited iteration16 manifest-project-URL assumption had already been corrected before that iteration's native qualification: `Repository` is XML-excluded and the installed writer supplies the configured repository name. This is not rediscovered as a new finding here.

The first complete positive attempt timed out after120s; a focused admission diagnostic also timed out after12s. Both used `by(compute)` on nested full World/Map histories. Replacing only those compute assertions with staged SMT assertions made the unchanged semantic model's full baseline pass in roughly1.5s. Exact earlier source/config/timeout metadata and the focused log are retained under `model-lane/attempt1-timeout`; neither timeout is credited. The initial runner failed to emit captured stdout on TimeoutExpired, so that raw output could not be recovered; this evidence limit is explicit in its receipt. The corrected runner writes solver output directly to retained logs and records a timeout before raising. The first clean suite is also preserved before final postcondition/replay support additions.

## Model and reuse

`reloading.rs` imports unchanged `iter16/provenance.rs`, including its unchanged `verus/history.rs` dependency. The latter supplies selection, configured roots/packages, installed module/code state, saved source receipts and show/list/history projections.

New `exact_request`, `existing_equal` and `load_scope` admit only a nonempty qualified request for `iter16-origin` at literal `1.0.0`, with either a fresh state or an already installed equal name/version plus selected code. `follows_plain_equal` is only the literal equal-version comparison calibration. It is not a raw SemVer parser or a general ordering theorem. Other versions remain unassessed.

`skip_equal_load=false` encodes the source-inspected **absence of an equal-version early skip** and makes that rule independently mutable. It is not a discovered product method or a supplied producer outcome. The control that changes it to true preserves success status but leaves old payload/current origin, which the selected witness rejects.

`save_selected_module`, `load_selected_code` and `save_successful_history` compose the selected effect. Their symbolic composition is proved equal to unchanged16's `apply_install` algebra. This reuses that conditional writer relation while adding a new applicable caller state; it does not promote the assignment algebra to proof of all successful `Main.Install` mechanics. All positive effects require successful ordinary manifest import, save, payload load/compile and history persistence providers. Native checks each resulting field independently. Failure, rollback, arbitrary dependencies, resource collision behavior, retained-manager behavior and general cache invalidation remain unassessed.

The selected Root is a logical provenance value. RootA/B bind concrete repository roots `/tmp/iter16/repos/a/` and `/tmp/iter16/repos/b/`; the fixture's module directories are those roots plus `iter16-origin/`. Filesystem cache/manifest decoding and this one-folder path correspondence are provider ports. The unchanged Details producer derives the distinct exact strings `/tmp/iter16/repos/{a,b}/*/module.xml` at Depth1.

`reload` qualifies the requested repository before successful selected loading. A missing qualifier cannot fall back to beta. Its equality/frame statement covers current module/code/configuration and the selected successful-history projection, including its logical next ordinal. It does not claim that the actual DB or failed-attempt history is unchanged.

Named states are `after_a`, `after_b`, `missing_after_b`/`after_missing` and `after_a_again`. There are no successful-uninstall transitions in this iteration. Current supplier is separate from old history: current show/list read the saved current module; history readers read the selected permanent row's saved source triple. No general ANSI/table formatter proof is claimed.

## Source roles

| Role | Exact scoped correspondence |
| --- | --- |
| ordinary qualified request/selection | `Main.Install:2558-2563,2591-2603`; unchanged16 configured/enabled/name/version relation, one qualified source |
| dispatch despite equal installed version | `Main.Install:2631-2633` always sets source then calls selected `LoadQualifiedReference`; no equal-version no-op is claimed |
| selected repository client/directory | `Utils.Module.LoadQualifiedReference:31-33`; `LoadModuleReference:87-99,128-140`; unchanged16 filesystem client/cache correspondence |
| equal-version admission | `Utils.Module.LoadNewModule:1203-1222`; strict `Follows` checks do not fire for literal equal versions |
| selected stored module | `LoadNewModule:1250-1252` imports selected manifest; `1290-1304` writes supplier, selected root and saves; version comes from the selected manifest |
| actual code load | `LoadNewModule:1330-1346` reloads manifest/module and executes ordinary phases through Activate; successful class loader/compile is a native-qualified provider, not body-proved here |
| snapshot/persistence | `Main.Install:2631`; `AbstractHistory.SetSource:86-96` copies configured name/MONIKER/Details; `HistoryTemp.PersistToTwin:38-55` copies fields and finalizes |
| refused qualified miss | `Main.Install:2591-2596,2635-2658`, no selected result means loader is not entered; failed receipt shape omitted |
| current reader | `ModuleSummary.GetLines:66` reads saved Repository; `Main.GetListModules:1379-1381` filters saved Repository |
| historical final reader | `Main.History:4146-4149`; `History.DisplayOneRecord:319-325`, `ShowColumns:360-372`, `SourceToString:457-470` read saved source fields |

These are hand-translated specification contracts and qualified source correspondences. None mechanically verifies an ObjectScript method body, directory enumeration, generic parser or whole caller. A source locator alone is not coverage.

## Controls and replay

The candidate uses full positive verification plus four unique same-source focused positive witnesses and15 local negative controls. Every focused witness unfolds definitions directly and calls no proof lemmas, broadcasts or unverified imported lemmas. Selectors are admission, current module/code, source receipt preservation and missing qualifier refusal. Mutants distinguish no-op despite success, rejection of existing equal state, old supplier/root/payload/version, lost or wrong history, receipt-ID collision and fallback/clearing on missing qualifier.

Negative controls use `--verify-root --verify-function <selector> --multiple-errors 0`; focused positives use the same selector on unchanged source, and the whole positive candidate uses default proof scope/options. A mutant with any resource/time/recursion limit is not credited. No imported dependency is mutated, avoiding unresolved nested-import rewriting. Receipts retain exact commands, source/config/import/tool hashes, counts and semantic rejection classification.

`reloading-cases.json` supplies57 model aliases plus93 gated native qualifications. Its first three aliases are the Boolean A, B and missing statuses. Current payload, stored supplier/root/version, actual show/list predicates and historical source details stay separate observations; history row success is modeled through the stored status/action/finalized/ended/committed conjunction. Additional raw/setup/CLI invocation predicates qualify provider ports rather than inflate modeled mechanics. The two Follows aliases read the literal installed/candidate pair in opposite directions; arbitrary parsing/order remains outside this port.

Final direct suite: **32 full units verified,0 errors (23 inherited +9 new)**; four focused positives each verify exactly one unit; **15 focused local controls reject semantically with no limit credit**. Independent generated replay verifies **33 units,0 errors**, covering all57 aliases against the final parent's150 observations, with all93 qualification gates matching. These are spec/model proofs, not ObjectScript implementation proofs. Exact commands and hashes are in `model-lane/controls/receipt.json`, `replay-receipt.json` and `final-summary.json`.

Frozen formal source SHA256: `bf8985b8201288cfe3594dcc63247ad6b8ad7eb772b841d215934a31c8f1cc12`; cases SHA256: `4e3f926a9d5d875b30c839f9013114d5d185c1f8ecb2ce748b89e2bd03f3ab77`.
