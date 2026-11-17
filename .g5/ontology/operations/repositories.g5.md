# Managing repositories

Operators choose which package sources a namespace can use and which allow publishing. Package selection uses the resulting list.

## Configure package sources

<a id="repo.manage"></a>
[I · repo.manage]

`repo` lists, adds, changes, enables, disables and deletes repositories, and lists the modules they hold.

- Credentials can come from `-password-stdin`, `-password-env` or `-token-env`.
- `-publish 1` allows publishing to a repository.
- `-copy-from` copies repository settings from another namespace.
- `-trust-server` makes an ORAS repository [check signatures](../architecture/package-sources.g5.md#repo.oras.signed-admission) on a Docker Content Trust server. Other repository types refuse it.

## Community registry on a new installation

<a id="repo.default-registry"></a>
[I · repo.default-registry]

IPM 0.9 and later does not enable the community registry by default. `repo -r -n registry -url https://pm.community.intersystems.com/` or `enable -community` adds it.

## Metadata

### repo.manage

- human-provided
- user-facing-behavior, security
- Expands: [Repositories](../concepts/repositories.g5.md#repo).

Sources: `Main.cls:222-266`; repository-type `XData Commands` in `Repo/*/Definition.cls`; CHANGELOG 0.10.5 (`#462`), 0.10.7 (`#1138`), 0.10.9 (`#1178`), 0.10.11 (`#1115`, `#1281`, `#1283`). Inventory: CAT-20; A2 1.2 row 16 (`Main:Repository`). Tests: `Unit/CLI.cls` `TestRepository`, `TestRepositoryEnvVars`. The help shows `-app`, `-local`, `-info` and a Perforce type with no implementation. Credential checks do not follow redirects to another host (CHANGELOG 0.10.11 `#1115`).

### repo.default-registry

- human-provided
- user-facing-behavior
- Expands: [Repositories](../concepts/repositories.g5.md#repo).

Sources: README L23-30; `preload/cls/IPM/Installer.cls:114`. Inventory: CAT-21. `wiki/01` L56 still says the community registry "is used by default".
