# Repository providers

IPM reads packages from local directories, remote package registries and OCI registries through a common repository manager. Each provider supplies different evidence about the version and contents it returns.

## Definitions

ORAS is the client IPM uses to store packages as OCI artifacts. A tag names an artifact in the registry. A layer contains part of the artifact.

## Filesystem catalog and source changes

<a id="repo.filesystem"></a>
[I · repo.filesystem]

A filesystem repository offers every module whose `module.xml` IPM finds under its folder, down to a set depth.

- IPM reads the folder into a cache when the repository is created and when it is refreshed. A refresh replaces the whole cache, so a module whose folder is gone stops being offered.
- A `module.xml` that is not a readable manifest is skipped. A readable manifest whose module name is invalid fails the whole refresh, which then changes nothing. Whether one bad module should block the rest is [open](../../Verus_verification/open_questions.md#repo.filesystem.bad-module).
- A refresh of an existing repository locks it. While another process holds that lock, the refresh fails after a timeout and changes nothing.
- The earlier claim that this explicit lock lasts until a caller's transaction ends was incorrect for the actual constructor. The default lock manager releases it when the refresh method returns, even if an outer transaction remains open. Explicit `%New(0)` keeps it through close until commit; default `%New(1)` releases at close.
- The cache still needs consistent replacement. How transaction isolation and other writers protect an uncommitted refresh remains [open](../../Verus_verification/open_questions.md#repo.filesystem.concurrent-refresh); the observed release does not authorize a race.

- Physical duplicates of one logical version follow the [first-admission rule](#repo.filesystem.duplicates). Later [source edits](#repo.filesystem.mutable-source) can make a selected cached version differ from the module actually loaded.

- Working assumption: no. Skip it with a warning, like an unreadable manifest, so the other modules stay available.
- Working assumption: keep the intended consistent cache and investigate competing refreshes under actual database isolation. Alternatives are deferred release, refusing an outer transaction or demonstrating that existing database locks suffice.

<a id="repo.filesystem.duplicates"></a>
[D · repo.filesystem.duplicates]

Within one filesystem root, the cache identifies a module by name and full version string. When two valid physical directories offer that identity, the first successfully admitted entry stays in the cache. A later duplicate prints a warning naming both directories and the retained one, and does not fail the build.

- First means the scan order processed by that cache build. It is not a promise that directory names have a portable lexical order. The checked constructor traces and separate scan samples agree for the selected stable fixtures.
- The retained manifest and directory belong to the same cache row. The manifest API returns its saved manifest, and the directory API returns its physical folder. With stable files and successful ordinary directory loading, installed description, root and callable payload follow that retained folder.
- Swapping the physical directory-to-payload assignment changes which duplicate is retained in the checked builds. A single valid entry has no duplicate warning. This is an explicit warning policy, not a newly found defect.
- This contract covers valid parsed manifests and fresh successful cache builds. Generic parsing/save failures, source edits after caching, removed directories, refresh races and other delivery branches remain outside the new qualification.

<a id="repo.filesystem.mutable-source"></a>
[D · repo.filesystem.mutable-source]

A filesystem cache saves a manifest but retains a pointer to its physical directory. Ordinary directory installation reads the current `module.xml` and source files from that directory, which may have changed since caching.

- In the checked sequence, the cache, manifest API and search still offer `1.0.0` after the same directory is replaced with a `2.0.0` manifest and different code. An explicit `1.0.0` install succeeds, saves `2.0.0` and its new description, and runs the new payload. This breaks [the requested-version rule](../concepts/repositories.g5.md#repo.install.version-request); it is C:D32, not a promise to accept the mismatch.
- Reconfiguring that same repository through the ordinary `repo -fs` command rebuilds its cache. Search then offers `2.0.0`; an explicit `1.0.0` request fails with not found and retains the installed `2.0.0`. An explicit `2.0.0` request succeeds.
- This qualification uses one ordinary directory, unchanged module name, plain releases `1.0.0` and `2.0.0`, successful file replacement and normal load/compile/save. It does not establish behavior during concurrent edits, source removal, parse/provider failure or other delivery branches. The [stable-source duplicate rule](#repo.filesystem.duplicates) retains its original scope.

## Downloads from a remote package registry

See [registry downloads, credentials and integrity](../concepts/download-security.g5.md#repo.remote.download).

## Availability and integrity of OCI artifacts

<a id="repo.oras"></a>
[I · repo.oras]

An ORAS registry can use a registry namespace, which is a path inside the registry and not an IRIS namespace.

- It supports Basic and token authentication.
- IPM finds a module there by its name or its external name.
- Errors name the repository and show the registry's status and message.

- The ORAS availability adapter accepts a successful HTTP 2xx reply or a 401 reply carrying `WWW-Authenticate`. A headerless 401 and a 403 are unavailable.

- This is an availability probe, not proof that authentication, pulling or publishing will work. Native loopback HTTP cases distinguish all four replies.

- Working assumption: ORAS `unpublish R/M V` deletes the plain tag and every `V__<platform>` tag of that version.
- Working assumption: Changing an ORAS registry's address clears its stored credentials, as for a remote registry.

<a id="repo.oras.integrity"></a>
[D · repo.oras.integrity]

As built, an ORAS pull without a tag selects the last tag supplied by the registry. The helper does not sort that list, despite its comment promising the highest version.

- A disposable provider returning `[2.0.0,1.0.0]` selects `1.0.0`; reversing the order selects `2.0.0`. The separate all-tags listing sorts versions, so it cannot justify the pull helper's ordering.
- Archive validation skips non-tar layers and compares size and digest with the first tar layer. A later matching tar layer does not rescue an earlier mismatch. The pull path also chooses the first downloaded `*.tgz`; those two choices need not name the same artifact.
- Failed validation returns an error even if the raw helper assigned a stream. The final `GetModule` reader throws that status instead of returning the stream to the loader. Multiple layers/files, provider order and digest integrity remain distinct premises.

- Working assumption: expose the mismatch and retain size/digest checking. Alternatives are sorting before selection or requiring a registry ordering contract, plus rejecting ambiguous archives or selecting by declared digest.

<a id="repo.oras.archive-boundary"></a>
[D · repo.oras.archive-boundary]

As built, the ORAS client can unpack a directory layer in its staging area before IPM checks the package archive. A failed archive size or digest check stops IPM from replacing the installed module's directory.

- A same-version reinstall with a size mismatch left an unpacked staging file. The installed files, callable code and module row stayed at the original revision. IPM recorded a failed, uncommitted install.
- ORAS dependency resolution reads the package's manifest annotation by tag, then retrieves the archive by tag again. A checked parent load resolved a child using revision 1 and installed revision 2 after the registry moved that same tag. The archive passed the later size and digest check against revision 2.

<a id="repo.oras.signed-admission"></a>
[I · repo.oras.signed-admission]

When verification is configured for an ORAS repository, IPM must verify the publisher's authorization for the selected artifact before unpacking it. Signing is part of external promotion.

- Proposed for [issue #281](https://github.com/intersystems/ipm/issues/281). The experiment contract also requires dependencies resolved from protected repositories to use metadata and bytes from the same authorized manifest. Failed verification must stop before unpacking or replacing the installed directory.
- Open decisions include the trust format and signer policy, whether protected resolution may choose an unprotected repository, and how a configuration change affects an in-flight operation. The proposed contract scopes each decision to the selected repository and policy revision.

## Platform tags and version listings

<a id="repo.oras.tag-decode"></a>
[D · repo.oras.tag-decode]

ORAS tags encode a version's `+` as `_` and append `__PLATFORM` for a deployed artifact. Splitting the platform suffix and decoding the version restores the checked prerelease/build string. A deployed reference with no selected platform is refused before download.

The model proves encode/decode roundtrip for version text without underscores and checks delimiter handling for the native literal. This is a tag codec rule, not proof that every encoded version is sorted correctly or that registry JSON/CSV transport is correct.

<a id="repo.oras.platform-selection"></a>
[D · repo.oras.platform-selection]

A deployed module can be selected for installation only when its reported platforms include the current IRIS platform. Normal repository listings apply the same test and show that platform for a compatible artifact; a nondeployed source package is not platform-specific.

- A module available only for another platform is hidden in the normal listing and refused at install. The all-versions listing may still show incompatible artifacts.
- The selected current platform is carried into the deployed artifact tag. A deployed reference with no selected platform is refused before download.
- [Raw tag decoding and selection modes](#repo.oras.selection-modes) are now checked for the pinned release/platform cases; general metadata transport and semantic-version parsing remain provider boundaries.

<a id="repo.oras.selection-modes"></a>
[D · repo.oras.selection-modes]

Raw tags are grouped by encoded package version, retaining the platform strings in their input order. Normal listing prefers a compatible version; dependency lookup enumerates candidate versions before the manager filters them and records the current platform for the pull tag.

- With current-platform 1.0.0 and only-incompatible 2.0.0, normal listing and installation select 1.0.0. A consumer requiring `>=1.0.0` loads that dependency and its 1.0.0 payload. A normal new consumer requiring `^2.0.0` fails with a satisfactory-version error and leaves neither consumer nor dependency record.
- AllVersions is tri-state at the service: 0 returns one preferred row with platforms; 1 returns one preferred row with raw all-tag text and no platform list; empty enumerates candidate rows for dependencies. The SQL show-all reader displays the grouped row, while feeding that same platform-empty deployed row through the manager filters it out.
- If 1.0.0 has both current and incompatible tags, its selected current tag still supplies the installed 1.0.0 payload. Metadata is fetched from the group's first platform tag; arbitrary input-order independence is not promised.
- These cases cover literal release versions, the pinned current/9999.1 platform pair, successful metadata/payload providers and a fresh normal consumer. Generic range parsing, other failures and preexisting state retain separate contracts.

<a id="repo.oras.build-readers"></a>
[D · repo.oras.build-readers]

For the two published current-platform builds `1.0.0+a.1.a` and `1.0.0+a.1.z`, raw tag sorting places the latter first. Aggregation parses the encoded keys without decoding `_` to `+`, reducing both comparison fields to the same values; its stable sort then keeps their natural key order.

- The service's first normal row and the SQL normal listing report `+a.1.a`. Dependency lookup emits both; the manager's map key omits Build, so the later row replaces the earlier one and actual installation selects `+a.1.z`.
- Both source manifest versions, packaged records and published tags were checked before installation. The installed callable payload succeeds. The broader prediction that installation would choose the first service row was refuted.
- C:D30 records this narrow listing/order discrepancy against aggregation's latest-first promise and IPM's deliberate alphabetical build comparison. [C:Q9](../../Verus_verification/open_questions.md#repo.oras.integrity.policy) still owns authoritative ordering policy. Decoding aggregation alone is not a checked complete repair because the manager has a separate last-writer rule.
- No general SemVer build-precedence mandate, arbitrary OCI tag order, install failure or business-data corruption is inferred.

- Working assumption: expose the mismatch and retain size/digest checking. Alternatives are sorting before selection or requiring a registry ordering contract, plus rejecting ambiguous archives or selecting by declared digest.

## Reusing a publishing connection

<a id="repo.publish-cache"></a>
[D · repo.publish-cache]

As built, a retained Remote publishing manager reuses an available client without rechecking Enabled or availability; it also keeps unavailable names suppressed for that object's lifetime. The ORAS publishing manager rereads Enabled and probes on each call.

- In paired native calls using the same retained managers, disabling a repository after success still yielded an available Remote client, while ORAS yielded unavailable. This does not promise a cache lifetime across separate shell commands.
- An available service may report that deletion is disabled while the manager returns a successful status. Capability, availability and operation status are different results.

## Metadata

### repo.filesystem

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Repositories](../concepts/repositories.g5.md#repo).

- Children: [Should a module with an invalid name in a filesystem repository's folder block the whole repository?](../../Verus_verification/open_questions.md#repo.filesystem.bad-module), [physical duplicates](#repo.filesystem.duplicates).

Sources: `Repo/Filesystem/Definition.cls`; `AGENTS.md` test guidance. Inventory: CAT-23. Observed: the folder scan is cached in `^IPM.Repo.Filesystem.CacheD`. Consolidated round 4 (`20261114T033125Z-r4-owners`, ARM64) and model `Verus_verification/consolidated/models/owners.py` (`BuildCache`, `AddCacheItem`, `ScanDirectory`, lock manager): an unreadable `module.xml` is not cached (LK1); a second process holding the repository's lock makes a refresh fail with ERROR #5803 after the timeout (LK2); a refresh drops a removed module (LK3); the lock is released afterwards at transaction level zero (LK4). The inherited LK5 result was interpreted as holding the default refresh lock through an outer transaction. Iter 2's actual constructor and successful refresh pairs contradict that interpretation; the old hand-model theorem remains true only of its incorrect abstraction. The first LK5 terminal `TSTART` attempt was already excluded. [Corrected binding and withdrawal](../../uplift/consolidated/iter2/boundaries/withdrawals.json). Hidden `.modules` folders are scanned: the same tree fails with `Depth=0` and builds with `Depth=1` (DC0, DC1). Round 1: an invalid name fails the cache build and its save is rolled back (TR7).

Extension Verus: [owners.rs::build_cache](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::lock_class_id](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::scan](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::unlock_all](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::unlock_class_id](../../Verus_verification/consolidated/verus/owners.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Open questions: [Should a module with an invalid name in a filesystem repository's folder block the whole repository?](../../Verus_verification/open_questions.md#repo.filesystem.bad-module), [Does cache replacement need its explicit lock until the caller's transaction ends?](../../Verus_verification/open_questions.md#repo.filesystem.concurrent-refresh)

### repo.filesystem.duplicates

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Filesystem repository](#repo.filesystem).

[First-admission and directory-reader contract](../../Verus_verification/consolidated/iter19/duplicates.rs) derives the logical key from physical entries, retains manifest/directory together, and follows the selected directory's actual code and description. [Iteration19](../../uplift/consolidated/iter19/HANDOFF.md) qualifies constructor order, sampled scan input, cache/service readers and actual install/show/list results. Marker fields are a manifest projection, not a formal proof of XML bytes. Indexed-row consistency and stable provider/filesystem premises are explicit; old cache and lock correspondence qualifications remain.

State machines: [duplicates · iter19](../../Verus_verification/models/iter19-duplicates/manifest.json)

### repo.filesystem.mutable-source

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [filesystem repositories](#repo.filesystem).

[Cache versus current-directory contract](../../Verus_verification/consolidated/iter21/temporal.rs) keeps the cached snapshot separate from physical manifest/code and follows successful installation to current metadata, callable payload and search/show readers. [Iteration21](../../uplift/consolidated/iter21/HANDOFF.md) records both native runs, source/provider scope, mismatch, refusal neighbor and C:D32 disposition. This extends the assessed temporal domain without changing iteration19's stable-source premise. Concurrent filesystem mutation and generic lifecycle guarantees remain unassessed.

State machines: [temporal · iter21](../../Verus_verification/models/iter21-temporal/manifest.json)

### repo.oras

- uncertain
- human-provided
- user-facing-behavior
- test-coverage
- Expands: [Repositories](../concepts/repositories.g5.md#repo).

Sources: CHANGELOG 0.10.0 (`#541`, `#754`), 0.10.7 (`#1130`), 0.10.10 (`#1267`), 0.10.11 (`#1115`, `#1269`, `#1281`); `Repo/Oras/Definition.cls:27-28`. Inventory: CAT-22, A1 seed 8. Test: `Oras.cls`.

Extension Verus: [thin_surfaces.rs::oras_available](../../Verus_verification/consolidated/verus/thin_surfaces.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Open questions: [Should changing an ORAS registry address clear its stored credentials?](../../Verus_verification/open_questions.md#repo.oras.credentials-change-policy), [Which platform tags should unpublish remove when only a version is named?](../../Verus_verification/open_questions.md#repo.oras.unpublish-platform-policy)

### repo.oras.integrity

- design-verified

- uncertain
- agent
- user-facing-behavior, security
- Expands: [ORAS](#repo.oras).
- Children: [Should an untagged pull always select the highest semantic version, and must a package have exactly one archive layer?](../../Verus_verification/open_questions.md#repo.oras.integrity.policy).

Sources: `Repo/Oras/PackageService.cls::GetLatestTag`, `::GetAllTags`, `::Pull`, `::VerifyModuleHashAndSize`, `::GetModule`; comments and pinned history. [Scoped state/contract and assertions](../../Verus_verification/consolidated/iter2/registry.rs); [actual provider observations](../../uplift/consolidated/iter2/native/receipt.json). These are source-backed observations and a suspected provider-dependent defect, not a registry prevalence claim.

Open questions: [Should an untagged pull always select the highest semantic version, and must a package have exactly one archive layer?](../../Verus_verification/open_questions.md#repo.oras.integrity.policy)

State machines: [registry · iter2](../../Verus_verification/models/iter2-registry/manifest.json)

### repo.oras.archive-boundary

- agent
- user-facing-behavior, security
- Expands: [ORAS integrity](#repo.oras.integrity).

[As-built model and receipts](../../uplift/consolidated/iter68/HANDOFF.md) trace the bundled provider, IPM's status gate, installed files, history and a live dependency consumer. [Pinned source bindings](../../uplift/consolidated/iter68/exact-bindings.json) include the installed wheel implementation.

State machines: [ORAS archive boundary](../../Verus_verification/models/iter68-as-built/manifest.json)

### repo.oras.signed-admission

- uncertain
- human-provided
- user-facing-behavior, security
- Expands: [ORAS archive boundary](#repo.oras.archive-boundary).

Authority: [issue #281](https://github.com/intersystems/ipm/issues/281) and its maintainer comments. The metadata, digest and policy-revision rules are the proposed [experiment contract](../../uplift/consolidated/iter68/HANDOFF.md). No product adapter or verifier is implemented by this model.

State machines: [Proposed signed admission](../../Verus_verification/models/iter68-admission/manifest.json)

### repo.oras.tag-decode

- design-verified

- agent; source-inspected; native-tested; Verus-scoped
- Expands: [ORAS](#repo.oras).

[Codec model](../../Verus_verification/consolidated/iter15/oras.rs): `replace`, `encode`, `decode`, `parse`, `tag_for`, `codec_roundtrip`, `raw_neighbors`. Sources: Common.inc macros Semver2Tag/Tag2Semver/OrasTagPlatformSeparator and Oras.PackageService::TagForReference. No direct body proof or generic invalid-tag parser claim.

State machines: [oras · iter15](../../Verus_verification/models/iter15-oras/manifest.json)

### repo.oras.platform-selection

- design-verified

- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [repo](../concepts/repositories.g5.md#repo).

Sources and readers: `Storage/ModuleInfo.cls::{IsCompatibleWithPlatform,FormatPlatformVersions}` → `Repo/Manager.cls::SearchRepositoriesForModule`, `Utils/Module.cls::GetModuleListExecute` and `Repo/Oras/PackageService.cls::TagForReference`; aggregation/ordering at `AggregatePlatformVersions` and `PreferCompatibleVersions` remain source-backed. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/catalog.rs): `compatible`, `catalog_visible`, `install_candidate`, `reported_platform`, `tag_for`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

State machines: [catalog · iter8](../../Verus_verification/models/iter8-catalog/manifest.json)

### repo.oras.selection-modes

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Platform selection](#repo.oras.platform-selection).

[Producer/reader model](../../Verus_verification/consolidated/iter15/oras.rs): `get_all`, `aggregate`, `prefer`, `service`, `manager`, `display`, `normal_consumer`. Sources: Oras.PackageService::{GetAllTags,AggregatePlatformVersions,PreferCompatibleVersions,AddModulesFromTags,ListModules}, EmbeddedPython::SortOCITags, Repo.Manager::SearchRepositoriesForModule, Utils.Module::GetModuleListExecute. The two actual consumer fixtures and callable dependency payload check final effects; normal-load composition retains inherited successful-pull/compile and rollback premises. Metadata's first-platform witness is conditional input-order behavior, not a registry-order guarantee. [Evidence](../../uplift/consolidated/iter15/HANDOFF.md).

State machines: [oras · iter15](../../Verus_verification/models/iter15-oras/manifest.json)

### repo.oras.build-readers

- design-verified

- uncertain
- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [ORAS selection](#repo.oras.selection-modes).

[Two-literal projection](../../Verus_verification/consolidated/iter15/oras.rs): `deep_fields`, `deep_aggregate`, `deep_oci`, `deep_service_first_version`, `same_base_insert`, `deep_manager_version`, `deep_pull_tag`. Sources: SemanticVersion::FromString dot/plus branches, EmbeddedPython::{CompareVersionTotalOrder,SortVersions,SortOCITags}, PackageService aggregate/first-row decision, Manager's Build-omitting key and SQL display reader. Numeric casts are not generalized; equality follows identical raw core fields for these literals and is natively calibrated. The release manager projection assumes distinct base keys; the deep same-base overwrite is separate. [Review](../../uplift/consolidated/iter15/model-review.md).

Open questions: [Should an untagged pull always select the highest semantic version, and must a package have exactly one archive layer?](../../Verus_verification/open_questions.md#repo.oras.integrity.policy)

State machines: [oras · iter15](../../Verus_verification/models/iter15-oras/manifest.json)

### repo.publish-cache

- design-verified

- agent
- user-facing-behavior
- Expands: [Repository management](../operations/repositories.g5.md#repo.manage).

Sources: `Repo/Remote/PublishManager.cls`, `Repo/Oras/PublishManager.cls`, `General/Singleton.cls`. [State and capability projections](../../Verus_verification/consolidated/iter2/registry.rs); [paired native manager observations](../../uplift/consolidated/iter2/native/receipt.json). Probe transport, manager lifetime and competing configuration writers remain explicit premises.

State machines: [registry · iter2](../../Verus_verification/models/iter2-registry/manifest.json)
