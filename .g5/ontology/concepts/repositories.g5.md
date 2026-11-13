# Repositories

Repositories are where `install`, `update` and dependency loads find modules. `load` takes a local path instead, but still finds its dependencies here.

## Definitions

- **Registry**: a server that stores published modules.
- **Enabled repository**: a repository set up in the namespace and not disabled.

## Statements

<a id="repo"></a>
[I · repo]

A repository is a place IPM finds modules, and each namespace has its own list of them. A repository can be:

- A remote registry, such as the community registry or a `zpm-registry` server.
- An ORAS (OCI) registry.
- A filesystem folder that IPM scans for `module.xml` files.

<a id="repo.manage"></a>
[I · repo.manage]

`repo` lists, adds, changes, enables, disables and deletes repositories, and lists the modules they hold.

- Credentials can come from `-password-stdin`, `-password-env` or `-token-env`.
- `-publish 1` allows publishing to a repository.
- `-copy-from` copies repository settings from another namespace.

<a id="repo.search-order"></a>
[I · repo.search-order]

IPM searches all enabled repositories for a module, in the order they were created. A `<repo>/<module>` prefix limits the search to one repository.

<a id="repo.read-only"></a>
[I · repo.read-only]

Modules installed from a read-only repository always install in normal mode, never in developer mode.

<a id="repo.prereleases"></a>
[I · repo.prereleases]

IPM uses a repository for `+snapshot` builds or prerelease versions only when the repository is set up for them.

<a id="repo.default-registry"></a>
[I · repo.default-registry]

IPM 0.9 and later does not enable the community registry by default. `repo -r -n registry -url https://pm.community.intersystems.com/` or `enable -community` adds it.

<a id="repo.oras"></a>
[I · repo.oras]

An ORAS registry can use a registry namespace, which is a path inside the registry and not an IRIS namespace.

- It supports Basic and token authentication.
- IPM finds a module there by its name or its external name.
- Errors name the repository and show the registry's status and message.

<a id="repo.filesystem"></a>
[I · repo.filesystem]

A filesystem repository offers every module whose `module.xml` IPM finds under its folder, down to a set depth.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). Repository search is used by the formal scope through [install](../features/install.g5.md#install) and [dependencies](../features/dependencies.g5.md#deps). The repository internals themselves are outside it.

### repo

- human-provided
- user-facing-behavior
- Children: [manage](#repo.manage), [search order](#repo.search-order), [read-only](#repo.read-only), [prereleases](#repo.prereleases), [default registry](#repo.default-registry), [ORAS](#repo.oras), [filesystem](#repo.filesystem).

Sources: README L1-6; `Main.cls:222-266`. Inventory: CAT-20.

### repo.manage

- human-provided
- user-facing-behavior, security
- Expands: [Repositories](#repo).

Sources: `Main.cls:222-266`; repository-type `XData Commands` in `Repo/*/Definition.cls`; CHANGELOG 0.10.5 (`#462`), 0.10.7 (`#1138`), 0.10.9 (`#1178`), 0.10.11 (`#1115`, `#1281`, `#1283`). Inventory: CAT-20; A2 1.2 row 16 (`Main:Repository`). Tests: `Unit/CLI.cls` `TestRepository`, `TestRepositoryEnvVars`. The help shows `-app`, `-local`, `-info` and a Perforce type with no implementation. Credential checks do not follow redirects to another host (CHANGELOG 0.10.11 `#1115`).

### repo.search-order

- human-provided
- user-facing-behavior
- Expands: [Repositories](#repo).

Sources: CHANGELOG 0.9.0 (`#261`); `Main.cls:413-414` (`install registry/HS.JSON`). Inventory: CAT-18. Observed: a load's `.modules` folder is searched before all others ([local repository](../features/load.g5.md#load.dependencies.local-repo)).

### repo.read-only

- human-provided
- user-facing-behavior
- Expands: [Repositories](#repo).

Sources: `Main.cls:262`; `wiki/00` L135. Inventory: INT-LOAD-13; OP-LOAD-10. Observed: normal mode is forced after the update-required check ([developer-mode bypass](../features/load.g5.md#load.refusal.update-required.dev-bypass)).

### repo.prereleases

- human-provided
- user-facing-behavior
- Expands: [Repositories](#repo).

Sources: `Main.cls:260-261`; CHANGELOG 0.9.0 (`#544`, `#557`). Inventory: CAT-19.

### repo.default-registry

- human-provided
- user-facing-behavior
- Expands: [Repositories](#repo).

Sources: README L23-30; `preload/cls/IPM/Installer.cls:114`. Inventory: CAT-21. `wiki/01` L56 still says the community registry "is used by default".

### repo.oras

- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Repositories](#repo).

Sources: CHANGELOG 0.10.0 (`#541`, `#754`), 0.10.7 (`#1130`), 0.10.10 (`#1267`), 0.10.11 (`#1115`, `#1269`, `#1281`); `Repo/Oras/Definition.cls:27-28`. Inventory: CAT-22, A1 seed 8. Test: `Oras.cls`.

### repo.filesystem

- human-provided
- user-facing-behavior
- Expands: [Repositories](#repo).

Sources: `Repo/Filesystem/Definition.cls`; `AGENTS.md` test guidance. Inventory: CAT-23. Observed: the folder scan is cached in `^IPM.Repo.Filesystem.CacheD`.
