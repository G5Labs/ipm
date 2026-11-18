# Iteration72: census re-audit under rule 4a and review-71 repairs

Builder session `fb9b10b7`, Opus 5.5 high. Starting commit `fca3ed0f`, product pin `571e04a1` (unchanged). No model, ontology or product files were edited, and no Docker was used. Verus was the pinned `0.2026.09.27.3cf1832` x86 build throughout.

## 1. Result

All 697 full grades were re-judged against rule 4a. 125 became partial. Together with the review-71 repairs, whole-foundation coverage drops from 0.6478 to 0.5314.

| | full | partial | absent | unassessed | assessed | coverage | full with passing check |
|---|---|---|---|---|---|---|---|
| before (`before-counts.json`) | 697 | 214 | 165 | 0 | 1076 | 0.6478 | 637 |
| after (`census/counts.json`) | 583 | 350 | 164 | 0 | 1097 | 0.5314 | 524 |

Design checks moved from proved 619 / bounded 18 / refuted 60 / unchecked 214 / none 165 to 623 / 19 / 64 / 227 / 164. `census.py count` prints no ERROR or WARN lines (265 blocks: 258 enumerated, 7 structural, 0 stale). `census.py selftest` reports 9 controls detected.

### Regrades kept apart from scope changes

`changes.json` classifies every obligation id: retained (same id, same text and cases), scope (text or cases changed), new, or alias-change.

Retained obligations (1,166 ids, aliases included; 1,033 counted before and after):

| before -> after | count |
|---|---|
| full -> full | 567 |
| **full -> partial** | **122** |
| absent -> full | 2 (build.publish#2, lifecycle.custom#2) |
| absent -> partial | 1 (update#2) |
| partial -> partial | 210 |
| absent -> absent | 161 |

Scope changes from the review-71 repairs (§3):
- **Changed text or cases (14 ids).**
  - full -> partial: repo.oras.integrity#1, uninstall.cleanup#1, uninstall.cleanup#2.
  - partial -> full: devtools.authoring.module-version#1, narrowed to its success half.
  - absent -> partial: history.actions#2, narrowed to attempts that reach the History init.
  - Text or cases changed with the grade unchanged: history.committed#8, history.committed.marking#1, settings.default-modifiers#10, transaction.flag-sources#1 and #8 (all full), and lifecycle.module-action#2, module.env#2 and module.env#8 (all partial).
  - module.processor.file-copy#12 is an alias whose text was narrowed.
- **New counted obligations (21):**
  - 8 full: history.actions#6 (refuted), history.committed#16 (bounded), settings.default-modifiers#12, transaction.flag-sources#10, transaction.flag-sources#12 (refuted), uninstall.cleanup#7, uninstall.cleanup#8, uninstall.cleanup#10 (refuted).
  - 10 partial: devtools.authoring.module-version#7, history.committed#15, history.committed.marking#3 and #4, lifecycle.custom#9, module.env#12, module.processor.file-copy#15 and #16, repo.oras.integrity#7, uninstall.cleanup#9.
  - 3 absent: history.actions#7, lifecycle.module-action#3, transaction.flag-sources#11.
- **New aliases (3):** load.options#11 -> deps.lockfile#1, load.options#12 -> transaction.opt-out#4, module.processor.file-copy#14 -> build.copy.overlap#1.
- **Re-aliased (1):** module.processor.file-copy#13 -> module.processor.file-copy.protection#9.

Totals:
- full -> partial: **125** (122 retained, 3 scope).
- partial or absent -> full: **3** (2 retained, 1 scope).
- New counted obligations: **21** (8 full, 10 partial, 3 absent).

Accounting check: 572 kept + 3 promoted + 8 new = 583 full.

The full -> partial rate on pre-existing obligations is 125/697 (17.9%). This matches review-71's sample estimate of 18% (Wilson interval 9–31%).

## 2. How the re-audit was run

The protocol is [AUDIT-PROTOCOL.md](AUDIT-PROTOCOL.md). Every full obligation got four questions:
1. Is the deciding value computed from the state and inputs the obligation names?
2. Does a theorem, neighbour assertion or rejected control exercise each stated case?
3. Does a mutation of the deciding product branch, as modelled, fail a cited check?
4. Do fixture instances cover every stated case? If so, the grade is marked instance-scoped.

One interpretation applied across all groups: a constant or table that transcribes a literal constant or table in the product's own deciding code counts as computation, provided the product line is cited. Examples are `oras_delete_enabled() = true` for `PublishService.cls` `return 1`, each processor's `SupportsSync()` override, `FollowRedirect=1`, and a status discarded by `do ResolveChildren`. A table that summarises emergent behaviour (transaction scope, a loop guard, a call sequence) counts as written down.

The 697 obligations were split into seven page groups (G1–G7), each audited by a forked sub-agent writing ledgers under `audit/`. I wrote the shared mutation runner and ran 17 mutations myself (group M).

| group | pages | full audited | kept | partial |
|---|---|---|---|---|
| G1 | history, transactions-and-dev-mode, data-policy | 94 | 86 | 8 |
| G2 | load, install, update, uninstall | 109 | 95 | 14 |
| G3 | lifecycle, resources | 99 | 76 | 23 |
| G4 | package-sources, download-security, repositories (concepts and operations), artifact-download | 92 | 75 | 17 |
| G5 | developer-tools, build-and-publish, package-commands | 94 | 66 | 28 |
| G6 | testing-policy, versions, dependencies, manifest | 100 | 83 | 17 |
| G7 | inspection, web-applications, packaging-deployed-code, namespaces, settings, ipm-setup | 109 | 92 | 17 |

[apply.py](apply.py) rebuilds the census records from `fca3ed0f`. It applies the ledgers, then the review-71 repairs on top, then writes [changes.json](changes.json). It is deterministic and safe to re-run. [summarize.py](summarize.py) prints the tables in this handoff.

### My re-judgement of fork samples

I re-opened the declarations and mutation diffs for 3 to 7 entries per group. All agreed with the forks except the one review-71 disagreement below.
- **G1:** history.actions#1 (G1-c01 killed), transaction.flag-sources#1 (G1-m33), history.committed#8 (r3 controls 03 and 13).
- **G2:** uninstall.class-delete-error#1, uninstall.dependents#3 (transcriptions of a product WARNING-and-continue and of `quit $$$OK`), load#1 (crate control), load.refusal#2 (crate inspection).
- **G3:** lifecycle.module-action#1 (G3-m22), module.invoke#2 (G3-m29), lifecycle.only#1 (G3-m06, a by-compute kill), lifecycle.installer#4 partial.
- **G4:** repo.remote.download#1 and #2 (m57, m58), redirect#1 and #2 (m124, m125), artifact.filter#1–#4 partial (m06, m08, m10 survive).
- **G5:** devtools.sync#3 (iter32 controls recorded as rejected), sync#12 (per-class literal), partial-effects#5 (product literal), module-version#4 (m12), build.copy.overlap#1 (m24), generate.resources#7 (instance-scoped), external-identity#2 partial (the constant ignores `use_external`, and no export writer is modelled).
- **G6:** module.requirement.values#3 stays full, against review-71. `requirement_neighbors` asserts both sides: line 186 (a non-Health instance gives NeedHealth) and line 180 (a Health instance gives none). G6-m71 and m72 are both killed.
- **G7:** namespace.enable#3 and mapping-results#4 (m01, m04), module.webapp.provider-status#2 (m70; the provider outcome is an environment input), inspect.show#3 partial (m60 adds an invented dependency, and the product's `%IsA` filter has no counterpart).

### Support among the 583 remaining full grades

| support | count |
|---|---|
| mutation-checked (a mutation run in this iteration was killed) | 477 |
| control (an existing recorded control mutates the deciding computation and is rejected) | 92 |
| inspection | 14 |
| of which instance-scoped | 130 |

The 14 inspection-only grades are the weakest fulls. A master who wants stricter grading should regrade these first:
- 10 rest on ipm-load-history crate cases that could not be mutated without cargo: load#2, load.dependencies#1, load.refusal#2/#3/#4, load.refusal.manifest#2, load.refusal.requirements#3, load.outcome.dev.early-failure#2, load.status#2, load.status.after-commit#1.
- 3 rest on recorded explorer runs that were not re-run: dev-mode#4, dev-mode.keep-on-failure#3, dev-mode.not-propagated#1.
- history.survives-rollback#1: the mutation would need a restructured machine.

Per-page support and the instance-scoped flags are in the census records (`support`, `instance_scoped`, and the basis tail "iteration-72 rule-4a re-audit (Gn): ...").

### Mutations

610 runs, logged in `mutations/{M,G1..G7}.jsonl`. Each record holds the model hash, the diff, the obligations it bears on, the verdict and the failing locations.

| | killed | survived | invalid |
|---|---|---|---|
| M | 13 | 4 | 0 |
| G1 | 51 | 3 | 0 |
| G2 | 57 | 3 | 0 |
| G3 | 80 | 6 | 1 |
| G4 | 121 | 23 | 3 |
| G5 | 39 | 8 | 2 |
| G6 | 87 | 9 | 5 |
| G7 | 91 | 4 | 0 |
| total | 539 | 60 | 11 |

Notes on the counts:
- **By-compute kills:** the first `mutate.py` logged a failed `by (compute)` assertion as invalid, because Verus prints no results line in that case. [reclassify.py](reclassify.py) re-read the scratch logs and turned 62 of those records into kills. G1 also reclassified its own earlier records with `verdict_raw`, and G4 kept its notes in `G4-reclassified.json`.
- **The 11 invalid runs** are typos, shell quoting or by-compute recursion limits. Each was re-run as a corrected mutant.
- **Benign survivors:** a few of the 60 are sanity no-ops or equivalent mutants (G4-m87/m98/m129, G6-m55/m52b, G1-m40). Each ledger names them.
- **Flawed mutant M-m15:** it also dropped the manifest write on success, so its kill is not meaningful. M-m16 is the corrected version, and it survived.

Mutations I ran:

| id | model | change | verdict | bears on |
|---|---|---|---|---|
| M-m01 | iter6 workflows.rs | `global_path` ignores a declared Directory | survived | module.global.roundtrip#1 -> partial |
| M-m02 | iter6 workflows.rs | `creates_file` skips zero-size entries (invented branch; the product size==0 branch has no counterpart) | killed | build.archive.empty-file#1 stays partial |
| M-m03, M-m04 | iter48 cpfmerge.rs | CustomPhase ignored / custom CPF merges in any phase | killed | lifecycle.custom#2 -> full |
| M-m05 | playbook settings.rs | publish ignores -repo | killed | build.publish#2 -> full |
| M-m06 | iter22 reinstall.rs | reinstall of a not-installed module enters install before refusing | killed | history.actions#6 (refuted) |
| M-m07 | verus loadtx.rs | `<Defaults>` win over a parent/command value | killed | transaction.flag-sources#10, #12 |
| M-m08 | playbook uninstall.rs | mapped code deleted without -global | **survived** | uninstall.cleanup#1 -> partial |
| M-m09 | playbook uninstall.rs | mapped code never deleted, even with -global | killed | uninstall.cleanup#1 |
| M-m10 | playbook uninstall.rs | `record_delete` keeps the stored manifest | killed | uninstall.cleanup#8 |
| M-m11 | playbook uninstall.rs | -r dependency removal also deletes its manifest | killed | uninstall.cleanup#10 (refuted) |
| M-m12 | playbook uninstall.rs | Global data deleted even with Preserve | **survived** | uninstall.cleanup#2 -> partial |
| M-m13 | playbook uninstall.rs | name-only global data deleted | killed | uninstall.cleanup#2 |
| M-m14 | verus owners.rs | success sets the record only | killed | module-version#1 -> full |
| M-m15 | verus owners.rs | flawed (also changed the success branch) | killed | superseded by M-m16 |
| M-m16 | verus owners.rs | a rewrite failure keeps the new record | **survived** | module-version#7 partial |
| M-m17 | iter50 defmods.rs | an empty -set clears the store | killed | settings.default-modifiers#12 |

The meaningful survivors from the forks, each tied to a partial, are listed in the [appendix](#surviving-mutants).

## 3. Review-71 repairs applied

- **A1 named obligations, part-B extras and C1.** All are now partial:
  - module.parameter.cycles#1, build.archive.empty-file#1, transaction.rollback-scope#7, load.status.after-commit#4
  - module.env#4 and #10, module.lifecycle-class#6 (#7 is its alias), module.global.roundtrip#1, inspect.list-installed#1
  - uninstall.cleanup#5 and #6, devtools.authoring.module-version#3 and #5, history.committed#4, devtools.generate.resources#2
- **A2 (grades too strict), after opening the declarations.**
  - **build.publish#2: absent -> full.** `publish_target` with `find_name`, `repo_refines_publish_named` under premise PE, M-m05 killed. A disabled named registry is outside the proved domain.
  - **update#2: absent -> partial, check refuted.** versions.rs computes the install choice. With prereleases enabled it picks 2.0.0-beta.1 over 2.0.0. The Update -> Install routing is source reading only.
  - **history.actions#2: absent -> partial,** narrowed to attempts that reach the History init. Two obligations split out:
    - history.actions#6: reinstall of a not-installed module throws before any row. Full and refuted, M-m06 killed.
    - history.actions#7: update refusals throw before UpdateInit (Main.cls:4415-4423). Absent, no model, left for iteration-74.
  - **lifecycle.custom#2: absent -> full.** Uses cpfmerge.rs, composed with lifecycle.custom#4 for the phase list without -only.
- **A3 (ontology contradicted).**
  - **history.committed.marking.** Split into #1 (load: full, crate controls 01–03 r3 and ISOK-REDUNDANT), #3 (install and update: partial) and #4 (uninstall: partial). For #4, `history.rs::finalize` states the as-built `Committed || ISOK` derivation, but no theorem exercises a row that never had SetCommitted.
  - **lifecycle.module-action.** Split into #2 (shortcuts other than update) and #3 (update as a shortcut: absent, contradicted by Main.cls:1118).
  - **namespace.enable#11 and settings#7** stay absent, with the as-built contradiction now recorded in their gaps (no model exists to refute them).
  - **inspect#3:** the gap now notes that -versions also lifts the platform filter.
- **Part B.**
  - **lifecycle.custom:** #9 added for two custom names (partial). #4 went partial in the audit because its -only case is unexercised.
  - **repo.oras.integrity:** #1 adds the explicit-tag case and becomes partial. #7 is the B4 working assumption (partial). #3 went partial in the audit, since digest-only and size-only checks survive (G4-m67, m68).
  - **transaction.flag-sources:**
    - #1 is split: the shell site stays in #1 and the LoadNewModule site becomes #10.
    - #11 is the journaling restore (absent).
    - #12 is B5 "Defaults apply whenever installed", refuted by `merge_key`.
    - #8 regains "a failure may leave a partial install".
  - **uninstall.cleanup:**
    - #1 keeps code only and becomes partial (M-m08). The other parts split into #7 record (control), #8 manifest (M-m10), #9 baseline (table, partial) and #10 -r manifest (refuted, M-m11).
    - #2 adds Preserve to its cases and becomes partial (M-m12).
  - **history.committed:** #8 is narrowed to NoTransaction. #16 covers NoJournal (bounded by the nojournal explorer run, control 03 over txms 1 and 2). #15 is the successful dependency under a successful parent (partial).
  - **devtools.authoring.module-version:** #1 is narrowed to success and becomes full (M-m14). #7 is the failure rollback (partial, M-m16). #6's gap now notes that only CheckOut is called.
  - **load.options:** B6 becomes alias #11 and the B8 transaction flags become alias #12. #9's alias is kept, with the imprecision recorded.
  - **module.env:** #2 drops "unset gives empty". #8 is split into intended (#8) and as-built C:D14 (#12). The B7 evidence sentence is noted in `unit_notes`.
  - **module.processor.file-copy:** B2 becomes alias #14, the B8 failed-copy sentence becomes #15 and the third sentence becomes #16 (both partial), #12 is narrowed, and #13 is re-aliased to protection#9.
  - **settings.default-modifiers:** #10 is split. #12 covers "-set without -D stores nothing" (full, M-m17).

Receipts. iter41/settings.rs (6/0) and iter50/defmods.rs (6/0) were re-run into `receipts/` and are cited by every obligation that uses them. The same was done for iter48/cpfmerge.rs, iter34/versions.rs and iter30/phases.rs, which the repairs cite. The runner is [run_verus.py](run_verus.py) and the index is `receipts/index.jsonl`.

## 4. Coverage per page after the re-audit

| page | full before | full after | coverage before | coverage after | delta |
|---|---|---|---|---|---|
| architecture/lifecycle | 49 | 37 | 0.731 | 0.536 | -0.195 |
| architecture/package-sources | 51 | 39 | 0.823 | 0.619 | -0.204 |
| concepts/data-policy | 19 | 17 | 0.559 | 0.500 | -0.059 |
| concepts/download-security | 12 | 11 | 0.923 | 0.846 | -0.077 |
| concepts/repositories | 11 | 11 | 0.688 | 0.688 | 0 |
| concepts/testing-policy | 36 | 33 | 0.837 | 0.767 | -0.070 |
| concepts/transactions-and-dev-mode | 23 | 23 | 0.605 | 0.561 | -0.044 |
| concepts/versions | 19 | 19 | 0.905 | 0.905 | 0 |
| features/artifact-download | 17 | 12 | 0.630 | 0.444 | -0.185 |
| features/build-and-publish | 25 | 19 | 0.532 | 0.404 | -0.128 |
| features/dependencies | 12 | 8 | 0.522 | 0.348 | -0.174 |
| features/developer-tools | 66 | 48 | 0.710 | 0.511 | -0.199 |
| features/history | 52 | 51 | 0.520 | 0.481 | -0.039 |
| features/inspection | 30 | 25 | 0.588 | 0.490 | -0.098 |
| features/install | 18 | 17 | 0.750 | 0.708 | -0.042 |
| features/load | 47 | 40 | 0.627 | 0.533 | -0.093 |
| features/manifest | 33 | 23 | 0.611 | 0.418 | -0.193 |
| features/package-commands | 3 | 1 | 1.000 | 0.333 | -0.667 |
| features/packaging-deployed-code | 13 | 12 | 0.684 | 0.632 | -0.053 |
| features/resources | 50 | 40 | 0.568 | 0.444 | -0.124 |
| features/uninstall | 30 | 28 | 0.769 | 0.651 | -0.118 |
| features/update | 14 | 12 | 0.583 | 0.500 | -0.083 |
| features/web-applications | 23 | 14 | 0.719 | 0.438 | -0.281 |
| operations/ipm-setup | 1 | 1 | 0.111 | 0.111 | 0 |
| operations/namespaces | 21 | 21 | 0.656 | 0.656 | 0 |
| operations/repositories | 1 | 1 | 0.167 | 0.167 | 0 |
| operations/settings | 21 | 20 | 0.583 | 0.541 | -0.043 |

Pages that moved most:
- **package-commands (-0.667):** a three-obligation page. ipm.shell.forms#1 and #2 are per-form outcome tables.
- **web-applications (-0.281):** constant `validate_dispatches`, outcome-table rollback, identity `compile_only` and upgrade.
- **package-sources (-0.204):** ORAS neighbours never exercised.
- **developer-tools (-0.199):** sync and generate inputs and tables.
- **lifecycle (-0.195):** per-kind phase tables, -only unexercised.
- **manifest (-0.193):** env loader, cycle guard, expressions.

Pages with no movement: namespaces, versions and concepts/repositories (every full grade was mutation-checked), plus ipm-setup and operations/repositories (one full each).

## 5. Ontology text repairs exposed (not applied)

1. **lifecycle.module-action P1:** `update` is not a phase shortcut. Main.cls:1118 dispatches it to Main::Update before the standard-phase check.
2. **namespace.enable B9:** as built, `enable -community` catches each step's failure, keeps going and reports no error (Main.cls:3564-3574). State the as-built behaviour beside the working assumption.
3. **history.committed.marking P1:** false for uninstall. Uninstall never calls SetCommitted, and Finalize derives Committed from the final status (AbstractHistory.cls:131).
4. **inspect B1:** `-versions` also lifts the platform filter (Utils/Module.cls:938).
5. **settings T3:** HistoryRetain is not seeded at setup (Installer.cls:110-124), so `config get` shows empty, not 0.
6. **update P1 "latest available":** with prereleases enabled the install choice follows SQL order and can pick 2.0.0-beta.1 over 2.0.0.
7. **history.actions P1/B1 "refused attempts are recorded":** false for update refusals (Main.cls:4415-4423) and reinstall of a not-installed module (Main.cls:2685-2687).
8. **lifecycle.custom B1:** state that two custom names together run nothing and the command succeeds.
9. **transaction.flag-sources P1:** state the precedence between default modifiers and `<Defaults>`. As built, default modifiers win, because the shell merges them into the command data. settings.default-modifiers B6 already says so, so link to it.
10. **uninstall.cleanup P1:** three omissions.
    - Mapped code is skipped unless `-global` (Lifecycle/Base.cls:375-378).
    - Global data is kept when Preserve is set (Default/Global.cls:76-80).
    - `-recurse` deletes dependency records but not their stored manifests (contradicts "its stored manifest").
11. **module.parameter:** state that an undeclared `-D` name is passed to the module and its phases. load.options B8's alias target covers only overriding a declared value.
12. **module.env B7:** the sentence "both direct merge and the actual file loader have receipts" is evidence and belongs in metadata.
13. **module.processor.file-copy B8:** the sentence about creating the destination directory and partial OS copy errors being separate effects reads as a scope note. Either move it to metadata or state the behaviour.
14. **devtools.authoring.module-version B4:** only CheckOut is called (Main.cls:4080-4084), so the source-control hand-off may be false as built. Confirm natively before rewording.
15. **settings.analytics** (G7): values such as "-1" or "2abc" keep analytics on, contrary to "any value that is not a positive number stops usage data".
16. **build.publish.partial-effects** (G5): the ORAS PublishService check is the placeholder `return 1 // TODO` (PublishService.cls:64-70). Say so.
17. **build.test.outcome** (G6): "FAILED lines list the same rows that are counted" looks false, because the printer filters on method status (Test/Manager.cls:145-171). Needs native confirmation.
18. **dev-mode.overwrite-guard** (G1, review-71 D2): the guard applies only to top-level installs.
19. **devtools.generate.resources** (review-71 C4/C6): the C:D72 trigger wording and readability. This is iteration-73 scope and is only listed here.

## 6. Model-repair candidates (cheap fixes that would restore full), ranked by consequence

Ranking puts security or data loss first, then the number of obligations restored, then cost. Each line names the obligations it restores.

1. **ORAS integrity neighbours (security).** Add size-only and digest-only mismatch neighbours to `iter2/registry.rs::verify_layers` and `iter68/as_built.rs::observe`, plus a matching writable-root case. Restores repo.oras.integrity#3 and repo.oras.archive-boundary#2, and supports integrity#7.
2. **Uninstall data-loss conditions.** Add two assertions to `playbook uninstall.rs`: mapped code survives without -global (kills M-m08), and Global data survives with Preserve (kills M-m12). Restores uninstall.cleanup#1 and #2. Needs the ontology repair in §5 item 10.
3. **Artifact filter neighbours** in `iter3/artifacts.rs`: property-only accepted, `type=file` accepted, an array condition refused, an unknown key refused. Restores artifact.filter#1–#4.
4. **Web application neighbours.**
   - `iter57/cspapp.rs`: create from None, and no DeployPath means no copy. Restores module.csp.lifecycle#1 and #3.
   - `iter8/webapps.rs`: NamespaceDefault, and a path outside the CSP root. Restores devtools.generate.webapps#3 and #4.
5. **Filesystem cache.** Add a `build_cache([valid, invalid-name])` neighbour asserting `!ok`. Restores repo.filesystem#4 and the refutation in #8.
6. **Environment loader.** Add a readable-file success witness to `env_config.rs`. Restores module.env#4. #10 also needs an input-independent failure.
7. **Lifecycle one-liners.** Each is one or two asserts:
   - phases.rs: -only for a custom phase alone, and two custom names. Restores lifecycle.custom#4, and #9 with ontology item 8.
   - workflows.rs: declared and explicitly empty Directory. Restores module.global.roundtrip#1.
   - cpfmerge.rs: When=Before. Restores module.processor.cpf#2.
   - `primary_after_set(1,2,true)==2`. Restores lifecycle.source-control.editor#7.
   - `resolve(None,true)==Module`. Restores module.lifecycle-class#1.
8. **Remaining single-assert fixes:**
   - `importcmd.rs` recursion and missing path: devtools.authoring.import#1 and #3.
   - ORAS `tag_for` source-artifact neighbours: repo.oras.tag-decode#1 and #3.
   - Publish-cache `provider_called` and the probe input: repo.publish-cache#1 and #3.
   - An `oras_available` status proof: repo.oras#5.
   - YAML and TOON accepted: settings.report-format#1.
   - Package-only test target: build.test.selection#1.
   - Health=On on an interoperability-enabled namespace: module.requirement.values#4.
   - Defined macro and the interpreter value: module.parameter.expressions#6 and #8.
   - Test-scoped requirement outside load phases: deps#4.
   - `get_add` keeps completion: update.steps.records#1.
   - Cleanup warning: load.dependencies.local-repo#6.
   - `base` is None after delete: devtools.sync#5.
   - Project neighbour: devtools.sync.compile-after-resolve#5.
   - `with_pre` and `with_build` applied: devtools.authoring.module-version#2.
   - Credentials through `remote_authorized`: build.publish.partial-effects#4.
   - Download requested on an absent folder: artifact.local-content#1.
9. **Citation only, to be checked by the master.** load.same-version#1 could cite `update_steps.rs::refuses`. G2-m12 is killed there, because an equal version is neither older nor newer. I did not apply this, because the obligation's cited `reloading.rs` path differs.
10. **Larger, high consequence.** These are not cheap:
    - A transaction-scope writer model in place of the failkeep, webapp and load outcome tables. Restores transaction.rollback-scope#2 and #7, module.processor.cpf-effects#1, module.webapp.failure#1 and #2, and load.outcome#7 and #8. This is QUEUE gap 1.
    - The `resolve_bounded` loop and its guard. Restores module.parameter.cycles#1 and expressions#13.
    - A crate case pinning a dependency's Committed. Restores history.dependencies.parent-rollback#2 and history.committed#15 (G1-c13 survived).
    - The module-version transaction rollback: module-version#7.
    - The generate `in_tests` derivation and UnitTest fields: generate.resources#1 and #2 (iteration-73 scope).

## 7. Limitations

- **Single-auditor verdicts.** Each obligation was judged by one fork. I re-judged 3 to 7 per group, and all of them agreed.
- **Product-literal exception.** I applied the exception for literal product tables consistently, but it is an interpretation of rule 4a. Under a stricter reading, about ten more fulls would become partial: repo.remote.download.redirect#1, artifact.failure#2, build.publish.partial-effects#5, devtools.sync#12, uninstall.child-error#1, uninstall.class-delete-error#1, uninstall.dependents#3, inspect.orphans.identifier-case#3, deps.lockfile#5 and settings.default-modifiers#2.
- **Inspection-only fulls.** The 14 inspection-only fulls (§2) were not mutation-checked. Crate controls are cited from the recorded r3 receipts. Their failing lists stop at the first `compute_only` failure, so a control kill on one lemma does not show which other lemmas would fail.
- **Mutations outside `mutate.py`.** G1's crate mutations used a scratch `main.rs` harness without the explorer crate. G6's include-target runs used a copy of `mutate.py` with `--run`. Both are recorded in the jsonl, with a `harness` field or `run_target`.
- **Scratch copies.** Mutation scratch copies under `/tmp/ipmcov72-*` are not committed. The diffs in the jsonl are enough to reproduce each run.
- **No native runs.** Ontology contradictions marked source-only (namespace.enable B9, settings T3, update refusals, module-version B4) stay unconfirmed.

## 8. Cost

- Wall time: 02:35–03:15 UTC, about 40 minutes.
- Fork sub-agent tokens: G1 320k, G2 358k, G3 249k, G4 311k, G5 315k, G6 300k, G7 300k, about 2.15M in total.
- Builder session tokens and currency are not visible to me. `cost.py` showed this session as running with no receipt yet.

## Files

- `before-counts.json`: counts at `fca3ed0f`.
- `AUDIT-PROTOCOL.md`: the rules each audit group applied.
- `audit/*.json`: 27 ledgers, one entry per full obligation.
- `mutate.py`, `mutations/*.jsonl`, `reclassify.py`, `mutations/G4-reclassified.json`: the mutation runner, its logs and the reclassification.
- `apply.py`, `changes.json`, `summarize.py`: the census rebuild and its tables.
- `run_verus.py`, `receipts/`: receipt re-runs.

## Appendix: full -> partial by page and surviving mutants

### Full -> partial by page (* = scope change by a review-71 repair)

- **architecture/lifecycle** (13): lifecycle#3, lifecycle.alias#2, lifecycle.chain#4, lifecycle.chain#5, lifecycle.custom#4, lifecycle.initialize#3, lifecycle.installer#4, lifecycle.source-control#6, lifecycle.source-control.editor#6, lifecycle.source-control.editor#7, module.lifecycle-class#1, module.lifecycle-class#4, module.lifecycle-class#6
- **architecture/package-sources** (12): repo.filesystem#4, repo.filesystem#8, repo.oras#5, repo.oras.archive-boundary#2, repo.oras.archive-boundary#4, repo.oras.integrity#1*, repo.oras.integrity#3, repo.oras.selection-modes#4, repo.oras.tag-decode#1, repo.oras.tag-decode#3, repo.publish-cache#1, repo.publish-cache#3
- **concepts/data-policy** (2): module.resource#2, repo.origin.same-version#1
- **concepts/download-security** (1): repo.remote.download.integrity#3
- **concepts/testing-policy** (3): build.test-output.counted-rows#4, build.test.outcome#4, build.test.selection#1
- **concepts/transactions-and-dev-mode** (2): transaction.rollback-scope#2, transaction.rollback-scope#7
- **features/artifact-download** (5): artifact.filter#1, artifact.filter#2, artifact.filter#3, artifact.filter#4, artifact.local-content#1
- **features/build-and-publish** (7): build.archive.empty-file#1, build.publish.external-identity#2, build.publish.partial-effects#4, build.test#1, build.test#5, build.test#6, build.test-output#7
- **features/dependencies** (4): deps#4, deps.installed#5, deps.installed#7, deps.lockfile.content#2
- **features/developer-tools** (19): devtools.authoring#7, devtools.authoring.import#1, devtools.authoring.import#3, devtools.authoring.module-version#2, devtools.authoring.module-version#3, devtools.authoring.module-version#5, devtools.exec#1, devtools.exec#2, devtools.generate.resources#1, devtools.generate.resources#2, devtools.generate.resources#5, devtools.generate.webapps#3, devtools.generate.webapps#4, devtools.sync#10, devtools.sync#15, devtools.sync#19, devtools.sync#5, devtools.sync#9, devtools.sync.compile-after-resolve#5
- **features/history** (3): history.committed#4, history.dependencies.parent-rollback#2, history.read.filter#6
- **features/inspection** (5): inspect.help#2, inspect.help#3, inspect.list-installed#1, inspect.list-installed#6, inspect.show#3
- **features/install** (1): reinstall.history#2
- **features/load** (7): load.dependencies.local-repo#6, load.outcome#5, load.outcome#7, load.outcome#8, load.same-version#1, load.status.after-commit#3, load.status.after-commit#4
- **features/manifest** (10): module.env#10, module.env#11, module.env#4, module.identity.document#6, module.identity.document#7, module.parameter.cycles#1, module.parameter.expressions#13, module.parameter.expressions#6, module.parameter.expressions#8, module.requirement.values#4
- **features/package-commands** (2): ipm.shell.forms#1, ipm.shell.forms#2
- **features/packaging-deployed-code** (1): module.deployed.project-load#6
- **features/resources** (10): module.global.roundtrip#1, module.processor.cpf#2, module.processor.cpf-effects#1, module.processor.deepsee#2, module.processor.deepsee#3, module.processor.deepsee#4, module.processor.deepsee#5, module.processor.file-copy#4, module.processor.file-copy.unix-overlay#1, module.processor.localized-messages.legacy#5
- **features/uninstall** (5): uninstall.all#4, uninstall.cleanup#1*, uninstall.cleanup#2*, uninstall.cleanup#5, uninstall.cleanup#6
- **features/update** (2): update.steps#1, update.steps.records#1
- **features/web-applications** (9): module.csp.lifecycle#1, module.csp.lifecycle#3, module.csp.lifecycle#4, module.webapp#10, module.webapp.failure#1, module.webapp.failure#2, module.webapp.package#2, module.webapp.upgrade#1, module.webapp.validation#1
- **operations/settings** (2): settings.analytics#2, settings.report-format#1

### Surviving mutants

| id | model | change | obligations |
|---|---|---|---|
| G1-m04 | history.rs | ConstructSQLWhere does not stop at an absent value | history.read.filter#6 |
| G1-c13 | model.rs | a dependency LoadNewModule does not mark its log committed after its final commit | history.dependencies.parent-rollback#2 |
| G1-m40 | load_general.rs | an operator Commit at the crash prompt also marks the row committed | history.committed#13 |
| G2-m07 | temp_repo.rs | cleanup never warns about a missing repository row | load.dependencies.local-repo#6 |
| G2-m38 | ownership.rs | registering an existing key for another module resets its completion | update.steps.records#1 |
| G2-m49 | versions.rs | the <n>.x range no longer checks the major version | install#3 |
| G3-m02 | phases.rs | Reload step also compiles | lifecycle#3 |
| G3-m12 | phases.rs | custom phase alone with -only fails | lifecycle.custom#4 |
| G3-m25 | record.rs | no LifecycleClass resolves to a named class | module.lifecycle-class#1 |
| G3-m43 | composite.rs | setting a class name never changes the primary | lifecycle.source-control.editor#7 |
| G3-m73 | cpfmerge.rs | Before CPF merges at both points | module.processor.cpf#2 |
| G3-m87 | resources.rs | (control) class match always true | module.processor.defaults#1 |
| G4-m06 | artifacts.rs | property-only filter refused (drops @property alternative) | artifact.filter#1 |
| G4-m08 | artifacts.rs | array condition accepted | artifact.filter#2, artifact.filter#4 |
| G4-m09 | artifacts.rs | unknown key ignored | artifact.filter#2 |
| G4-m10 | artifacts.rs | type=file refused | artifact.filter#3 |
| G4-m32 | artifacts.rs | absent local folder never requests download | artifact.local-content#1 |
| G4-m37 | oras.rs | source tag not encoded | repo.oras.tag-decode#1 |
| G4-m38 | oras.rs | source package without platform also refused | repo.oras.tag-decode#3 |
| G4-m39 | catalog.rs | source package tag refused | repo.oras.tag-decode#3 |
| G4-m64 | registry.rs | gzip result not deciding (junk fixture fixes has_manifest=false) | repo.remote.download.integrity#3 |
| G4-m67 | registry.rs | size check dropped (digest only) | repo.oras.integrity#3 |
| G4-m68 | registry.rs | digest check dropped (size only) | repo.oras.integrity#3 |
| G4-m73 | registry.rs | retained client re-probes the provider | repo.publish-cache#1 |
| G4-m77 | registry.rs | ORAS manager does not probe | repo.publish-cache#3 |
| G4-m84 | owners.rs | refresh merges instead of replacing | repo.filesystem#2 |
| G4-m86 | owners.rs | invalid name no longer fails the refresh (intended skip) | repo.filesystem#4, repo.filesystem#8 |
| G4-m87 | owners.rs | no-op sanity (should survive) | repo.filesystem#4 |
| G4-m98 | duplicates.rs | no-op sanity (should survive) | repo.filesystem.duplicates#3 |
| G4-m105 | temporal.rs | no-downgrade guard removed | repo.filesystem.mutable-source#3 |
| G4-m115 | as_built.rs | root never replaced (match case unexercised?) | repo.oras.archive-boundary#2 |
| G4-m116 | as_built.rs | digest check dropped | repo.oras.archive-boundary#2 |
| G4-m117 | as_built.rs | loader entered after failed validation | repo.oras.integrity#6, repo.oras.archive-boundary#2 |
| G4-m128 | thin_surfaces.rs | 403 with header accepted (spec and body together) | repo.oras#5 |
| G4-m129 | settings.rs | no-op sanity (should survive) | repo.manage#3 |
| G5-m01 | sync.rs | -delete keeps the baseline row of a deleted file (baseline removal dropped) | devtools.sync#5 |
| G5-m02 | sync_compile.rs | deployed-project resources are compiled (from_project dropped from the compile-set guard) | devtools.sync.compile-after-resolve#5 |
| G5-m10 | importcmd.rs | directory import never recurses (recurse flag ignored) | devtools.authoring.import#1 |
| G5-m11 | importcmd.rs | a missing path still selects (loads) items | devtools.authoring.import#3 |
| G5-m14 | owners.rs | -prerelease is ignored by module_version | devtools.authoring.module-version#2 |
| G5-m16 | webapps.rs | NamespaceDefault no longer skipped from generated props | devtools.generate.webapps#3 |
| G5-m17 | webapps.rs | every old path becomes {$cspdir}-relative (CSP-root prefix check dropped) | devtools.generate.webapps#4 |
| G5-m43 | registry.rs | remote delete without credentials | build.publish.partial-effects#4 |
| G6-m15 | public_repairs.rs | a Package-only test resource is refused | build.test.selection#1 |
| G6-m40 | semver.rs | exact intersection keeps build metadata | version.range.snapshot-intersection#1, version.range.snapshot-intersection#2 |
| G6-m55 | installed.rs | unversioned install picks a satisfying offer | deps.installed#4 |
| G6-m58 | dep_resolve.rs | phase-scoped requirement resolved in every command | deps#4 |
| G6-m73 | resources.rs | Health=true fails Interoperability even on an interoperability-enabled namespace | module.requirement.values#4 |
| G6-m77 | attr_evaluator.rs | defined macro also evaluates to nothing | module.parameter.expressions#6 |
| G6-m78 | attr_evaluator.rs | successful expression evaluates to nothing | module.parameter.expressions#8 |
| G6-m94 | env_config.rs | loader always fails, even on a readable file (review-71 calibration) | module.env#4, module.env#10 |
| G6-m52b | semver.rs | operator on a partial version kept (run against semver_native.rs) | version.range.partial#4, version.range.partial#1, version.range.partial#3 |
| G7-m18 | public_repairs.rs | YAML and TOON no longer accepted | settings.report-format#1 |
| G7-m30 | settings.rs | YAML and TOON codes not valid | settings.report-format#1 |
| G7-m76 | cspapp.rs | load only updates an existing application, never creates one | module.csp.lifecycle#1 |
| G7-m77 | cspapp.rs | load copies into the deploy files even without DeployPath | module.csp.lifecycle#3 |
| M-m01 | workflows.rs | global_path ignores a declared Directory (always gbl/) | module.global.roundtrip#1 |
| M-m08 | uninstall.rs | uninstall deletes mapped code even without -global | uninstall.cleanup#1 |
| M-m12 | uninstall.rs | Global data deleted even when Preserve is set | uninstall.cleanup#2 |
| M-m16 | owners.rs | a manifest rewrite failure after the record save keeps the new record version (no rollback); success branch un | devtools.authoring.module-version#7 |
