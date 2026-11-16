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

<a id="lifecycle.chain"></a>
[I · lifecycle.chain]

A command runs its phase together with all the phases it needs first. For example, `load`, `install` and the Activate phase run Initialize, Reload, Validate, Compile and Activate, and `test` adds Test.

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
- An unknown name beside a standard phase is left out the same way, while an unknown name on its own fails. What should run instead is the [phase-list policy](#lifecycle.phase-list-policy) question.

<a id="lifecycle.hooks"></a>
[I · lifecycle.hooks]

Resource processors can act before, during and after each phase, so the same phases behave differently depending on a module's resources.

<a id="lifecycle.alias"></a>
[D · lifecycle.alias]

Module actions first look up the installed internal name and then its external name. Either name selects the same module for its requested phases, including packaging and custom phases; an unknown name fails.

A declared custom hook runs when its phase is selected, not on an ordinary load or uninstall. Custom phase names are matched without regard to case.

<a id="lifecycle.phase-list-policy"></a>
[Q · lifecycle.phase-list-policy]

When a phase list mixes standard phases with a custom or unknown name, what should run?

- Working assumption: Run the standard chain, then each custom phase in the order named. Refuse the whole command before any phase runs when a name is neither a standard nor a declared custom phase.
- Alternative and implications: Run each custom phase at its position in the command, which needs a rule for where it falls inside a chain. Or refuse any mix of custom and standard phases with a clear error, which is simpler but breaks lists that work alone.
- This assumption guides review; it is not a maintainer decision. The command help ("Space-delimited list of module lifecycle phases to run") shows lists are intended but does not settle order or refusal.

<a id="lifecycle.empty-reload-policy"></a>
[Q · lifecycle.empty-reload-policy]

Should standalone reload warn or fail when a module has no source root?

- Working assumption: A standalone `reload` of a module with no root says that nothing was loaded; the phase stays silent inside other phase lists.
- Alternative and implications: Fail the standalone command (stricter for scripts).
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). The phase chains of `load`, `install` and `update` are in the formal scope through [load](../features/load.g5.md#load.core). The rest is outside it.

### lifecycle

- human-provided
- user-facing-behavior
- Children: [chain](#lifecycle.chain), [only](#lifecycle.only), [module-action](#lifecycle.module-action), [custom](#lifecycle.custom), [hooks](#lifecycle.hooks).

Sources: `wiki/00` L3-39; `Main.cls:32-58`. Inventory: CAT-11; A2 1.3 lifecycle phases (`%IPM.Lifecycle.Base`, `%IPM.Lifecycle.Module`). The help lists a `register` phase and example (`Main.cls:53-55`, `:64`) that has no implementation.

### lifecycle.chain

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Lifecycle](#lifecycle).

Sources: `wiki/00` L41-58; `%IPM.Lifecycle.Base:GetCompletePhasesForOne` (`Lifecycle/Base.cls:165-189`). Inventory: CAT-11; OP-LOAD-23. Test: `History.cls` `TestPhases` (load and install record the five phases).

Model: [phases.rs](../../Verus_verification/consolidated/iter30/phases.rs) `complete_plan`, `fixed_order`, `execute`. Iteration30 native cases replay the START order, compiled value and hook counts for full chains, reordered lists and Activate's nested Configure ([index](../../uplift/consolidated/iter30/HANDOFF.md)).

### lifecycle.only

- human-provided
- user-facing-behavior
- Expands: [Lifecycle](#lifecycle).

Sources: `wiki/00` L71-83; `Main.cls:74`. Inventory: CAT-11. Observed: `-only` skips the declared chain but not phases a phase calls itself, such as MakeDeployed inside Package (`wiki/00` L83). The modifier help says "Only runs the specified phase(s)".

Model: [phases.rs](../../Verus_verification/consolidated/iter30/phases.rs) `only_plan`, `only_versus_chain`, `only_multiple_names_fail`.

Findings: [`-only` with two or more phase names fails before any phase runs](../../uplift/consolidated/BUGS.md#c-d34)

### lifecycle.module-action

- human-provided
- user-facing-behavior
- Expands: [Lifecycle](#lifecycle).

Sources: `wiki/00` L60-69; `Main.cls:32-83`; CHANGELOG 0.10.0 (`#702`, `#722`, `#769`). Inventory: CAT-11; A2 1.2 rows 3, 5, 7-11, 13 (`Main:ModuleAction`, `Main:RunOnePhase`). Phase commands are not recorded in [history](../features/history.g5.md#history.actions). The help says `-DClean.Force=1`, while `wiki/00` L26 says `-force`.

### lifecycle.custom

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Lifecycle](#lifecycle).

Sources: `wiki/00` L85-103; `wiki/03` L492-529; CHANGELOG 0.10.5 (`#828`). Inventory: CAT-06. Test: `CustomPhase.cls`.

Model: [phases.rs](../../Verus_verification/consolidated/iter30/phases.rs) `custom_and_unknown_neighbors`.

Findings: [A phase list silently drops custom and unknown names listed beside a standard phase](../../uplift/consolidated/BUGS.md#c-d35)

### lifecycle.hooks

- human-provided
- architecture-decision
- Expands: [Lifecycle](#lifecycle).

Sources: `wiki/00` L5-7. Inventory: A2 1.3 resource processor hooks (`ResourceProcessor/Abstract.cls:168-209`), `Storage.Module:ExecutePhases` (`Storage/Module.cls:575-691`).

### lifecycle.alias

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Module actions](#lifecycle.module-action).

Sources: `Storage/Module.cls::{ExecutePhases,GetCustomPhases}` → lifecycle and Invoke readers. Native S6-04 loads product fixtures then requests Greeting by both names, packages by external name and runs ordinary/custom phase calls. [Contract](../../Verus_verification/consolidated/iter6/workflows.rs): `selected_module`, `custom_hook`. Phase chain construction and case folding retain inherited source evidence; this new contract takes the resolved phase sequence as its boundary input. [Index](../../uplift/consolidated/iter6/INDEX.md).

### lifecycle.phase-list-policy

- agent
- uncertain
- Expands: [Custom phases](#lifecycle.custom).

Sources: `Lifecycle/Base.cls::GetCompletePhases` (a non-standard name contributes no chain unless it is alone); `Main.cls` module-action help. Native iteration30 cases c4, c6 and c10.

Findings: [A phase list silently drops custom and unknown names listed beside a standard phase](../../uplift/consolidated/BUGS.md#c-d35)

### lifecycle.empty-reload-policy

- agent
- uncertain
- user-facing-behavior
- Expands: [lifecycle](#lifecycle)

Sources: [DEC-SYNC-4](../../../playbook-uplift/decisions/07-09.md); Help: reload "pulls module source code into the namespace from disk"; the code's "For now" no-op.

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#lifecycle.empty-reload-policy). The recorded assumption is provisional; no new maintainer approval was found.

Findings: [P:F-SYNC-3 — e2e reproduced](../../../playbook-uplift/BUGS.md#p-f-sync-3). The linked entry qualifies the caller, reader and providers.
