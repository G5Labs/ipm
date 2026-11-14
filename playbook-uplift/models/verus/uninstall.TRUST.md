# Trust inventory: uninstall.rs (module 05)

What a "proved" (P) or "model-checked" (M) claim from `models/verus/uninstall.rs` rests on without itself being proved.
The settings items (S1-S7) moved to `models/verus/settings.TRUST.md` (review/REVIEW.md finding 15). The file uses no
`assume`, `admit`, `external_body` or `external` (record_proofs.py checks this and records `trusted_items: []`).
Premises are not trusted items: each one is a named `premise_*` spec function, declared at the top of the file as
`// PREMISE <fn> = <LABEL>`, appears in the `requires` of the lemmas that need it, and is dropped once by a control
(`controls/uninstall.json`) that the proof then rejects. `scripts/ontology_index.py` checks that each P tag's statement
names every premise of its lemma.

| # | Trusted item | Why it is trusted | What checks it |
|---|---|---|---|
| U1 | The L2 spec functions transcribe the ObjectScript (Main.cls Uninstall, Storage/Module.cls ExecutePhases and Uninstall, Lifecycle/Module.cls and Base.cls %Clean and CheckBeforeClean, FileCopy, Global and WebApplication processors) | Written by hand from the source at 571e04a1, line anchors in comments; ObjectScript has no verifier | The executable twin (`models/explorer/repo/unin_twin.py`, same function names; parity check `repo-transcription-parity-uninstall`) replays 20 IRIS observations with 0 mismatches (`repo-conf-*`); 4 twin mutants each contradicted by IRIS (`repo-twin-mutants-rejected`); 4 model-derived predictions were probed before being written down and held (`nc-repo-model-unin-*`) |
| U2 | The twin is the same machine as the Verus L2 | Hand transcription of a transcription | `repo-witness-agreement`: the twin reproduces every fact of the 9 uninstall witnesses; exhaustive exploration finds 0 violations of each L1 clause under the same premises the lemmas use |
| U3 | Dependency resolution is data: `Module.graph` stands for what `BuildDependencyGraph` returns, `Module.needs` for the true set of needs, and both stay fixed during one command | Resolution belongs to modules 03-04; a module removed during `-r` could make a later graph fail to build | PG names the condition `graph == needs`; F-UNIN-4 observed when it fails. A graph that changes during one command is not modelled (gap) |
| U4 | Fuel bounds the nesting of the `-r` recursion; running out is an error outcome | Spec functions need a decreasing measure; the code has none (the Cycle list stops loops) | Every lemma holds for every fuel, except `unin_refines_honest`, which needs `recurse ==> fuel > 0` (with fuel spent at the top, `uninstall zpm -r` would delete IPM's classes and then report the fuel error; control `bound-drop-fuel`). The conformance runs use fuel 6 (deeper than any fixture). That enough fuel never runs out is argued from the Cycle list, not proved |
| U5 | Ownership of each resource is a static map (`Res.owner`), standing for `ResourceReference.GetChildren`'s ownership check | That check walks the class dictionary and package names | UNIN-B6 test PkgCaseUninstall (T) and observation nc-unin-3; not proved |
| U6 | A file copy's target is a folder id plus a single-file flag; deleting a folder removes every copy into it | Paths and directory nesting are abstracted to one level | nc-unin-7-control-keep-module and nc-unin-7-shared-target-folder replayed with 0 mismatches; nested sub-folders are not modelled |
| U7 | `sd()` (class `%IPM.StudioDocument.Module`) stands for all of IPM's own classes: once it is gone, the next IPM call fails | The code checks that one class before the record delete (Base.cls#L494) and fails on `##class(%IPM.Storage.Module)` after a nested self-clean | nc-repo-model-unin-recurse-ipm-dependency (all three IPM classes absent, CLASS DOES NOT EXIST) and nc-unin-self-* observations |
| U8 | Upper-cased keys stand for the case-blind Name and ExternalName indexes; a typed name carries a separate "exactly zpm" bit | SQLUPPER collation | nc-unin-env-name-case (index subscripts and Exists results) replayed |
| U9 | The full cleanup path (`UninstallAll`) is not modelled: answering Yes only sets `full` | `-all` is a separate loop (Utils/Module.cls#L709-L825) | Premise PNF excludes it from every lemma that needs it; `-all` has inspection and test evidence only |
| U10 | Resource kinds not modelled have no Clean effect: CPF merges, Python wheels, author `<Invoke>` hooks at Clean Before, source-control hooks, class data globals | Out of scope | UNIN-B14/B15 by inspection; nc-unin-env-obj-delete-* (class data) not expressible |
| U11 | History rows are reduced to (module, success); their fields and Committed belong to module 02. For a self-uninstall the row is written before %Clean (#L626-L638) | Row content beyond success is module 02 | nc-unin-1 and nc-unin-8 history rows replayed |
| U12 | Uninstall has no crash step. A process killed during Clean is not modelled | Uninstall runs no transaction (UNIN-B43); what a crash leaves in the history row is module 02 | Not checked. For IPM itself the row is written as a success before Clean (U11), so a crash after that leaves a success row; for other modules the row is finalized after Clean, so a crash leaves it unfinalized |
| U13 | IPM's own record declares no dependencies (`wf`: `deps.len() == 0` for `ipm_id()`) | Fact of ipm/module.xml at 571e04a1 (no `<Dependencies>` element), used for "worked => success" of `uninstall zpm -r` | Control `wf-drop-ipm-no-deps` (the honest lemma fails without it); every twin world builds IPM without dependencies |

Bounds of the model checks (part C of `models/explorer/repo/REPORT.md`): IPM plus two modules with every attribute
combination (56,448 worlds), IPM plus three modules with every acyclic graph (256), a namespace that borrows IPM (14);
2,951,728 uninstall and clean runs (settings bounds: settings.TRUST.md). The Verus lemmas have no such bound.

## Premises (labels used in the ontology Notes)

| Label | Spec fn | Excludes | Lemmas | Control |
|---|---|---|---|---|
| PW | `premise_pw` | F-UNIN-1 (UNIN-B31) | `unin_refines_blocked_unchanged` | `premise-drop-pw` |
| PB | `premise_pb` | F-UNIN-6 (UNIN-B30) | `unin_refines_honest`, `unin_refines_rows_honest` | `premise-drop-pb` |
| PG | `premise_pg` | F-UNIN-4 (UNIN-B33) | `unin_refines_blocked_unchanged`, `_honest`, `_dependents_protect`, `_rows_honest` | `premise-drop-pg` |
| PGA | `premise_pga` | F-UNIN-5 (UNIN-B12) | `unin_refines_complete` | `premise-drop-pga` |
| PF | `premise_pf` | UNIN-B21 against UNIN-B19 | `unin_refines_only_own`, `_kept_files`, `_clean_keeps_target` | `premise-drop-pf` |
| PN | `premise_pn` | F-UNIN-3 (UNIN-B62) | `unin_refines_self_guard` | `premise-drop-pn` |
| PI | `premise_pi` | F-UNIN-8 (UNIN-B78) | `unin_refines_honest`, `_ipm_kept` | `premise-drop-pi` |
| PCW | `premise_pcw` | F-UNIN-1, clean part (UNIN-B66) | `unin_refines_clean_keeps_target` | `premise-drop-pcw` |
| PNF | `premise_pnf` | full cleanup (U9) | most uninstall lemmas | `premise-drop-pnf` |
| PR | `premise_pr` | F-UNIN-9, UNIN-B79 (-r) | `unin_refines_manifest_with_record`, `_rows_honest` | `premise-drop-pr` |

A lemma whose premise excludes exactly a statement's case is not cited as P on that statement (UNIN-B12, B31, B66,
B78 carry F and the witness instead). `unin_refines_honest` and `unin_refines_rows_honest` state both directions
(success exactly when the module is gone); controls `impl-success-reported-failed`, `impl-row-always-failure`,
`impl-hook-failure-ok`, `impl-top-record-kept` and `impl-row-always-success` break one direction each.
