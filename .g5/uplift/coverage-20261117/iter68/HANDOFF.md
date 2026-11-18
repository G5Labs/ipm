# Iteration 68 (builder): obligation census, chunk A

Product `571e04a1` unchanged. Ontology, models and findings were not edited. No Docker was used.

Records:
- [developer-tools](../census/blocks/features__developer-tools.json)
- [manifest](../census/blocks/features__manifest.json)
- [resources](../census/blocks/features__resources.json)
- [packaging-deployed-code](../census/blocks/features__packaging-deployed-code.json)
- [web-applications](../census/blocks/features__web-applications.json)

Generated counts are in [COUNTS.md](../census/COUNTS.md). Receipts are in [receipts/index.jsonl](receipts/index.jsonl), written by [run_verus.py](run_verus.py) with pinned Verus `0.2026.09.27.3cf1832` x86.

Checks:
- `census.py count` reports no ERROR or WARN.
- `census.py selftest` detected all 9 controls.
- Freshness gate `common30/freshness.py` is clean for iterations 30-67.
- Every `proved` or `refuted` obligation cites a receipt: an iteration `run.json` / `run_all.json`, the playbook or extension results, or an iteration-68 re-run.

## 1. Counts

Chunk A has 58 blocks (56 enumerated, 2 structural) and 240 units. There are 280 counted obligations plus 28 aliases.

| Grade | Count |
|---|---|
| Full | 174 |
| Partial | 63 |
| Absent | 43 |
| Unassessed | 0 |

Assessed coverage is 174/280 = 0.621. With nothing unassessed, the upper bound equals it. Whole-foundation coverage stays unmeasured, because chunks B-D are not enumerated.

| Check | Count |
|---|---|
| Proved | 166 |
| Bounded | 0 |
| Refuted | 8 |
| Unchecked | 63 |
| None | 43 |

All 174 full obligations carry a passing or refuting check. Of these, 166 are proved and 8 are refuted.

Check convention: `check` is over the obligation's full stated scope (README rule 7).
- Every partial is `unchecked`. Its partial model has a current receipt, which is listed in `evidence`.
- Every absent is `none`.
- An as-built defect claim whose model proves the as-built behaviour is full/proved.
- An intended promise the model contradicts is full/refuted. There are 8 of these:
  - devtools.sync#3, dependency order
  - devtools.sync#21, the re-detect working assumption
  - module-version#4, C:D13
  - two on resources: deepsee wildcard and FileCopy Overlay=0
  - three on manifest: parameter#1 C:D41, a literal-values case and a cycles case

A spec definition that encodes a case but is exercised by no theorem, neighbour assertion or rejected control was graded partial.

Excluded units (33 in total), summarised:
- **Developer-tools (6):** page and list headings; a navigation link; the working assumption restated from devtools.generate.resources#5; the webapps evidence-scope disclosure.
- **Resources (15):** the 9 units of the link-only `module.processor` parent; headings; scope statements; file-copy B2, which restates build.copy.overlap (chunk D).
- **Manifest (5):** restatements of load.refusal.requirements and transaction.flag-sources (owners not yet enumerated); C:Q2 rationale; a scope statement; an open policy question.
- **Packaging (3):** a pointer to project-load; an evidence statement; the unapplied repair C:S13.
- **Web (4):** two as-built working assumptions restating owned rules; history; a premise.


### developer-tools.g5.md

Obligations 87: full 55, partial 16, absent 16, unassessed 0 (coverage 0.632). Checks: proved 52, bounded 0, refuted 3, unchecked 16, none 16. Aliases 12, excluded units 6.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| devtools | structural | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | N/A |
| devtools.sync | enumerated | 14 | 4 | 2 | 0 | 12 | 0 | 2 | 4 | 2 | 1 | 0 | 0.70 |
| devtools.sync.retry | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 1.00 |
| devtools.sync.compile-after-resolve | enumerated | 4 | 1 | 0 | 0 | 4 | 0 | 0 | 1 | 0 | 0 | 0 | 0.80 |
| devtools.sync.empty-hash | enumerated | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 1 | 1 | 0 | 0 | 0.33 |
| devtools.sync.test-paths | enumerated | 3 | 0 | 1 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 0.75 |
| devtools.sync.skipped-tracking | enumerated | 4 | 0 | 2 | 0 | 4 | 0 | 0 | 0 | 2 | 0 | 0 | 0.67 |
| devtools.authoring | enumerated | 2 | 0 | 4 | 0 | 2 | 0 | 0 | 0 | 4 | 3 | 2 | 0.33 |
| devtools.authoring.generate | enumerated | 3 | 1 | 3 | 0 | 3 | 0 | 0 | 1 | 3 | 2 | 0 | 0.43 |
| devtools.authoring.import | enumerated | 4 | 1 | 0 | 0 | 4 | 0 | 0 | 1 | 0 | 0 | 0 | 0.80 |
| devtools.authoring.module-version | enumerated | 4 | 2 | 0 | 0 | 3 | 0 | 1 | 2 | 0 | 0 | 0 | 0.67 |
| devtools.generate.resources | enumerated | 2 | 4 | 1 | 0 | 2 | 0 | 0 | 4 | 1 | 1 | 0 | 0.29 |
| devtools.generate.webapps | enumerated | 4 | 2 | 0 | 0 | 4 | 0 | 0 | 2 | 0 | 0 | 1 | 0.67 |
| devtools.generate.cors | enumerated | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.50 |
| devtools.scripting | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 4 | 1 | 1.00 |
| devtools.scripting.file-sequence | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| devtools.exec | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| devtools.init | enumerated | 2 | 0 | 1 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 0.67 |

### manifest.g5.md

Obligations 54: full 33, partial 15, absent 6, unassessed 0 (coverage 0.611). Checks: proved 30, bounded 0, refuted 3, unchecked 15, none 6. Aliases 3, excluded units 5.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| module.identity.document | enumerated | 6 | 1 | 0 | 0 | 6 | 0 | 0 | 1 | 0 | 0 | 0 | 0.86 |
| module.dependency | enumerated | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0.00 |
| module.requirement | enumerated | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 3 | 1 | 0.00 |
| module.requirement.values | enumerated | 5 | 3 | 1 | 0 | 5 | 0 | 0 | 3 | 1 | 0 | 0 | 0.56 |
| module.parameter | enumerated | 2 | 1 | 1 | 0 | 1 | 0 | 1 | 1 | 1 | 0 | 1 | 0.50 |
| module.parameter.expressions | enumerated | 9 | 3 | 1 | 0 | 9 | 0 | 0 | 3 | 1 | 0 | 1 | 0.69 |
| module.parameter.literal-values | enumerated | 2 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1.00 |
| module.parameter.cycles | enumerated | 1 | 2 | 0 | 0 | 0 | 0 | 1 | 2 | 0 | 0 | 0 | 0.33 |
| module.list.delimiter | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| module.env | enumerated | 6 | 4 | 1 | 0 | 6 | 0 | 0 | 4 | 1 | 0 | 1 | 0.55 |

### resources.g5.md

Obligations 88: full 50, partial 24, absent 14, unassessed 0 (coverage 0.568). Checks: proved 48, bounded 0, refuted 2, unchecked 24, none 14. Aliases 12, excluded units 15.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| module.processor | structural | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 9 | N/A |
| module.processor.defaults | enumerated | 3 | 4 | 0 | 0 | 3 | 0 | 0 | 4 | 0 | 0 | 0 | 0.43 |
| module.processor.default-sweep | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| module.placement | enumerated | 5 | 3 | 1 | 0 | 5 | 0 | 0 | 3 | 1 | 0 | 1 | 0.56 |
| module.global.roundtrip | enumerated | 2 | 0 | 1 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 1 | 0.67 |
| module.processor.deepsee | enumerated | 4 | 0 | 1 | 0 | 3 | 0 | 1 | 0 | 1 | 0 | 0 | 0.80 |
| module.processor.interoperability | enumerated | 1 | 2 | 1 | 0 | 1 | 0 | 0 | 2 | 1 | 0 | 0 | 0.25 |
| module.processor.localized-messages | enumerated | 2 | 2 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0 | 0.50 |
| module.processor.localized-messages.legacy | enumerated | 2 | 2 | 1 | 0 | 2 | 0 | 0 | 2 | 1 | 0 | 0 | 0.40 |
| module.processor.copy | enumerated | 5 | 0 | 1 | 0 | 5 | 0 | 0 | 0 | 1 | 0 | 0 | 0.83 |
| module.processor.file-copy | enumerated | 4 | 4 | 1 | 0 | 3 | 0 | 1 | 4 | 1 | 4 | 1 | 0.44 |
| module.processor.file-copy.protection | enumerated | 6 | 0 | 2 | 0 | 6 | 0 | 0 | 0 | 2 | 1 | 0 | 0.75 |
| module.processor.file-copy.unix-overlay | enumerated | 2 | 1 | 1 | 0 | 2 | 0 | 0 | 1 | 1 | 0 | 0 | 0.50 |
| module.processor.cpf | enumerated | 4 | 1 | 1 | 0 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0.67 |
| module.processor.cpf-effects | enumerated | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 1 | 1 | 0.50 |
| module.system-setting | enumerated | 2 | 1 | 1 | 0 | 2 | 0 | 0 | 1 | 1 | 1 | 0 | 0.50 |
| module.python | enumerated | 0 | 1 | 2 | 0 | 0 | 0 | 0 | 1 | 2 | 4 | 1 | 0.00 |
| module.python.explicit-wheel | enumerated | 6 | 2 | 0 | 0 | 6 | 0 | 0 | 2 | 0 | 1 | 0 | 0.75 |

### packaging-deployed-code.g5.md

Obligations 19: full 13, partial 4, absent 2, unassessed 0 (coverage 0.684). Checks: proved 13, bounded 0, refuted 0, unchecked 4, none 2. Aliases 1, excluded units 3.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| module.deployed | enumerated | 4 | 3 | 1 | 0 | 4 | 0 | 0 | 3 | 1 | 0 | 1 | 0.50 |
| build.makedeployed | enumerated | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | 0 | 0.00 |
| module.deployed.project-load | enumerated | 6 | 0 | 0 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| build.package.deployed-status | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 2 | 1.00 |

### web-applications.g5.md

Obligations 32: full 23, partial 4, absent 5, unassessed 0 (coverage 0.719). Checks: proved 23, bounded 0, refuted 0, unchecked 4, none 5. Aliases 0, excluded units 4.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| module.webapp | enumerated | 8 | 2 | 1 | 0 | 8 | 0 | 0 | 2 | 1 | 0 | 1 | 0.73 |
| module.webapp.upgrade | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| module.webapp.validation | enumerated | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.50 |
| module.webapp.provider-status | enumerated | 2 | 1 | 1 | 0 | 2 | 0 | 0 | 1 | 1 | 0 | 1 | 0.50 |
| module.webapp.failure | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| module.webapp.package | enumerated | 2 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 0 | 1 | 0.67 |
| module.csp.directory | enumerated | 3 | 0 | 1 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 0.75 |
| module.csp.lifecycle | enumerated | 4 | 0 | 1 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 0.80 |


## 2. Reused versus new; withdrawn claims

All grades were judged in this iteration after opening each cited declaration. "Reused" means a metadata, handoff or project-map link was confirmed to state the property and both neighbours. Most full grades are reused links. Every partial and absent grade, and the following links, are new judgments:

- **`playbook-uplift/models/verus/sync_baseline.rs`** (17 verified, 4+ controls killed in place). No developer-tools block links it. It carries:
  - devtools.sync.retry#1 (SyncFailed retry after an exact revert)
  - devtools.sync#16 (`load -skip-sync`)
  - devtools.sync#18 (outside-sync revert, F-SYNC-1)
  - devtools.sync#19 (a test failure does not undo the commit)
  - the refutation of devtools.sync#21
- **iter26 `sync_summary.rs`**: carries the whole-namespace vs named summary activation for compile-after-resolve#4.
- **Resources:**
  - iter63 `cleanup.rs` for FileCopy mappings that outlive uninstall
  - iter2 `boundaries.rs::cpf_*`
- **Manifest:**
  - iter36 `installer.rs` for parameter#1/#2
  - iter34 `versions.rs` for dependency#1
  - iter15 `oras.rs` for deployed#7
  - `attr_evaluator.rs` for webapp#8

Withdrawn or narrowed. In each case the model does not establish the clause.

- **iter53 `generator.rs::grouping_neighbors`** (devtools.authoring.generate / devtools.generate.resources metadata): credit is withdrawn for the emitted group name and narrowed to partial for grouping. See section 4, item 1.
- **Project-map link `Storage/FileHash.cls::StampModule -> iter7 inputs.rs::stamp`**: `stamp` models only the helper. The StampModule skip of empty computed hashes (empty-hash P2) is absent.
- **iter6 `workflows.rs::{test_item, template_status, application_item}`** (generate.resources): these give membership and status only.
  - The UnitTest attributes are not modeled.
  - "Fails before resources are added" is not modeled.
  - The unloadable and unsupported-extension skips are never exercised.
- **iter8 `webapps.rs`** (generate.webapps): prompt precedence is proved for JWT only, with no CORS keys. The emission of the FileCopy resource is not modeled.
- **iter4 `sync_compile.rs`** does not establish "recompiles only what is out of date" (devtools.sync#2). That comes from the compiler flag `u`, a provider.
- **verus `owners.rs::module_version`**: all-or-nothing is assumed by the spec (state unchanged unless ok). The rollback after a record save is never exercised.
- **Resources:**
  - `interopres.rs` and `locmsgs.rs` take replace-vs-merge as a load premise.
  - `thin_surfaces.rs` `copies_before`/`copies_after` select only by Defer.
  - `rollback_*` and `rollback_cpf` define filesystem survival.
  - iter3 `resources.rs` gives only the pairwise order, with no list sort and no stability.
  - iter59 `mappings.rs` does not show GlobalScope visibility.
  - iter60 `replace_and_merge` lacks the mapped-domain qualifier.
- **Manifest:**
  - `requirement_failures` has no `Interoperability="enabled"` neighbour and no Health-unset neighbour.
  - iter8 and iter5 hard-code webapp export handling and validation dispatch as constants. Export handling is kept partial. Validation is graded full only as the as-built claim.

## 3. Parent blocks

No parent obligation in chunk A is credited by composition.

| Parent | Beyond its children | Disposition |
|---|---|---|
| devtools | Nothing; summary lead plus a restated working assumption | Structural |
| devtools.sync | Lead (hash-driven load, update-only recompile) and 12 bullets: order, -delete/-test, manifest reload, scan scope, kinds, layout, skip-sync, outside-sync, test-after-commit, two working assumptions | Graded directly; B8 aliases retry#1 |
| devtools.authoring | arrange (order, stored document vs file, added fields, C:D49, C:D50) and dependency-analyzer | generate, init and module-version bullets alias children; the rest graded here |
| devtools.scripting | Accepted run-from-file shape (txt, or JSON object with a commands array) | Graded; dry-run, exec and import alias children |
| module.processor | Nothing; link-only | Structural |
| module.processor.file-copy | "Never deletes system folders" | Kept partial, not composed. protection#2 and #4 entail it only if the caller's root list (`DatabaseDirectories` plus system and IPM directories) is complete and normalization holds; neither is modeled |
| module.python | Requirements Reload timing, Python version gate, pip selection | Open; wheel bullets alias explicit-wheel |
| module.requirement | Version and IPMVersion/PythonVersion checks | Open (absent); three bullets alias requirement.values |
| module.parameter | Delivery, `-D`, dotted names, repeated filling | Graded directly; children not composed |
| module.deployed, module.webapp | Their own rules | Graded directly; children add separate outcomes |

## 4. Ten most consequential partial/absent obligations

Ranked by outcome-changing decision, persistence, failure/recovery and interactions.

1. **`generate` grouping: emitted group name and leftovers.**
   - Obligations: devtools.generate.resources#8 (absent), #7 (partial) and #2 (partial).
   - The ontology promises:
     - devtools.authoring.generate P1: "source resources grouped by package".
     - devtools.generate.resources B3: "grouped by directory and their shared top-level package", with "no general proof".
     - Nothing about leftover individual resources.
   - Product behaviour:
     - Decider: `Storage/ModuleTemplate.cls::GroupClasses` keys `packages("/"_Directory, topLevel)`.
     - Group name: seeded with the first class's full package and narrowed by `SharedPackage` (longest common dotted prefix). So G53.A.X + G53.B.Y gives G53.PKG, G53.A.X + G53.A.Y gives G53.A.PKG, and a lone A.B.C gives A.B.PKG.
     - Writers: `AddPackages` writes `pkg:<dir>:<pkg>` with Name `<pkg>.PKG` and a Directory attribute. For tests it writes Name `/<tests><dir>/`, UnitTest, Package and Phase=test.
     - Leftovers: `ReadResources` merges the leftover items (package-less classes, .mac/.inc/.int) as individual resources named by the loaded item. The leftover test-folder items are never merged, so they are silently dropped.
     - Readers: `ProcessResources` and `SaveFile`, then module.xml.
   - What iteration53 `generator.rs` does:
     - `shared_resource(first_top, second_top) = first_top == second_top`.
     - `grouping_neighbors` proves only `shared_resource(1,1)` and `!shared_resource(1,2)`, a Boolean on top-level identity.
     - Control `no-package-sharing` (returning false) is rejected.
   - What it does not do:
     - No directory key, so same top-level classes in two directories would wrongly be "shared".
     - No name computation. Replay field `t2.pkgG53A` is the negation of the same Boolean, so G53.PKG and G53.A.PKG are not distinguished.
     - No leftovers and no test attributes.
   - Closing contrast: a model of `GroupClasses`/`SharedPackage`/`AddPackages` over (directory, package) multisets that proves:
     - {G53.A.X, G53.B.Y} in one directory gives exactly G53.PKG.
     - {G53.A.X, G53.A.Y} gives G53.A.PKG.
     - The same top level in cls/ and src2/ gives two resources with Directory attributes.
     - A package-less class or a .mac stays an individual resource.
     - A .mac in the tests folder produces no resource.
2. **devtools.sync#20 (absent): whole-namespace sync status.**
   - Members: `Main.cls::Sync` loop, `General/Sync/Summary`, `Pipeline.cls::Run` per module.
   - The working assumption says it reports each failure and a summary, then returns OK (open question aggregate-status-policy).
   - Contrast: one failing module out of two. Compare the returned status and that module's failure line and summary entry, versus the all-succeed case.
3. **devtools.authoring.module-version#1 (partial): all-or-nothing persistence.**
   - Members: `Main.cls:4011-4099` transaction, installed record save, `StudioDocument/Module.cls::UpdateVersion`.
   - Contrast: UpdateVersion fails after the record save. The record and the stored manifest must both keep the old version, against the success neighbour.
4. **module.processor.file-copy#6 (absent) and #5 (partial): immediate and deferred copies around a failing Activate hook.**
   - Members: `FileCopy.cls::OnBeforePhase`/`OnAfterPhase` (no formal target), Lifecycle Activate.
   - Contrast: immediate copy then a hook failure leaves the immediate target and no deferred target, versus the success history.
5. **module.processor.file-copy#2 and protection#3: system-folder safety.**
   - The safety depends on `Utils/File.cls::DatabaseDirectories` completeness and `NormalizeForComparison`.
   - Contrast: a case or separator variant, or an unlisted database root, must still be classed protected and survive Clean.
6. **module.requirement#1/#2 (absent): Version, IPMVersion and PythonVersion requirements.**
   - Members: `Storage/SystemRequirements.cls::{CheckVersion, CheckIPMVersion, CheckPythonVersion}`, read by `CheckRequirements`, then the load refusal.
   - Contrast: `Version=">=2099.1"` vs `">=2020.1"` on one instance gives refusal vs success, with both kept in the aggregate.
7. **devtools.sync.compile-after-resolve#3 (partial): stale dependents after a warned compile are not retried.**
   - Members: `Pipeline.cls::SyncCompile` -> `Run` step 9 `FileHash.cls::CommitChanges`.
   - Contrast: a joint model where a resource fails ResolveChildren, the hash commits, and the next sync detects nothing, while the dependents' compiled code stays old. This links the `sync_compile.ok` and `sync_baseline` commit rules.
8. **manifest parameter.expressions#10 (partial): evaluation order.**
   - Member: `Storage/Module.cls::%Evaluate`, which expands macros before placeholders, then system expressions, `{name}` and `#{}`.
   - Contrast: a parameter whose value contains `$$$name` stays unexpanded.
   - The block's B5 text states the opposite order (finding below).
9. **devtools.sync#6 (partial) and #2 (partial): which tests and which compiled items sync actually runs.**
   - Members: `Pipeline.cls::SyncRunTests` and the Test processor; the `SyncCompile` "cku" flag to `LegacyCompat::UpdateSuperclassAndCompile`.
   - Contrast for #6: with -test, a changed test class runs and an unchanged one does not.
   - Contrast for #2: an up-to-date class in the compile set is not recompiled.
10. **module.deployed#6/#8 and makedeployed#1/#3: deployed-code effect and `-recurse`.**
    - Decider: `Lifecycle/Base.cls::%MakeDeployed`, modeled only through `deploy`/`selected`.
    - Contrast: a selected .cls loses its source while an .inc with Deploy="true" keeps it, and a dependency changes only with -recurse.

Next tier, all partial or absent:
- devtools.sync#8 (the not-applied deletion count is reported only on the nothing-to-sync path)
- module.env#6 (per-command configuration clearing)
- webapp#7/#11 and provider-status#3/#4 (NameSpace default, Get/Create failure, %SYS restore)
- devtools.authoring#3-#5 and #8 (arrange order and stored document, dependency-analyzer: no model)
- devtools.authoring.import#5 (source-control notify, load and save order)
- devtools.sync.skipped-tracking#5 (recorder cleared on throw)

## Findings for the master (not acted on)

1. **devtools.generate.resources B3 misstates the group name.** "Shared top-level package" is wrong for groups whose classes share a deeper package, and for single classes: G53.A.X + G53.A.Y gives G53.A.PKG. The page also says nothing about leftover individual resources, or about test-folder leftovers being dropped. This is a source-discovered omission.
2. **devtools.sync B2 is broader than the source.** "Without `-delete`, sync ... reports how many deleted files it did not apply" is unconditional, but `Pipeline.cls:92-97` (and iter32 `sync.rs`) report the count only when nothing was modified.
3. **devtools.sync B12 is refuted.** The working assumption "files that a failed compile, load -dev or reload put on the server are re-detected by the next sync" is refuted by `sync_baseline.rs::sync_witness_failed_compile_then_revert`, and it contradicts B9 on the same page. It should be qualified to "unless the file is reverted to its baseline bytes".
4. **devtools.sync P1 conflicts with compile-after-resolve P1.** "Recompiles only what is out of date" sits beside "recompiles the module's compilable resources". The update-only behaviour is a compiler flag, and the page should say so.
5. **parameter.expressions B5 reverses the evaluation order.** It says lookup runs before macro and expression evaluation, but `%Evaluate` expands macros first, and the block's own metadata agrees with the source.
6. **module.parameter P1 lacks the C:D41 qualification.** iter36 shows a Default never reaches the installer variables.
7. **module.python B2 is over-broad.** Its "`-bypass-py-deps` skips this" covers wheels too, but module.python.explicit-wheel (iter8 `python.rs`) installs explicit wheels under bypass. B2 should be limited to requirements.txt.
8. **Model-strength gaps.** These are not wrong statements, but in each the deciding value is fixed by the fixture or a disjunct goes unexercised:
   - iter3 `requirement_values` never exercises `Interoperability="enabled"`.
   - The `module.processor.cpf` default "before Initialize" is hard-coded in fixture `A()`.
9. **csp.lifecycle "each load prints a deprecation warning" is unmodeled and the timing is unverified.** The source prints it once per module per shell command.
10. **`sync_baseline.rs` is unlinked.** The playbook model is the strongest formal evidence for devtools.sync and devtools.sync.retry, but no block metadata links it. The project map binds it only under origin "extension".

## Limitations and remaining work

- Two builder forks enumerated resources and the manifest/packaging/web-applications pages under the same rules and convention. I enumerated developer-tools and reviewed the fork tallies and their receipt citations. I did not re-judge each fork obligation.
- A full grade means the declared rule distinguishes the cases. It does not mean the ObjectScript body is verified: direct implementation proofs remain 0. Some full grades rest on category-level tables (e.g. nsinit `exec`, comments `arrange`). Each still states the outcome over the deciding attribute.
- The test-paths full grades observe the relative path, and the class-name rendering is native-qualified.
- Iteration-68 receipts re-run only Verus on each file. The iteration controls are cited from the original runs: `proofs/controls`, cases.json and playbook `results/*.json`, whose model hashes still match.
- Chunks B-D remain unenumerated (207 blocks), so whole-foundation coverage is unmeasured.

## 5. Cost

- Wall time: 01:13:50Z to about 01:31Z (UTC clock of the session, about 17 minutes).
- Subagents: the two forks used 188,882 and 222,639 tokens, with 25 and 42 tool calls, over 478 s and 609 s.
- Main-thread token usage is not visible to the builder.
- Verus re-runs took about 1 s each.
