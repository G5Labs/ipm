# Package source selection

Install, reinstall, update and dependency resolution share these rules for choosing a package source. Operators configure the list through [repository management](../operations/repositories.g5.md).

## Definitions

An enabled repository is a source configured for use in this namespace. A qualified request names the repository as well as the module.

## Eligible sources

<a id="repo"></a>
[I · repo]

A repository is a place IPM finds modules, and each namespace has its own list of them. A repository can be:

- A remote registry, such as the community registry or a `zpm-registry` server.
- An ORAS (OCI) registry.
- A filesystem folder that IPM scans for `module.xml` files.

<a id="repo.read-only"></a>
[I · repo.read-only]

Modules installed from a read-only repository always install in normal mode, never in developer mode.

<a id="repo.prereleases"></a>
[I · repo.prereleases]

IPM uses a repository for `+snapshot` builds or prerelease versions only when the repository is set up for them.

## Version and supplier selection

<a id="repo.search-order"></a>
[I · repo.search-order]

IPM searches enabled repositories for a module, in the order they were created. A `<repo>/<module>` prefix limits the search to one repository.

- A request containing keywords skips filesystem repositories; ordinary name searches can use them. This filter applies even when that filesystem repository is explicitly named.
- Keywords given as text are split on spaces, and empty pieces are dropped. A keyword string of only spaces therefore counts as no keywords and filesystem repositories are searched. The module name is lowercased before any repository is asked.

- IPM takes the highest version that satisfies the request, whichever repository offers it.
- As built, a filesystem repository offers only one version per module to that choice: the first row after it re-sorts its cache by version columns, comparing prerelease text as text and putting a release after its own prereleases. An unversioned install can therefore pick `2.0.0-beta.1` over `2.0.0`, or `1.0.0-rc.2` over `1.0.0-rc.10` (defect C:D39). Its version index itself lists versions in [version order](versions.g5.md#version.order).
- When several repositories offer that version, the one created first wins. A repository's sort position does not change this.
- The selected repository also supplies the [saved supplier name and historical source details](data-policy.g5.md#repo.origin.installed).

<a id="repo.install.version-request"></a>
[I · repo.install.version-request]

An explicit install version constrains the module that is actually installed, not only the repository entry selected. A successful request for `1.0.0` must not silently install `2.0.0`.

## Versions in search results

<a id="repo.search.version-list"></a>
[D · repo.search.version-list]

An AllVersions search groups matching versions into one result per module. Its selected version is the newest eligible version, and its AllVersions text lists eligible versions from newest to oldest.

For releases 1.0.0, 2.0.0 and 3.0.0, an unrestricted search selects 3 and lists 3,2,1; a `<3.0.0` search selects 2 and lists 2,1. Filtering happens before the grouped result is formed.

The new proof covers this major-only release domain after range parsing. General semantic-version parsing, prerelease order, repository ties and multi-module grouping retain their separate contracts and gaps.

## Related rules on other pages

<a id="repo.manage"></a>
- Configure package sources — [repo.manage](../operations/repositories.g5.md#repo.manage).

<a id="repo.origin.installed"></a>
<a id="repo.origin.same-version"></a>
- Current supplier and historical source — [repo.origin.installed](data-policy.g5.md#repo.origin.installed), [repo.origin.same-version](data-policy.g5.md#repo.origin.same-version).

<a id="repo.default-registry"></a>
- Community registry on a new installation — [repo.default-registry](../operations/repositories.g5.md#repo.default-registry).

<a id="repo.oras"></a>
<a id="repo.oras.integrity"></a>
- Availability and integrity of OCI artifacts — [repo.oras](../architecture/package-sources.g5.md#repo.oras), [repo.oras.integrity](../architecture/package-sources.g5.md#repo.oras.integrity).

<a id="repo.filesystem"></a>
<a id="repo.filesystem.duplicates"></a>
<a id="repo.filesystem.mutable-source"></a>
- Filesystem catalog and source changes — [repo.filesystem](../architecture/package-sources.g5.md#repo.filesystem), [repo.filesystem.duplicates](../architecture/package-sources.g5.md#repo.filesystem.duplicates), [repo.filesystem.mutable-source](../architecture/package-sources.g5.md#repo.filesystem.mutable-source).

<a id="repo.remote.download"></a>
<a id="repo.remote.download.redirect"></a>
- Remote package registries — [repo.remote.download](download-security.g5.md#repo.remote.download), [repo.remote.download.redirect](download-security.g5.md#repo.remote.download.redirect).

<a id="repo.remote.download.integrity"></a>
<a id="repo.remote.download.token"></a>
- Checking the downloaded file and reporting installation — [repo.remote.download.integrity](download-security.g5.md#repo.remote.download.integrity), [repo.remote.download.token](download-security.g5.md#repo.remote.download.token).

<a id="repo.publish-cache"></a>
- Reusing a publishing connection — [repo.publish-cache](../architecture/package-sources.g5.md#repo.publish-cache).

<a id="repo.oras.platform-selection"></a>
<a id="repo.oras.tag-decode"></a>
<a id="repo.oras.selection-modes"></a>
<a id="repo.oras.build-readers"></a>
- Platform tags and version listings — [repo.oras.platform-selection](../architecture/package-sources.g5.md#repo.oras.platform-selection), [repo.oras.tag-decode](../architecture/package-sources.g5.md#repo.oras.tag-decode), [repo.oras.selection-modes](../architecture/package-sources.g5.md#repo.oras.selection-modes), [repo.oras.build-readers](../architecture/package-sources.g5.md#repo.oras.build-readers).

## Metadata

### repo

- human-provided
- user-facing-behavior
- Children: [manage](../operations/repositories.g5.md#repo.manage), [search order](#repo.search-order), [read-only](#repo.read-only), [prereleases](#repo.prereleases), [default registry](../operations/repositories.g5.md#repo.default-registry), [ORAS](../architecture/package-sources.g5.md#repo.oras), [filesystem](../architecture/package-sources.g5.md#repo.filesystem), [remote download](download-security.g5.md#repo.remote.download).

Sources: README L1-6; `Main.cls:222-266`. Inventory: CAT-20.

### repo.read-only

- human-provided
- user-facing-behavior
- Expands: [Repositories](#repo).

Sources: `Main.cls:262`; `wiki/00` L135. Inventory: INT-LOAD-13; OP-LOAD-10. Observed: normal mode is forced after the update-required check ([Should `-dev` also lift the update-required refusal?](../../Verus_verification/open_questions.md#load.refusal.update-required.dev-bypass)).

### repo.prereleases

- human-provided
- user-facing-behavior
- Expands: [Repositories](#repo).

Sources: `Main.cls:260-261`; CHANGELOG 0.9.0 (`#544`, `#557`). Inventory: CAT-19.

### repo.search-order

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Repositories](#repo).

Sources: CHANGELOG 0.9.0 (`#261`); CHANGELOG 0.9.x "Repo check should happen in the order to repo creation, not by repo name" (HSIEO-9269, HSIEO-9404); `Main.cls:413-414` (`install registry/HS.JSON`). Inventory: CAT-18. Code: `Repo/Manager.cls:67-70` (SQL order by sort position), `:150` (results keyed by version, then repository ID), `:153-187` (highest version first). Consolidated round 1 native cases TR5, TR5b, TR5c (`20261114T025102Z-r1-temp-repo`) and model `Verus_verification/consolidated/models/temp_repo.py` (`search`; wrong variants `search-tie-by-sortorder`, `search-first-repo`, `search-lowest-version` rejected). The earlier observation that a load's `.modules` folder is searched before all others holds only for versions no older repository offers ([When another repository offers the same version of a dependency as the `.modules` folder, which copy should IPM install?](../../Verus_verification/open_questions.md#load.dependencies.local-repo.precedence)).

Extension Verus: [temp_repo.rs::select](../../Verus_verification/consolidated/verus/temp_repo.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Iteration 3 search repair: `Repo/Manager.cls::SearchRepositoriesForModule` and `Repo/SearchCriteria.cls`; Verus `public_repairs.rs::repo_visited` and `search_neighbors`. Native same-repository search finds the seeded module by name and returns no result after adding a keyword. This changes no prior version/tie rule; command-local rejected-credential suppression remains a separate gate.

Iteration 4 token producer: `Repo/SearchCriteria.cls::KeywordsSet` (text split on single spaces, `$list` input not split, empty elements dropped from both; a list object assigned as given) and the lowercased criteria name in `SearchRepositoriesForModule`. Verus [repairs4.rs](../../Verus_verification/consolidated/iter4/repairs4.rs) `{keywords,visits_filesystem,criteria_name,keyword_neighbors}`; controls `empty-tokens-kept`, `dollar-list-not-filtered`, `object-filtered`, `keywords-ignored-at-gate` rejected. The token rule is source-only; no new native case.

Iteration 34 version producer: `General/SemanticVersion/Index.cls::GetIndexGlobalNode`, `General/SemanticVersion/Find.cls` reverse walk and flags, `Repo/Utils.cls::moduleSqlToList` re-sort and first row per name. Model: [versions.rs](../../Verus_verification/consolidated/iter34/versions.rs) `releases_agree`, `release_versus_own_prerelease`, `fixture_neighbors` ([handoff](../../uplift/consolidated/iter34/HANDOFF.md)). Release-only offers keep the highest-first premise relied on by iteration31.

Findings: [P:F-INST-4 — e2e reproduced](../../Verus_verification/bugs_issues.md#p-f-inst-4). The linked entry qualifies the caller, reader and providers.

Findings: [A filesystem repository install picks a prerelease over its own release and orders prerelease numbers as text](../../Verus_verification/bugs_issues.md#c-d39)

State machines: [versions · iter34](../../Verus_verification/models/iter34-versions/manifest.json), [repairs4 · iter4](../../Verus_verification/models/iter4-repairs4/manifest.json)

### repo.install.version-request

- design-verified

- agent; inferred-from-product-help; user-facing-behavior
- Expands: [repository selection](#repo.search-order).

The product-authored install help defines the version argument as the version or version expression of the module to install; the pinned-install comment says selection filters its requested version. This recovers the existing user-facing obligation, with no new owner decision. [Temporal model](../../Verus_verification/consolidated/iter21/temporal.rs) distinguishes the exact checked release requests from the actual loaded module. Generic version/range conformance retains its separate gaps.

Findings: [C:D32 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d32). The linked entry qualifies the caller, reader and providers.

State machines: [temporal · iter21](../../Verus_verification/models/iter21-temporal/manifest.json)

### repo.search.version-list

- design-verified

- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [repo](#repo).

Sources and readers: `Repo/Filesystem/PackageService.cls::ListModules` → `Repo/Utils.cls::moduleSqlToList` → `Repo/Manager.cls::SearchRepositoriesForModule` → `Repo/Utils.cls::SearchRepositoriesForModule`. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/catalog.rs): `eligible`, `ordered`, `grouped`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

State machines: [catalog · iter8](../../Verus_verification/models/iter8-catalog/manifest.json)
