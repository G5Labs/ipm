# Developing and synchronizing modules

Commands that help people write and maintain modules in a namespace. None of them is recorded in [history](history.g5.md#history.actions).

## Definitions

- **File hash**: a fingerprint of a file's contents that `sync` uses to tell whether the file changed.

## Working on a module

<a id="devtools"></a>
[I · devtools]

IPM includes tools to keep developer-mode modules in step with files on disk, to write manifests and to script IPM.

- Working assumption: Generation skips an invalid source file and may produce a manifest without it.

## Synchronizing edited files

<a id="devtools.sync"></a>
[I · devtools.sync]

`sync [<module>]` brings changed files from disk into developer-mode modules without a full reload. It finds files changed since the last load or sync by their hash and recompiles only what is out of date.

- With no module, it syncs every developer-mode module in dependency order. As built, only dependencies between developer-mode modules count, so a module that depends on another through a module not in developer mode can be synced first (defect C:D37).
- `-delete` also removes items whose files were deleted, and `-test` runs test classes that changed. Without `-delete`, sync keeps those items and reports how many deleted files it did not apply.
- A changed `module.xml` is reloaded first, so files of a newly declared resource are synced as new.
- Within those folders, sync finds files at any depth and in any extension case, except under `.git`, `__pycache__` and `node_modules`. A changed file it cannot read fails the sync, and the next sync retries it.
- Sync applies class, include, routine, `.LOC` and `.DFI` files. Global data, FileCopy and WebApplication resources have no sync support, so sync leaves their edits unapplied and names them only with `-verbose`, which suggests `reload`.
- Sync looks for a resource's files only in `{SourcesRoot}/{Directory}`, where Directory defaults to the file type, such as `cls`. As built, a module whose class files sit directly under SourcesRoot still loads, but sync never sees their edits or deletions and reports "Nothing to sync" (defect C:D36).
- `load -skip-sync` leaves recording the hashes to the next `sync`.

- A failed sync with an existing baseline keeps its retry mark. The next sync retries that file even if its bytes equal the stored baseline.

- A failed compile, load or reload outside sync can change server code without setting that mark. Reverting to exact baseline bytes can then make sync report nothing to do while the server keeps the edited code; this is the separate outside-sync premise and P:F-SYNC-1.

- A test failure after successful sync activation happens after hash commit and does not undo it.

- Working assumption: `sync` with no module name reports each failure and a summary, then returns OK.
- Working assumption: Files that a failed `compile`, `load -dev` or `reload` put on the server are re-detected by the next `sync`.

<a id="devtools.sync.retry"></a>
[I · devtools.sync.retry]

If a file fails to sync, the next `sync` tries it again. With an existing baseline, `SyncFailed` keeps it selected even after an exact-byte revert.

A compile, load or reload outside sync follows the distinct [outside-sync condition](#devtools.sync); it may not set a retry mark.

<a id="devtools.sync.compile-after-resolve"></a>
[D · devtools.sync.compile-after-resolve]

After loading changed files, sync recompiles the module's compilable resources so dependent code is refreshed. A resource whose contents cannot be listed is left out with a warning, and sync still succeeds.

- The other resources are still compiled. If no resource is left to compile, no compile runs at all.
- Classes in the left-out resource keep their old compiled code. A changed file loaded by the default path was already compiled when it was loaded. Its hash is still recorded, so the next sync does not retry it, and only the dependents stay stale.
- The warning is always printed. A whole-namespace sync also keeps it in its final summary; a single-module sync does not.
- Only class files that are already defined, and all non-class files, are recompiled. Resources compiled from a deployed project are skipped without a warning.

<a id="devtools.sync.empty-hash"></a>
[D · devtools.sync.empty-hash]

Stamping a supplied empty file hash fails before looking up or saving a baseline row. An existing row keeps its hash. A nonempty hash is saved only after the row opens successfully, and a save failure is returned.

`StampModule` computes file hashes before calling this helper and skips empty computed values. That skip differs from the error returned by a direct call supplying an empty hash.

## Selecting and reporting tests during sync

<a id="devtools.sync.test-paths"></a>
[D · devtools.sync.test-paths]

Sync turns a changed test file's path into a class name by removing the resource folder as plain leading text. It does not check that the path is actually inside that folder.

- A path outside the folder keeps all of its folders in the class name: `other/SyncTest/Tests/Trivial.cls` becomes `other.SyncTest.Tests.Trivial`. The method comment promises "" for such paths.
- A sibling folder that starts with the same text is treated as inside: with folder `tests/unit`, `tests/unitX/A/B.cls` becomes `X.A.B`.
- Files that are not `.cls` give "". Whether a path outside its resource can reach this step depends on how sync routes files to resources, which was not traced end to end here.

<a id="devtools.sync.skipped-tracking"></a>
[D · devtools.sync.skipped-tracking]

Sync summary recording is local to the current process and is enabled around a whole-namespace sync with developer modules to check. A named-module sync still skips verify-only tests, but it does not start the aggregate recorder; verbose output can show the skip.

- While recording is inactive, `AddSkipped` writes nothing. While active, it increments that module's sequence and stores the supplied class at that sequence. Other modules keep their own counters, and repeated classes remain separate entries.
- Reports follow the supplied module order and each module's stored sequence. Reporting preserves the records; starting a new recording clears the old ones, and ending recording clears them. The whole-namespace caller ends recording after reporting. Its source also clears recording if the loop throws; that branch is source-inspected here.
- The checked directory workflow reloads a changed verify-only class without running its test callback. Whole-namespace sync reports the skipped class and then clears the recorder; named-module sync has no aggregate report. General test-resource routing, callback failures and other summary fields remain separate boundaries.

## Creating or importing a module

<a id="devtools.authoring"></a>
[I · devtools.authoring]

Commands for writing modules:

- `generate` writes a `module.xml`, asking for details, including a tests folder.
  - [Generation details](#devtools.authoring.generate).
- `init` sets up a namespace for IPM and its editor extensions.
- `arrange` puts a manifest's resources in the standard order.
  - It rewrites the stored `<Name>.ZPM` document, not `module.xml` on disk. Resources are written in name order inside a `<Resources>` element, and fields such as `LastUpdated` and dependency `DisplayName` are added.
  - As built, `arrange` works only when the namespace has a source-control class. Otherwise it fails with "Not logged into source control system" and changes nothing (defect C:D49).
  - Comments before `Name` and inside dependencies are kept. As built, comments before each resource and a module's last comment are dropped (defect C:D50).
- `dependency-analyzer` finds references to items outside the module and its dependencies.
- `module-version` sets or bumps a module's version, and refuses a downgrade unless `-force` is given.

<a id="devtools.authoring.generate"></a>
[D · devtools.authoring.generate]

`generate -t <dir>` writes a fixed example manifest without asking questions. Interactive `generate` writes the answers it is given, plus source resources grouped by package, test classes as UnitTest entries and the dependencies entered as `name:version`.

- As built, with `-author` the License and Copyright Date answers are stored in each other's fields (defect C:D54).
- `-export <flags>,<mask>,<ignore>` first exports matching namespace items into the source folder. As built, the ignore list applies only to classes, and routines match when the mask appears anywhere in their name, so ignored or unrelated routines are exported and become resources (defect C:D55).

<a id="devtools.authoring.import"></a>
[D · devtools.authoring.import]

`import <file|dir>` loads class files into the current namespace and compiles them. A directory is searched recursively unless `-norecurse` is given, and `-nocompile` loads without compiling.

- A class that fails to compile fails the command with "Error in import", while the other classes in the same import stay loaded and compiled. A missing path fails before anything loads.
- When the namespace has a source-control class, IPM first tells it which items will load, then loads them, then saves each loaded item through it. An item it already tracks is re-exported, and a new one is added.

<a id="devtools.authoring.module-version"></a>
[I · devtools.authoring.module-version]

`module-version` changes the version in the module's stored manifest and its installed record together, or changes neither.

- `major`, `minor` and `patch` bump that part and reset the parts after it. `-prerelease` and `-build` then set those parts.
- A version that is not higher than the current one is refused unless `-force` is given. A request for the current version should report it unchanged; as built, it is refused. This is a known defect.
- The `module.xml` file the module was loaded from is not changed. Whether it should be is [open](../../Verus_verification/open_questions.md#devtools.authoring.module-version.file).

- Working assumption: no, as built. The command edits the namespace's copy, and source control hooks, when present, take it from there.

## Generating resource declarations

<a id="devtools.generate.resources"></a>
[D · devtools.generate.resources]

`generate` keeps classes under the chosen tests folder out of application resources, even when that folder is inside the source folder. It writes those tests as UnitTest entries with Phase="test" and their package and test directory.

- A missing source folder fails before resources are added. A missing tests folder adds no tests.
- Files with unsupported extensions or source that cannot load are skipped. [Whether invalid source should instead fail generation is open](../../Verus_verification/open_questions.md#devtools.generate.invalid-source).
- Classes are grouped by directory and their shared top-level package. The exact grouping algorithm has source evidence but no general proof here.

- Working assumption: Generation skips an invalid source file and may produce a manifest without it.

<a id="devtools.generate.webapps"></a>
[D · devtools.generate.webapps]

Generating web-app resources always writes their authentication mode and keeps values supplied by the JWT/CORS prompts ahead of provider values. It leaves out the installed namespace and app-default marker so installation can choose them.

- A file-serving app with no dispatch class gets a Path. When its old path is under the current CSP root, the declaration replaces that root with `{$cspdir}`. Dispatch-only apps do not get that file path.
- A chosen CSP source folder becomes a FileCopy resource whose target is the app's declared Path. Loading the generated module creates the app and copies its files there.
- JWT prompt fields enableJWT, JWTAccessTokenTO and JWTRefreshTokenTO become JWTAuthEnabled, JWTAccessTokenTimeout and JWTRefreshTokenTimeout. Prompt values win even when a provider reports other values.
- Missing-app fallback, wildcard app discovery, all CORS properties and every nondefault provider property retain source evidence beyond the scoped proofs.

<a id="devtools.generate.cors"></a>
[D · devtools.generate.cors]

Generation validates comma-separated CORS origins in order. It stops at the first provider rejection and returns that failure, or succeeds after every origin is accepted.

The validator runs in `%SYS` and restores the caller's namespace. The accepted origin syntax belongs to the Security.Applications provider; this contract does not invent a separate URL policy.

## Running scripts and initializing a namespace

<a id="devtools.scripting"></a>
[I · devtools.scripting]

Commands for scripting:

- `run-from-file` runs IPM commands listed in a `.txt` file or a JSON object whose `commands` member is an array. `-dry-run` lists prepared commands without running them. The single-array sentence and `.text` help example are inherited documentation defects (P:F-RES-5); the object format is the settled intent (DEC-RES-2).
- `exec` runs ObjectScript.
- `import` loads classes from files and re-exports them to source control if needed.

<a id="devtools.scripting.file-sequence"></a>
[D · devtools.scripting.file-sequence]

IPM prepares the whole command file before running it. Text preparation trims and collects nonempty lines; JSON preparation also validates and translates every command, so a later invalid JSON command prevents earlier commands from running.

- During execution, a failing command stops later commands and keeps effects already made. The checked settings reader sees the first write after a later command fails, and the second write when both succeed.
- Dry-run lists prepared commands without executing them. An invalid JSON command still fails preparation; a text line is not checked as a shell command until execution.

<a id="devtools.exec"></a>
[D · devtools.exec]

`exec <expression>` runs ObjectScript in the current namespace. Data changes it makes persist, and a runtime or syntax error fails the command.

- As built, the expression runs under a saved `$namespace`, so changing the namespace has no lasting effect, although the command help gives exactly that as its example (defect C:D56).

<a id="devtools.init"></a>
[D · devtools.init]

`init -noprompt` reindexes the filesystem cache and changes the namespace's source-control setup only as asked. A configured class that no longer exists is disabled, and `-extension <class>` makes IPM's composite extension active with that class as primary.

- As built, without `-extension` an existing plain class is only stored as the composite's primary and stays the active class, although the command reports that it enabled the package manager extension. The IPM hooks of the [source-control statement](../architecture/lifecycle.g5.md#lifecycle.source-control) then never run (defect C:D57).

## Related rules on other pages

<a id="devtools.sync.test-stop"></a>
- What makes a command fail — [devtools.sync.test-stop](../concepts/testing-policy.g5.md#devtools.sync.test-stop).

## Metadata

### devtools

- uncertain
- human-provided
- user-facing-behavior
- Children: [sync](#devtools.sync), [authoring](#devtools.authoring), [scripting](#devtools.scripting).

Sources: `Main.cls:85-119`, `:322-404`, `:509-522`, `:639-705`, `:781-805`. Inventory: CAT-31, CAT-36.

Open questions: [Should `generate` report or refuse a source file that cannot load?](../../Verus_verification/open_questions.md#devtools.generate.invalid-source)

### devtools.sync

- design-verified

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Developer tools](#devtools).

- Children: [retry](#devtools.sync.retry), [compile after a resolution failure](#devtools.sync.compile-after-resolve), [test stop](../concepts/testing-policy.g5.md#devtools.sync.test-stop), [skipped-test tracking](#devtools.sync.skipped-tracking), [test paths](#devtools.sync.test-paths).

Sources: `Main.cls:108-119`; `wiki/02` L1136-1189; CHANGELOG 0.10.10 (`#1117`), 0.10.11 (`#1290`). Inventory: CAT-31; A2 1.2 row 6 (`Main:Sync`, `General/Sync/Pipeline.cls`). Tests: `Sync.cls` `TestNoChangeIsNoOp`, `TestModifiedClassRecompiles`, `TestLoadSkipSyncStampsNoBaseline`. Observed: `sync` runs without a transaction.

Model: [sync.rs](../../Verus_verification/consolidated/iter32/sync.rs) `sync`, `delete_schedule`, `flat_schedule`, `order_neighbors`. Iteration32 native cases cover `-delete` against its absence, an edit with a deletion, a single-class resource, a manifest that adds a resource, the flat-layout contrast and whole-namespace order ([handoff](../../uplift/consolidated/iter32/HANDOFF.md)).

Findings: [A module whose classes sit directly under SourcesRoot loads, but sync never sees its edits or deletions](../../Verus_verification/bugs_issues.md#c-d36)

Findings: [Whole-namespace sync ignores dependencies that pass through a module not in developer mode](../../Verus_verification/bugs_issues.md#c-d37)


Iteration45 discovery: [scan.rs](../../Verus_verification/consolidated/iter45/scan.rs) `depth_and_case_do_not_matter`, `unreadable_fails_then_retries`, `skipped_directory_never_synced` ([handoff](../../uplift/consolidated/iter45/HANDOFF.md)).

Iteration65: [moderefresh.rs](../../Verus_verification/consolidated/iter65/moderefresh.rs) `sync_skips_unsupported` ([handoff](../../uplift/consolidated/iter65/HANDOFF.md)).

Open questions: [Should sync without a module name return an error when a module fails?](../../Verus_verification/open_questions.md#devtools.sync.aggregate-status-policy), [Should sync retry a failed compile or reload after its source is reverted?](../../Verus_verification/open_questions.md#devtools.sync.failed-import-policy)

State machines: [sync · iter32](../../Verus_verification/models/iter32-sync/manifest.json), [scan · iter45](../../Verus_verification/models/iter45-scan/manifest.json), [moderefresh · iter65](../../Verus_verification/models/iter65-moderefresh/manifest.json)

### devtools.sync.retry

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Sync](#devtools.sync).

Sources: issue #1224; `General/Sync/Pipeline.cls:146-153`, `:201-222`. Inventory: CAT-31, A1 seed 10. Test: `Sync.cls` L653-734.

### devtools.sync.compile-after-resolve

- design-verified

- agent
- user-facing-behavior
- Expands: [Sync](#devtools.sync).

Direct decision: `General/Sync/Pipeline.cls::SyncCompile`. Callers and readers: `::Run` steps 7 and 8 (`$$$ThrowOnError` on its status, then hash commit in step 9), `::SyncDispatchProcessors` (loads each changed file with compile), `General/Sync/Summary.cls::AddWarning` (keeps text only while the summary is active). Verus [sync_compile.rs](../../Verus_verification/consolidated/iter4/sync_compile.rs) `{sync_compile,sync_compile_neighbors}`; six controls rejected. Native Y1 (survivor recompiled to the new definition, failed resource stays stale, warning retained with an active summary) and Y2 (no compile, status OK, no retained warning) call the real private step through a subclass, with a fixture processor whose resolution fails on demand. The full `sync` command around this step was not run. Answers public case I3F20.

State machines: [sync compile · iter4](../../Verus_verification/models/iter4-sync-compile/manifest.json)

### devtools.sync.empty-hash

- design-verified

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [devtools.sync](#devtools.sync).

Sources and readers: `Storage/FileHash.cls::StampOneFileWithHash` → `StampModule`, `OpenByPath` and later sync baseline reads. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `stamp`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### devtools.sync.test-paths

- design-verified

- agent
- user-facing-behavior
- Expands: [Sync](#devtools.sync).

Direct decision: `ResourceProcessor/Test.cls::RelPathToClassName`; caller `::OnSync`. Verus [repairs4.rs](../../Verus_verification/consolidated/iter4/repairs4.rs) `{package_rel_path,rel_path_neighbors}`; controls `outside-path-returns-empty` and `prefix-requires-boundary` rejected. Native C cases call the real private method through a subclass. The comment contradiction is recorded as suspected defect C:D21; its reach through sync routing is unproved. Answers public case I2C04.

Findings: [C:D21 — partial reproduced](../../Verus_verification/bugs_issues.md#c-d21). The linked entry qualifies the caller, reader and providers.

State machines: [repairs4 · iter4](../../Verus_verification/models/iter4-repairs4/manifest.json)

### devtools.sync.skipped-tracking

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Sync](#devtools.sync).

[Iteration 26](../../uplift/consolidated/iter26/HANDOFF.md) and the [summary contract](../../Verus_verification/consolidated/iter26/sync_summary.rs) connect `Main.Sync` tracking lifetime, `Summary.Begin`/`End`, the active guard and per-module increment in `AddSkipped`, stored module/sequence/class readers and the selected `Report` loop. Direct API neighbors cover inactive, interleaved, duplicate and reset records; ordinary named and namespace syncs qualify the verify-only skip, actual class reload, absence of a test call, aggregate output and cleanup. Existing compile/test-stop contracts remain valid and do not supply these missing stored fields. This addresses an explicitly disclosed closed-panel coverage gap; it is repair evidence, not unseen discovery. No permanent SQL/history storage or full sync-body proof is implied.

State machines: [sync summary · iter26](../../Verus_verification/models/iter26-sync-summary/manifest.json)

### devtools.authoring

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Developer tools](#devtools).
- Children: [module-version](#devtools.authoring.module-version).

Sources: `Main.cls:85-99`, `:639-705`, `:781-805`; CHANGELOG 0.10.11 (`#150`, `#806`, `#503`), 0.10.0 (`#710`). Inventory: CAT-36; A2 1.2 rows 4, 29, 30, 32, 37. Tests: `Generate.cls`, `ModuleVersion.cls`.


Iteration44 `arrange`: [comments.rs](../../Verus_verification/consolidated/iter44/comments.rs) `arrange`, `without_source_control_nothing_changes`, `resource_comments_dropped`, `trailing_comment_dropped`, `leading_and_dependency_comments_kept` ([handoff](../../uplift/consolidated/iter44/HANDOFF.md)).

Findings: [`arrange` fails in a namespace without a source-control class](../../Verus_verification/bugs_issues.md#c-d49)

Findings: [`arrange` drops comments placed before resources and a module's last comment](../../Verus_verification/bugs_issues.md#c-d50)

State machines: [comments · iter44](../../Verus_verification/models/iter44-comments/manifest.json)

### devtools.authoring.generate

- design-verified

- agent
- user-facing-behavior
- Expands: [Authoring commands](#devtools.authoring).

Sources: `Main.cls::GenerateModuleXML`; `Storage/ModuleTemplate.cls`. Model: [generator.rs](../../Verus_verification/consolidated/iter53/generator.rs) `author_swapped`, `export_neighbors`, `grouping_neighbors` ([handoff](../../uplift/consolidated/iter53/HANDOFF.md)).

Findings: [`generate -author` stores the license and copyright answers in each other's fields](../../Verus_verification/bugs_issues.md#c-d54)

Findings: [`generate -export` ignores the ignore list for routines and matches routine masks anywhere in the name](../../Verus_verification/bugs_issues.md#c-d55)

State machines: [generator · iter53](../../Verus_verification/models/iter53-generator/manifest.json)

### devtools.authoring.import

- design-verified

- agent
- user-facing-behavior
- Expands: [Authoring commands](#devtools.authoring).

Sources: `Main.cls::Import`; command help. Model: [importcmd.rs](../../Verus_verification/consolidated/iter49/importcmd.rs) `norecurse_skips_subdirectories`, `nocompile_defines_only`, `one_bad_item_fails_command_but_keeps_good`, `source_control_routing`, `missing_path_fails` ([handoff](../../uplift/consolidated/iter49/HANDOFF.md)).

State machines: [importcmd · iter49](../../Verus_verification/models/iter49-importcmd/manifest.json)

### devtools.authoring.module-version

- uncertain
- agent
- user-facing-behavior
- test-coverage
- Expands: [Authoring](#devtools.authoring).
- Children: [Should `module-version` also update the `module.xml` file a module was loaded from?](../../Verus_verification/open_questions.md#devtools.authoring.module-version.file).

Sources: `Main.cls:781-795` (help), `:4011-4099` (one transaction around the record save and `StudioDocument/Module.cls:UpdateVersion`, rolled back on any error), `:4060-4067` (the "unchanged" branch after the higher-version check); CHANGELOG 0.10.0 (`#710`); test `ModuleVersion.cls`. Native MV0-MV10 (`20261114T033125Z-r4-owners`, ARM64) and model `Verus_verification/consolidated/models/owners.py`: `minor` 1.2.3 -> 1.3.0; `1.0.0` refused, then accepted with `-force`; `patch -prerelease rc.1 -build b7` -> 1.0.1-rc.1+b7; no version (MV5) refused as "not strictly higher" (defect C:D13); a prerelease of another version refused (MV6); `01.0.0` invalid (MV7); `major` -> 2.0.0; the record and the stored manifest always agree, and the file stayed 1.2.3 (MV10). The rollback path was not exercised natively.

Extension Verus: [owners.rs::bump](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::module_version](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::with_build](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::with_pre](../../Verus_verification/consolidated/verus/owners.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [C:D13 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d13). The linked entry qualifies the caller, reader and providers.

Open questions: [Should `module-version` also update the `module.xml` file a module was loaded from?](../../Verus_verification/open_questions.md#devtools.authoring.module-version.file)

### devtools.generate.resources

- design-verified

- uncertain
- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Authoring](#devtools.authoring).
- Children: [Should `generate` report or refuse a source file that cannot load?](../../Verus_verification/open_questions.md#devtools.generate.invalid-source).

Sources: `Storage/ModuleTemplate.cls::{ReadResources,ReadItems,AddPackages,GroupClasses,ProcessResources,ReadFolder}` → `SaveFile`. Tests: `Generate::{TestTestsUnderSourceFolderAreNotPackaged,TestUnitTestsFromTestsFolder,TestMissingSourceFolderThrows}`. [Iteration 6 index](../../uplift/consolidated/iter6/INDEX.md) S6-01. [Contract](../../Verus_verification/consolidated/iter6/workflows.rs): `application_item`, `test_item`, `template_status`; normalized paths, file-loader success and filesystem existence are explicit provider inputs. General package grouping remains unproved.

Open questions: [Should `generate` report or refuse a source file that cannot load?](../../Verus_verification/open_questions.md#devtools.generate.invalid-source)

State machines: [generator · iter53](../../Verus_verification/models/iter53-generator/manifest.json), [workflows · iter6](../../Verus_verification/models/iter6-workflows/manifest.json)

### devtools.generate.webapps

- design-verified

- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [devtools.authoring](#devtools.authoring).

Sources and readers: `Storage/ModuleTemplate.cls::{SetJWTProps,SetCORSProps,AddWebApps,SetSourcePathForCSPApp,ProcessResources,SaveFile}` → `Main.cls::GenerateModuleXML`, WebApplication and FileCopy phase readers. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/webapps.rs): `jwt_prompt`, `generated_props`, `generated_path`, `manifest_path`, `filecopy_target`, `copy_one`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

State machines: [webapps · iter8](../../Verus_verification/models/iter8-webapps/manifest.json)

### devtools.generate.cors

- design-verified

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [devtools.authoring](#devtools.authoring).

Sources and readers: `Storage/ModuleTemplate.cls::ValidateCorsOrigin` → `Main.cls::GenerateModuleXML` retry prompt and `SetCORSProps`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `split`, `cors_calls`, `cors_ok`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### devtools.scripting

- human-provided
- user-facing-behavior
- Expands: [Developer tools](#devtools).

Sources: `Main.cls:322-404`, `:509-522`; CHANGELOG 0.10.6 (`#996`). Inventory: CAT-36; A2 1.2 rows 18, 19, 23. The `run-from-file` help says the JSON root is "a single array" (`Main.cls:352`), while the examples and code need an object with `commands`. Observed: each line of `run-from-file` runs as its own shell command with its own history record.

### devtools.scripting.file-sequence

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Scripting](#devtools.scripting).

[Scoped contract](../../Verus_verification/consolidated/iter28/script.rs) connects selected raw files and qualified JSON parsing to preparation, ordered execution, fail-stop, dry-run listing and actual stored-setting reads. [Evidence index](../../uplift/consolidated/iter28/INDEX.md). The selected config writer/read relations reuse unchanged settings definitions; known child-command execution/SDK errors, JSON parsing, file IO and command metadata are qualified providers. The runtime-error fixture increments a job-local counter before division by zero; no full ObjectScript interpreter, arbitrary JSON/lexer/quoting or rollback guarantee is inferred. Settings are read through GetValue and independently through the actual `%SYS` global. This closes a builder-selected unmodeled workflow, not a new defect or an unseen-case score.

State machines: [script · iter28](../../Verus_verification/models/iter28-script/manifest.json)

### devtools.exec

- design-verified

- agent
- user-facing-behavior
- Expands: [Scripting](#devtools.scripting).

Sources: `Main.cls::ExecuteCOS`; command help example. Model: [nsinit.rs](../../Verus_verification/consolidated/iter55/nsinit.rs) `exec`, `exec_neighbors` ([handoff](../../uplift/consolidated/iter55/HANDOFF.md)).

Findings: [The `exec` help example that changes the namespace has no effect](../../Verus_verification/bugs_issues.md#c-d56)

State machines: [nsinit · iter55](../../Verus_verification/models/iter55-nsinit/manifest.json)

### devtools.init

- design-verified

- agent
- user-facing-behavior
- Expands: [Authoring commands](#devtools.authoring).

Sources: `Main.cls::Init`; `ExtensionBase/CompositeMethodOverrides.cls::SetExtensionClasses`. Model: [nsinit.rs](../../Verus_verification/consolidated/iter55/nsinit.rs) `init_quiet`, `init_neighbors` ([handoff](../../uplift/consolidated/iter55/HANDOFF.md)). Interactive prompts, `-zpm` and `-menuextension` are not assessed.

Findings: [`init -noprompt` reports enabling the package manager extension but leaves a plain source-control class active](../../Verus_verification/bugs_issues.md#c-d57)

State machines: [nsinit · iter55](../../Verus_verification/models/iter55-nsinit/manifest.json)
