# Iteration 70 (builder): obligation census, chunks B, C and D

Product `571e04a1` unchanged. Ontology, models and findings were not edited. No Docker was used.

Chunks B, C and D are enumerated: 207 blocks on 22 pages, 735 units. With chunk A (iterations 68 and 69), every one of the 265 inventory blocks is now enumerated or structural, so whole-foundation coverage is measured for the first time.

Records:
- B: [load](../census/blocks/features__load.json), [history](../census/blocks/features__history.json), [transactions-and-dev-mode](../census/blocks/concepts__transactions-and-dev-mode.json)
- C: [lifecycle](../census/blocks/architecture__lifecycle.json), [package-sources](../census/blocks/architecture__package-sources.json), [data-policy](../census/blocks/concepts__data-policy.json), [testing-policy](../census/blocks/concepts__testing-policy.json), [versions](../census/blocks/concepts__versions.json), [download-security](../census/blocks/concepts__download-security.json), [repositories](../census/blocks/concepts__repositories.json)
- D: [artifact-download](../census/blocks/features__artifact-download.json), [build-and-publish](../census/blocks/features__build-and-publish.json), [dependencies](../census/blocks/features__dependencies.json), [inspection](../census/blocks/features__inspection.json), [install](../census/blocks/features__install.json), [package-commands](../census/blocks/features__package-commands.json), [uninstall](../census/blocks/features__uninstall.json), [update](../census/blocks/features__update.json), [ipm-setup](../census/blocks/operations__ipm-setup.json), [namespaces](../census/blocks/operations__namespaces.json), [operations/repositories](../census/blocks/operations__repositories.json), [settings](../census/blocks/operations__settings.json)

Generated counts are in [COUNTS.md](../census/COUNTS.md). Receipts: [receipts/index.jsonl](receipts/index.jsonl) (72 runs of 49 model files, pinned Verus `0.2026.09.27.3cf1832` x86, sha256 `2fe2fca3…1bc854`), written by [run_verus.py](run_verus.py).

Checks:
- `census.py count` reports no ERROR or WARN. `census.py selftest` detects all 9 controls.
- Freshness gate `common30/freshness.py` is clean for iterations 30-67.
- Every proved or refuted obligation cites a receipt:
  - an iteration `run.json`/`run_all.json`/`proofs/models.json`, or the playbook, extension and crate results
  - or an iteration-68/70 re-run
- The `ipm-load-history` crate receipts (`results/verus/20261113T222933Z-asbuilt-proofs-r3`, 263 verified; survey `20261113T223110Z-survey-asbuilt-r3`, 5,850 configurations; explorer runs `2234xxZ`) have input hashes equal to the current sources. Only `crates/scripts/final-candidate-batch.sh` differs, and it is not a proof input.
- Playbook `load_general.rs` verifies 39/0 only with `--rlimit 30`, the setting `playbook-uplift/models/verus/controls.py` uses. Four runs at the default rlimit gave 37 verified with 2 rlimit errors. Some of these ran concurrently with other forks, and they stay in the index. The cited log is `receipts/playbook-uplift__models__verus__load_general.rs.rlimit30.log`.

## 1. Counts

Whole foundation (all chunks): 265 blocks (258 enumerated, 7 structural), 1076 counted obligations.

| Grade | Count |
|---|---|
| Full | 697 |
| Partial | 214 |
| Absent | 165 |
| Unassessed | 0 |

Coverage is 697/1076 = 0.648, and with nothing unassessed the upper bound equals it.

| Check | Count |
|---|---|
| Proved | 619 |
| Bounded | 18 |
| Refuted | 60 |
| Unchecked | 214 |
| None | 165 |

Full with a passing (proved or bounded) check: 637. The 60 refuted obligations are full grades whose intended promise or working assumption a model contradicts.

Chunks B+C+D alone: 790 obligations (full 512, partial 154, absent 124), coverage 0.648. Checks: proved 442, bounded 18, refuted 52, unchecked 154, none 124. Aliases 77, excluded units 125.

Check convention (same as iteration 68):
- Every partial is `unchecked`, with the partial model's receipt in `evidence`.
- Every absent is `none`.
- An as-built claim whose model proves the as-built behaviour is full/proved.
- An intended promise or working assumption that a model contradicts is full/refuted.
- `bounded` is used only for exhaustive explorer or survey runs with recorded domains:
  - the crate survey: 5,850 configurations, at most two leaf dependencies, single fault
  - the crate explorer runs, and the crate `cases.json` L2 computations
- A spec definition that no theorem, neighbour assertion, executed native case or rejected control exercises is partial.

| Chunk | Page | Blocks | Units | Full | Partial | Absent | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| B | load | 22 | 85 | 47 | 15 | 13 | 28 | 12 | 7 | 15 | 13 | 13 | 21 | 0.627 |
| B | history | 24 | 99 | 52 | 21 | 27 | 36 | 2 | 14 | 21 | 27 | 12 | 10 | 0.520 |
| B | transactions-and-dev-mode | 10 | 45 | 23 | 10 | 5 | 17 | 4 | 2 | 10 | 5 | 7 | 11 | 0.605 |
| C | lifecycle | 15 | 44 | 49 | 13 | 5 | 46 | 0 | 3 | 13 | 5 | 5 | 1 | 0.731 |
| C | package-sources | 12 | 57 | 51 | 4 | 7 | 50 | 0 | 1 | 4 | 7 | 1 | 14 | 0.823 |
| C | data-policy | 12 | 37 | 19 | 7 | 8 | 18 | 0 | 1 | 7 | 8 | 0 | 11 | 0.559 |
| C | testing-policy | 11 | 35 | 36 | 4 | 3 | 35 | 0 | 1 | 4 | 3 | 4 | 5 | 0.837 |
| C | versions | 7 | 16 | 19 | 2 | 0 | 14 | 0 | 5 | 2 | 0 | 1 | 1 | 0.905 |
| C | download-security | 5 | 14 | 12 | 0 | 1 | 12 | 0 | 0 | 0 | 1 | 1 | 1 | 0.923 |
| C | repositories | 6 | 17 | 11 | 3 | 2 | 10 | 0 | 1 | 3 | 2 | 0 | 5 | 0.688 |
| D | artifact-download | 6 | 22 | 17 | 5 | 5 | 16 | 0 | 1 | 5 | 5 | 2 | 2 | 0.630 |
| D | build-and-publish | 12 | 37 | 25 | 17 | 5 | 25 | 0 | 0 | 17 | 5 | 5 | 4 | 0.532 |
| D | dependencies | 5 | 28 | 12 | 8 | 3 | 12 | 0 | 0 | 8 | 3 | 1 | 8 | 0.522 |
| D | inspection | 10 | 40 | 30 | 11 | 10 | 30 | 0 | 0 | 11 | 10 | 7 | 1 | 0.588 |
| D | install | 7 | 25 | 18 | 4 | 2 | 16 | 0 | 2 | 4 | 2 | 2 | 6 | 0.750 |
| D | package-commands | 3 | 5 | 3 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 1 | 1 | 1.000 |
| D | uninstall | 13 | 33 | 30 | 6 | 3 | 25 | 0 | 5 | 6 | 3 | 6 | 6 | 0.769 |
| D | update | 7 | 27 | 14 | 5 | 5 | 12 | 0 | 2 | 5 | 5 | 0 | 9 | 0.583 |
| D | ipm-setup | 3 | 9 | 1 | 3 | 5 | 1 | 0 | 0 | 3 | 5 | 1 | 1 | 0.111 |
| D | namespaces | 8 | 28 | 21 | 6 | 5 | 17 | 0 | 4 | 6 | 5 | 3 | 6 | 0.656 |
| D | repositories | 2 | 5 | 1 | 2 | 3 | 0 | 0 | 1 | 2 | 3 | 0 | 0 | 0.167 |
| D | settings | 7 | 27 | 21 | 8 | 7 | 19 | 0 | 2 | 8 | 7 | 5 | 1 | 0.583 |
| **B total** | | 56 | 229 | 122 | 46 | 45 | 81 | 18 | 23 | 46 | 45 | 32 | 42 | 0.573 |
| **C total** | | 68 | 220 | 197 | 33 | 26 | 185 | 0 | 12 | 33 | 26 | 12 | 38 | 0.770 |
| **D total** | | 83 | 286 | 193 | 75 | 53 | 176 | 0 | 17 | 75 | 53 | 33 | 45 | 0.601 |
| **B+C+D** | | 207 | 735 | 512 | 154 | 124 | 442 | 18 | 52 | 154 | 124 | 77 | 125 | 0.648 |

Per-block tables for all 22 pages are in the appendix at the end of this file.

Blocks with no applicable model, where every counted obligation is absent:
- repo.read-only
- history.committed.side-effects
- history.delete.global-unfiltered
- load.path.archive
- load.write-check
- setup
- namespace.legacy

Blocks with no full grade:
- lifecycle.hooks, module.identity, namespace, history.keep-on-ipm-uninstall
- repo.prereleases, dev-mode.not-propagated.local-snapshot, artifact
- build.verify, build.publish, deps.conflict, history.committed.uninstall
- history.read, history.read.find, history.read.global
- install.refusal, load.path, load.refusal.path
- update.dev-mode, update.requirements
- setup.installer, repo.default-registry, settings.install-flags

Excluded units (125), summarised:
- **Headings, leads, list headings and definitions (about 25).** For example, path P1, options P1, outcome P1, `history` P1, the `fields` list heading, module P1, module.identity P1/B1, version P1, dev-mode P1, and the update and install refusal leads.
- **Navigation and pointers to an owner on the same page (about 20).** For example, load core/dependencies/local-repo/refusal pointers, transaction B2/B3, namespace enable B5/B6, unmap B1, and filesystem B6.
- **Scope, evidence or provenance statements (about 35).** For example, the duplicates, mutable-source and selection-modes scope lines, temp-copy B2, status-bytes B3, counted-rows B2, rollback-scope B6, partial-batch B3, mapped-reader B2, reinstall.selection B4, and update.steps.records B2.
- **Undecided policy, open questions, proposed or unapplied repairs, roadmap items and checker guidance (about 20).** For example, outcome B3 (dependency `<Defaults>`), history.retention B4/B5/B6, survives-rollback B1/B5, flag-sources B1, overwrite-guard B1, include-refresh B2, deps.lockfile B3, update.steps B4, and mapping-results B3.
- **Working assumptions that only list alternatives (about 5).** These are filesystem B8, oras.integrity B4, build-readers B5, remote.download B3 and artifact B2.
- **Restatements of an owner on another page in these chunks (22).** The owner is named in each reason; see section 2.

## 2. Reused versus new; withdrawn claims

Seven forked builders enumerated the pages under the same rules and conventions:
- load
- history
- transactions, lifecycle and versions
- package-sources, repositories, download-security and artifact-download
- data-policy, testing-policy and inspection
- build-and-publish, dependencies, install, package-commands and update
- uninstall, ipm-setup, namespaces, operations/repositories and settings

Every grade was judged in this iteration after opening its cited declaration. "Reused" means a metadata, coverage-map or project-map link was confirmed to state the property and both neighbours. Most full grades are reused links. Every partial and absent grade is a new judgment, and so are the following links.

New credit from models that no block metadata links:
- **Playbook `update_steps.rs`.** It covers:
  - update#1, update.refusal#1 and update.steps#1-#7, with the refutations of the developer-mode seeding and busy-lock working assumptions
  - load -dev/-nodev and the stored-mode working assumption (load.options#2)
  - downgrade in every flag combination
  - -force not running update steps
  - the refutation of dev-mode#6 ("-dev applies to update", F-INST-5)
- **Playbook `load_general.rs`.** It covers:
  - normal-mode atomicity with any number of dependencies
  - status honesty (load#1)
  - crash honesty (history.record#3)
  - dependency records lost (history.survives-rollback#3)
  - transaction#1/#2
- **Playbook `uninstall.rs`.** It covers:
  - uninstall#3 (non-transactional partial removal)
  - uninstall.global and uninstall.recurse
  - uninstall.ipm#2
  - the F-UNIN-4 refutation of uninstall.dependents#1
- **Playbook `ns_mapping.rs` and `settings.rs`.**
  - ns_mapping covers namespace.mapped-refusal (F-NSP-5 refutation) and enable#10 (F-NSP-7).
  - settings covers repo.manage#3 (F-REPO-3 refutation), settings.default-modifiers#2 (F-REPO-5), settings.writes#4/#5 and report-format YAML/TOON.
- **Playbook `scopes.rs` and `dep_resolve.rs`.** These cover build.test#2, module.resource.scope#1/#2, deps#4 and build.package#5.
- **Extension and iteration models.**
  - `owners_native.rs` native_109-115: repo.filesystem#2 (a removed module is dropped on refresh).
  - iter12 `namespaces.rs::repo_visible`: repo#1.
  - iter8 `catalog.rs`: search platform filtering.
  - `history.rs::details`: namespace-scoped IDs.
  - iter16 `provenance.rs`: Action and source on install and load rows.
  - iter25 `developer_sources.rs::overwrite_guard`: dev-mode.overwrite-guard. The coverage map's "Not modeled" is stale.
  - the semver `follows` post branch: version.post-release.
  - iter50 `defmods` composed with `loadtx.merge_key`: flag-sources#1.

Withdrawn or narrowed, because in each case the model does not establish the clause:
- **Load:**
  - The G crate takes path validity (`path_ok`), manifest validity (`mf`) and system requirements (`sysreq`) as input bits. load.refusal.path#1-#3, refusal.manifest#1 and refusal.requirements#1 are partial.
  - `temp_repo.rs::skip_create` is never exercised, so local-repo B1 is partial. `cleanup` never asserts row removal, so the P1 removal is partial.
  - `attr_evaluator.rs::after_message` assumes the commit rather than deriving it, so after-commit B1 and after-install-message#1 are partial.
  - `loadtx.rs`, which the dependencies metadata cites, is not credited there, because its rules belong to transaction.flag-sources.
- **History:**
  - iter38 `render.rs` proves the reader, but the "0.0.0-0 until known" writer is unmodeled (history.fields partial).
  - `history.rs::cli_global` is never exercised.
  - `programmatic_global` rows carry id and namespace, so "the schema has neither" is only a comment.
  - `render.rs` unfinished durations: every fixture row is ended.
  - The coverage-map "phases" row: the crate keeps only a phase count and the last status.
  - `history.rs::purge` takes the matched rows as input and never exercises retain=0 or ran-today (history.retention#1-#3, #5).
  - `persist_after_delete` takes "save recreates the row" as an input (delete#9).
- **Transactions and lifecycle:**
  - schooks `root_hook_not_reached` and composite `per_item` are constants.
  - installer.rs hard-codes the rollback and the empty DatabaseName (lifecycle.installer#5/#8 partial).
  - `failkeep.external` is an input table without lock files, pip or remote calls (rollback-scope#1 partial).
  - `loadtx` takes `Write.journal` as an input (flag-sources#5 partial).
  - iter30 never exercises the Test chain (chain#3).
  - The Invoke side-mismatch case is never exercised (invoke#7).
- **Sources:**
  - `owners.rs::successful_refresh_holds_outer_lock` / `build_cache.lock_held` were already withdrawn in iter2 `withdrawals.json`. repo.filesystem#6 is credited to `boundaries.rs` instead.
  - `thin_surfaces.rs` supports only the repo.oras availability bullet. Namespace, Basic/token, name or external name, and error text are absent.
  - `as_built.rs` `provider_extracts` is an input ("client unpacks before check" partial), and so is "row and history unchanged".
  - iter3 `artifacts.rs`: MissingServer non-dev, activate after a skipped download, and scalar property data are not exercised or not representable.
  - repairs4/public_repairs `repo_visited`: `enabled` and `registry_match` are never false in any neighbour (search-order#1/#2 partial).
- **Policy and inspection:**
  - `thin_surfaces::inspection_preserves_stores` is an identity by definition, so the inspect no-change frame is partial.
  - iter46 `scoped_hidden` is a tautology over an input, and `keywords_shown` is a constant.
  - iter51 `shows_build` is a constant.
  - iter54 `listed` is a constant 39, and name case is an input.
  - iter2 `requirements` only exercises the comment line.
  - report_identity: the device-fallback branch is never exercised.
  - `public_repairs::suite_path` is never applied to a Suite containing a slash.
- **Build, dependencies and update:**
  - iter47 `summaries_agree` is an identity ("same counts" partial).
  - `thin_surfaces` `lock_written`/`lock_in_module_root` restate their inputs.
  - iter6 `copy_ok` is a Boolean, so "before deleting anything", nested files and alias detection are partial.
  - iter10 `export_phases` never exercises "does not remove an already requested phase".
  - iter2 `verify_plan` gives only the setup plan.
  - The crate explorer's passing L1 runs use the conforming domain, which the check itself filters, so they are not credited as bounded.
- **Uninstall and operations:**
  - iter12 `own_cleared` does not show that a later enable shares the repositories (mapping-results#3).
  - iter12 `new_normal_load` sets row := ok by definition (mapped-reader#2).
  - iter2 `uninstall_all_step` takes store retention as an input and has no traversal (uninstall.all#1-#3).
  - `owners.rs::zlang_one` never exercises the "no block yet" branch (language-extension#1).
  - iter9 takes case-insensitivity as a premise (package-ownership#2).

Composition credited (2):
- load.refusal.requirements#2 from module.requirement.values#7 (chunk A). The SYSNamespace decision fails exactly outside %SYS, and G verdict 5 turns a requirement failure into a refusal.
- deps.lockfile#2 from deps.lockfile.content#1 and #3.

### Corrections made in my review

The review covered at least three full grades per page, or all of a page's full grades where it has fewer than three: 66 re-judged grades. It also included a mechanical sweep of every full grade in chunks B-D whose cited declarations are all spec or exec definitions.

- **versions page: placeholder locator.** Nine full grades cited `semver_native.rs::native_0`, which asserts only that `parse("1").major == "1"`. The grades stand, because specific neighbour assertions exist. I replaced the locator in each:
  - order#1: native_297/493
  - order#2: native_143/234
  - order#3: native_242/255 (`1.0.0` follows `1.0.0+snapshot`, not the reverse)
  - order#5: native_152/165/210/195
  - post-release#1: native_339/430/143
  - range#1: native_799/800/766 plus `witnessed_product_distinctions`
  - range#3: native_1327-1329/766
  - range.partial#2: native_766-769
  - range.partial#5: native_1327-1329 against native_1354-1356, which contrasts `2024.1` with `2024.12`
- **settings#14 was a second count of build.test-output#3.** "TestReportFormat sets the default format of test summaries" restates the format selection that build.test-output#3 owns. It is now an alias, which removes one absent from the count.
- **Added exercising locators.** inspect.webapp.detail#1/#2 now cite iter7 `directory_and_reader_neighbors`, the neighbour asserts at lines 136-140. load.options#2 now cites `update_steps.rs::inst_refines`, the general refinement theorem that makes `want_dev` binding.
- **Sweep results.** The sweep flagged seven full grades with only definitional locators. All seven are exercised:
  - The thin_surfaces exec functions (`oras_available`, `dependency_status`, `inspection_choice`) were executed on native cases with rejected controls (`results/thin_surfaces.json`).
  - history.locked#3 is an explorer refutation (`asbuilt-l1-end`).
- **Re-judged and kept, with reasons:**
  - module.resource#2 rests on a frame-by-construction `load_changed`. Its control `old_class_defined: false` is rejected (iter52 `cases.json`), so it stays full.
  - build.publish.external-identity#2 rests on a constant `archive_manifest_name`, but the control `external-renames-manifest` is rejected.
  - setup.language-extension#2 rests on a single computed witness, which shows a generated block replaced and a user line kept, idempotently.
  - repo.search-order#9 quantifies over every value of `sort`.
  - committed#8 uses `theorem_commit_sound` (changed modules imply Committed=1) over every transaction mode, plus case c27.

Cross-page restatements (22 units) are excluded with the owner named. Each owner block in chunks B-D now has obligations:
- load.outcome B7/B8/B11/B12 → transaction.flag-sources (#1-#9)
- options B6 → deps.lockfile
- dependencies B3 → history.dependencies
- refusal B1 and install.refusal B4 → dev-mode.overwrite-guard
- refusal B5 and update B1 → history.actions
- same-version B1 → repo.origin.same-version
- data-policy same-version B3 → load.same-version
- testing-policy exit B1 → ipm.shell.forms
- repo.search-order B6 → repo.origin.installed
- `history` P1 → history.namespace
- keep-on-failure B1 → load.outcome.dev.early-failure
- lifecycle B3 → uninstall
- uninstall B2 → history.committed.uninstall
- deps B3/B4 → repo.oras.selection-modes/archive-boundary
- deps.conflict B1 → load.outcome
- install B4 → load.options
- install.refusal B3 → namespace.mapped-refusal
- update.refusal B4 → load.refusal.downgrade

Chunk A exclusions now have owners:
- load.refusal.requirements (#1 partial, #2 full by composition, #3 refuted)
- transaction.flag-sources (#1-#9)
- build.copy.overlap (#1 full, #2-#5 partial)

Aliases into chunk A:
- options -env → module.env#1
- -bypass-py-deps and -extra-pip-flags → module.python.explicit-wheel#2/#4
- -skip-sync → devtools.sync#16
- -D → module.parameter#2
- devtools.sync.test-stop #2/#3/#4 → devtools.sync#6/#19 and skipped-tracking#6
- rollback-scope → file-copy#7/#11 and cpf-effects#1/#2
- uninstall.cleanup → file-copy#1/#3, module.placement#2 and module.python#6

## 3. Parent blocks

Two parent obligations are credited by composition: load.refusal.requirements#2 and deps.lockfile#2. Every other parent obligation is graded directly or kept open.

| Parent | Beyond its children | Disposition |
|---|---|---|
| history, module (data-policy), build, ipm, version | Nothing beyond a lead, a list heading or a definition | Structural, with owners named |
| load | Successful load (module present at manifest version, OK) and dependency loading | Graded directly (bounded by the crate survey) |
| load.refusal | No-write guarantee across every refusal trigger | Graded directly and not composed. The children own the triggers, but the parent's no-write rule fails on its own for manifest refusals under NoTransaction, so composing would be unsound |
| load.core, load.dependencies, load.outcome, load.outcome.dev, load.status | Their own rules (core flow, dependency order, outcome table, dev outcome, status meaning) | Graded directly. Bullets that restate children are aliased (outcome B1, B6; status B2) |
| history.committed | Committed=1 implies the writes are kept | Kept open. Per-commit marking plus `theorem_commit_sound` do not entail it, because the early NoTransaction marker breaks it (D12) |
| history.survives-rollback, history.dependencies, history.read, history.delete | Their own rules | Graded directly |
| transaction, dev-mode, dev-mode.not-propagated | Atomic default load; -dev semantics; propagation | Graded directly. dev-mode B1 aliases source-files#1 |
| lifecycle | Phase order, Compile/Activate, Clean | Graded directly. B1 aliases initialize#1, B4 aliases custom#7/#8, B3 is excluded to the uninstall owner |
| lifecycle.installer, lifecycle.source-control, lifecycle.hooks, version.range, version.range.combine | Their own rules | Graded directly |
| repo.filesystem | Scan, refresh, lock and the invalid-name rule | Graded directly. Children are only pointed to |
| repo | Per-namespace repository lists | Graded directly |
| repo.remote.download | URL following and credential forwarding | Graded directly. Redirect, integrity and token are the children's |
| artifact | "Activate copies into InstallDirectory" | Partial. Export and reuse clauses alias local-content#4/#1 |
| inspect | Frame (no store change), search, information, show lookup, -python, info, zn listing | Graded directly. 7 bullets alias children |
| build.test.outcome | Aggregate outcome | Graded directly. B4 aliases the exit child |
| deps, install, update, reinstall | Their own rules | Graded directly. reinstall P1's first clause and B1 alias reinstall.selection |
| deps.lockfile | Lock file reflects installed identities | #2 composed from deps.lockfile.content#1/#3: the writer opens installed modules by name and keeps one keyed entry, so the file's entries are the installed identities |
| uninstall, uninstall.dependents | Removal, the name-required rule, non-transactional failure; dependent protection | Graded directly. The dependents intent is refuted (F-UNIN-4). B1 restates the as-built child |
| namespace.enable, setup, settings | Their own rules | Graded directly. Two enable bullets alias mapping-results |

## 4. Fifteen most consequential partial/absent obligations in chunks B-D

Ranked by consequence: outcome-changing decisions, persistence, failure and recovery, and interactions. Members are at `571e04a1`. "Contrast" is the observation pair that would close the gap.

1. **History rows and Committed for install, update and uninstall.**
   - Obligations: history.actions#2 (absent), history.record#2 (absent), history.committed#3 (partial), history.committed.uninstall#1 (partial).
   - Every history model (the crate, `load_general.rs`, `provenance.rs`) represents `Main:Load` only. Install, update and uninstall write the same durable rows, through different init points and a different commit structure.
   - Members:
     - deciders: `Main.cls::Install` (`HistoryTemp.InstallInit` at :2566, before the name check), `Main.cls::Update` (`UpdateInit` at :4438), and the uninstall path
     - writers: `General/AbstractHistory.cls` Finalize and `SetCommitted` in `Utils/Module.cls::LoadNewModule`
     - reader: `history find` / `General/History.cls`
   - Contrast:
     - `install nosuchmodule` leaves one error row with Committed=0.
     - An update that fails in `ApplyUpdateSteps` after its commit reads Committed=1 with the new version kept.
     - An uninstall that fails after deleting resources reads a row whose Committed matches the deletions.
2. **Archive and git load paths, and the write check. No model applies.**
   - Obligations:
     - load.path#2-#4 and load.path.archive#1-#3 (all absent)
     - load.write-check#1 (absent)
     - install.package#1 (absent)
   - Members: `Main.cls::LoadInternal` 2447-2467 (extension test, D15), 2498-2510 (unpack), the clone step, `Utils/Module.cls:228-233` (write check, repository archives only), `Utils/Module.cls::LoadModuleFromArchive`.
   - Contrast:
     - `x.tgz` loads while `x.tar.gz` is refused.
     - The unpack goes to `<ModuleRoot>/<name>/<version>/`.
     - The temporary folder is gone after both a failed and a successful load.
     - A read-only ModuleRoot is refused before any file is written, for both an archive load and a repository install.
3. **IPM installer and setup. The setup page has one full grade out of nine obligations.**
   - Obligations: setup#1 (absent); setup.installer#1-#7 (four absent, two partial).
   - Members: `preload/cls/IPM/Installer.cls` setup, ZPMInit (setting seeding), ZPMLoad and MapIfLegacy. The settings defaults (settings#6, #7, #12) depend on the same seeding and are absent for the same reason.
   - Contrast:
     - Re-running the same-version installer changes nothing, while a newer one replaces IPM.
     - `config set analytics 0` followed by a reinstall keeps 0.
     - After setup, zpm has DeveloperMode=0 and no automatic mappings.
4. **NoJournal `<Defaults>` on a dependency escapes the parent rollback.**
   - Obligations: transaction.flag-sources#5/#7/#9 (partial or absent); history.committed#14 (absent).
   - `loadtx.rs` takes `Write.journal` as an input, and the crate has no NoJournal-dependency configuration (D14/Q-22).
   - Members: `Utils/Module.cls::LoadNewModule:1166-1196` (Defaults merge and `JournalManager`), `LoadModuleReference` (row insert), History readers.
   - Contrast:
     - A NoJournal dependency under a failing normal-mode parent keeps its module record, and its history row says committed.
     - A journaled dependency in the same position is rolled back.
     - The parent row's Committed should count the surviving writes.
5. **Effects outside the database across a rollback.**
   - Obligations:
     - transaction.rollback-scope#1 (partial)
     - history.committed.side-effects#1 (absent)
     - deps.lockfile#4/#6 (partial)
     - artifact.failure#5 (partial)
   - Chunk A item 4 (file-copy#5/#6 around a failing Activate) is the same family.
   - Members: `General/LockFile.cls` and the `LoadDependencies` lock write (:1323-1328), pip calls, the FileCopy processor, the ModuleRoot replacement (OBS-L7), and the git clone folder.
   - Contrast:
     - Activate fails after a lock file, a pip install and a FileCopy. The record is rolled back and Committed=0, while `module-lock.json`, the installed wheel and the copied files remain.
     - A first load with an empty Root writes `./module-lock.json` in the process directory, while a reload writes under the root.
6. **Dependency version conflict.**
   - Obligations: deps.conflict#1 (partial), #2 (absent), #3 (partial).
   - The crate's `found` bit is the only failure input. No resolution-conflict case exists.
   - Members: `Storage/Module.cls::BuildDependencyGraph` (:954-959), then `Utils/Module.cls::LoadDependencies`.
   - Contrast:
     - A needs C =1 and B needs C =2. The load stops, and the error names A and B.
     - Under `-dev` the top module's record and manifest are kept (committed at C1), while in normal mode they are rolled back.
7. **Update refusals and update-specific resolution.**
   - Obligations:
     - update.refusal#3/#4 (absent), #5 (partial)
     - update.requirements#1/#2 (partial)
     - update.dev-mode#1 (partial)
   - Members: `Main.cls::Update` (:4415-4423, which supplies `g.known` to BuildDependencyGraph) and `Utils/Module.cls::LoadNewModule` (:1207, :1232).
   - Contrast:
     - `update A -path <archive of B>` refuses with the namespace and ModuleRoot unchanged. As built it crashes with `<INVALID OREF>` (D16) and leaves the archive unpacked.
     - Updating A when shared dependency S must move from 1.x to 2.x together with B succeeds, while the same request through `install` fails.
     - `update m 1.2.0 -DPermitDowngrade=1` is refused by the guard at `Main.cls:4415-4417` before any history row, while without the flag the update proceeds.
8. **ORAS: the validated layer is not the loaded layer.**
   - Obligations: repo.oras.integrity#4 (partial) and repo.oras.archive-boundary#3 (partial). repo.oras#2/#3 are absent (Basic/token auth and name/external-name lookup).
   - Members: `Repo/Oras/PackageService.cls::Pull` (takes the first `*.tgz` file) versus `VerifyModuleHashAndSize` (checks the first tar layer); history Finalize; the module record.
   - Contrast:
     - Layers [A, B], with B's file listed first: validation passes on A but B is loaded.
     - A size-mismatching reinstall versus a matching one, observing `^IPM.Storage.ModuleD` and the history row (Success, Committed).
9. **A read-only repository forces normal mode. No model applies.**
   - Obligations: repo.read-only#1 (absent) and dev-mode#8 (absent).
   - Members:
     - decider: `Utils/Module.cls:1280` (`ReadOnlyGetStored` forces developer mode off)
     - writer: the module record save
     - reader: the stored DeveloperMode
   - Contrast: `install -dev` from a ReadOnly repository stores DeveloperMode 0, and from a writable one stores 1.
10. **History deletion and retention scope. These decide destructive persistence.**
    - Obligations:
      - history.delete#2/#3/#6/#8 (absent), #9 (partial)
      - history.delete.global-unfiltered#1 (absent)
      - history.retention#1-#3/#5 (partial or absent)
    - Members:
      - `General/History.cls` delete and purge (:172-173, the quoted cutoff)
      - `Main.cls:840-842` (Shell-only purge trigger)
      - `SetHistoryLastPurge`
      - the `-globally` loop over namespaces
    - Contrast:
      - `history delete` with no filter and no `-confirm` is refused.
      - `history delete -globally` empties every namespace, and one namespace throwing still returns OK.
      - retain=7 deletes a 10-day-old row and keeps a 1-day-old row, while retain=0 deletes neither. A second command on the same day deletes nothing.
11. **uninstall -all traversal and the before-Clean hook order.**
    - Obligations: uninstall.all#1-#3 (partial) and uninstall.dependents#5 (absent).
    - Members: `Utils/Module.cls::UninstallAll`, `Storage/Module.cls::Uninstall`/`ExecutePhases`, `Lifecycle/Base.cls::CheckBeforeClean`.
    - Contrast:
      - zpm, A and B with A needing B: without `-force` A fails and B is never attempted, while with `-force` B is removed and A's error is returned.
      - A blocked uninstall whose Clean/Before Invoke sets a global: the global is set before the dependency check refuses.
12. **System requirement kinds.**
    - Obligations: load.refusal.requirements#1 (partial). This is the same gap as chunk A item 6, module.requirement#1/#2.
    - Members: `Storage/SystemRequirements.cls::{CheckVersion, CheckIPMVersion, CheckPythonVersion}` → `CheckRequirements` → `Utils/Module.cls:1321`.
    - Contrast: `Version=">=2099.1"` versus `">=2020.1"` on one instance gives refusal versus load, with both failures kept in the aggregate. Only SYSNamespace, Health and Interoperability are decided today, and the refusal consequence is the G input bit `sysreq`.
13. **Path refusals and the temporary local repository.**
    - Obligations: load.refusal.path#1-#3 (partial) and load.dependencies.local-repo#2/#3 (partial).
    - Members: `Main.cls:2447-2452` and `General/TempLocalRepoManager.cls` (`%OnNew`, `CleanUp`, `Main.cls:2523-2531`).
    - Contrast:
      - An existing relative path loads with Committed=1, while a missing relative path and an empty path are refused.
      - A missing folder and an already-registered root each create no `ipm-temp-modules-N` row.
      - After a load that fails in Activate, that row is removed just as after success.
14. **Repository selection filters and concurrent refresh.**
    - Obligations:
      - repo.search-order#1/#2 (partial)
      - repo.prereleases#1 (partial), #2 (absent)
      - repo.filesystem#7 (absent)
    - Members:
      - `Repo/Manager.cls::SearchRepositoriesForModule` and `SearchCriteria.Registry`
      - `Repo/Manager.cls:99-108` and `Repo/Utils.cls:95-98` (prerelease and snapshot override)
      - `Repo/Filesystem/Definition.cls::BuildCache` (purge and rebuild under a lock released at return, writing `^IPM.Repo.Filesystem.CacheD`)
    - Contrast:
      - A disabled repository with a higher version is not visited.
      - `install other/M` installs other's 1.0.0 although main offers 2.0.0.
      - Snapshots=0 versus 1 with only `1.0.0+snapshot` offered gives not found versus installed.
      - Concurrent refreshes or reads see either the old or the new cache, never a mix.
15. **ExternalName uniqueness and `enable -community`.**
    - Obligations: module.identity#2 (absent) and namespace.enable#9 (partial), #11 (absent).
    - Members: `Storage/Module.cls %OnValidateObject` (1510-1591); the `-community` branch of namespace enable.
    - Contrast:
      - Module B whose ExternalName equals installed module a's name is refused and the record is unchanged, while a unique ExternalName saves.
      - On an instance with an empty NS2, one `enable -community` leaves NS2 reading its own repositories (F-NSP-4 likely inherited).
      - A failing `-community` step still returns OK.

Next tier:
- build.test.outcome.not-fatal#2 and nested#4 (absent): non-fatal runs skip `GetAllTestsStatus`/`OutputFailures`.
- build.package#3/#4 (absent): `-path` and `-export-deps`.
- build.test-output#3 (absent): format precedence.
- repo.manage#1/#2/#4 and repo.default-registry (absent).
- inspect frame and listing constants (partial).
- history.read.find/global (absent).

Comparison with chunk A's ranked list (iter68 §4):
- **Chunk A item 1** (generate grouping) was closed by iteration 69.
- **Chunk A item 4** (file-copy around a failing Activate) belongs to the same failure-and-recovery family as items 4 and 5 here. Together they describe one unmodeled boundary: which effects outside a transaction survive a rolled-back load, and what History's Committed says about them.
- **Chunk A item 6** is the same gap as item 12 here, counted once in the queue.
- **Items 1-3 here outrank everything left on chunk A's list.**
  - Items 1 and 2 cover whole command paths (install, update, uninstall rows; archive and git loads) where a persistent outcome changes and no model applies.
  - Item 3 is the only operation that installs IPM itself, and it has one full grade.
  - Chunk A's remaining items (sync aggregate status, module-version atomicity, system-folder safety, stale dependents, evaluation order, sync tests, deployed code) are narrower, mostly one decision inside an otherwise modeled workflow.
- **Items 6-11 here rank alongside chunk A items 2-5.** All are outcome-changing decisions or failure paths with a single missing contrast.
- **Items 12-15 rank with chunk A items 6-10.**

## Findings for the master (not acted on)

Wrong or unqualified ontology statements:
1. **history.locked B1 is refuted as built.** The working assumption "a record without an end time stays unfinalized" fails: crate explorer `asbuilt-l1-end` (2,925 configurations, not truncated) shows a terminated process leaving Finalized=1 with no TimeEnd.
2. **load.outcome B7/B8/B11/B12 duplicate transaction.flag-sources word for word.** One commitment has two owners.
3. **load.refusal B2 conflicts with B6.** B2 ("timing is open") sits beside B6's working assumption "yes", which the G model refutes (case_c23, D4). The block also says "every mode", while the requirements exception is stated only in a bullet.
4. **dev-mode B4 ("-dev applies to … update") is contradicted for registry updates** by playbook `update_steps` F-INST-5. The metadata calls this only a suspected gap (OBS-L4).
5. **update.refusal B3 needs an as-built qualification.** It says a mismatched `-path` archive is refused, but its own metadata (D16) says the update crashes with `<INVALID OREF>` first.
6. **update.steps B3 misstates the step order.** It says steps run from the current to the new version, but as built they run by completed-step records.
7. **deps.lockfile B8 conflicts with `lockfile.rs` `add_repo`.** B8 says a folder-loaded dependency is written with an empty registry, but `add_repo` marks the document not ok for an unknown repository. Either the model lacks the empty-name case or the assumption is unsupported.
8. **version.range P1 "skips prereleases and snapshots" needs a qualification.** It has no snapshot model, and with prereleases enabled the install picks a prerelease (C:D39). The qualification belongs in the body.
9. **lifecycle.source-control P1 has no positive witness.** "Calls it when a module's root changes" has no path in `schooks.rs`, where root is fixed to 0.
10. **settings.default-modifiers P1 is wrong about scope.** It says "in the current namespace", but the store is shared by every namespace (F-REPO-5, D19).
11. **namespace.mapped-refusal does not disclose the installer path.** The installer path contradicts it (F-NSP-5, `nsp_witness_installer_in_mapped`).
12. **namespace.enable B7 (`-community`) likely inherits F-NSP-4** but is stated without qualification.
13. **uninstall.global / C:S17 should say when the skip happens.** The global-scope skip comes after `OnBeforePhase("Clean")` (playbook `exec_clean`), so a skipped module that owns a web app loses it.
14. **uninstall.cleanup P1 omits Preserve.** A global is killed only when Global is set and Preserve is off.
15. **namespace P1 says each namespace has its own repositories,** but config settings are instance-wide. This is noted only in metadata.
16. **artifact.selection B2 has no formal or metadata support.** The metadata of "returned download path omits the dot folder" cites only `accepted`.
17. **Possible finding, unverified:**
    - Playbook `uninstall.rs` `clean_cmd` (`<module> clean`, Recurse 1) reuses `exec_clean`, whose `dep_step` writes History rows for cleaned dependencies.
    - If that is faithful, a phase command writes history rows, contradicting history.actions B3.
18. **Checker guidance and duplicated text in product blocks:**
    - history.retention B4 ("preserve record identities when checking retention") is checker guidance inside a product block.
    - repo.oras.build-readers B5 duplicates the oras.integrity B4 working assumption.

Metadata and coverage-map claims the models do not support:

19. **Metadata credits non-distinguishing declarations:**
    - inspect cites `thin_surfaces::inspection_preserves_stores`, an identity.
    - inspect.show cites `scoped_hidden`, a tautology.
    - repo.filesystem cites `owners.rs::unlock_*` and `build_cache.lock_held`, which iter2 already withdrew.
    - repo.oras cites only `oras_available`, while four bullets and two working assumptions have no model.
20. **The coverage map is stale or wrong in places:**
    - "dev-mode.overwrite-guard: Not modeled" is stale (iter25).
    - "load.options -skip-sync unestablished" is wrong (`sync_baseline.rs`, devtools.sync#16).
21. **Strong models no block links:**
    - playbook `update_steps.rs` (update), `uninstall.rs` (uninstall.global, recurse, parent), `ns_mapping.rs` (mapped-refusal), `settings.rs` (default-modifiers, repo.manage), `scopes.rs`/`dep_resolve.rs` (build.test, deps), `load_general.rs` (history)
    - iter16 `provenance.rs` (history.fields), iter34 `versions.rs` (repo.prereleases), iter8 `catalog.rs`, `history.rs::details`
22. **`history.rs` `Filter.column_known` is dead.** The G:D2 find/delete divergence for an unknown column has no formal rule.
23. **`load_general.rs` needs `--rlimit 30`.** Its two two-dependency witnesses exceed the default rlimit on x86, so any re-run must record the flag.

## Limitations and remaining work

- **Fork grades are mostly not re-judged.** The seven forks graded under the same rules. I re-judged 66 full grades, at least three per page, or all of a page's full grades where it has fewer than three. I also checked every fork-reported composition and the bounded-check receipts, and swept every full grade with only definitional locators. I did not re-judge each partial or absent gap text.
  - That sample found one systematic locator defect (the versions page's `native_0`) and one double count (settings#14). Both are fixed.
  - Other fork pages may still have locator imprecision that does not change a grade.
- **Full is a statement about the declared rule.** It means the rule distinguishes the cases, not that the ObjectScript body is verified. Direct implementation proofs remain 0.
  - Many full grades rest on fixture-level neighbours, such as the iter-NN `*_neighbors` functions and the semver native table.
  - Others rest on the G crate's transcription (L3 is a source-bound transcription plus native observations).
- **Bounded grades depend on the crate's exhaustive domain.** The 18 bounded grades come from the crate survey and explorer: at most two leaf dependencies, a single fault, and the provisional Q settings recorded in `survey.json`. Generalisation beyond that domain rests on `load_general.rs` where it is cited.
- **Cross-page restatements were mostly excluded rather than aliased.** 22 units are excluded with the owner named rather than aliased to a specific obligation. Either form counts the restatement once, but aliases would let a later seal trace them to the owner obligation.
- **Not done in this iteration:** the member-census reconciliation and the source-selected draws from uplift-scoring.md are not part of this census.

## 5. Cost

- Wall time: 01:58Z to about 02:21Z (UTC session clock).
- Seven forked subagents ran concurrently: 1,925,492 tokens and 396 tool calls in total, each taking 562-889 s.
  - load: 247k tokens
  - history: 267k
  - transactions/lifecycle/versions: 292k
  - sources/download: 250k
  - policy/inspection: 269k
  - build/deps/install/update: 307k
  - uninstall/operations: 293k
- Main-thread token usage is not visible to the builder.
- Verus re-runs took 1-60 s each. The longest was `load_general.rs` at `--rlimit 30`, about 60 s.

## Appendix: per-block tables (chunks B, C, D)

#### features/load.g5.md (chunk B)

Blocks 22 (22 enumerated, 0 structural), units 85. Obligations 75: full 47, partial 15, absent 13, unassessed 0 (coverage 0.627). Checks: proved 28, bounded 12, refuted 7, unchecked 15, none 13. Aliases 13, excluded units 21.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| load | enumerated | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| load.path | enumerated | 0 | 1 | 3 | 0 | 0 | 0 | 0 | 1 | 3 | 0 | 1 | 0.00 |
| load.path.archive | enumerated | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.00 |
| load.write-check | enumerated | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.00 |
| load.core | enumerated | 2 | 2 | 0 | 0 | 1 | 1 | 0 | 2 | 0 | 0 | 1 | 0.50 |
| load.options | enumerated | 2 | 0 | 2 | 0 | 2 | 0 | 0 | 0 | 2 | 6 | 2 | 0.50 |
| load.dependencies | enumerated | 2 | 1 | 0 | 0 | 1 | 1 | 0 | 1 | 0 | 0 | 2 | 0.67 |
| load.dependencies.local-repo | enumerated | 3 | 3 | 2 | 0 | 3 | 0 | 0 | 3 | 2 | 2 | 1 | 0.38 |
| load.refusal | enumerated | 4 | 1 | 0 | 0 | 1 | 1 | 2 | 1 | 0 | 2 | 3 | 0.80 |
| load.refusal.path | enumerated | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0.00 |
| load.refusal.manifest | enumerated | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0.50 |
| load.refusal.downgrade | enumerated | 3 | 0 | 1 | 0 | 2 | 0 | 1 | 0 | 1 | 0 | 0 | 0.75 |
| load.refusal.update-required | enumerated | 3 | 0 | 1 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 0.75 |
| load.refusal.requirements | enumerated | 2 | 1 | 0 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 0.67 |
| load.same-version | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| load.reload.root | enumerated | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| load.outcome | enumerated | 6 | 0 | 0 | 0 | 4 | 2 | 0 | 0 | 0 | 2 | 9 | 1.00 |
| load.outcome.dev | enumerated | 4 | 0 | 0 | 0 | 1 | 3 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| load.outcome.dev.early-failure | enumerated | 2 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| load.status | enumerated | 2 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 1 | 0 | 1.00 |
| load.status.after-commit | enumerated | 3 | 1 | 0 | 0 | 2 | 0 | 1 | 1 | 0 | 0 | 0 | 0.75 |
| load.after-install-message | enumerated | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 0 | 0.50 |

#### features/history.g5.md (chunk B)

Blocks 24 (23 enumerated, 1 structural), units 99. Obligations 100: full 52, partial 21, absent 27, unassessed 0 (coverage 0.520). Checks: proved 36, bounded 2, refuted 14, unchecked 21, none 27. Aliases 12, excluded units 10.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| history | structural | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | N/A |
| history.actions | enumerated | 1 | 1 | 2 | 0 | 1 | 0 | 0 | 1 | 2 | 1 | 0 | 0.25 |
| history.record | enumerated | 5 | 1 | 2 | 0 | 3 | 0 | 2 | 1 | 2 | 1 | 0 | 0.62 |
| history.fields | enumerated | 5 | 4 | 2 | 0 | 4 | 0 | 1 | 4 | 2 | 2 | 1 | 0.45 |
| history.phases | enumerated | 2 | 2 | 1 | 0 | 0 | 1 | 1 | 2 | 1 | 0 | 0 | 0.40 |
| history.survives-rollback | enumerated | 4 | 0 | 0 | 0 | 2 | 0 | 2 | 0 | 0 | 0 | 3 | 1.00 |
| history.survives-rollback.temp-copy | enumerated | 2 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 0 | 1 | 0.67 |
| history.dependencies | enumerated | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| history.dependencies.parent-rollback | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 2 | 2 | 1.00 |
| history.committed | enumerated | 10 | 1 | 1 | 0 | 7 | 0 | 3 | 1 | 1 | 2 | 0 | 0.83 |
| history.committed.marking | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 1.00 |
| history.committed.uninstall | enumerated | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0.00 |
| history.committed.vs-success | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| history.committed.side-effects | enumerated | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0.00 |
| history.read | enumerated | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0.00 |
| history.read.find | enumerated | 0 | 1 | 2 | 0 | 0 | 0 | 0 | 1 | 2 | 0 | 0 | 0.00 |
| history.read.filter | enumerated | 4 | 2 | 5 | 0 | 2 | 0 | 2 | 2 | 5 | 0 | 0 | 0.36 |
| history.read.details | enumerated | 3 | 1 | 0 | 0 | 3 | 0 | 0 | 1 | 0 | 0 | 0 | 0.75 |
| history.read.status-bytes | enumerated | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| history.read.unfinished | enumerated | 2 | 1 | 0 | 0 | 1 | 0 | 1 | 1 | 0 | 0 | 0 | 0.67 |
| history.read.global | enumerated | 0 | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 2 | 0 | 0 | 0.00 |
| history.read.api | enumerated | 2 | 1 | 1 | 0 | 1 | 0 | 1 | 1 | 1 | 3 | 0 | 0.50 |
| history.delete | enumerated | 3 | 1 | 6 | 0 | 2 | 0 | 1 | 1 | 6 | 0 | 0 | 0.30 |
| history.delete.global-unfiltered | enumerated | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.00 |

#### concepts/transactions-and-dev-mode.g5.md (chunk B)

Blocks 10 (10 enumerated, 0 structural), units 45. Obligations 38: full 23, partial 10, absent 5, unassessed 0 (coverage 0.605). Checks: proved 17, bounded 4, refuted 2, unchecked 10, none 5. Aliases 7, excluded units 11.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| transaction | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 1.00 |
| transaction.rollback-scope | enumerated | 2 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 4 | 2 | 0.67 |
| transaction.opt-out | enumerated | 2 | 2 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0 | 0.50 |
| transaction.flag-sources | enumerated | 5 | 1 | 2 | 0 | 4 | 0 | 1 | 1 | 2 | 1 | 2 | 0.62 |
| dev-mode | enumerated | 4 | 1 | 3 | 0 | 1 | 2 | 1 | 1 | 3 | 1 | 1 | 0.50 |
| dev-mode.source-files | enumerated | 4 | 1 | 0 | 0 | 4 | 0 | 0 | 1 | 0 | 0 | 1 | 0.80 |
| dev-mode.overwrite-guard | enumerated | 1 | 2 | 0 | 0 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0.33 |
| dev-mode.keep-on-failure | enumerated | 2 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 1 | 2 | 1.00 |
| dev-mode.not-propagated | enumerated | 1 | 1 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0.50 |
| dev-mode.not-propagated.local-snapshot | enumerated | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0.00 |

#### architecture/lifecycle.g5.md (chunk C)

Blocks 15 (15 enumerated, 0 structural), units 44. Obligations 67: full 49, partial 13, absent 5, unassessed 0 (coverage 0.731). Checks: proved 46, bounded 0, refuted 3, unchecked 13, none 5. Aliases 5, excluded units 1.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| lifecycle | enumerated | 3 | 0 | 1 | 0 | 3 | 0 | 0 | 0 | 1 | 3 | 1 | 0.75 |
| lifecycle.chain | enumerated | 4 | 1 | 0 | 0 | 4 | 0 | 0 | 1 | 0 | 0 | 0 | 0.80 |
| lifecycle.initialize | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| lifecycle.document | enumerated | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| lifecycle.module-action | enumerated | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 0 | 0.50 |
| lifecycle.alias | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 1 | 0 | 1.00 |
| lifecycle.only | enumerated | 3 | 0 | 0 | 0 | 2 | 0 | 1 | 0 | 0 | 0 | 0 | 1.00 |
| lifecycle.custom | enumerated | 6 | 0 | 2 | 0 | 4 | 0 | 2 | 0 | 2 | 0 | 0 | 0.75 |
| module.lifecycle-class | enumerated | 3 | 2 | 1 | 0 | 3 | 0 | 0 | 2 | 1 | 1 | 0 | 0.50 |
| lifecycle.hooks | enumerated | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0.00 |
| module.invoke | enumerated | 5 | 2 | 0 | 0 | 5 | 0 | 0 | 2 | 0 | 0 | 0 | 0.71 |
| lifecycle.installer | enumerated | 5 | 3 | 0 | 0 | 5 | 0 | 0 | 3 | 0 | 0 | 0 | 0.62 |
| lifecycle.installer.parameters | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| lifecycle.source-control | enumerated | 4 | 2 | 0 | 0 | 4 | 0 | 0 | 2 | 0 | 0 | 0 | 0.67 |
| lifecycle.source-control.editor | enumerated | 5 | 1 | 1 | 0 | 5 | 0 | 0 | 1 | 1 | 0 | 0 | 0.71 |

#### architecture/package-sources.g5.md (chunk C)

Blocks 12 (12 enumerated, 0 structural), units 57. Obligations 62: full 51, partial 4, absent 7, unassessed 0 (coverage 0.823). Checks: proved 50, bounded 0, refuted 1, unchecked 4, none 7. Aliases 1, excluded units 14.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| repo.filesystem | enumerated | 7 | 0 | 1 | 0 | 6 | 0 | 1 | 0 | 1 | 0 | 2 | 0.88 |
| repo.filesystem.duplicates | enumerated | 7 | 0 | 0 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 2 | 1.00 |
| repo.filesystem.mutable-source | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| repo.oras | enumerated | 1 | 0 | 6 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 1 | 0.14 |
| repo.oras.integrity | enumerated | 5 | 1 | 0 | 0 | 5 | 0 | 0 | 1 | 0 | 0 | 1 | 0.83 |
| repo.oras.archive-boundary | enumerated | 2 | 2 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0 | 0.50 |
| repo.oras.signed-admission | enumerated | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| repo.oras.tag-decode | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| repo.oras.platform-selection | enumerated | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 0 | 1 | 1 | 1.00 |
| repo.oras.selection-modes | enumerated | 8 | 0 | 0 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| repo.oras.build-readers | enumerated | 2 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 0 | 3 | 0.67 |
| repo.publish-cache | enumerated | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |

#### concepts/data-policy.g5.md (chunk C)

Blocks 12 (11 enumerated, 1 structural), units 37. Obligations 34: full 19, partial 7, absent 8, unassessed 0 (coverage 0.559). Checks: proved 18, bounded 0, refuted 1, unchecked 7, none 8. Aliases 0, excluded units 11.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| module | structural | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | N/A |
| module.identity | enumerated | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 2 | 0.00 |
| module.resource | enumerated | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 2 | 0.50 |
| module.resource.identity | enumerated | 3 | 0 | 1 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 0.75 |
| namespace | enumerated | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0.00 |
| history.namespace | enumerated | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.50 |
| repo.origin.installed | enumerated | 5 | 0 | 1 | 0 | 5 | 0 | 0 | 0 | 1 | 0 | 1 | 0.83 |
| repo.origin.same-version | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| history.fields.source-snapshot | enumerated | 3 | 0 | 1 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 1 | 0.75 |
| history.locked | enumerated | 2 | 1 | 0 | 0 | 1 | 0 | 1 | 1 | 0 | 0 | 0 | 0.67 |
| history.retention | enumerated | 1 | 3 | 1 | 0 | 1 | 0 | 0 | 3 | 1 | 0 | 3 | 0.20 |
| history.keep-on-ipm-uninstall | enumerated | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0.00 |

#### concepts/testing-policy.g5.md (chunk C)

Blocks 11 (11 enumerated, 0 structural), units 35. Obligations 43: full 36, partial 4, absent 3, unassessed 0 (coverage 0.837). Checks: proved 35, bounded 0, refuted 1, unchecked 4, none 3. Aliases 4, excluded units 5.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| module.resource.scope | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| build.test.selection | enumerated | 6 | 1 | 0 | 0 | 6 | 0 | 0 | 1 | 0 | 0 | 1 | 0.86 |
| build.test.outcome | enumerated | 4 | 1 | 0 | 0 | 4 | 0 | 0 | 1 | 0 | 1 | 0 | 0.80 |
| build.test.outcome.not-fatal | enumerated | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.50 |
| build.test.outcome.empty | enumerated | 2 | 0 | 1 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 2 | 0.67 |
| build.test.outcome.nested | enumerated | 4 | 0 | 1 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 0.80 |
| devtools.sync.test-stop | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 3 | 0 | 1.00 |
| build.test.outcome.exit | enumerated | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0.50 |
| build.test-output.run-identity | enumerated | 6 | 1 | 0 | 0 | 5 | 0 | 1 | 1 | 0 | 0 | 0 | 0.86 |
| build.test-output.counted-rows | enumerated | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| build.test-output.counts | enumerated | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |

#### concepts/versions.g5.md (chunk C)

Blocks 7 (6 enumerated, 1 structural), units 16. Obligations 21: full 19, partial 2, absent 0, unassessed 0 (coverage 0.905). Checks: proved 14, bounded 0, refuted 5, unchecked 2, none 0. Aliases 1, excluded units 1.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| version | structural | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | N/A |
| version.order | enumerated | 6 | 0 | 0 | 0 | 5 | 0 | 1 | 0 | 0 | 0 | 0 | 1.00 |
| version.post-release | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| version.range | enumerated | 2 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 0 | 0 | 0.67 |
| version.range.partial | enumerated | 5 | 0 | 0 | 0 | 3 | 0 | 2 | 0 | 0 | 1 | 0 | 1.00 |
| version.range.combine | enumerated | 3 | 1 | 0 | 0 | 1 | 0 | 2 | 1 | 0 | 0 | 0 | 0.75 |
| version.range.snapshot-intersection | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |

#### concepts/download-security.g5.md (chunk C)

Blocks 5 (5 enumerated, 0 structural), units 14. Obligations 13: full 12, partial 0, absent 1, unassessed 0 (coverage 0.923). Checks: proved 12, bounded 0, refuted 0, unchecked 0, none 1. Aliases 1, excluded units 1.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| repo.remote.download | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 1 | 1 | 1.00 |
| repo.remote.download.redirect | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| repo.remote.download.integrity | enumerated | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| repo.remote.download.token | enumerated | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.50 |
| artifact.authentication | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |

#### concepts/repositories.g5.md (chunk C)

Blocks 6 (6 enumerated, 0 structural), units 17. Obligations 16: full 11, partial 3, absent 2, unassessed 0 (coverage 0.688). Checks: proved 10, bounded 0, refuted 1, unchecked 3, none 2. Aliases 0, excluded units 5.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| repo | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 3 | 1.00 |
| repo.read-only | enumerated | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.00 |
| repo.prereleases | enumerated | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0.00 |
| repo.search-order | enumerated | 7 | 2 | 0 | 0 | 7 | 0 | 0 | 2 | 0 | 0 | 1 | 0.78 |
| repo.install.version-request | enumerated | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 1.00 |
| repo.search.version-list | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |

#### features/artifact-download.g5.md (chunk D)

Blocks 6 (6 enumerated, 0 structural), units 22. Obligations 27: full 17, partial 5, absent 5, unassessed 0 (coverage 0.630). Checks: proved 16, bounded 0, refuted 1, unchecked 5, none 5. Aliases 2, excluded units 2.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| artifact | enumerated | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 2 | 1 | 0.00 |
| artifact.filter | enumerated | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| artifact.selection | enumerated | 5 | 2 | 1 | 0 | 5 | 0 | 0 | 2 | 1 | 0 | 0 | 0.62 |
| artifact.failure | enumerated | 1 | 2 | 2 | 0 | 1 | 0 | 0 | 2 | 2 | 0 | 0 | 0.20 |
| artifact.archive-admission | enumerated | 3 | 0 | 0 | 0 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 1.00 |
| artifact.local-content | enumerated | 3 | 0 | 2 | 0 | 3 | 0 | 0 | 0 | 2 | 0 | 0 | 0.60 |

#### features/build-and-publish.g5.md (chunk D)

Blocks 12 (11 enumerated, 1 structural), units 37. Obligations 47: full 25, partial 17, absent 5, unassessed 0 (coverage 0.532). Checks: proved 25, bounded 0, refuted 0, unchecked 17, none 5. Aliases 5, excluded units 4.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| build | structural | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | N/A |
| build.test | enumerated | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 0 | 1 | 0 | 1.00 |
| build.test-output | enumerated | 3 | 4 | 1 | 0 | 3 | 0 | 0 | 4 | 1 | 0 | 1 | 0.38 |
| build.package | enumerated | 1 | 1 | 2 | 0 | 1 | 0 | 0 | 1 | 2 | 1 | 0 | 0.25 |
| build.package.test-path | enumerated | 2 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 1 | 0 | 0.67 |
| build.package.include-refresh | enumerated | 2 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 1 | 1 | 0.67 |
| build.archive.empty-file | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| build.copy.overlap | enumerated | 1 | 4 | 0 | 0 | 1 | 0 | 0 | 4 | 0 | 0 | 0 | 0.20 |
| build.verify | enumerated | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0.00 |
| build.publish | enumerated | 0 | 3 | 1 | 0 | 0 | 0 | 0 | 3 | 1 | 1 | 0 | 0.00 |
| build.publish.external-identity | enumerated | 3 | 2 | 0 | 0 | 3 | 0 | 0 | 2 | 0 | 0 | 0 | 0.60 |
| build.publish.partial-effects | enumerated | 6 | 0 | 0 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |

#### features/dependencies.g5.md (chunk D)

Blocks 5 (5 enumerated, 0 structural), units 28. Obligations 23: full 12, partial 8, absent 3, unassessed 0 (coverage 0.522). Checks: proved 12, bounded 0, refuted 0, unchecked 8, none 3. Aliases 1, excluded units 8.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| deps | enumerated | 1 | 2 | 1 | 0 | 1 | 0 | 0 | 2 | 1 | 0 | 2 | 0.25 |
| deps.conflict | enumerated | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 2 | 1 | 0 | 1 | 0.00 |
| deps.installed | enumerated | 6 | 0 | 0 | 0 | 6 | 0 | 0 | 0 | 0 | 1 | 0 | 1.00 |
| deps.lockfile | enumerated | 2 | 4 | 1 | 0 | 2 | 0 | 0 | 4 | 1 | 0 | 4 | 0.29 |
| deps.lockfile.content | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |

#### features/inspection.g5.md (chunk D)

Blocks 10 (10 enumerated, 0 structural), units 40. Obligations 51: full 30, partial 11, absent 10, unassessed 0 (coverage 0.588). Checks: proved 30, bounded 0, refuted 0, unchecked 11, none 10. Aliases 7, excluded units 1.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| inspect | enumerated | 4 | 4 | 4 | 0 | 4 | 0 | 0 | 4 | 4 | 7 | 0 | 0.33 |
| inspect.help | enumerated | 3 | 3 | 0 | 0 | 3 | 0 | 0 | 3 | 0 | 0 | 0 | 0.50 |
| inspect.namespace | enumerated | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.50 |
| inspect.list-installed | enumerated | 3 | 0 | 3 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0.50 |
| inspect.show | enumerated | 3 | 2 | 1 | 0 | 3 | 0 | 0 | 2 | 1 | 0 | 0 | 0.50 |
| inspect.webapp.detail | enumerated | 2 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 0 | 0 | 0.67 |
| inspect.dependents | enumerated | 2 | 1 | 1 | 0 | 2 | 0 | 0 | 1 | 1 | 0 | 0 | 0.50 |
| inspect.list-dependents | enumerated | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| inspect.orphans.ownership | enumerated | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| inspect.orphans.identifier-case | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |

#### features/install.g5.md (chunk D)

Blocks 7 (7 enumerated, 0 structural), units 25. Obligations 24: full 18, partial 4, absent 2, unassessed 0 (coverage 0.750). Checks: proved 16, bounded 0, refuted 2, unchecked 4, none 2. Aliases 2, excluded units 6.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| install | enumerated | 3 | 1 | 1 | 0 | 2 | 0 | 1 | 1 | 1 | 0 | 1 | 0.60 |
| install.refusal | enumerated | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 3 | 0.00 |
| install.package | enumerated | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0.50 |
| reinstall | enumerated | 2 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 2 | 0 | 1.00 |
| reinstall.selection | enumerated | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| reinstall.snapshot-dependencies | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| reinstall.history | enumerated | 4 | 1 | 0 | 0 | 4 | 0 | 0 | 1 | 0 | 0 | 0 | 0.80 |

#### features/package-commands.g5.md (chunk D)

Blocks 3 (2 enumerated, 1 structural), units 5. Obligations 3: full 3, partial 0, absent 0, unassessed 0 (coverage 1.000). Checks: proved 3, bounded 0, refuted 0, unchecked 0, none 0. Aliases 1, excluded units 1.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ipm | structural | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | N/A |
| ipm.shell | enumerated | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | N/A |
| ipm.shell.forms | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |

#### features/uninstall.g5.md (chunk D)

Blocks 13 (13 enumerated, 0 structural), units 33. Obligations 39: full 30, partial 6, absent 3, unassessed 0 (coverage 0.769). Checks: proved 25, bounded 0, refuted 5, unchecked 6, none 3. Aliases 6, excluded units 6.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| uninstall | enumerated | 2 | 0 | 1 | 0 | 1 | 0 | 1 | 0 | 1 | 0 | 1 | 0.67 |
| uninstall.cleanup | enumerated | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 2 | 0 | 1.00 |
| uninstall.package-ownership | enumerated | 3 | 1 | 0 | 0 | 3 | 0 | 0 | 1 | 0 | 0 | 0 | 0.75 |
| uninstall.class-delete-error | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 3 | 1.00 |
| uninstall.dependents | enumerated | 2 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 2 | 3 | 0 | 0.50 |
| uninstall.dependents.as-built | enumerated | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| uninstall.recurse | enumerated | 2 | 1 | 0 | 0 | 1 | 0 | 1 | 1 | 0 | 0 | 0 | 0.67 |
| uninstall.child-error | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| uninstall.all | enumerated | 1 | 3 | 0 | 0 | 1 | 0 | 0 | 3 | 0 | 0 | 0 | 0.25 |
| uninstall.global | enumerated | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| uninstall.purge | enumerated | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 1.00 |
| uninstall.purge.as-built | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| uninstall.ipm | enumerated | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 0 | 0.50 |

#### features/update.g5.md (chunk D)

Blocks 7 (7 enumerated, 0 structural), units 27. Obligations 24: full 14, partial 5, absent 5, unassessed 0 (coverage 0.583). Checks: proved 12, bounded 0, refuted 2, unchecked 5, none 5. Aliases 0, excluded units 9.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| update | enumerated | 1 | 0 | 2 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 1 | 0.33 |
| update.refusal | enumerated | 1 | 1 | 3 | 0 | 1 | 0 | 0 | 1 | 3 | 0 | 2 | 0.20 |
| update.dev-mode | enumerated | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0.00 |
| update.requirements | enumerated | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0.00 |
| update.steps | enumerated | 6 | 1 | 0 | 0 | 4 | 0 | 2 | 1 | 0 | 0 | 2 | 0.86 |
| update.steps.shared-execution | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 1.00 |
| update.steps.records | enumerated | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |

#### operations/ipm-setup.g5.md (chunk D)

Blocks 3 (3 enumerated, 0 structural), units 9. Obligations 9: full 1, partial 3, absent 5, unassessed 0 (coverage 0.111). Checks: proved 1, bounded 0, refuted 0, unchecked 3, none 5. Aliases 1, excluded units 1.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| setup | enumerated | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.00 |
| setup.installer | enumerated | 0 | 2 | 4 | 0 | 0 | 0 | 0 | 2 | 4 | 1 | 1 | 0.00 |
| setup.language-extension | enumerated | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 0 | 0.50 |

#### operations/namespaces.g5.md (chunk D)

Blocks 8 (8 enumerated, 0 structural), units 28. Obligations 32: full 21, partial 6, absent 5, unassessed 0 (coverage 0.656). Checks: proved 17, bounded 0, refuted 4, unchecked 6, none 5. Aliases 3, excluded units 6.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| namespace.enable | enumerated | 3 | 3 | 3 | 0 | 1 | 0 | 2 | 3 | 3 | 2 | 2 | 0.33 |
| namespace.enable.mapping-results | enumerated | 4 | 1 | 0 | 0 | 4 | 0 | 0 | 1 | 0 | 0 | 1 | 0.80 |
| namespace.enable.partial-batch | enumerated | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 1 | 1.00 |
| namespace.unmap | enumerated | 2 | 1 | 0 | 0 | 1 | 0 | 1 | 1 | 0 | 1 | 1 | 0.67 |
| namespace.unmap.routine-result | enumerated | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| namespace.mapped-refusal | enumerated | 2 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1.00 |
| namespace.legacy | enumerated | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.00 |
| namespace.version.mapped-reader | enumerated | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0.50 |

#### operations/repositories.g5.md (chunk D)

Blocks 2 (2 enumerated, 0 structural), units 5. Obligations 6: full 1, partial 2, absent 3, unassessed 0 (coverage 0.167). Checks: proved 0, bounded 0, refuted 1, unchecked 2, none 3. Aliases 0, excluded units 0.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| repo.manage | enumerated | 1 | 1 | 2 | 0 | 0 | 0 | 1 | 1 | 2 | 0 | 0 | 0.25 |
| repo.default-registry | enumerated | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0.00 |

#### operations/settings.g5.md (chunk D)

Blocks 7 (7 enumerated, 0 structural), units 27. Obligations 36: full 21, partial 8, absent 7, unassessed 0 (coverage 0.583). Checks: proved 19, bounded 0, refuted 2, unchecked 8, none 7. Aliases 5, excluded units 1.

| Block | Status | Full | Partial | Absent | Unass. | Proved | Bounded | Refuted | Unchecked | None | Aliases | Excluded units | Coverage |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| settings | enumerated | 1 | 3 | 6 | 0 | 0 | 0 | 1 | 3 | 6 | 4 | 0 | 0.10 |
| settings.print | enumerated | 2 | 1 | 0 | 0 | 2 | 0 | 0 | 1 | 0 | 0 | 0 | 0.67 |
| settings.writes | enumerated | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| settings.report-format | enumerated | 2 | 1 | 1 | 0 | 2 | 0 | 0 | 1 | 1 | 0 | 1 | 0.50 |
| settings.analytics | enumerated | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 1.00 |
| settings.default-modifiers | enumerated | 9 | 2 | 0 | 0 | 8 | 0 | 1 | 2 | 0 | 0 | 0 | 0.82 |
| settings.install-flags | enumerated | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0.00 |
