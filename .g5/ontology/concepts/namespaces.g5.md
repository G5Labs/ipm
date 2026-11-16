# Namespaces

An IRIS namespace is where IPM installs modules. Since IPM 0.9, IPM itself also lives in each namespace separately.

## Definitions

- **Mapping**: an IRIS setting that makes code or data from one database visible in another namespace.

## Statements

<a id="namespace"></a>
[I · namespace]

IPM 0.9 and later is installed per namespace. Each namespace can have its own IPM version, repositories, installed modules and [history](../features/history.g5.md#history.namespace).

<a id="namespace.enable"></a>
[I · namespace.enable]

`enable` adds IPM to other namespaces.

- `-ns` names them, and `-globally` picks every namespace without IPM.
- `-map` maps IPM from the current namespace instead of installing a copy. `-repos` also shares repository settings and needs `-map`.
- `-v`, `-remote`, `-local-only` and `-allow-upgrade` choose the version and where it comes from.
- `-preview` shows the changes without making them.
- The pinned map path has [first-pass and local-repository limits](#namespace.enable.mapping-results).
- A missing later target can leave [earlier configuration entries pending](#namespace.enable.partial-batch).
- `-community` sets up the community registry and maps IPM and its repositories to all namespaces, like IPM 0.7.

- Working assumption: The source of a mapped namespace is the namespace whose own code database holds the IPM code.
- Working assumption: If a step of `enable -community` fails, the command stops and returns that error.

<a id="namespace.unmap"></a>
[I · namespace.unmap]

`unmap` removes IPM mappings from the named namespaces, or from all of them with `-globally`. It also unmaps shared repository settings, and `-repos-only` unmaps only those.

- The pinned product has a [routine-mapping removal gap](#namespace.unmap.routine-result).

<a id="namespace.mapped-refusal"></a>
[I · namespace.mapped-refusal]

IPM cannot be installed in a namespace where it is mapped from another namespace.

<a id="namespace.legacy"></a>
[I · namespace.legacy]

Upgrading from IPM 0.7 or earlier maps IPM's code to all namespaces again, as before. `unmap -globally` undoes this.

<a id="namespace.enable.mapping-results"></a>
[D · namespace.enable.mapping-results]

For an empty target, the first `enable -map -repos` shares IPM's code but leaves repository settings local; running it again shares the repositories. The repository loop checks active code mappings before the new ones are activated.

- A target with its own repository data keeps those settings instead of sharing the source's. Clearing that local data allows a later command to share the source's repositories.
- Preview changes no mappings. Requesting `-repos` without `-map` is refused before changes.
- These are the inherited P:F-NSP-4 behavior and its neighboring cases. They describe successful configuration calls, not failures during a batch.

<a id="namespace.unmap.routine-result"></a>
[D · namespace.unmap.routine-result]

Full unmap removes the package mapping and shared repository mappings but leaves the `%IPM.*` routine mapping. The target can still run the source's IPM, even from a fresh process.

- `-repos-only` removes shared repository mappings and deliberately keeps both code mappings. Actual repository-name reads follow the resulting mappings.
- The full-unmap result is inherited P:F-NSP-6: the caller passes `1` in the routine-type argument, so the helper tries a different mapping name. The intended removal rule above is preserved.

<a id="namespace.version.mapped-reader"></a>
[D · namespace.version.mapped-reader]

An IPMVersion requirement reads the `zpm` record in the current namespace, even when that namespace runs another namespace's IPM code. With no local record, the checked minimum `0.1.0` is refused, although the same requirement passes in the source namespace.

- A normal `-nodev` load of the fixture succeeds locally but fails in the mapped target and leaves no target module record. This is inherited P:F-NSP-2, with the actual load caller now checked.
- This rule does not claim general version parsing or developer-mode rollback behavior.

<a id="namespace.enable.partial-batch"></a>
[D · namespace.enable.partial-batch]

When `enable -map -ns VALID,MISSING` reaches a nonexistent later namespace, it returns an error after creating the earlier target's package and routine configuration entries. Those entries are still inactive: a fresh process in the valid target cannot run IPM. Retrying that valid target succeeds and activates them.

- Putting the missing namespace first creates no entries for the valid target. Preview with a missing later namespace also returns an error and creates no entries.
- Enabling a different valid target leaves the earlier pending target inactive, even for a fresh process. Activation loads the namespaces marked by that successful command.
- These cases use initially empty targets and successful configuration calls. They establish partial effects within the running instance, not restart durability or an intended all-or-nothing policy. Repository-loop failures are a separate case.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). Scoped formal associations and their provider premises are listed below; they do not cover every namespace command or migration path.

### namespace

- human-provided
- user-facing-behavior
- Children: [enable](#namespace.enable), [unmap](#namespace.unmap), [mapped refusal](#namespace.mapped-refusal), [legacy upgrade](#namespace.legacy).

Sources: README L23-30, L64-70. Inventory: CAT-29. Observed: IPM settings (`config`) and default modifiers are stored in `%SYS` or `^%` globals and may apply to the whole instance ([settings](settings.g5.md#settings)).

### namespace.enable

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Namespace](#namespace).

Sources: `Main.cls:726-761`; README L66-70; `wiki/Installation-Patterns`; CHANGELOG 0.9.1 (`#663`), 0.9.2 (`#681`, `#682`), 0.10.7 (`#1001`). Inventory: CAT-29; A2 1.2 row 35 (`Main:EnableIPM`, `Main.cls:3553-3925`). The example `enable -v latest -allow-upgrade NS1,NS2,NS3` (`Main.cls:756`) omits `-ns`.

Open questions: [namespace.enable.community-failure-policy](../../Verus_verification/open_questions.md#namespace.enable.community-failure-policy), [namespace.enable.source-ambiguity-policy](../../Verus_verification/open_questions.md#namespace.enable.source-ambiguity-policy)

### namespace.unmap

- human-provided
- user-facing-behavior
- Expands: [Namespace](#namespace).

Sources: `Main.cls:763-778`; README L66. Inventory: CAT-29; A2 1.2 row 36 (`Main:UnmapIPM`). The `-globally` description (`Main.cls:770`) is copied from `enable` and reads backwards.

### namespace.mapped-refusal

- human-provided
- user-facing-behavior
- Expands: [Namespace](#namespace).

Sources: message `Main.cls:2534-2552` ("Cannot install '%1' in namespace '%2' because it is mapped from namespace '%3'"); `wiki/Installation-Patterns` L15. Inventory: CAT-29; OP-LOAD-17, OP-LOAD-22.

### namespace.legacy

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Namespace](#namespace).

Sources: README L68; `preload/cls/IPM/Installer.cls:133-139`. Inventory: CAT-29, A1 seed 1. Test: `tests/migration` `GloballyEnabled`. Observed: the legacy hook maps code only, not repositories.

### namespace.enable.mapping-results

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Enable](#namespace.enable).

[Inherited state contract](../../../playbook-uplift/models/verus/ns_mapping.rs): `map_code_one`, `map_repo_one`, `enable_map`; [current boundary and named readers](../../Verus_verification/consolidated/iter12/namespaces.rs): `request`, `repo_visible`, `own_target`, `own_skip`, `own_cleared`. Native observations cover saved D/S/I mappings, active code and fresh-process name lookup while a local repository writer is present or cleared. [Index](../../uplift/consolidated/iter12/INDEX.md).

Findings: [P:F-NSP-4 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-nsp-4). The linked entry qualifies the caller, reader and providers.

State machines: [namespaces · iter12](../../Verus_verification/models/iter12-namespaces/manifest.json); [ns_mapping · inherited playbook](../../Verus_verification/models/playbook-ns-mapping/manifest.json)

### namespace.unmap.routine-result

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Unmap](#namespace.unmap).

[Inherited contract](../../../playbook-uplift/models/verus/ns_mapping.rs): `remove_rtn`, `unmap_one`, `unmap`, `has_ipm`; [current reader wrapper](../../Verus_verification/consolidated/iter12/namespaces.rs): `unmapped`, `unmap_request`, `repo_visible`. The routine type and active-vs-saved tables are distinct. Actual configuration success, fresh shell execution and repository visibility were checked; partial provider failures and source-selection ties remain conditional.

Findings: [P:F-NSP-6 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-nsp-6). The linked entry qualifies the caller, reader and providers.

State machines: [namespaces · iter12](../../Verus_verification/models/iter12-namespaces/manifest.json); [ns_mapping · inherited playbook](../../Verus_verification/models/playbook-ns-mapping/manifest.json)

### namespace.version.mapped-reader

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Namespaces](#namespace).

[Formal contract](../../Verus_verification/consolidated/iter12/namespaces.rs): `requirement`, `new_normal_load`, using inherited `check_ipm_version`. Version values are ranks for the checked current/minimum pair after parsing, not a new SemVer parser. Normal load rollback uses the existing [load contract](../features/load.g5.md#load.core); the fixture has no dependencies/resources and no prior module row. Native source/target loads and persisted rows are read separately. [Evidence](../../uplift/consolidated/iter12/INDEX.md).

Findings: [P:F-NSP-2 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-nsp-2). The linked entry qualifies the caller, reader and providers.

State machines: [namespaces · iter12](../../Verus_verification/models/iter12-namespaces/manifest.json)

### namespace.enable.partial-batch

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Enable](#namespace.enable).

[Ordered prefix and activation contract](../../Verus_verification/consolidated/iter13/batch.rs): `visit`, `run`, `publish`, `selected_neighbors`. Source: `Main::EnableIPM` switches namespace before each code-map call, defers activation until both loops finish and marks a retry even when entries already exist; `Utils.Module::PerformBatchActivation` loads only marked namespaces. Package/routine pairs are aggregated after successful providers. The model omits already-installed targets, repointing and arbitrary provider errors. A whole-configuration publication hypothesis was refuted by fresh native consumers and retained in the [evidence](../../uplift/consolidated/iter13/HANDOFF.md).

State machines: [batch · iter13](../../Verus_verification/models/iter13-batch/manifest.json)
