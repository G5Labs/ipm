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
- `-community` sets up the community registry and maps IPM and its repositories to all namespaces, like IPM 0.7.

<a id="namespace.unmap"></a>
[I · namespace.unmap]

`unmap` removes IPM mappings from the named namespaces, or from all of them with `-globally`. It also unmaps shared repository settings, and `-repos-only` unmaps only those.

<a id="namespace.mapped-refusal"></a>
[I · namespace.mapped-refusal]

IPM cannot be installed in a namespace where it is mapped from another namespace.

<a id="namespace.legacy"></a>
[I · namespace.legacy]

Upgrading from IPM 0.7 or earlier maps IPM's code to all namespaces again, as before. `unmap -globally` undoes this.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). These blocks are outside the formal scope.

### namespace

- human-provided
- user-facing-behavior
- Children: [enable](#namespace.enable), [unmap](#namespace.unmap), [mapped refusal](#namespace.mapped-refusal), [legacy upgrade](#namespace.legacy).

Sources: README L23-30, L64-70. Inventory: CAT-29. Observed: IPM settings (`config`) and default modifiers are stored in `%SYS` or `^%` globals and may apply to the whole instance ([settings](settings.g5.md#settings)).

### namespace.enable

- human-provided
- user-facing-behavior
- Expands: [Namespace](#namespace).

Sources: `Main.cls:726-761`; README L66-70; `wiki/Installation-Patterns`; CHANGELOG 0.9.1 (`#663`), 0.9.2 (`#681`, `#682`), 0.10.7 (`#1001`). Inventory: CAT-29; A2 1.2 row 35 (`Main:EnableIPM`, `Main.cls:3553-3925`). The example `enable -v latest -allow-upgrade NS1,NS2,NS3` (`Main.cls:756`) omits `-ns`.

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
