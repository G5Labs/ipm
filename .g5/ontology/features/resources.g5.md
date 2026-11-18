# Installing module resources

The manifest lists the resources IPM must install. The processor for each resource type decides when it becomes usable and what Clean can remove.

## Definitions

A resource processor handles one kind of item during the module lifecycle. Reload reads resources into the namespace. Activate runs setup after compilation. Clean removes resources.

## Processor selection, defaults and ordering

<a id="module.processor"></a>
[I · module.processor]

Resource processors with special behavior include:

- `FileCopy`, which copies files or folders ([file copy](#module.processor.file-copy)).
- `Copy`, which copies classes from another namespace ([namespace copy](#module.processor.copy)).
- `.DFI` resources, which load Analytics folder items ([DeepSee items](#module.processor.deepsee)).
- `.LUT`, `.HL7`, `.X12` and `.ESD` interoperability documents ([interoperability documents](#module.processor.interoperability)).
- `WebApplication`, which sets up [web applications](web-applications.g5.md#module.webapp). `CSPApplication` is deprecated.
- `CPF`, which merges configuration into the instance ([CPF timing](#module.processor.cpf)).
- `SystemSetting`, [Artifactory downloads](artifact-download.g5.md#artifact), unit test and [localization](#module.processor.localized-messages) processors.

Processor defaults and processing order are [owned here](#module.processor.defaults).

<a id="module.processor.defaults"></a>
[D · module.processor.defaults]

A processor copies explicit attributes before applying matching defaults in declaration order. An explicit attribute, including an empty value, protects that property from defaults; without one, a later matching default can replace an earlier default.

- Each default tests its conditions against the processor's current values, including changes from earlier defaults. It must name the same processor class, using its full name or the built-in short form; inheritance alone is not a match.
- Supplying parameters again reevaluates attributes and defaults. It does not turn a default into an explicit attribute.
- Named document resources run first, sorted by name without the extension. Equal names keep their declaration order. Path-style and other resources follow in declaration order. The formal contract checks pair ordering and default precedence; a complete sorting refinement and all comparison operators remain open.

<a id="module.processor.default-sweep"></a>
[D · module.processor.default-sweep]

After successful individual replacements, a processor default's custom-parameter substitution makes one pass in parameter-name order. A replacement that introduces a later parameter can be filled in during that pass; one that introduces an already-visited parameter remains unfilled here.

This is the `EvaluateAttribute` helper used by processor attributes, not the repeated placeholder-resolution step for module defaults. The scoped rule covers ordinary parameter names and successfully decoded replacement values. A raw `$` in a replacement can fail before the next pass, as in the [existing literal-value defect](manifest.g5.md#module.parameter.literal-values). The native raw forward/backward examples both fail at that provider. Successful introduction of another placeholder by ordinary replacement values has not been established; the abstract sweep alone does not close that case.

## Database placement and package files

<a id="module.placement"></a>
[I · module.placement]

A module controls where its code and data live.

- `GlobalScope` makes it available in all namespaces without per-namespace mappings.
- `NoMapping` turns off the automatic mappings for its resources.
  - Automatic mapping applies when a resource would resolve to a database other than the namespace's own routine database, such as a `%Z` package or a subpackage of a package an administrator mapped elsewhere. Loading then maps that package to the namespace's routine database. With `NoMapping` or `GlobalScope` the code is stored where the package resolves, for example IRISSYS.
  - As built, uninstall never removes those mappings, so the package keeps resolving to the namespace's database and overrides an administrator's mapping (defect C:D65).
  - As built, uninstall skips classes that resolve outside the namespace's database unless `-global` is given, so a module installed with `NoMapping` leaves its code behind, shared by every namespace (defect C:D66).
- `<Mappings>` adds explicit mappings.
- `SourcesRoot` names the folder that holds its code.
- Expressions such as `${ipmDir}` stand for system folders.

<a id="module.global.roundtrip"></a>
[D · module.global.roundtrip]

A global resource is exported and read back at `<SourcesRoot>/<Directory>/<global name>.xml`; Directory defaults to `gbl/`. Packaging a global with SourcesRoot="src", clearing it and loading that package restores its stored values.

- The name drops `.GBL`; its special characters are translated by the processor's filename settings. A preload resource has `preload/` before SourcesRoot.
- This round-trip contract covers ordinary, non-generated, non-preload globals with ordinary names. Mapping, preservation on uninstall and custom filename translation retain source evidence and open formal gaps.

## Analytics and interoperability documents

<a id="module.processor.deepsee"></a>
[D · module.processor.deepsee]

A `.DFI` resource loads an Analytics folder item, such as a dashboard or pivot, from `<SourcesRoot>/dfi/`, where `-` in the item name becomes a folder separator. A name containing `*` loads every matching file in that folder.

- As built on Linux, a wildcard resource loads only files ending in `.xml` or `.DFI`. Files ending in `.dfi` or `.XML` are silently skipped (defect C:D62).
- Uninstall removes explicitly named items. As built, items loaded through a wildcard resource remain (defect C:D63).

<a id="module.processor.interoperability"></a>
[D · module.processor.interoperability]

Interoperability documents such as `.LUT` lookup tables load from `<SourcesRoot>/i14y/<name>.xml`, or from `<SourcesRoot>/misc/` when only that folder exists, unless the resource declares a Directory. With neither folder present, a file directly under SourcesRoot is also found.

- Loading a changed lookup table replaces the whole table, so a removed key disappears, and uninstall removes the table.
- HL7, X12 and ESD documents need the matching document class in the namespace. On an instance without it the load fails.

## Localized messages and generated includes

<a id="module.processor.localized-messages"></a>
[D · module.processor.localized-messages]

A `.LOC` resource holds localized messages, which are loaded into the namespace's message dictionary when the module compiles. Loading a changed file replaces that domain's messages for its language, so a message removed from the file disappears.

- During Activate, IPM also writes the file's domains to `<module root>/localize/<name>.xml`. As built, that write is required, so a module with a `.LOC` resource cannot be loaded from a read-only folder (defect C:D58).
- As built, uninstall deletes the `.LOC` document but leaves its messages in the message dictionary and the exported file in place (defect C:D59).

<a id="module.processor.localized-messages.legacy"></a>
[D · module.processor.localized-messages.legacy]

A resource with `ProcessorClass="LegacyLocalizedMessages"` loads a message XML file from the module root when the module compiles. It generates include files of message-name macros: one for `includeFiles="<include>"`, or one per target for `includeFiles="<domain>:<include>,..."`.

- Each domain in the file replaces that domain's messages, except that with `merge="1"` earlier messages of the mapped domains are kept.
- Uninstall deletes the generated include of the single form. As built, with two or more `<domain>:<include>` entries no generated include is recognized as the module's, so uninstall leaves them all (defect C:D64).

## Copying code between namespaces

<a id="module.processor.copy"></a>
[D · module.processor.copy]

A `Copy` resource copies the classes of the named package from its `Source` namespace into the installing namespace during Reload. They are then compiled there.

- With `Overwrite` true, the default, a class that already exists in the installing namespace is replaced, even when it was changed after the source.
- With `Overwrite="0"`, existing classes are kept and only missing ones are copied.
- A class that another installed module lists as a resource is never copied.
- As built, uninstall deletes every class of that package in the installing namespace except classes another module owns. That includes classes the load kept because of `Overwrite="0"`, which the module never wrote (defect C:D40).

## Copying and removing files

<a id="module.processor.file-copy"></a>
[I · module.processor.file-copy]

`FileCopy` copies files at install and removes them at uninstall.

- It never deletes IRIS or IPM system folders, or any folder that contains one.
- It refuses to copy when the source and target are the same folder or one contains the other.
- `KeepOnUninstall` leaves the copied files in place on `uninstall` and `clean`, for files other modules may share.
- `Overlay=0` requests full directory replacement, including removal of unrelated old entries. The pinned Unix path has a [replacement discrepancy](#module.processor.file-copy.unix-overlay).

- Immediate copies run before Activate; `Defer` runs them after successful Activate. A failing hook between them leaves an immediate target but prevents the deferred copy.

- Database rollback does not restore either filesystem state. With `KeepOnUninstall`, Clean preserves the target; other directory targets are removed subject to the [current protection check](#module.processor.file-copy.protection).

- Directory overlay rules protect system directories. These cases were checked through both the processor and an actual failing load; no general filesystem compensation is proved.
- Direct `DoCopy` with a missing source returns an error and creates no copied target file. A successful copy remains after database rollback while a journaled database sentinel disappears; rollback does not turn a failed copy into a successful one. Creating the destination directory and partial operating-system copy errors are separate effects.

- Working assumption: Protect configured roots and their ancestors and an exact target containing an IRIS.DAT marker.

<a id="module.processor.file-copy.protection"></a>
[D · module.processor.file-copy.protection]

Directory protection is checked again at copy and Clean time. A marker at `target/IRIS.DAT` protects that directory even when it is not configured as a database. Otherwise the target must equal or contain a listed system, IPM or configured database directory, after path normalization and case folding. An ordinary child of a protected directory is not automatically protected.

- Adding a synthetic marker after load makes Clean keep the copied directory. Removing its marker before Clean makes the ordinary scratch directory eligible for removal. The checked commands succeed and remove the module record in both cases.
- A single-file resource inside a protected directory is still deleted on Clean when KeepOnUninstall is false; the surrounding directory and unrelated files remain.
- The marker check examines the target itself, not descendants. A scratch parent containing an otherwise unknown child with a synthetic IRIS.DAT is unprotected and Clean removes the whole parent. [The intended unknown-descendant policy remains open](../../Verus_verification/open_questions.md#module.processor.file-copy.unknown-descendants).

- Working assumption: Protect configured roots and their ancestors and an exact target containing an IRIS.DAT marker.

<a id="module.processor.file-copy.unix-overlay"></a>
[D · module.processor.file-copy.unix-overlay]

On the pinned Unix runtime, an ordinary directory load with Overlay=0 copies the new payload but keeps unrelated old entries. The Unix CopyDir wrapper always enables the provider's overlay option; its delete-first argument instead controls deletion of matching files before replacement. Both direct flag values kept an unrelated sentinel while replacing an old same-name payload. This contradicts the documented full-replacement rule above and is C:D29.

Protected-directory checks still change the requested argument and Clean behavior. They must not be treated as proof that the Unix copy operation clears an unprotected target. The Windows wrapper explicitly removes the target first, but no Windows execution is claimed.

## Instance configuration

<a id="module.processor.cpf"></a>
[D · module.processor.cpf]

A `<CPF>` file merges into the instance configuration before Initialize by default, or before or after another named phase, or only when its custom phase is requested.

- A command whose phases do not include that phase, such as `compile -only`, does not merge it.
- A merge the instance rejects, such as an unknown section, fails the phase with the merge tool's message and a pointer to messages.log.

<a id="module.processor.cpf-effects"></a>
[D · module.processor.cpf-effects]

A CPF processor runs a child command that changes instance configuration. Its exit status and the final configuration must be read separately from database rollback.

- In the disposable IRIS provider, a valid role-creation merge returned success and the role survived rollback, while a journaled sentinel disappeared. An invalid-section neighbor returned error with child exit 7 and created no role.
- These observations cover one configuration action and one refusal. Other actions, partial failures and settings that need restart remain external premises; exit zero alone does not prove their effect.

- Working assumption: database rollback alone gives no such promise. Alternatives are compensating selected changes or refusing them inside package transactions.

<a id="module.system-setting"></a>
[D · module.system-setting]

The legacy SystemSetting processor writes a named CSP setting during Compile, replacing its old value. Other phases, including Clean on uninstall, do not restore or delete that setting.

Only names whose first dot-separated segment is CSP are handled; the second segment is the setting key. The declared value is written to the current namespace's mapped `^%SYS("CSP", key)` global. New modules are directed by the product to use CPF instead. [Whether uninstall should retain or restore that setting is open](../../Verus_verification/open_questions.md#module.system-setting.cleanup-policy).

- Working assumption: Uninstall keeps the installed SystemSetting value.

## Python requirements and local wheels

<a id="module.python"></a>
[I · module.python]

A module can depend on Python packages.

- IPM installs `requirements.txt` with pip in the Reload phase, and `<PythonWheel>` files in Initialize.
- `-bypass-py-deps` skips this, and `-extra-pip-flags` adds pip flags.
- Python 3.9 or later is required. The `PipCaller` and `UseStandalonePip` settings pick which pip runs.

- Working assumption: Clean preserves the current shared Python installation.

<a id="module.python.explicit-wheel"></a>
[D · module.python.explicit-wheel]

An explicit PythonWheel resource installs its local wheel during Initialize, even with `-bypass-py-deps`; that flag skips requirements.txt processing instead. The wheel path comes from the module root, its declared Directory and its filename, and extra pip flags are passed through the shared pip runner after empty arguments are removed.

- A missing wheel or failed pip call fails an ordinary module. IPM's own module instead warns and continues with the resource unhandled, so setup can continue without that optional package.
- Clean marks the wheel handled and leaves the shared Python installation in place. Uninstalling a module therefore does not establish that its Python package is gone.
- [Cleanup of shared Python packages remains an owner decision](../../Verus_verification/open_questions.md#module.python.cleanup-policy). After a failed pip call, package presence is unconfirmed: pip can have made partial changes. Missing-file refusal never calls pip. Pip dependency resolution, online indexes and compensation after partial installation remain external boundaries.

- Working assumption: Clean preserves the current shared Python installation.

## Metadata

### module.processor

- human-provided
- user-facing-behavior
- Expands: [Module](../concepts/data-policy.g5.md#module).
- Children: [file copy](#module.processor.file-copy), [web applications](web-applications.g5.md#module.webapp).

Sources: `wiki/03` L581-785; CHANGELOG 0.9.0 (`#562`, `#593`), 0.10.0 (`#702`), 0.10.11 (`#1274`, `#1232`). Inventory: CAT-07; OP-LOAD-24. Observed: the Compile phase ignores a processor's compile status (OBS-L8). `wiki/06` describes a `<WSGIApplication>` element with no processor.

Extension Verus: [thin_surfaces.rs::cpf_status](../../Verus_verification/consolidated/verus/thin_surfaces.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### module.processor.defaults

- agent
- Expands: [Resource processors](#module.processor).

Sources: `ResourceProcessor/Abstract.cls::{%OnNew,SetParams,CopyAttributes,ApplyDefaults,CopyDefaultAttributes,SatisfiesCondition}`; `Storage/Module.cls::GetOrderedResourceList`. Verus `resources.rs::{defaults_step,apply_defaults,ordered_before,chained_defaults_and_last_writer,explicit_default_is_protected,resource_order}`. Defaults use literal equality conditions in the checked native/formal group; expression conversion, other condition operators and full list ordering are residuals.

### module.processor.default-sweep

- design-verified

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [module.processor](#module.processor).

Sources and readers: `Storage/ModuleSetting/Default.cls::EvaluateAttribute` → `ResourceProcessor/Abstract.cls::%Evaluate`; `Utils/Module.cls::%RegExReplace`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `replace`, `sweep`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### module.placement

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Module](../concepts/data-policy.g5.md#module).

Sources: `wiki/03` L188-241, L274-277, L533-573; CHANGELOG 0.9.0 (`#575`), 0.10.4 (`#374`). Inventory: CAT-10. Tests: `Mappings.cls`, `ModuleRoot.cls`. `wiki/03` L775 documents `${authUnauthenticated}`, which is not implemented.


Iteration59: [mappings.rs](../../Verus_verification/consolidated/iter59/mappings.rs) `needs_map`, `load`, `uninstall`, `mapping_outlives_uninstall`, `nomapping_code_orphaned`, `global_module_skipped` ([handoff](../../uplift/consolidated/iter59/HANDOFF.md)).

Findings: [Uninstall leaves the package mappings IPM created, overriding an administrator's mapping](../../Verus_verification/bugs_issues.md#c-d65)

Findings: [Uninstall of a NoMapping module leaves its code outside the namespace](../../Verus_verification/bugs_issues.md#c-d66)

State machines: [mappings · iter59](../../Verus_verification/models/iter59-mappings/manifest.json)

### module.global.roundtrip

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Modules](../concepts/data-policy.g5.md#module).

Sources: `ResourceProcessor/Default/Global.cls::{OnItemRelativePath,OnResolveChildren,OnPhase}` → `Lifecycle/Base.cls::ExportSingleModule` and `$system.OBJ.Load`. Native S6-02 explicitly kills `^My.Settings` before archive load and reads Parameter=42, so a surviving value cannot supply the result. [Contract](../../Verus_verification/consolidated/iter6/workflows.rs): `global_path`, `package_global`, `reload_global`. Serialization and database provider success remain premises. [Index](../../uplift/consolidated/iter6/INDEX.md).

State machines: [workflows · iter6](../../Verus_verification/models/iter6-workflows/manifest.json)

Iteration75 neighbours: [global_path.rs](../../Verus_verification/coverage-20261117/iter75/global_path.rs) ([handoff](../../uplift/coverage-20261117/iter75/HANDOFF.md)).

### module.processor.deepsee

- design-verified

- agent
- user-facing-behavior
- Expands: [Resource processors](#module.processor).

Sources: `ResourceProcessor/Default/DeepSeeItem.cls`; product fixture `resource-tests`. Model: [dfiitems.rs](../../Verus_verification/consolidated/iter58/dfiitems.rs) `wildcard_extension_neighbors`, `uninstall_neighbors`, `leftover_after_uninstall` ([handoff](../../uplift/consolidated/iter58/HANDOFF.md)). Keywords filtering and other platforms are not assessed.

Findings: [On Linux, a wildcard DFI resource skips `.dfi` and `.XML` files](../../Verus_verification/bugs_issues.md#c-d62)

Findings: [Uninstall leaves DFI items loaded through a wildcard resource](../../Verus_verification/bugs_issues.md#c-d63)

State machines: [dfiitems · iter58](../../Verus_verification/models/iter58-dfiitems/manifest.json)

### module.processor.interoperability

- design-verified

- agent
- user-facing-behavior
- Expands: [Resource processors](#module.processor).

Sources: `ResourceProcessor/Default/Interoperability.cls::DirectoryGet`; `Lifecycle/Base.cls::GetDefaultResourceProcessor`. Model: [interopres.rs](../../Verus_verification/consolidated/iter62/interopres.rs) `directory_neighbors`, `reload_replaces`, `uninstall_clears` ([handoff](../../uplift/consolidated/iter62/HANDOFF.md)). Only `.LUT` was exercised natively; IRIS Community lacks an HL7 document class.

State machines: [interopres · iter62](../../Verus_verification/models/iter62-interopres/manifest.json)

### module.processor.localized-messages

- design-verified

- agent
- user-facing-behavior
- Expands: [Resource processors](#module.processor).

Sources: `ResourceProcessor/Default/LocalizedMessages.cls`, `ResourceProcessor/LocalizationExport.cls::OnBeforePhase`; test `Resources.cls`, which deletes the exported file from its fixture. Model: [locmsgs.rs](../../Verus_verification/consolidated/iter56/locmsgs.rs) `reload_replaces_domain`, `readonly_root_fails`, `uninstall_keeps_messages` ([handoff](../../uplift/consolidated/iter56/HANDOFF.md)).

Findings: [A module with a `.LOC` resource cannot be loaded from a read-only folder](../../Verus_verification/bugs_issues.md#c-d58)

Findings: [Uninstall leaves a module's localized messages in the message dictionary](../../Verus_verification/bugs_issues.md#c-d59)

State machines: [locmsgs · iter56](../../Verus_verification/models/iter56-locmsgs/manifest.json)

### module.processor.localized-messages.legacy

- design-verified

- agent
- user-facing-behavior
- Expands: [Localized messages](#module.processor.localized-messages).

Sources: `ResourceProcessor/LegacyLocalizedMessages.cls` (`OnBeforePhase`, `OnResolveChildren`, description and `merge` comment). Model: [legacymsgs.rs](../../Verus_verification/consolidated/iter60/legacymsgs.rs) `replace_and_merge`, `single_include_cleaned`, `mapped_includes_leak` ([handoff](../../uplift/consolidated/iter60/HANDOFF.md)). Messages left after uninstall are C:D59.

Findings: [Uninstall leaves the includes generated by a multi-entry LegacyLocalizedMessages resource](../../Verus_verification/bugs_issues.md#c-d64)

State machines: [legacymsgs · iter60](../../Verus_verification/models/iter60-legacymsgs/manifest.json)

### module.processor.copy

- design-verified

- agent
- user-facing-behavior
- Expands: [Resource processors](#module.processor).

Sources: `ResourceProcessor/Copy.cls` description and property comments ("If set to false (default is true), resources that already exist in the current/target namespace will not be overwritten"), `::OnAfterPhase`, `::OnResolveChildren`; `Lifecycle/Base.cls::%Clean` child deletion. Model: [copy.rs](../../Verus_verification/consolidated/iter37/copy.rs) `load`, `uninstall`, `overwrite_false_keeps_existing`, `owned_never_copied_or_deleted`, `uninstall_deletes_kept_item` ([handoff](../../uplift/consolidated/iter37/HANDOFF.md)).

Findings: [Uninstalling a Copy resource deletes classes that Overwrite="0" kept and the module never wrote](../../Verus_verification/bugs_issues.md#c-d40)

State machines: [copy · iter37](../../Verus_verification/models/iter37-copy/manifest.json)

### module.processor.file-copy

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Processors](#module.processor).

Sources: CHANGELOG 0.10.11 (`#300`, `#934`, `#1135`); `ResourceProcessor/FileCopy.cls:32-36`, `:222-240`. Inventory: CAT-07, A1 seed 11. Tests: `FileCopy.cls` (`KeepOnUninstall` on uninstall), `CopyDirOverlap.cls`. `KeepOnUninstall` is not yet in the wiki.

Extension Verus: [thin_surfaces.rs::clean_removes](../../Verus_verification/consolidated/verus/thin_surfaces.rs), [thin_surfaces.rs::copies_after](../../Verus_verification/consolidated/verus/thin_surfaces.rs), [thin_surfaces.rs::copies_before](../../Verus_verification/consolidated/verus/thin_surfaces.rs), [thin_surfaces.rs::file_copy_observation](../../Verus_verification/consolidated/verus/thin_surfaces.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

The prior thin_surfaces `clears_target` is a requested-argument predicate, not a qualified Unix clearing guarantee. Its `copy_step` success clause removing unrelated entries loses Unix correspondence for unprotected Overlay=0 targets; earlier source/proofs/receipts are preserved. [Superseding boundary](#module.processor.file-copy.unix-overlay).

Open questions: [Should FileCopy protect a parent containing an unconfigured descendant with an IRIS.DAT marker, or only configured roots/ancestors and a marker at the exact target?](../../Verus_verification/open_questions.md#module.processor.file-copy.unknown-descendants)

### module.processor.file-copy.protection

- design-verified

- uncertain
- agent; source-inspected; native-tested; Verus-scoped
- Expands: [FileCopy](#module.processor.file-copy).

[Classifier and lifecycle model](../../Verus_verification/consolidated/iter14/protection.rs): `prefix`, `ancestor_of_root`, `protected`, `marker_write`, `clean`, `lifecycle_neighbors`. Source: Utils.File::{IsProtectedDirectory,DatabaseDirectories,NormalizeForComparison}; FileCopy::{NormalizeNames,DoCopy,OnPhase}. Paths are already normalized lowercase segments; root enumeration and raw path/file classification remain provider boundaries. Native scratch load/uninstall checks files, marker changes and module rows separately. No real database directory was modified.

Open questions: [Should FileCopy protect a parent containing an unconfigured descendant with an IRIS.DAT marker, or only configured roots/ancestors and a marker at the exact target?](../../Verus_verification/open_questions.md#module.processor.file-copy.unknown-descendants)

State machines: [protection · iter14](../../Verus_verification/models/iter14-protection/manifest.json)

### module.processor.file-copy.unix-overlay

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [FileCopy](#module.processor.file-copy).

[Unix provider model](../../Verus_verification/consolidated/iter14/protection.rs): `delete_first`, `unix_args`, `unix_copy`, `copy`. Source: FileCopy Overlay property and DoCopy; Utils.File::CopyDir Unix and contrasting Windows branches. [Direct calibration and provider reference](../../uplift/consolidated/iter14/provider-reference.md). The initial expected-clear model and failed native replay remain visible; bytes/status/rows are native qualification, not implementation proofs.

Findings: [C:D29 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d29). The linked entry qualifies the caller, reader and providers.

State machines: [protection · iter14](../../Verus_verification/models/iter14-protection/manifest.json)

### module.processor.cpf

- design-verified

- agent
- user-facing-behavior
- Expands: [Resource processors](#module.processor).

Sources: `ResourceProcessor/CPF.cls` (`OnBeforePhase`, `OnAfterPhase`, `OnCustomPhase`, `DoMerge`, `MergeCPF`); test `CPFMerge.cls` and fixture `cpf-merge`. Model: [cpfmerge.rs](../../Verus_verification/consolidated/iter48/cpfmerge.rs) `merges_in`, `neighbors`, `after_is_not_before` ([handoff](../../uplift/consolidated/iter48/HANDOFF.md)). Compensation after a later failure is the open question C:Q12.

State machines: [cpfmerge · iter48](../../Verus_verification/models/iter48-cpfmerge/manifest.json)

Iteration75 neighbours: [lifecycle_neighbours.rs](../../Verus_verification/coverage-20261117/iter75/lifecycle_neighbours.rs) ([handoff](../../uplift/coverage-20261117/iter75/HANDOFF.md)).

### module.processor.cpf-effects

- uncertain
- agent
- user-facing-behavior
- Expands: [Processors](#module.processor).
- Children: [Must package failure undo instance configuration changes?](../../Verus_verification/open_questions.md#module.processor.cpf-effects.policy).

Sources: `ResourceProcessor/CPF.cls::GetSourceFile`, `::DoMerge`, `::MergeCPF`; [exact bindings, premises and final readers](../../uplift/consolidated/iter2/boundaries/bindings.json); [native and Verus receipts](../../uplift/consolidated/iter2/boundaries/REPORT.md). Hand translation under explicit provider outcomes; no ObjectScript-body proof.

Open questions: [Must package failure undo instance configuration changes?](../../Verus_verification/open_questions.md#module.processor.cpf-effects.policy)

### module.system-setting

- design-verified

- uncertain
- agent; user-facing-behavior; native-tested; Verus-scoped
- Expands: [Processors](#module.processor).

Sources: `ResourceProcessor/SystemSetting.cls::{OnPhase,OnResolveChildren}` → `Storage/Module.cls::ExecutePhases`. Product test `SystemSetting::TestSystemSetting` supplies the independent seed. Native S7-10 first sets the value to old, loads the product fixture, reads 123, uninstalls and reads 123 again. [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `system_target`, `system_value`. Global mapping and database transaction semantics are provider premises. [Index](../../uplift/consolidated/iter7/INDEX.md).

Open questions: [Should uninstall keep a legacy SystemSetting value or restore the value that preceded installation?](../../Verus_verification/open_questions.md#module.system-setting.cleanup-policy)

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### module.python

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Module](../concepts/data-policy.g5.md#module).

Sources: `wiki/03` L117-178; `wiki/07`; README L52-58; `Main.cls:310-311`; CHANGELOG 0.9.0 (`#538`, `#595`, `#647`), 0.10.10. Inventory: CAT-08; OP-LOAD-25. Test: `ProcessPythonWheel.cls`. pip changes stay after a rollback ([rollback scope](../concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope)).

Open questions: [How should IPM remove Python packages without removing a package another module or administrator uses?](../../Verus_verification/open_questions.md#module.python.cleanup-policy)

### module.python.explicit-wheel

- design-verified

- uncertain
- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [module.python](#module.python).

Sources and readers: `ResourceProcessor/PythonWheel.cls::{OnPhase,OnResolveChildren}` → `Lifecycle/Base.cls::InstallOrDownloadPythonRequirements`, `Utils/Module.cls::PipInstall` and subsequent Python metadata/import readers. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/python.rs): `wheel_path`, `requirements_called`, `wheel`, `without_empty`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

Open questions: [How should IPM remove Python packages without removing a package another module or administrator uses?](../../Verus_verification/open_questions.md#module.python.cleanup-policy)

State machines: [python · iter8](../../Verus_verification/models/iter8-python/manifest.json)
