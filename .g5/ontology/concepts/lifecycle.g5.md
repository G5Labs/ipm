# Lifecycle phases

Every operation IPM performs on a module runs as a sequence of lifecycle phases. Loading, testing, packaging and uninstalling all reuse these phases.

## Definitions

- **Phase**: one named step of work on a module, such as Compile.
- **Phase chain**: the phases a command runs, ending with the phase it names.

## Statements

<a id="lifecycle"></a>
[I · lifecycle]

IPM runs a module's phases in this fixed order: Clean, Initialize, Reload, Validate, ExportData, Compile, Activate, Document, MakeDeployed, Test, Package, Verify, Publish, Configure, Unconfigure and ApplyUpdateSteps.

- Initialize brings in preload code, and Reload brings source into the namespace without compiling it.
- Compile compiles the module, and Activate runs post-compile setup, including Configure.
- Clean removes the module's resources.

- Working assumption: Run the standard chain, then each custom phase in the order named. Refuse the whole command before any phase runs when a name is neither a standard nor a declared custom phase.
- Working assumption: A standalone `reload` of a module with no root says that nothing was loaded; the phase stays silent inside other phase lists.


<a id="lifecycle.initialize"></a>
[D · lifecycle.initialize]

Initialize imports and compiles everything in the module's `preload` folder before Reload.

- It finds that folder only through the root a directory load supplies. A module action such as `<module> compile` or `<module> initialize` on an installed developer-mode module skips preload with "Skipping preload - directory does not exist", so edited preload code is not refreshed (defect C:D47).
- As built, a preload class that fails to compile does not fail Initialize. The load succeeds and the class stays uncompiled, while the same error in an ordinary resource fails the load (defect C:D46).


<a id="lifecycle.chain"></a>
[I · lifecycle.chain]

A command runs its phase together with all the phases it needs first. For example, `load`, `install` and the Activate phase run Initialize, Reload, Validate, Compile and Activate, and `test` adds Test.

- Each resource kind becomes visible at its own phase: include text, global data and `.DFI` items at Reload, classes, routines and `.LOC` messages at Compile, and FileCopy and WebApplication at Activate. A `compile` module action therefore leaves FileCopy targets and web applications stale.


<a id="lifecycle.document"></a>
[D · lifecycle.document]

The Document phase runs Document-phase hooks for the module and, unless `-DDocument.Recurse=0` is given, first for each installed dependency, all in one transaction. A failing hook fails the command and rolls back what the hooks wrote, and a dependency that is not installed stops the command before any hook runs.

- As built, IPM itself generates no documentation in this phase, although the module-action help says `document` regenerates the module's API documentation (defect C:D69).


<a id="lifecycle.only"></a>
[I · lifecycle.only]

`-only` runs just the named phases, without the phases before them.

- As built, this works for one named phase only. Two or more names given with `-only` are joined into one name that matches no phase, so the command fails with an internal "method does not exist" error before any of them runs (defect C:D34).


<a id="lifecycle.module-action"></a>
[I · lifecycle.module-action]

`module-action <module> <phases>`, or simply `<module> <phases>`, runs phases on a module. `compile`, `reload`, `test`, `package`, `verify`, `publish`, `makedeployed` and `update` are shortcuts for it.


<a id="lifecycle.custom"></a>
[I · lifecycle.custom]

A module can define custom phases with `<Invoke>` or `<CPF>`. A custom phase runs only when requested by name, and its name is not case-sensitive.

- As built, a custom phase runs only when it is the only name in the command, with or without `-only`.
- Named beside a standard phase without `-only`, a custom phase is silently left out and the command still succeeds (defect C:D35).
- An unknown name beside a standard phase is left out the same way, while an unknown name on its own fails. What should run instead is the [phase-list policy](../../Verus_verification/open_questions.md#lifecycle.phase-list-policy) question.

- Working assumption: Run the standard chain, then each custom phase in the order named. Refuse the whole command before any phase runs when a name is neither a standard nor a declared custom phase.


<a id="lifecycle.hooks"></a>
[I · lifecycle.hooks]

Resource processors can act before, during and after each phase, so the same phases behave differently depending on a module's resources.


<a id="lifecycle.installer"></a>
[D · lifecycle.installer]

A module can name an installer class with `<InstallerClass>`. During Activate, IPM's Configure step calls its `OnInstallComponent` and then its `OnConfigureComponent`. At uninstall, Clean first calls `OnUnconfigureComponent` and then `OnUnInstallComponent`.

- `OnInstallComponent` receives the database name, the verbose flag, the developer-mode flag and an upgrade flag. `OnConfigureComponent` receives the namespace, the verbose flag and the phase parameters, plus the module root as `InstallDir`.
- If `OnInstallComponent` returns an error, `OnConfigureComponent` is not called and the load fails. An ordinary load then rolls back, including data the installer wrote.
- A module without an installer class gets no callbacks.

- Working assumption: as built, IPM never sets them. The upgrade flag is 0 and the database name is empty unless `-DConfigure.IsUpgrade` or `-DConfigure.DatabaseName` supplies them, even when a newer version replaces an installed one.


<a id="lifecycle.installer.parameters"></a>
[D · lifecycle.installer.parameters]

The installer's parameters include `<Defaults><Parameter>` values and `-D` values, with `-D` winning. As built, `<Default Name Value>` values are not included, although Invoke arguments can use them (defect C:D41).


<a id="lifecycle.source-control"></a>
[D · lifecycle.source-control]

A namespace's source-control extension can follow module lifecycles. IPM calls it before and after every phase it runs, including nested phases such as Configure inside Activate. It also calls it when a module's resources or manifest change, and when a module's root changes.

- These calls reach only the primary extension of IPM's composite source-control class, which `init` configures by wrapping the namespace's existing extension. An extension set directly as the namespace source control, or listed only as a secondary extension, receives no calls.
- If the before-phase call returns an error, that phase and the command fail, and the after-phase call for it is not made.
- As built, the root-change call is not made when a module is loaded from another directory or reloaded with an explicit root, although the stored root changes (defect C:D51).


<a id="lifecycle.source-control.editor"></a>
[D · lifecycle.source-control.editor]

Editor callbacks on IPM's composite source-control class, such as save, load, compile, delete and icon state, go only to the primary extension, with item names normalized (a path such as `/A/B.cls` becomes `A.B.CLS`). A menu action goes to the extension that defines the menu, and other actions go to the primary.

- An item is editable only if every configured extension allows editing. As built, the read-only check asks only the primary, so another extension that marks an item read-only is ignored there (defect C:D70).
- As built, configuring a nonexistent extension class returns success, and every later creation of the composite then fails (defect C:D71). An empty primary name leaves the previous primary in place.


<a id="lifecycle.alias"></a>
[D · lifecycle.alias]

Module actions first look up the installed internal name and then its external name. Either name selects the same module for its requested phases, including packaging and custom phases; an unknown name fails.

A declared custom hook runs when its phase is selected, not on an ordinary load or uninstall. Custom phase names are matched without regard to case.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). The phase chains of `load`, `install` and `update` are in the formal scope through [load](../features/load.g5.md#load.core). The rest is outside it.

### lifecycle

- uncertain
- human-provided
- user-facing-behavior
- Children: [chain](#lifecycle.chain), [only](#lifecycle.only), [module-action](#lifecycle.module-action), [custom](#lifecycle.custom), [hooks](#lifecycle.hooks).

Sources: `wiki/00` L3-39; `Main.cls:32-58`. Inventory: CAT-11; A2 1.3 lifecycle phases (`%IPM.Lifecycle.Base`, `%IPM.Lifecycle.Module`). The help lists a `register` phase and example (`Main.cls:53-55`, `:64`) that has no implementation.

Open questions: [Should standalone reload warn or fail when a module has no source root?](../../Verus_verification/open_questions.md#lifecycle.empty-reload-policy), [When a phase list mixes standard phases with a custom or unknown name, what should run?](../../Verus_verification/open_questions.md#lifecycle.phase-list-policy)

### lifecycle.initialize

- design-verified

- agent
- user-facing-behavior
- Expands: [Lifecycle](#lifecycle).

Sources: `Lifecycle/Base.cls::%Initialize`; CHANGELOG 0.10.0 (`#702`: preload moved to Initialize; `reload -only` no longer compiles preload). Model: [initialize.rs](../../Verus_verification/consolidated/iter42/initialize.rs) `module_action_skips_preload`, `directory_load_refreshes`, `preload_error_swallowed` ([handoff](../../uplift/consolidated/iter42/HANDOFF.md)).

Findings: [A preload class that fails to compile does not fail the load](../../Verus_verification/bugs_issues.md#c-d46)

Findings: [Module actions never refresh a developer-mode module's preload code](../../Verus_verification/bugs_issues.md#c-d47)

State machines: [initialize · iter42](../../Verus_verification/models/iter42-initialize/manifest.json)

### lifecycle.chain

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Lifecycle](#lifecycle).

Sources: `wiki/00` L41-58; `%IPM.Lifecycle.Base:GetCompletePhasesForOne` (`Lifecycle/Base.cls:165-189`). Inventory: CAT-11; OP-LOAD-23. Test: `History.cls` `TestPhases` (load and install record the five phases).

Model: [phases.rs](../../Verus_verification/consolidated/iter30/phases.rs) `complete_plan`, `fixed_order`, `execute`. Iteration30 native cases replay the START order, compiled value and hook counts for full chains, reordered lists and Activate's nested Configure ([index](../../uplift/consolidated/iter30/HANDOFF.md)).


Iteration65: [moderefresh.rs](../../Verus_verification/consolidated/iter65/moderefresh.rs) `compile_chain_skips_activate`.

State machines: [phases · iter30](../../Verus_verification/models/iter30-phases/manifest.json), [moderefresh · iter65](../../Verus_verification/models/iter65-moderefresh/manifest.json)

### lifecycle.document

- design-verified

- agent
- user-facing-behavior
- Expands: [Lifecycle](#lifecycle).

Sources: `Lifecycle/Base.cls::%Document`; `Main.cls` module-action help. Model: [docphase.rs](../../Verus_verification/consolidated/iter66/docphase.rs) `document`, `neighbors`, `phase_generates_nothing` ([handoff](../../uplift/consolidated/iter66/HANDOFF.md)).

Findings: [`document` regenerates no API documentation unless a module adds its own Document hooks](../../Verus_verification/bugs_issues.md#c-d69)

State machines: [docphase · iter66](../../Verus_verification/models/iter66-docphase/manifest.json)

### lifecycle.only

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Lifecycle](#lifecycle).

Sources: `wiki/00` L71-83; `Main.cls:74`. Inventory: CAT-11. Observed: `-only` skips the declared chain but not phases a phase calls itself, such as MakeDeployed inside Package (`wiki/00` L83). The modifier help says "Only runs the specified phase(s)".

Model: [phases.rs](../../Verus_verification/consolidated/iter30/phases.rs) `only_plan`, `only_versus_chain`, `only_multiple_names_fail`.

Findings: [`-only` with two or more phase names fails before any phase runs](../../Verus_verification/bugs_issues.md#c-d34)

State machines: [phases · iter30](../../Verus_verification/models/iter30-phases/manifest.json)

### lifecycle.module-action

- human-provided
- user-facing-behavior
- Expands: [Lifecycle](#lifecycle).

Sources: `wiki/00` L60-69; `Main.cls:32-83`; CHANGELOG 0.10.0 (`#702`, `#722`, `#769`). Inventory: CAT-11; A2 1.2 rows 3, 5, 7-11, 13 (`Main:ModuleAction`, `Main:RunOnePhase`). Phase commands are not recorded in [history](../features/history.g5.md#history.actions). The help says `-DClean.Force=1`, while `wiki/00` L26 says `-force`.

### lifecycle.custom

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Lifecycle](#lifecycle).

Sources: `wiki/00` L85-103; `wiki/03` L492-529; CHANGELOG 0.10.5 (`#828`). Inventory: CAT-06. Test: `CustomPhase.cls`.

Model: [phases.rs](../../Verus_verification/consolidated/iter30/phases.rs) `custom_and_unknown_neighbors`.

Findings: [A phase list silently drops custom and unknown names listed beside a standard phase](../../Verus_verification/bugs_issues.md#c-d35)

Open questions: [When a phase list mixes standard phases with a custom or unknown name, what should run?](../../Verus_verification/open_questions.md#lifecycle.phase-list-policy)

State machines: [phases · iter30](../../Verus_verification/models/iter30-phases/manifest.json)

### lifecycle.hooks

- human-provided
- architecture-decision
- Expands: [Lifecycle](#lifecycle).

Sources: `wiki/00` L5-7. Inventory: A2 1.3 resource processor hooks (`ResourceProcessor/Abstract.cls:168-209`), `Storage.Module:ExecutePhases` (`Storage/Module.cls:575-691`).

### lifecycle.installer

- design-verified

- uncertain
- agent
- user-facing-behavior
- Expands: [Lifecycle](#lifecycle).

Sources: `Lifecycle/Base.cls::%Configure`, `::%Unconfigure`; `Lifecycle/Module.cls::%Clean`; the `simple-app` test installer. Model: [installer.rs](../../Verus_verification/consolidated/iter36/installer.rs) `load`, `uninstall`, `refusal_stops`, `no_installer_no_calls` ([handoff](../../uplift/consolidated/iter36/HANDOFF.md)). Purge forwarding is owned by the uninstall purge statement.

Open questions: [What should the installer's upgrade flag and database name mean?](../../Verus_verification/open_questions.md#lifecycle.installer.upgrade-policy)

State machines: [installer · iter36](../../Verus_verification/models/iter36-installer/manifest.json)

### lifecycle.installer.parameters

- design-verified

- agent
- user-facing-behavior
- Expands: [Installer class](#lifecycle.installer).

Sources: `Lifecycle/Base.cls::%Configure` (`GetCustomParameters` result unused). Model: [installer.rs](../../Verus_verification/consolidated/iter36/installer.rs) `default_versus_parameter`, `override_reaches_both`.

Findings: [The installer class does not receive `<Default>` values](../../Verus_verification/bugs_issues.md#c-d41)

State machines: [installer · iter36](../../Verus_verification/models/iter36-installer/manifest.json)

### lifecycle.source-control

- design-verified

- agent
- user-facing-behavior
- Expands: [Resource hooks](#lifecycle.hooks).

Sources: `ExtensionBase/SourceControl/Interface.cls` comments and `%GetClass`; `ExtensionBase/CompositeMethodOverrides.cls::SetExtensionClasses`; `Main.cls` init (lines 1876-1890); `Storage/Module.cls::ExecutePhases`. Model: [schooks.rs](../../Verus_verification/consolidated/iter43/schooks.rs) `active`, `observe`, `before_error_stops_phase`, `nested_hooks_inside_parent`, `root_hook_not_reached` ([handoff](../../uplift/consolidated/iter43/HANDOFF.md)). Interactive `init` prompts, Studio menus and `%OnMakeModuleEditable` are not assessed.

Findings: [A source-control extension is never told that a module's root changed](../../Verus_verification/bugs_issues.md#c-d51)

State machines: [schooks · iter43](../../Verus_verification/models/iter43-schooks/manifest.json)

### lifecycle.source-control.editor

- design-verified

- agent
- user-facing-behavior
- Expands: [Source-control extension](#lifecycle.source-control).

Sources: `ExtensionBase/Composite.cls` (GetStatus comment "An item is editable only if all extensions allow editing"), `ExtensionBase/CompositeMethodOverrides.cls`, `ExtensionBase/Utils.cls::CorrectInternalNames`. Model: [composite.rs](../../Verus_verification/consolidated/iter67/composite.rs) `other_extension_inconsistency`, `neighbors`, `misconfigured_other_breaks_creation`, `empty_primary_ignored` ([handoff](../../uplift/consolidated/iter67/HANDOFF.md)). Mapped source-control routing is not exercised.

Findings: [The composite's read-only check ignores other extensions](../../Verus_verification/bugs_issues.md#c-d70)

Findings: [Configuring a nonexistent composite extension succeeds and then breaks source control in the namespace](../../Verus_verification/bugs_issues.md#c-d71)

State machines: [composite · iter67](../../Verus_verification/models/iter67-composite/manifest.json)

### lifecycle.alias

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Module actions](#lifecycle.module-action).

Sources: `Storage/Module.cls::{ExecutePhases,GetCustomPhases}` → lifecycle and Invoke readers. Native S6-04 loads product fixtures then requests Greeting by both names, packages by external name and runs ordinary/custom phase calls. [Contract](../../Verus_verification/consolidated/iter6/workflows.rs): `selected_module`, `custom_hook`. Phase chain construction and case folding retain inherited source evidence; this new contract takes the resolved phase sequence as its boundary input. [Index](../../uplift/consolidated/iter6/INDEX.md).

State machines: [workflows · iter6](../../Verus_verification/models/iter6-workflows/manifest.json)
