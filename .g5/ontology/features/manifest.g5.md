# Authoring a module manifest

A module author declares the package in `module.xml`. IPM uses these declarations to identify the module, check its requirements and supply values to its resources. See the [data policy](../concepts/data-policy.g5.md) for name and ownership rules.

## Editing the installed manifest

<a id="module.identity.document"></a>
[D · module.identity.document]

An installed module's manifest can be edited as its `<Name>.ZPM` document, as editors do. Saving the document updates the stored module record at once.

- A valid edit replaces the version and the other manifest fields. A lower version is accepted on this path.
- Saving an empty document keeps the record and its resources but sets its version to `0.0.1+snapshot`.
- Deleting the document deletes the module record and leaves the module's code in the namespace.
- As built, a save that IPM refuses, such as malformed XML, still empties the stored document text while the record keeps the earlier manifest (defect C:D44).
- As built, a manifest naming a different module is accepted under the old name: the record takes the other fields and the document keeps the new name (defect C:D45).

## Declaring dependencies and system requirements

<a id="module.dependency"></a>
[I · module.dependency]

A `<ModuleReference>` names a dependency and the versions it accepts, either exact or a [range](../concepts/versions.g5.md#version.range). An omitted version means any version. How IPM picks versions is owned by [dependency resolution](dependencies.g5.md#deps).

<a id="module.requirement"></a>
[I · module.requirement]

`<SystemRequirements>` lists conditions that must hold before the module is installed. A failed condition [refuses the load](load.g5.md#load.refusal.requirements).

- `Version`: the IRIS versions the module supports.
- `Interoperability`: the namespace must have interoperability enabled.
- `Health`: the instance must be HealthShare, HealthConnect or IRIS for Health.
- `IPMVersion` and `PythonVersion`: the IPM and Python versions it needs. A mapped namespace has a [local IPM-version reader gap](../operations/namespaces.g5.md#namespace.version.mapped-reader).
- `SYSNamespace`: the module must be installed in `%SYS`.

<a id="module.requirement.values"></a>
[D · module.requirement.values]

An omitted Health requirement accepts either kind of instance; explicit false excludes Health products, while true requires one. Health true also requires an interoperability-enabled namespace, even with `Interoperability="disabled"`.

- `Interoperability="enabled"` requires an enabled namespace. As built, `disabled` does not require it to be disabled. The tests accept that declaration without an exclusion check.
- `SYSNamespace="true"` requires the exact `%SYS` namespace. False or omission adds no namespace restriction.
- The aggregate appends returned errors from its checks in order rather than stopping after the first returned error. A thrown exception may stop later checks. Version parsing, runtime detection and the known SemVer exceptions remain separate dependencies.

## Supplying parameters and evaluating expressions

<a id="module.parameter"></a>
[I · module.parameter]

`<Default>` and `<Defaults><Parameter>` give values the module receives at load and install. A `-D<name>=<value>` on the command line overrides them.

- Dotted names become nested parameters.
- Placeholders in values are filled in, repeatedly if needed.
- Defaults can also set the [transaction flags](../concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources).

<a id="module.parameter.expressions"></a>
[I · module.parameter.expressions]

Attribute values in a manifest, such as file targets, invoke arguments and the after-install message, can contain expressions that IPM fills in at install time. A value without a `{` is used as written.

- `${name}` or `{$name}` takes a parameter's value. Names match regardless of case. The built-in `root`, `packagename`, `version`, `verbose` and `ipmDir` take precedence over module defaults with the same name.
- The same forms also give instance values such as `namespace`, `namespaceLower` and `mgrDir`.
- `$$$name` takes the value of an IRIS macro, or nothing if the macro is undefined.
- `{name}` gives the module name, and `#{expression}` gives the value of an ObjectScript expression, or nothing if it fails.

- As built, a value without `{` returns unchanged before parameter lookup, macro expansion or expression compilation. With `{`, lookup and nested placeholders run, followed by macro/expression evaluation.

- A replacement containing `$` may raise; a dotted parameter name can match another name because it becomes regex syntax. A cyclic reference exhausts the loop before the declared cycle error can run (C:D3–D5).

- These failures contradict literal-name/value intent. [C:Q2](../../Verus_verification/open_questions.md#module.parameter.expressions.brace-policy) asks whether brace-free macros should expand; it does not leave the observed gate undecided.

- Working assumption: keep the observed brace gate until a maintainer chooses otherwise.

<a id="module.parameter.literal-values"></a>
[I · module.parameter.literal-values]

A parameter value is inserted exactly as written. Characters such as `$` and `\` in a value must not change the result or cause an error.

<a id="module.parameter.cycles"></a>
[I · module.parameter.cycles]

If parameters refer to each other in a loop, IPM should report an error that names the circular reference instead of installing with an unfilled value.

- This requirement is recovered from the named error and guard in the product, rather than an owner decision. As built, the guard is unreachable: `a=${b}`, `b=${a}` returns unresolved `${a}` without that error. A resolving neighbor, `a=${b}`, `b=ok`, returns `ok`.
- Using an unresolved value later may raise a regular-expression error. This defect does not satisfy the circular-reference requirement. [Iter 2 native reader](../../uplift/consolidated/iter2/native/receipt.json) and [retained Verus counterexample](../../Verus_verification/consolidated/verus/attr_evaluator.rs) preserve both outcomes.

## Reading list values and environment files

<a id="module.list.delimiter"></a>
[D · module.list.delimiter]

An XML option-list value is split at its configured ODBC delimiter. An omitted or comma delimiter splits at commas; an explicit pipe delimiter keeps embedded commas inside each list item.

The contract covers nonempty input and delimiter splitting. Value/display mappings and allowed-value validation are separate checks.

<a id="module.env"></a>
[I · module.env]

`-env` takes `;`-separated JSON files of settings for the module, merges them and replaces `${VAR}` with environment values. The module reads them with `%IPM.General.EnvironmentConfig:GetArg`, and a missing file is an error.

- Files merge in order. A later value replaces an earlier one, and nested objects merge key by key.
- Every command that runs module phases starts from an empty configuration and loads only its own `-env` files. A later command in the same shell session does not see an earlier command's files.
- While a configuration is loaded, IPM compiles the module's classes in one process, so code generated at compile time can read it.
- What a failing file should leave behind, and how arrays merge, is [open](../../Verus_verification/open_questions.md#module.env.failure).
- An environment value is inserted literally. As built, a `$` or `\` in the value changes or breaks the result. This is a known defect.

- Object elements in arrays merge recursively at the same array index: `[{"x":1}]` followed by `[{"y":2}]` keeps both keys. Primitive arrays merge by position: `[1,2]` then `[3]` yields `[3,2]`.

- Both direct merge and the actual file loader have receipts. A missing or malformed later file clears earlier values and raises; each `LoadEnvironmentConfig` call clears first.

- Process-local config reaches a same-process compile, while a fresh worker process starts empty. These are observed implementation rules; C:Q6 below keeps the desired failure/merge policy open.

- Working assumption: as built. The command fails and keeps nothing from any file, and a later array overwrites the earlier one position by position, so it cannot remove items.

## Metadata

### module.identity.document

- design-verified

- agent
- user-facing-behavior
- Expands: [Module identity](../concepts/data-policy.g5.md#module.identity).

Sources: `StudioDocument/Module.cls::Save`, `::ImportStream`, `::UpdatePersistentFromStream`, `::Delete`. Model: [document.rs](../../Verus_verification/consolidated/iter39/document.rs) `save`, `delete`, `valid_edit_updates_record`, `refused_save_erases_text`, `other_name_accepted_under_old_name`, `empty_resets_version_keeps_resources`, `delete_keeps_classes` ([handoff](../../uplift/consolidated/iter39/HANDOFF.md)). Compile-time document handling is not assessed.

Findings: [A refused module document save empties the stored document text](../../Verus_verification/bugs_issues.md#c-d44)

Findings: [A module document that names a different module is saved under the old name](../../Verus_verification/bugs_issues.md#c-d45)


Iteration52: the record's LastModifiedTimestamp, which the `.ZPM` document timestamp falls back to, changes on every save because the hashed export includes LastUpdated ([record.rs](../../Verus_verification/consolidated/iter52/record.rs) `identical_reload_still_modified`).

State machines: [document · iter39](../../Verus_verification/models/iter39-document/manifest.json), [record · iter52](../../Verus_verification/models/iter52-record/manifest.json)

### module.dependency

- human-provided
- user-facing-behavior
- Expands: [Module](../concepts/data-policy.g5.md#module).

Sources: `wiki/03` L243-250; CHANGELOG 0.10.7 (`#408`), 0.10.6 (`#1072`). Inventory: CAT-03. Test: `DepNoVersion.cls`. Observed: a `Scope` on a dependency limits it to the test or verify phase, which only the code and tests describe ([phase scope](dependencies.g5.md#deps)). `wiki/03` L247 has a wrong `^2.2.1` example.

### module.requirement

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Module](../concepts/data-policy.g5.md#module).
- Children: [requirement values](#module.requirement.values).

Sources: `wiki/03` L260-272; CHANGELOG 0.9.0 (`#364`), 0.10.0 (`#469`), 0.10.4 (`#874`), 0.10.10 (`#106`). Inventory: CAT-05; OP-LOAD-12. Tests: `VersionReq.cls` (`IPMVersion`, `PythonVersion`), `SystemRequirements.cls`, `Health.cls`, `Interoperability.cls`. When the check runs is open ([Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?](../../Verus_verification/open_questions.md#load.refusal.requirements.timing)). `wiki/03` L267 dates `IPMVersion` to 0.9.0, CHANGELOG says 0.10.0.

### module.requirement.values

- agent
- Expands: [System requirements](#module.requirement).

Sources: `Storage/SystemRequirements.cls::{CheckRequirements,CheckHealth,CheckInteroperability,CheckSYSNamespace}`; tests `Health.cls`, `Interoperability.cls`, `SystemRequirements.cls`. Verus `resources.rs::{requirement_failures,requirement_neighbors}`. Native environment-subclass probes exercise the unmodified check bodies, not every product variant or version expression.

Iteration75 neighbours: [r8b_manifest.rs](../../Verus_verification/coverage-20261117/iter75/r8b_manifest.rs) ([handoff](../../uplift/coverage-20261117/iter75/HANDOFF.md)).

### module.parameter

- human-provided
- user-facing-behavior
- Expands: [Module](../concepts/data-policy.g5.md#module).

- Children: [expressions](#module.parameter.expressions), [literal values](#module.parameter.literal-values), [cycles](#module.parameter.cycles).

Sources: `wiki/03` L252-258; `wiki/00` L175-180; CHANGELOG 0.10.3 (`#779`), 0.10.7 (`#1013`). Inventory: CAT-04; OP-LOAD-05 (`Storage/Module.cls:799-831`). Native: a `-D` value overrides a default (E07), and chains resolve through several levels (E22, E31, E36).

Findings: [C:D3 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d3). The linked entry qualifies the caller, reader and providers.

Findings: [C:D4 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d4). The linked entry qualifies the caller, reader and providers.

Findings: [C:D5 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d5). The linked entry qualifies the caller, reader and providers.

Findings: [The installer class does not receive `<Default>` values](../../Verus_verification/bugs_issues.md#c-d41)

### module.parameter.expressions

- uncertain
- agent
- user-facing-behavior
- Expands: [Parameters](#module.parameter).

Sources: doc comment `Utils/Module.cls:2678-2696` (system expressions and `$$$macros`); code `Storage/Module.cls:1833-1894` (order: macros, parameters, system values, `{name}`, `#{}`), `Storage/ModuleSetting/Default.cls:14-110`, `Utils/Module.cls:2697-2815`. Native corpus E01-E36 (`20261114T025636Z-r1-evaluator`, IRIS 2026.1 build 234U ARM64) and model `Verus_verification/consolidated/models/attr_evaluator.py`, which matches every observed value and rejects 23 wrong variants. Observed details a reader may need: `$$$OK no brace` stays unchanged (E15); a default named `version` does not override the module version (E20); a parameter name is used as a regular expression, so `a.b` also matches `{$axb}` (E21).

Extension Verus: [attr_evaluator.rs::evaluate](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::evaluate_expression](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::macro_helper](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::macro_value](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::parameters](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::regex_replace_match](../../Verus_verification/consolidated/verus/attr_evaluator.rs), [attr_evaluator.rs::system_expression](../../Verus_verification/consolidated/verus/attr_evaluator.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Open questions: [Should a `$$$macro` expand when the attribute contains no opening brace?](../../Verus_verification/open_questions.md#module.parameter.expressions.brace-policy)

Iteration75 neighbours: [r8b_manifest.rs](../../Verus_verification/coverage-20261117/iter75/r8b_manifest.rs) ([handoff](../../uplift/coverage-20261117/iter75/HANDOFF.md)).

### module.parameter.literal-values

- agent
- user-facing-behavior
- Expands: [Parameters](#module.parameter).

Sources: the product escapes backslashes in values for exactly this reason (`Utils/Module.cls:2811`, `:2813`). Native: a default `win=C:\dir\sub` is inserted unchanged (E34), but a default `price=$5` makes `{$price}` raise `<REGULAR EXPRESSION>` (E32), while `{$namespace}` in the same module still works (E33). Defect C:D4.

Extension Verus: [attr_evaluator.rs::regex_replace_match](../../Verus_verification/consolidated/verus/attr_evaluator.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### module.parameter.cycles

- agent
- user-facing-behavior
- Expands: [Parameters](#module.parameter).

Sources: the product's own message "Circular reference or too many levels in placeholders" and its 20-level guard (`Storage/ModuleSetting/Default.cls:62-77`). The guard is unreachable: the loop condition already stops at zero. Native: `a=${b}`, `b=${a}` resolve without error to `${a}` (E30), and using `${a}` then raises `<REGULAR EXPRESSION>` (E18, E35) rather than the named error. Defect C:D3.

Extension Verus: [attr_evaluator.rs::resolve_bounded](../../Verus_verification/consolidated/verus/attr_evaluator.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### module.list.delimiter

- design-verified

- agent; user-facing-behavior; source-inspected; Verus-scoped
- Expands: [module.resource](../concepts/data-policy.g5.md#module.resource).

Sources and readers: `DataType/ListOfOptions.cls::XSDToLogical` → generated property conversions and `ResourceProcessor/Abstract.cls::CopyAttributes`. [Iteration 7 index](../../uplift/consolidated/iter7/INDEX.md). [Contract](../../Verus_verification/consolidated/iter7/inputs.rs): `split`, `delimiter`. These are specification proofs; native observations and remaining provider boundaries are in the index and trust inventory.

State machines: [inputs · iter7](../../Verus_verification/models/iter7-inputs/manifest.json)

### module.env

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Module](../concepts/data-policy.g5.md#module).

- Children: [When one of several `-env` files is missing, malformed or an array, what should the command keep, and how should two arrays merge?](../../Verus_verification/open_questions.md#module.env.failure).

Sources: `Main.cls:66`, `:73`, `:304`; CHANGELOG 0.10.0 (`#704`, `#743`), 0.10.10 (`#1260`). Inventory: CAT-37. Test: `EnvironmentConfig.cls`. The help points to a wiki page that does not exist. Code: `General/EnvironmentConfig.cls:9-129`, `Lifecycle/Base.cls:2188-2200` (clear, then load in order; called in every Reload phase, `:631`, and by the phase commands, `Main.cls:2755`, `:2832`), `Utils/LegacyCompat.cls:36-41` (single-process compile). Consolidated round 5 native runs `20261114T033831Z-r5-env` and `20261114T033908Z-r5-env-b` (ARM64) and model `Verus_verification/consolidated/models/env_config.py`: deep merge and position-by-position arrays (U2, U2r); a missing, malformed or array file fails and empties the configuration (U3-U5); a later scalar replaces an object, a later object onto a scalar fails (U6, U6r); `${CONS_VAR}` fills from the OS environment, an unset one is empty (U7); `a$1b` becomes `aCONS_DOLLARb`, `a\b` becomes `ab`, `a$9b` raises `<REGULAR EXPRESSION>` (U7, U8; defect C:D14); in one interactive session the next commands read an empty configuration (S1); a compile-time generator sees the configuration only in a single-process compile (M1-M4), which IPM selects while a configuration is loaded (S3).

Extension Verus: [env_config.rs::clear](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::command](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::evaluate_json](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::generator_visible](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::is_empty](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::load_file](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::lookup](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::merge_value](../../Verus_verification/consolidated/verus/env_config.rs), [env_config.rs::singleton_get](../../Verus_verification/consolidated/verus/env_config.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [C:D14 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d14). The linked entry qualifies the caller, reader and providers.

Open questions: [When one of several `-env` files is missing, malformed or an array, what should the command keep, and how should two arrays merge?](../../Verus_verification/open_questions.md#module.env.failure)

Iteration75 neighbours: [env_loader.rs](../../Verus_verification/coverage-20261117/iter75/env_loader.rs) ([handoff](../../uplift/coverage-20261117/iter75/HANDOFF.md)).
