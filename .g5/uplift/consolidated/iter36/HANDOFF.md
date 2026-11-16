# Iteration 36: module installer class callbacks

Batch 4 lane, product `571e04a1` unchanged. Not sealed; the main thread integrates. [Model](../../../Verus_verification/consolidated/iter36/installer.rs), [cases and controls](../../../Verus_verification/consolidated/iter36/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json).

## Gap

`<InstallerClass>` had no ontology owner. `Lifecycle/Base.cls::%Configure` was unbound. Purge forwarding on uninstall is already owned by [uninstall.purge.as-built](../../../ontology/features/uninstall.g5.md#uninstall.purge.as-built) (iteration27 `purge.rs`, P:F-UNIN-2) and is only reused here.

## Predictions and observations

Predictions were written before native runs. They are partly source-informed, because the parent thread had read `%Configure`/`%Unconfigure` and the ledger records that exposure. The intent sources were thin: the `simple-app` test installer declares four callbacks that all return OK, and no test asserts their arguments.

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| l1 `load m36a` | Install(db "", dev 0, upgrade 0); Configure(USER, Mode=param, Greeting ""); Invoke `${Greeting}`=hello | installer configured; Default and Parameter both reach the module (module.parameter) | same as observed |
| l3 load again `-DGreeting=bye -DMode=cli` | upgrade 0 although installed; Mode=cli, Greeting=bye; Invoke bye | upgrade unspecified | same |
| l5 `-DConfigure.IsUpgrade=1` | upgrade 1 | - | same |
| u1/u2 `uninstall` | Unconfigure(purge 0) then UnInstall(db "") | Unconfigure exists | same |
| l2 `load -dev` | dev 1 | - | same |
| e1 OnInstallComponent returns an error | load fails at Configure/Activate; no Configure call; module not installed; the installer's own record is rolled back (its `$increment` counter is not) | failing phase fails load | same, rollback of the record not predicted |
| n1 module without InstallerClass | no calls | none | none |

explore1 matched every prediction, but its reader crashed on the rolled-back e1 record. That log is kept and not counted, and the fixed reader ran explore2.

## Evidence

- Formal: `installer.rs`, 6 verified, 0 errors.
- Generic lemmas:
  - `default_versus_parameter`: a `<Default>` value reaches Invoke arguments but not installer variables, while a `<Parameter>` value does reach them.
  - `override_reaches_both`: `-D` overrides both.
  - `upgrade_ignores_installed_state`: IsUpgrade depends only on `Configure.IsUpgrade`.
  - `refusal_stops`, `no_installer_no_calls`.
- Controls, each rejected by an assertion (7):
  - `default-reaches-installer-vars`
  - `upgrade-from-installed-state`
  - `configure-after-refusal`
  - `refusal-record-kept`
  - `dev-flag-ignored`
  - `parameter-overrides-cli`
  - `unconfigure-after-uninstall-callback`
- Native: [receipt](native/20261116T173156335552Z/observations.json), 81 fields, all gated, network none, source pin matched.
- Replay: all 81 fields asserted in Verus ([receipt](replay/receipt.json)). The flips of `l1.call2.greeting`, `l3.call1.upgrade` and `e1.call1.kind` were rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter36/run.py` (15 s).

## Reach candidates

- **New:** `Lifecycle/Base.cls::%Configure`, `Storage/Module.cls::GetDefaultParameters`, `Storage/ModuleSetting/Default.cls::EvaluateArgs`.
- **Strengthened:** `%Unconfigure`, `Lifecycle/Module.cls::%Clean`, `Lifecycle/Module.cls::%Activate`, `%DispatchMethod`, `GetCustomParameters`, `ExecutePhases`, `InvokeReference.doInvoke`.

## Discrepancies

1. **`<Default>` values do not reach the installer.** `%Configure` computes `customParams` from `<Default>` elements and then never uses it. The installer's variables therefore carry `<Parameter>` and `-D` values, but not `<Default Name Value>` values. The same Default does reach Invoke arguments. [module.parameter](../../../ontology/concepts/modules.g5.md) says both kinds are values "the module receives at load and install". Proposed as a defect.
2. **IsUpgrade and DatabaseName are never set by IPM.** No product code writes `Configure.IsUpgrade` or `Configure.DatabaseName`, so a reload over an installed module reports IsUpgrade 0 and DatabaseName is always empty. The intended meaning is unknown, so this is proposed as a question.

## Limits

- One installer fixture, run in USER and in normal mode, except l2.
- Not assessed:
  - `OnConfigureComponent` failure
  - developer-mode rollback of a refusal
  - a failing reinstall of an already-installed module (the model keeps the earlier installed state; that is a premise)
  - `<Parameter LifecycleClass>` filtering
  - `module-action m configure`
  - Lifecycle/StudioProject
- No ObjectScript body is verified directly.

## Proposed owner text

New statements for `.g5/ontology/concepts/lifecycle.g5.md`, placed after `lifecycle.hooks`:

```
<a id="lifecycle.installer"></a>
[D · lifecycle.installer]

A module can name an installer class with `<InstallerClass>`. During Activate, IPM's Configure step calls its `OnInstallComponent` and then its `OnConfigureComponent`; at uninstall, Clean first calls `OnUnconfigureComponent` and then `OnUnInstallComponent`.

- `OnInstallComponent` receives the database name, the verbose flag, the developer-mode flag and an upgrade flag. `OnConfigureComponent` receives the namespace, the verbose flag and the phase parameters, plus the module root as `InstallDir`.
- If `OnInstallComponent` returns an error, `OnConfigureComponent` is not called, the load fails and an ordinary load rolls back, including data the installer wrote.
- A module without an installer class gets no callbacks.

<a id="lifecycle.installer.parameters"></a>
[D · lifecycle.installer.parameters]

The installer's parameters include `<Defaults><Parameter>` values and `-D` values, with `-D` winning. As built, `<Default Name Value>` values are not included, although Invoke arguments can use them (defect C:D-next).

<a id="lifecycle.installer.upgrade-policy"></a>
[Q · lifecycle.installer.upgrade-policy]

What should the installer's upgrade flag and database name mean?

- Working assumption: as built, IPM never sets them. The upgrade flag is 0 and the database name is empty unless `-DConfigure.IsUpgrade` or `-DConfigure.DatabaseName` supplies them, even when a newer version replaces an installed one.
- Alternative and implications: set the upgrade flag when the module is already installed, which lets installers migrate data but changes calls existing installers receive.
- This assumption guides review; it is not a maintainer decision.
```

Metadata for the three statements:
- **`lifecycle.installer`:** agent; user-facing-behavior. Sources: `Lifecycle/Base.cls::%Configure`, `::%Unconfigure`, `Lifecycle/Module.cls::%Clean`, and the `simple-app` test installer. Model: [installer.rs](../../Verus_verification/consolidated/iter36/installer.rs) `load`, `uninstall`, `refusal_stops`, `no_installer_no_calls`. Purge forwarding is owned by `uninstall.purge.as-built`.
- **`lifecycle.installer.parameters`:** agent; user-facing-behavior. Model: `default_versus_parameter`, `override_reaches_both`. Findings line to the new entry.
- **`lifecycle.installer.upgrade-policy`:** agent; uncertain. Model: `upgrade_ignores_installed_state`.

Proposed C register entry:

```
### C:D-next: The installer class does not receive `<Default>` values

Severity: Low · Decision: Open

Found by: source review of Lifecycle/Base.cls::%Configure (GetCustomParameters result unused), iteration36 model default_versus_parameter with control default-reaches-installer-vars.

Reproduced by: E2E reproduced. Iteration36 native receipt iter36/native/20261116T173156335552Z. Module m36a declares <Default Name="Greeting" Value="hello"/> and <Parameter Name="Mode">param</Parameter>. On load, OnConfigureComponent receives Mode=param but no Greeting (case l1), while the module's After-Activate Invoke receives ${Greeting}=hello. Passing neighbour: with -DGreeting=bye both receive bye (l3).

%Configure calls GetCustomParameters into customParams and then builds the installer variables only from the phase parameters, which hold <Parameter> and -D values. A repair would merge customParams into the variables before OnConfigureComponent, keeping -D precedence. It was not checked.
```

Severity is Low because the installer can still read `-D` values and `<Parameter>` defaults. Raise it to Medium if the owner treats `<Default>` as the documented installer input.
