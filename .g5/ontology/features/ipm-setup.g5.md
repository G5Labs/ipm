# Installing and upgrading IPM

How IPM itself gets into an IRIS instance. IPM installs itself with its own [load](load.g5.md), so its installation is also recorded in [history](history.g5.md#history.actions).

## Statements

<a id="setup"></a>
[I · setup]

IPM is installed with a one-line installer downloaded from the registry, or offline from a `zpm.xml` file. A user with the `%ALL` role runs it.

<a id="setup.installer"></a>
[I · setup.installer]

The installer:

- Installs only when its version is newer than the IPM already present.
- Maps the `%IPM` package into the current namespace.
- Sets default settings without overwriting existing ones.
- Loads IPM in normal mode without automatic mappings.
- Moves data from legacy IPM 0.7, and maps IPM to all namespaces when upgrading from it.
- Installs the Python packages ORAS needs and the `ipm` and `iriscli` helper scripts.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). These blocks are outside the formal scope, except that the installer's `load` follows [load](load.g5.md).

### setup

- human-provided
- user-facing-behavior
- Children: [installer](#setup.installer).

Sources: README L13-42. Inventory: CAT-38; A2 1.3 installer bootstrap (`IPM.Installer:setup`, `preload/cls/IPM/Installer.cls:69`).

### setup.installer

- human-verified
- user-facing-behavior
- test-coverage
- Expands: [Setup](#setup).

Sources: README L13-42, L64-70; `preload/cls/IPM/Installer.cls` (`ZPMInit` L110-124, `ZPMLoad` L126-131, `MapIfLegacy` L133); root `module.xml` L23-59; CHANGELOG 0.9.0 (`#454`), 0.10.7 (`#1029`, `#1110`), 0.10.10 (`#1215`). Inventory: CAT-38, A1 seed 1; OP-LOAD-27. Test: `tests/migration/v0.7-to-v0.9` (`GloballyEnabled`, `ModuleStorage`). Observed: the installer runs `load <dir> -DDeveloperMode=0 -DNoMapping=1` through the shell, so it gets a history record, and IPM's own Invokes run nested shell commands inside that load's transaction. A custom `ModuleRoot` breaks the helper-script install (`preload/cls/IPM/Installer.cls:564`).
