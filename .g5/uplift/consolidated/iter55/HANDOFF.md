# Iteration 55: `namespace`/`zn`, `exec`/`cos` and `init -noprompt`

Batch 11 (convergence test over the remaining untested commands). Product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter55/nsinit.rs), [cases and controls](../../../Verus_verification/consolidated/iter55/cases.json), [session](../../../Verus_verification/consolidated/iter55/session.txt), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Not sealed or committed; the main thread integrates it.

## Gap

No product test calls `zn`, `exec` or `init`. `Main.cls::Namespace`, `ExecuteCOS` and `Init` had only historical citations. The ontology had one line each, in [inspect](../../../ontology/features/inspection.g5.md#inspect) and [devtools.authoring](../../../ontology/features/developer-tools.g5.md#devtools.authoring).

## Predictions and observations

| Case | Observed | Owner/help predicted | Source predicted |
|---|---|---|---|
| z1 `zn NS55A` (namespace without IPM) | "IPM is not enabled", status OK, no switch | switch or warn | same as observed |
| z2 `zn NOPE55` | error "Failed to switch", no switch | error | same |
| z3 `zn USER` | OK, stays in USER | switch | same |
| z4 `zn %SYS` | not enabled, no switch | unspecified | switch if %IPM visible (refuted: not visible) |
| z5 `zn NS55*`, empty answer | lists `NS55A> (IPM not enabled)`, no switch | lists matching namespaces | same |
| e1 `exec set ^IPMCONT55=1` | OK, global set | runs | runs |
| e2 `exec set $namespace="%SYS"` (help example) | OK, namespace unchanged | switches (help example) | no lasting effect (`new $namespace`) |
| e3/e4 runtime or syntax error | command fails | fails | fails |
| i1 `init -noprompt`, no source control | class stays empty | configures | same |
| i2 plain `%Studio.SourceControl.Base` configured, `init -noprompt` | class stays Base. The Composite stores Base as primary. Output says "Enabling package manager extension if present... done" and lists Base as "(primary)" | wraps the extension | full wrap (refuted in explore1); round 2: stays plain |
| i3 `init -noprompt -extension %Studio.SourceControl.Base` | Composite active, primary Base | sets extension | same |
| i4 dangling class configured, `init -noprompt` | disabled with message, class empty | unspecified | same |

## Evidence

- Formal: `nsinit.rs` 3 verified, 0 errors. Five controls were each rejected: `init-activates-composite`, `exec-keeps-namespace-change`, `zn-switch-without-ipm`, `dangling-kept` and `unknown-ok`.
- Native: [receipt](native/20261116T181741769046Z/observations.json), 214 fields, all gated. The disposable container creates namespace NS55A.
- Replay: 22 fields in Verus. Flips of `i2.sc.base`, `z1.after.user` and `e2.after.user` were each rejected.
- Attempts: explore1 refuted the full-wrap prediction, and its listing markers matched prompt text. Both logs are kept under `attempts/`.

## New discrepancies

Yes, two, both Low and both E2E reproduced:

1. `exec`'s help example ("Run the Objectscript expression to set the namespace to MYAPP": `cos set $Namespace = "MYAPP"`) cannot take effect. `ExecuteCOS` runs under `new $namespace`, so the session stays in its namespace (e2), while data effects persist (e1).
2. `init -noprompt`, with a plain existing source-control class, prints that it enabled the package manager extension and lists the class as the configured primary. But the namespace keeps the plain class active, so the IPM hooks bound in iteration43 never run (i2). Passing neighbour: `-extension <class>` activates the Composite (i3).

## Reach candidates

Newly reached: `Main.cls::Namespace`, `::ExecuteCOS` and `::Init`. Strengthened with this role: `IsIPMEnabled`, `Shell`, `CompositeMethodOverrides.SetExtensionClasses` and `GetPrimaryExtensionClass`.

## Limits

There was no second IPM-enabled namespace, so a successful switch to another namespace was not exercised. Interactive `init` prompts, `-zpm`, `-menuextension` and the general-settings prompts are not exercised. `GetListNamespace` is not on the `zn` path, and its other callers are not assessed. Namespace and database creation are providers.

## Proposed owner text

- `inspection.g5.md`: add `[D · inspect.namespace]` after `inspect.list-installed`:
  "`zn <name>` switches the current session to that namespace only when IPM is available there. Otherwise it prints "IPM is not enabled" and leaves the namespace and the command status unchanged, while an unknown namespace fails the command. `zn <pattern>` lists the matching namespaces, marking those without IPM, and switches to the one the user enters; an empty answer keeps the current namespace."
  Metadata: `agent`, `user-facing-behavior`, Expands [Inspection](#inspect). Model: [nsinit.rs](../../Verus_verification/consolidated/iter55/nsinit.rs) `zn`, `zn_neighbors` ([handoff](../../uplift/consolidated/iter55/HANDOFF.md)).
- `developer-tools.g5.md`: add `[D · devtools.exec]` (its bullet `exec runs ObjectScript` already exists in the scripting statement):
  "`exec <expression>` runs ObjectScript in the current namespace. Data changes it makes persist, and a runtime or syntax error fails the command. As built, the expression runs under a saved `$namespace`, so changing the namespace has no lasting effect, although the command help gives exactly that as its example (defect C:D-next-1)."
  Metadata: `agent`, `user-facing-behavior`. Model: `exec`, `exec_neighbors`.
- `developer-tools.g5.md`: add `[D · devtools.init]` (expanding `devtools.authoring`):
  "`init -noprompt` reindexes the filesystem cache and changes the namespace's source-control setup only as asked. A configured class that no longer exists is disabled, and `-extension <class>` makes IPM's composite extension active with that class as primary. As built, without `-extension` an existing plain class is only stored as the composite's primary while it stays the active class, although the command reports that it enabled the package manager extension (defect C:D-next-2)."
  Metadata: `agent`, `user-facing-behavior`. Model: `init_quiet`, `init_neighbors`. It links [lifecycle.source-control](../../ontology/concepts/lifecycle.g5.md#lifecycle.source-control) for why the active class matters.
- C register, Low · Decision: Open: "The `exec` help example that changes the namespace has no effect". Found by: source review of `Main.cls::ExecuteCOS` (`new $namespace`), with model control `exec-keeps-namespace-change`. Reproduced by: E2E reproduced, receipt `iter55/native/20261116T181741769046Z`. `exec set $namespace="%SYS"` returns OK and the session stays in USER (e2). Passing neighbour: `exec set ^IPMCONT55=1` persists (e1). Repair, unchecked: either drop `new $namespace` or change the help example.
- C register, Low · Decision: Open: "`init -noprompt` reports enabling the package manager extension but leaves a plain source-control class active". Found by: explore1 refuting the source prediction, then source review of `Main.cls::Init` (`SetExtensionClasses` stores the primary without setting the namespace class), with control `init-activates-composite`. Reproduced by: E2E reproduced, same receipt. With `%Studio.SourceControl.Base` configured, `init -noprompt` prints "Enabling package manager extension if present... done" and lists Base as "(primary)", but the namespace class stays Base (i2). Passing neighbour: `-extension %Studio.SourceControl.Base` activates the Composite (i3). Repair, unchecked: also set the namespace class to the Composite when wrapping, or word the message as a stored preference.
