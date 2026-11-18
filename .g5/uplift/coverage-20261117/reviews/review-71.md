# Review-71: checkpoint review of the obligation census and iteration69

The census grades too many obligations as full. In the sample, 8 of 45 full grades (18%) are too generous. The common cause is a model that writes the outcome down as a constant or a lookup table instead of computing it from product state. Iteration69 is sound overall: the model follows the product's real decisions, and C:D72 and C:D73 are real and reachable through ordinary `generate` and `load`. But one of its full grades is too generous, its general theorems are weaker than they look, and both new findings are rated below the project's own severity policy.

Everything was read-only. I made no repository edits, started no Docker containers and found no STOP file. `census.py check` exits 0. Scratch work is under `/tmp/ipmcov-review71-{work,c,g1..g5}`. Part A and part B were run by five parallel read-only reviewers. Part C (iteration69) I did myself. I re-checked each reviewer's strongest claims against source; those are marked "confirmed" below.

## A. Census fidelity

**How the sample was drawn:** 15 of the 27 pages. On each page I sorted the full obligations by block and number and took the first, middle and last, then took the first and middle of the sorted partial and absent list. That gives 75 obligations (`/tmp/ipmcov-review71-work/draw.json`). The positions were fixed, not random, so this is a rough indicator rather than a project-wide rate.

| Page | Agree | Page | Agree | Page | Agree |
|---|---|---|---|---|---|
| architecture/lifecycle | 4/5 | features/build-and-publish | 3/5 | features/manifest | 3/5 |
| architecture/package-sources | 5/5 | features/uninstall | 5/5 | features/update | 4/5 |
| concepts/data-policy | 5/5 | features/developer-tools | 5/5 | features/resources | 4/5 |
| concepts/transactions-and-dev-mode | 4/5 | features/history | 4/5 | operations/settings | 5/5 |
| features/inspection | 4/5 | features/load | 4/5 | operations/namespaces | 5/5 |

- **Overall:** 64 of 75 agree (85%).
- **Full grades:** 37 of 45 agree. The 8 disagreements are all "should be partial". A Wilson 95% interval puts the rate of too-generous full grades at roughly 9–31%. Extrapolated with care, the headline 0.648 coverage may be closer to 0.45–0.59.
- **Partial and absent grades:** 27 of 30 agree. All 3 disagreements are grades that are too strict.
- **Receipts:** every sampled receipt is current (hash matches the current file, 0 errors). One bookkeeping gap: `iter41/settings.rs` and `iter50/defmods.rs` are missing from the iter70 receipt index the records cite. Their older `run.json` files do match, and both re-ran clean.

### Finding 1 (most material): full credited to models that write down the claim instead of computing it

| Obligation | Evidence | Disposition |
|---|---|---|
| module.parameter.cycles#1 (full/refuted) | `resolve_bounded` returns `circular_error: false` on every branch (`consolidated/verus/attr_evaluator.rs:178,181`), so `declared_cycle_error_is_unreachable` is true by construction. The product guard is inside the loop at `Storage/ModuleSetting/Default.cls:69-75`. Confirmed. | Regrade to partial. Model the loop and guard. |
| build.archive.empty-file#1 | `creates_file` never reads `size` (`iter6/workflows.rs:36`). The deciding branch is `Utils/FileBinaryTar.cls:181-188`. Confirmed. | Partial. Model both write branches. |
| transaction.rollback-scope#7 | Which kinds roll back is a constant table, `failkeep.rs:21,25`. Its sibling #1 cites the same table and is already partial. | Partial. Then decide the policy once (see below). |
| load.status.after-commit#4 | `temp_repo.rs:45-48` is an outcome table. No model injects a failure after a dependency commits (`Utils/Module.cls:1347-1361`). | Partial. Add that fault to the explorer. |
| module.env#4 | No theorem or control covers the readable-file case. A loader that always fails still verifies (`env_config.rs:121-141`). Same weakness in module.env#10. | Partial. |
| module.lifecycle-class#6 | `record.rs:21` returns only the lifecycle class, so "without a warning" is never observed. The stored LifecycleClass is also overwritten (`Storage/Module.cls:1443-1446`). | Partial. Same for its alias #7. |
| module.global.roundtrip#1 | Only the no-Directory branch is asserted (`workflows.rs:47`). An explicitly empty Directory gives `gbl/` in the product (`Default/Global.cls:107-108`) but drops the segment in the model. Confirmed. | Partial. Model fix. |
| inspect.list-installed#1 | `version-desc` and the `date` modes are not modelled (`Main.cls:3040-3044`, confirmed; `listing.rs:43-48`). | Partial, or narrow the text. |

Part B found the same pattern outside the sample:
- uninstall.cleanup#5 and #6 rest on the constant table `cleanup.rs::remains`.
- devtools.authoring.module-version#3 exercises only the equal-version case, and #5 holds by construction.
- history.committed#4 claims every action but its models cover load only.

**Proposed disposition:**
- Add a census rule: a rule whose deciding value is a constant or a per-kind table counts as taking the decision as input, so it is partial. This settles the conflict between iter68 `HANDOFF.md:304` (which accepts category tables) and the iter64/iter70 handoffs.
- Add a mechanical check per full grade: remove the deciding product branch from the model and confirm the cited rule then fails.
- Re-audit the remaining 652 full grades with that check before publishing coverage.

### Finding 2: grades that are too strict (models exist)

- **build.publish#2, absent → full (or at least partial).** `playbook-uplift/models/verus/settings.rs:315-318`, with proofs at `:444-450` and `:467-476`. The receipt is current, and the census already cites it for repo.manage#3.
- **update#2, absent → partial.** `iter34/versions.rs` models the `Install` latest-version choice (`Main.cls:2603`). With prereleases enabled it picks `2.0.0-beta.1` over `2.0.0` (`versions.rs:200-202`), which contradicts "latest".
- **history.actions#2, absent → partial.** `iter22/reinstall.rs::history_neighbor` (`:175-186`) has a current receipt. Separately, as built, update and reinstall refusals throw before any history row is written (`Main.cls:4415-4423`, `2685-2687`); record that as a refuted obligation.
- **lifecycle.custom#2 (part B), absent → partial or full.** `iter48/cpfmerge.rs:44-46` contrasts a requested custom phase with an ordinary load. The receipt is current.

### Finding 3: ontology text contradicted by the product (ontology fixes)

- **lifecycle.module-action** (`lifecycle.g5.md:50`): `update` is dispatched to `Main::Update` before the standard-phases check (`Main.cls:1118`), so it is not a phase shortcut. Confirmed.
- **namespace.enable B9:** `enable -community` catches each step's failure, keeps going and reports no error (`Main.cls:3564-3574`). Confirmed. The record's contrast describes the intent, not the behaviour.
- **history.committed.marking#1** (graded full) is false for uninstall. `SetCommitted` is called only from `Utils/Module.cls:1227,1313,1350`. Uninstall's Committed comes from `AbstractHistory.cls:131` (`Committed || ISOK(status)`). Confirmed.
- **inspect B1:** `-versions` also lifts the platform filter (`Utils/Module.cls:938`).
- **settings T3:** HistoryRetain is never seeded at setup (`Installer.cls:110-124`), so `config get` shows empty, not 0.

## B. Census completeness (10 blocks)

| Block | Findings |
|---|---|
| lifecycle.custom | The `-only` case for a custom phase alone has no proof. Two custom phase names together run nothing and succeed, and no record names that case. |
| repo.oras.integrity | B4 is excluded as an undecided alternative, but it is a working assumption, so rule 3 makes it an obligation. #1 drops the "pull without a tag" condition. #3's neighbour fails size and digest together, so a digest-only check would pass. |
| transaction.flag-sources | #1 merges two precedence sites (`Main.cls:1068-1070` and `Utils/Module.cls:1166-1169`). Which wins between default modifiers and `<Defaults>` is never stated (product: default modifiers). In #5, journaling being restored after the child activation is a separate outcome. B5 ("Defaults apply whenever installed") is missed and is refuted when a parent sets the flag. #8 drops "partial install". |
| uninstall.cleanup | #1 merges code, record, manifest and baseline. The manifest part is refuted for `-recurse` dependencies. The condition that mapped code is skipped unless `-global` is missing (`Lifecycle/Base.cls:375-378`). The Preserve condition is missing from #2's cases. |
| history.committed | #4 is over-scoped. "A successful dependency under a successful parent reads Committed=1" is missing. #8 merges NoTransaction (proved) with NoJournal (bounded only). |
| devtools.authoring.module-version | #1 merges success (proved, `owners.rs:132-136`) with failure (not modelled). #3 should be partial. #6 (source-control hand-off) may be false as built (`Main.cls:4084`). |
| load.options | B6 should alias deps.lockfile#1. B8 drops "including the transaction flags" (alias transaction.opt-out#4) and its alias target is imprecise. |
| module.env | #8 merges the intended promise with the as-built defect (rule 3). The B7 evidence sentence should be excluded. #2 adds "unset gives empty", which the block doesn't say. |
| module.processor.file-copy | The B2 exclusion is stale; alias it to build.copy.overlap#1. B8's "rollback does not turn a failed copy into success" needs its own split. The B8 third sentence is unaccounted for. #13 should alias protection#9. |
| settings.default-modifiers | Matches my own derivation. #10 merges "-set replaces" with "-set with no -D stores nothing". No grade change. |

## C. Iteration69 (generate grouping, writer collapse, load)

**What holds:**
- I derived the behaviour from `ModuleTemplate.cls` first. `grouping.rs` transcribes the real decisions rather than precomputed outcomes:
  - Directory is the path minus one segment per package segment (`:477-487`).
  - Groups are keyed by directory and top-level package and folded with SharedPackage (`:494-528`).
  - Leftover files are merged in, and tests-folder leftovers are dropped (`:401-408`).
  - The writer collapse follows `CodeGeneration.cls:4-31` and `ResourceReference.cls:509-537`. Names starting with "/" get the object reference in their sort key, so UnitTest entries are correctly not collapsed.
  - The load reader matches `Document.cls:72-125` (Directory, else the extension folder, else the source root).
- I re-ran the baseline: 48 verified, hash `bed62dde…` matches the receipt.
- The native run drove the real interactive `generate` prompts and then `load` (`session.log`). The replay asserts each observed value against a computed model expression, not a constant.

**Findings:**

1. **devtools.generate.resources#2 is graded full, should be partial.** `TestEntry` has only a directory and a package (`grouping.rs:32`). The UnitTest Name (`/src/tests/` when tests sit inside src) and `Phase="test"` are checked only by the native string match (`Reader.cls:46,59`). The handoff says so itself: "Phase/ProcessorClass constants checked natively". Under rule 4 that makes it partial.

2. **The general theorems don't pin down the grouping key, test exclusion or extension filter.** I stripped the instance assertions and re-ran the mutants:
   - `top-level-only-key`, `tests-not-excluded` and `unsupported-kept` each still verify all 41 general proofs.
   - `writer-keys-by-directory`, `directory-only-key`, `name-top-level-only` and `drop-leftovers` do fail the general proofs.
   - All 13 controls are rejected only through concrete instance assertions, and Verus stops before checking the general lemmas.
   - #1 takes `in_tests` (the tests-folder prefix check) as an input.
   - There is no general positive theorem saying that when no two directories produce the same group name, every class is written and loads. That rests on instances N5b, m1 and m3.

   Disposition: add those general theorems, or record the grades as instance-scoped.

3. **The replay is thin in places.** Only 3 of 59 observed fields are flipped to show the replay notices a change. The C:D73 load fields are not among them. The key order that decides which directory wins the collapse is a hand-written input; it matches the product's ordering for m3 but isn't derived.

4. **C:D72 is rated below policy.** It is rated Medium with no stated basis. The severity policy says "High covers incorrect results" (`severity-review/README.md`), and here classes are silently missing while `load` reports success. Reachability is confirmed: native case m3 through real `generate` and `load`. Its trigger wording is also imprecise: loss happens when two directories' groups get the same name. For example, x holding `A.B.C.X` and `A.B.D.Y` collides with y holding `A.B.E.Z` and `A.B.F.W`. And `A.B` in x next to `A.B.C` in y does not lose anything. Disposition: rate High or add a severity basis, and reword the trigger in both the finding and ontology B6.

5. **C:D73 is rated below policy, and its most ordinary trigger isn't stated.** It is rated Low because "the failure is loud", but under the policy visible misbehaviour is Medium. The root-class case is confirmed (m2a). Also, `generate -export` writes dotted routines and includes flat (`ModuleTemplate.cls::Expfilename:134-155`; only `.cls` files get nested folders), and the loader looks for them in nested folders (`Document.cls:34-38,145`). So `generate -export` itself produces a layout its own `load` can't read. That load step has only a scratch probe, not a committed receipt. Disposition: rate Medium, add a receipt for the export-then-load case, and mention it in ontology B7.

6. **Ontology block devtools.generate.resources.** It is accurate apart from the C:D72 wording. For readability:
   - The opening paragraph is about tests rather than what `generate` produces.
   - "A.B.C with A.B.D" doesn't say whether these are class or package names.
   - The Directory bullet only holds for files stored in package folders.
   - Not in the ontology: an existing `module.xml` is renamed to `module_<timestamp>.xml` (`SaveFile:662-665`).

## D. Coverage beyond the lists

**Consequential behaviours the census missed (source-confirmed unless noted):**
1. **`<m> activate configure` runs Configure twice.** `Lifecycle/Module.cls:102` calls `..Configure` inside `%Activate`, and Configure is also scheduled as its own phase, so installer callbacks run twice. Confirmed in source, not run natively. The comment at `phases.rs:211` ("runs each phase once") is false.
2. **A normal-mode parent install silently replaces a developer-mode dependency and clears its DeveloperMode.** The guard at `Utils/Module.cls:109` depends on an Install parameter that is removed at `:318-319`, and the stored row is written at `:1298`. Source reading only. This likely refutes dev-mode.overwrite-guard B3.
3. **`module-version` re-imports the whole module record from the `.ZPM` document,** replacing Dependencies, Resources, Defaults, Mappings and Invokes (`StudioDocument/Module.cls:62-66,149,190`). Source reading only.
4. **`enable -community` partial failure is reported as success** (`Main.cls:3564-3574`).
5. **The `generate -export` → `load` round trip fails for dotted routines and includes** (see C5).

**Gaps to prioritise next:**

| # | Gap | Deciding / writing / observing members | Contrast that closes it |
|---|---|---|---|
| 1 | Rollback scope as a writer model instead of tables | `Utils/Module.cls::LoadNewModule` transaction boundary / `Config.Map*`, `Security.Applications`, CPF merge, `%Clean` / namespace state after rollback | Failure after Activate: mapping undone vs CPF kept, derived rather than looked up |
| 2 | Parameter cycle guard | `ModuleSetting/Default.cls` loop (`:66-75`) / resolved values / phase parameter reader | `a=${b}, b=${a}` leaves `${a}` unresolved with no circular error, vs an acyclic pair that resolves |
| 3 | Dependency failure after its commit | `Utils/Module.cls:1347-1361` (`WriteAfterInstallMessage`) / parent rollback / module record and history row | Dependency's message step fails, so the parent rolls back to its first commit, vs success |
| 4 | Export → load round trip | `ModuleTemplate::Expfilename` / `ReadItems` / `Document::OnPhase` | Dotted routine exported then loaded fails, vs an undotted routine that loads |
| 5 | Committed flag for uninstall and update | `AbstractHistory::Finalize:131` / `Lifecycle/Base.cls:446-484` / history Committed column | Uninstall that fails after its deletes, vs one that succeeds |

## Not reviewed

- The 12 pages outside the sample: download-security, repositories, testing-policy, versions, artifact-download, dependencies, install, package-commands, packaging-deployed-code, web-applications, operations/ipm-setup and operations/repositories.
- Full grades in sampled blocks beyond the draw and the part B blocks.
- Explorer bounds and per-config breakdowns (load#1 and load.outcome.dev.early-failure#2 were checked from receipt counts only).
- Native runs, which I didn't repeat (no Docker).
- The iter70 models `dep_resolve.rs`, `load_general.rs` and `scopes`.
- The member-reach and binding scores. I didn't evaluate them.
- Builder handoffs: read only after forming verdicts, and only in part.

Adjudicating these findings, applying the repairs, and deciding the next assignment are for the master.
