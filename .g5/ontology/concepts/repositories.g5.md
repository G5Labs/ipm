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

IPM searches enabled repositories for a module, in the order they were created. A `<repo>/<module>` prefix limits the search to one repository.

- A request containing keywords skips filesystem repositories; ordinary name searches can use them. This filter applies even when that filesystem repository is explicitly named.
- Keywords given as text are split on spaces, and empty pieces are dropped. A keyword string of only spaces therefore counts as no keywords and filesystem repositories are searched. The module name is lowercased before any repository is asked.

- IPM takes the highest version that satisfies the request, whichever repository offers it.
- When several repositories offer that version, the one created first wins. A repository's sort position does not change this.
- The selected repository also supplies the [saved supplier name and historical source details](#repo.origin.installed).

<a id="repo.origin.installed"></a>
[D · repo.origin.installed]

A repository install writes its configured repository name into the module record. `show` and `list -repository` read that saved name, even after the repository definition is deleted or reused for a different location.

- In the checked fresh filesystem installs, qualified alpha and beta requests select their respective payloads despite identical module names and versions. Recreating alpha at a new folder changes the next fresh install, while the earlier module and [history source snapshot](../features/history.g5.md#history.fields.source-snapshot) retain their old values.
- A later local `load` clears the module's repository name. `show` omits its Repository field and neither repository-filtered list includes it. Earlier installation history still describes the past installs, not the current payload.
- Repository settings stay stable during each checked command. [Same-version supplier changes](#repo.origin.same-version) have a separate contract; concurrent reconfiguration and general cache lifetimes remain unassessed.
- Installed supplier names differ from project-repository metadata in available-module searches. That separate catalog distinction is source-inspected here, not a newly executed provider case.

<a id="repo.origin.same-version"></a>
[D · repo.origin.same-version]

Reinstalling the same plain `1.0.0` from a different qualified filesystem repository loads that supplier's files again. The saved supplier, root and callable payload change even though the version string stays the same.

- The checked sequence installs alpha, then beta at the same version without uninstalling, then alpha again. Each successful action gets a new source snapshot; earlier alpha and beta history still describe their past suppliers.
- A request for that version from a missing repository fails before loading. The installed beta payload and current supplier stay beta, and earlier successful source snapshots remain available. The installed copy does not satisfy a request for a missing qualifier.
- This is the supported [same-version reload](../features/load.g5.md#load.same-version), not a new product defect. The domain has one valid package per configured root, stable definitions and successful load/compile/save providers. Other versions, provider failure effects and refused-attempt history details remain unassessed. Logical successful-record labels are separate from database IDs, which can include failed attempts.

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

- The ORAS availability adapter accepts a successful HTTP 2xx reply or a 401 reply carrying `WWW-Authenticate`. A headerless 401 and a 403 are unavailable.

- This is an availability probe, not proof that authentication, pulling or publishing will work. Native loopback HTTP cases distinguish all four replies.





<a id="repo.oras.unpublish-platform-policy"></a>
[Q · repo.oras.unpublish-platform-policy]

Which platform tags should unpublish remove when only a version is named?

- Working assumption: ORAS `unpublish R/M V` deletes the plain tag and every `V__<platform>` tag of that version.
- Alternative and implications: Require the platform to be named; the plain form must still not fail as "not found".
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

<a id="repo.oras.credentials-change-policy"></a>
[Q · repo.oras.credentials-change-policy]

Should changing an ORAS registry address clear its stored credentials?

- Working assumption: Changing an ORAS registry's address clears its stored credentials, as for a remote registry.
- Alternative and implications: Keep and warn (operator only renamed the host): F-REPO-11 becomes a missing warning.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

<a id="repo.filesystem"></a>
[I · repo.filesystem]

A filesystem repository offers every module whose `module.xml` IPM finds under its folder, down to a set depth.

- IPM reads the folder into a cache when the repository is created and when it is refreshed. A refresh replaces the whole cache, so a module whose folder is gone stops being offered.
- A `module.xml` that is not a readable manifest is skipped. A readable manifest whose module name is invalid fails the whole refresh, which then changes nothing. Whether one bad module should block the rest is [open](#repo.filesystem.bad-module).
- A refresh of an existing repository locks it. While another process holds that lock, the refresh fails after a timeout and changes nothing.
- The earlier claim that this explicit lock lasts until a caller's transaction ends was incorrect for the actual constructor. The default lock manager releases it when the refresh method returns, even if an outer transaction remains open. Explicit `%New(0)` keeps it through close until commit; default `%New(1)` releases at close.
- The cache still needs consistent replacement. How transaction isolation and other writers protect an uncommitted refresh remains [open](#repo.filesystem.concurrent-refresh); the observed release does not authorize a race.

- Physical duplicates of one logical version follow the [first-admission rule](#repo.filesystem.duplicates). Later [source edits](#repo.filesystem.mutable-source) can make a selected cached version differ from the module actually loaded.

<a id="repo.filesystem.duplicates"></a>
[D · repo.filesystem.duplicates]

Within one filesystem root, the cache identifies a module by name and full version string. When two valid physical directories offer that identity, the first successfully admitted entry stays in the cache. A later duplicate prints a warning naming both directories and the retained one, and does not fail the build.

- First means the scan order processed by that cache build. It is not a promise that directory names have a portable lexical order. The checked constructor traces and separate scan samples agree for the selected stable fixtures.
- The retained manifest and directory belong to the same cache row. The manifest API returns its saved manifest, and the directory API returns its physical folder. With stable files and successful ordinary directory loading, installed description, root and callable payload follow that retained folder.
- Swapping the physical directory-to-payload assignment changes which duplicate is retained in the checked builds. A single valid entry has no duplicate warning. This is an explicit warning policy, not a newly found defect.
- This contract covers valid parsed manifests and fresh successful cache builds. Generic parsing/save failures, source edits after caching, removed directories, refresh races and other delivery branches remain outside the new qualification.

<a id="repo.install.version-request"></a>
[I · repo.install.version-request]

An explicit install version constrains the module that is actually installed, not only the repository entry selected. A successful request for `1.0.0` must not silently install `2.0.0`.

<a id="repo.filesystem.mutable-source"></a>
[D · repo.filesystem.mutable-source]

A filesystem cache saves a manifest but retains a pointer to its physical directory. Ordinary directory installation reads the current `module.xml` and source files from that directory, which may have changed since caching.

- In the checked sequence, the cache, manifest API and search still offer `1.0.0` after the same directory is replaced with a `2.0.0` manifest and different code. An explicit `1.0.0` install succeeds, saves `2.0.0` and its new description, and runs the new payload. This breaks [the requested-version rule](#repo.install.version-request); it is C:D32, not a promise to accept the mismatch.
- Reconfiguring that same repository through the ordinary `repo -fs` command rebuilds its cache. Search then offers `2.0.0`; an explicit `1.0.0` request fails with not found and retains the installed `2.0.0`. An explicit `2.0.0` request succeeds.
- This qualification uses one ordinary directory, unchanged module name, plain releases `1.0.0` and `2.0.0`, successful file replacement and normal load/compile/save. It does not establish behavior during concurrent edits, source removal, parse/provider failure or other delivery branches. The [stable-source duplicate rule](#repo.filesystem.duplicates) retains its original scope.

<a id="repo.filesystem.bad-module"></a>
[Q · repo.filesystem.bad-module]

Should a module with an invalid name in a filesystem repository's folder block the whole repository?

- Provisional answer: no. Skip it with a warning, like an unreadable manifest, so the other modules stay available.
- As built: yes. The same failure stops a load's temporary `.modules` repository from being created, which then fails the load after its commit.
- Alternative: fail every bad manifest, including unreadable ones, so nothing is silently hidden.

<a id="repo.oras.integrity"></a>
[D · repo.oras.integrity]

As built, an ORAS pull without a tag selects the last tag supplied by the registry. The helper does not sort that list, despite its comment promising the highest version.

- A disposable provider returning `[2.0.0,1.0.0]` selects `1.0.0`; reversing the order selects `2.0.0`. The separate all-tags listing sorts versions, so it cannot justify the pull helper's ordering.
- Archive validation skips non-tar layers and compares size and digest with the first tar layer. A later matching tar layer does not rescue an earlier mismatch. The pull path also chooses the first downloaded `*.tgz`; those two choices need not name the same artifact.
- Failed validation returns an error even if the raw helper assigned a stream. The final `GetModule` reader throws that status instead of returning the stream to the loader. Multiple layers/files, provider order and digest integrity remain distinct premises.

<a id="repo.oras.integrity.policy"></a>
[Q · repo.oras.integrity.policy]

Should an untagged pull always select the highest semantic version, and must a package have exactly one archive layer?

- Owner: registry maintainers and IPM maintainers; no owner decision was available. The method comment supports highest-version intent, while source behavior depends on provider order.
- Working assumption: expose the mismatch and retain size/digest checking. Alternatives are sorting before selection or requiring a registry ordering contract, plus rejecting ambiguous archives or selecting by declared digest.
- Proposed repairs are not implemented or proved. Preserve deployed platform tags and compare both a valid single archive and ambiguous multi-layer input before a repair.

<a id="repo.remote.download"></a>
[D · repo.remote.download]

To install from a remote registry, IPM asks the registry for the package's URL and hash, then downloads that URL. It sends the repository's credentials on both requests, even when the URL names a different server.

- The download follows the URL exactly as the registry returns it. This was a deliberate 2022 change from a path relative to the repository. Sending credentials to another server was not discussed; see the [credential question](#repo.remote.download.credentials).
- Artifactory downloads do the opposite and send no credentials on the byte request ([artifact authentication](../features/artifact-download.g5.md#artifact.authentication)).

<a id="repo.remote.download.redirect"></a>
[D · repo.remote.download.redirect]

Registry requests follow HTTP redirects automatically and send the same credentials on each hop. A download URL on the repository's own server that redirects to another server therefore still delivers the Basic credentials and API key there.

- Checking only the URL the registry returns would not stop this. A fix has to decide credentials per hop, or stop following redirects on its own.

<a id="repo.remote.download.integrity"></a>
[D · repo.remote.download.integrity]

When the registry gives a hash, the downloaded file's SHA-1 must equal it exactly. The computed SHA-1 is lowercase, so an uppercase hash from the registry is refused even when it is the same digest.

- A mismatch fails the install with "Download module, hash mismatch." No install token is kept and no install report is sent to the registry.
- An empty hash skips the check. Bytes that are not an archive then pass the download, extract to nothing and fail later with "File not found to obtain module object", after the registry has received an install report.
- A failed metadata request reports its HTTP status. A failed download reports only "tRequest statuscode not 200".

<a id="repo.remote.download.token"></a>
[D · repo.remote.download.token]

The registry may return an install token with the package. IPM keeps it only after the hash check passes, sends it once with the install report and then deletes it.

<a id="repo.remote.download.credentials"></a>
[Q · repo.remote.download.credentials]

Should registry credentials be sent to a download URL on a different server?

- Evidence: commit `d6cc4204` switched to the registry-supplied URL to "fix downloading packages"; a native provider received the repository's Basic credentials and API key at a different host and port. Elsewhere the maintainers treat credentials as belonging to one host: CHANGELOG 0.10.11 `#1115` stops ORAS credential checks from following redirects "so credentials are not sent to another host", and `#649` clears a remote registry's credentials when its URL changes. No source decides the download case.
- Working assumption: keep following the returned URL, and record the forwarding as suspected defect C:D20. The proposed alternative sends credentials only when the URL's server and port match the repository; a third choice is to refuse such URLs. Either change can break registries that redirect downloads to storage needing the same credentials.
- Register: C:Q14 and proposal C:S14, revised as C:S14b after the [redirect challenge](#repo.remote.download.redirect).

<a id="repo.publish-cache"></a>
[D · repo.publish-cache]

As built, a retained Remote publishing manager reuses an available client without rechecking Enabled or availability; it also keeps unavailable names suppressed for that object's lifetime. The ORAS publishing manager rereads Enabled and probes on each call.

- In paired native calls using the same retained managers, disabling a repository after success still yielded an available Remote client, while ORAS yielded unavailable. This does not promise a cache lifetime across separate shell commands.
- An available service may report that deletion is disabled while the manager returns a successful status. Capability, availability and operation status are different results.

<a id="repo.filesystem.concurrent-refresh"></a>
[Q · repo.filesystem.concurrent-refresh]

Does cache replacement need its explicit lock until the caller's transaction ends?

- Owner: IPM maintainers. The default constructor releases at object close; the deferred constructor is a separate supported mechanism.
- Working assumption: keep the intended consistent cache and investigate competing refreshes under actual database isolation. Alternatives are deferred release, refusing an outer transaction or demonstrating that existing database locks suffice.
- No race corruption or repair is established by the observed lock release. [Withdrawal and paired evidence](../../uplift/consolidated/iter2/boundaries/withdrawals.json).

<a id="repo.search.version-list"></a>
[D · repo.search.version-list]

An AllVersions search groups matching versions into one result per module. Its selected version is the newest eligible version, and its AllVersions text lists eligible versions from newest to oldest.

For releases 1.0.0, 2.0.0 and 3.0.0, an unrestricted search selects 3 and lists 3,2,1; a `<3.0.0` search selects 2 and lists 2,1. Filtering happens before the grouped result is formed.

The new proof covers this major-only release domain after range parsing. General semantic-version parsing, prerelease order, repository ties and multi-module grouping retain their separate contracts and gaps.

<a id="repo.oras.platform-selection"></a>
[D · repo.oras.platform-selection]

A deployed module can be selected for installation only when its reported platforms include the current IRIS platform. Normal repository listings apply the same test and show that platform for a compatible artifact; a nondeployed source package is not platform-specific.

- A module available only for another platform is hidden in the normal listing and refused at install. The all-versions listing may still show incompatible artifacts.
- The selected current platform is carried into the deployed artifact tag. A deployed reference with no selected platform is refused before download.
- [Raw tag decoding and selection modes](#repo.oras.selection-modes) are now checked for the pinned release/platform cases; general metadata transport and semantic-version parsing remain provider boundaries.

<a id="repo.oras.tag-decode"></a>
[D · repo.oras.tag-decode]

ORAS tags encode a version's `+` as `_` and append `__PLATFORM` for a deployed artifact. Splitting the platform suffix and decoding the version restores the checked prerelease/build string. A deployed reference with no selected platform is refused before download.

The model proves encode/decode roundtrip for version text without underscores and checks delimiter handling for the native literal. This is a tag codec rule, not proof that every encoded version is sorted correctly or that registry JSON/CSV transport is correct.

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
- C:D30 records this narrow listing/order discrepancy against aggregation's latest-first promise and IPM's deliberate alphabetical build comparison. [C:Q9](#repo.oras.integrity.policy) still owns authoritative ordering policy. Decoding aggregation alone is not a checked complete repair because the manager has a separate last-writer rule.
- No general SemVer build-precedence mandate, arbitrary OCI tag order, install failure or business-data corruption is inferred.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md) and [A2](../../uplift/ipm-verus-20261113/A/operation-inventory.md). Repository search is used by the formal scope through [install](../features/install.g5.md#install) and [dependencies](../features/dependencies.g5.md#deps). The repository internals themselves are outside it.

### repo

- human-provided
- user-facing-behavior
- Children: [manage](#repo.manage), [search order](#repo.search-order), [read-only](#repo.read-only), [prereleases](#repo.prereleases), [default registry](#repo.default-registry), [ORAS](#repo.oras), [filesystem](#repo.filesystem), [remote download](#repo.remote.download).

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

Sources: CHANGELOG 0.9.0 (`#261`); CHANGELOG 0.9.x "Repo check should happen in the order to repo creation, not by repo name" (HSIEO-9269, HSIEO-9404); `Main.cls:413-414` (`install registry/HS.JSON`). Inventory: CAT-18. Code: `Repo/Manager.cls:67-70` (SQL order by sort position), `:150` (results keyed by version, then repository ID), `:153-187` (highest version first). Consolidated round 1 native cases TR5, TR5b, TR5c (`20261114T025102Z-r1-temp-repo`) and model `Verus_verification/consolidated/models/temp_repo.py` (`search`; wrong variants `search-tie-by-sortorder`, `search-first-repo`, `search-lowest-version` rejected). The earlier observation that a load's `.modules` folder is searched before all others holds only for versions no older repository offers ([precedence](../features/load.g5.md#load.dependencies.local-repo.precedence)).

Extension Verus: [temp_repo.rs::select](../../Verus_verification/consolidated/verus/temp_repo.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Iteration 3 search repair: `Repo/Manager.cls::SearchRepositoriesForModule` and `Repo/SearchCriteria.cls`; Verus `public_repairs.rs::repo_visited` and `search_neighbors`. Native same-repository search finds the seeded module by name and returns no result after adding a keyword. This changes no prior version/tie rule; command-local rejected-credential suppression remains a separate gate.

Iteration 4 token producer: `Repo/SearchCriteria.cls::KeywordsSet` (text split on single spaces, `$list` input not split, empty elements dropped from both; a list object assigned as given) and the lowercased criteria name in `SearchRepositoriesForModule`. Verus [repairs4.rs](../../Verus_verification/consolidated/iter4/repairs4.rs) `{keywords,visits_filesystem,criteria_name,keyword_neighbors}`; controls `empty-tokens-kept`, `dollar-list-not-filtered`, `object-filtered`, `keywords-ignored-at-gate` rejected. The token rule is source-only; no new native case.

Findings: [P:F-INST-4 — e2e reproduced](../../../playbook-uplift/BUGS.md#p-f-inst-4). The linked entry qualifies the caller, reader and providers.

### repo.origin.installed

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Repositories](#repo).
- Children: [same-version supplier changes](#repo.origin.same-version).

[Supplier and reader contract](../../Verus_verification/consolidated/iter16/provenance.rs): `select`, `apply_install`, `install`, `local_load`, `show_repository`, `listed_for_repository`, `history_detail`. The model derives source identity from a configured supplier and its selected fixture package. Fresh installs require actual module/code absence; `successful_uninstall` is an observed-success projection, not general removal or compensation. Existing-installed behavior is unassessed by this fresh-install contract and covered separately below. [Correspondence and runbook](../../uplift/consolidated/iter16/HANDOFF.md).

### repo.origin.same-version

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Installed supplier](#repo.origin.installed).

[Equal-version source and reader contract](../../Verus_verification/consolidated/iter18/reloading.rs) extends the unchanged fresh-install model with the plain equal-version admission and selected load writers. [Iteration18](../../uplift/consolidated/iter18/HANDOFF.md) records actual callable bytes, stored metadata, unique permanent successful records and rendered current/history readers. Rejected-attempt history and generic provider mechanics are outside the new guarantee; earlier proof and native receipts remain unchanged.

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

Extension Verus: [thin_surfaces.rs::oras_available](../../Verus_verification/consolidated/verus/thin_surfaces.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### repo.oras.unpublish-platform-policy

- agent
- uncertain
- user-facing-behavior
- Expands: [repo.oras](#repo.oras)

Sources: [DEC-REPO-5](../../../playbook-uplift/decisions/05-06.md); Today a deployed version cannot be unpublished by version at all, which no source intends.

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.oras.unpublish-platform-policy). The recorded assumption is provisional; no new maintainer approval was found.

### repo.oras.credentials-change-policy

- agent
- uncertain
- user-facing-behavior
- Expands: [repo.oras](#repo.oras)

Sources: [DEC-REPO-3](../../../playbook-uplift/decisions/05-06.md); Standard practice (credentials belong to a host); the remote registry already does it (`Remote/Definition.cls#L81-L86`).

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.oras.credentials-change-policy). The recorded assumption is provisional; no new maintainer approval was found.

### repo.filesystem

- human-provided
- user-facing-behavior
- Expands: [Repositories](#repo).

- Children: [bad module](#repo.filesystem.bad-module), [physical duplicates](#repo.filesystem.duplicates).

Sources: `Repo/Filesystem/Definition.cls`; `AGENTS.md` test guidance. Inventory: CAT-23. Observed: the folder scan is cached in `^IPM.Repo.Filesystem.CacheD`. Consolidated round 4 (`20261114T033125Z-r4-owners`, ARM64) and model `Verus_verification/consolidated/models/owners.py` (`BuildCache`, `AddCacheItem`, `ScanDirectory`, lock manager): an unreadable `module.xml` is not cached (LK1); a second process holding the repository's lock makes a refresh fail with ERROR #5803 after the timeout (LK2); a refresh drops a removed module (LK3); the lock is released afterwards at transaction level zero (LK4). The inherited LK5 result was interpreted as holding the default refresh lock through an outer transaction. Iter 2's actual constructor and successful refresh pairs contradict that interpretation; the old hand-model theorem remains true only of its incorrect abstraction. The first LK5 terminal `TSTART` attempt was already excluded. [Corrected binding and withdrawal](../../uplift/consolidated/iter2/boundaries/withdrawals.json). Hidden `.modules` folders are scanned: the same tree fails with `Depth=0` and builds with `Depth=1` (DC0, DC1). Round 1: an invalid name fails the cache build and its save is rolled back (TR7).

Extension Verus: [owners.rs::build_cache](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::lock_class_id](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::scan](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::unlock_all](../../Verus_verification/consolidated/verus/owners.rs), [owners.rs::unlock_class_id](../../Verus_verification/consolidated/verus/owners.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### repo.filesystem.duplicates

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Filesystem repository](#repo.filesystem).

[First-admission and directory-reader contract](../../Verus_verification/consolidated/iter19/duplicates.rs) derives the logical key from physical entries, retains manifest/directory together, and follows the selected directory's actual code and description. [Iteration19](../../uplift/consolidated/iter19/HANDOFF.md) qualifies constructor order, sampled scan input, cache/service readers and actual install/show/list results. Marker fields are a manifest projection, not a formal proof of XML bytes. Indexed-row consistency and stable provider/filesystem premises are explicit; old cache and lock correspondence qualifications remain.

### repo.install.version-request

- agent; inferred-from-product-help; user-facing-behavior
- Expands: [repository selection](#repo.search-order).

The product-authored install help defines the version argument as the version or version expression of the module to install; the pinned-install comment says selection filters its requested version. This recovers the existing user-facing obligation, with no new owner decision. [Temporal model](../../Verus_verification/consolidated/iter21/temporal.rs) distinguishes the exact checked release requests from the actual loaded module. Generic version/range conformance retains its separate gaps.

Findings: [C:D32 — e2e reproduced](../../uplift/consolidated/BUGS.md#c-d32). The linked entry qualifies the caller, reader and providers.

### repo.filesystem.mutable-source

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [filesystem repositories](#repo.filesystem).

[Cache versus current-directory contract](../../Verus_verification/consolidated/iter21/temporal.rs) keeps the cached snapshot separate from physical manifest/code and follows successful installation to current metadata, callable payload and search/show readers. [Iteration21](../../uplift/consolidated/iter21/HANDOFF.md) records both native runs, source/provider scope, mismatch, refusal neighbor and C:D32 disposition. This extends the assessed temporal domain without changing iteration19's stable-source premise. Concurrent filesystem mutation and generic lifecycle guarantees remain unassessed.

### repo.filesystem.bad-module

- agent
- uncertain
- user-facing-behavior
- Expands: [Filesystem repository](#repo.filesystem).

Sources: `Repo/Filesystem/Definition.cls:184-241` (unreadable manifests skipped at `:196-200`; a save error returned at `:234-237` and aggregated at `:170-176`). Native LK1 (skipped), TR7 and Dtxn (blocked). G's attempt 1 for case C29 also found that an unparseable manifest is skipped (`native/make_replay.py:248`). Question C:Q5.

Extension Verus: [owners.rs::add_cache](../../Verus_verification/consolidated/verus/owners.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.filesystem.bad-module). The recorded assumption is provisional; no new maintainer approval was found.

### repo.oras.integrity

- agent
- user-facing-behavior, security
- Expands: [ORAS](#repo.oras).
- Children: [selection and layer policy](#repo.oras.integrity.policy).

Sources: `Repo/Oras/PackageService.cls::GetLatestTag`, `::GetAllTags`, `::Pull`, `::VerifyModuleHashAndSize`, `::GetModule`; comments and pinned history. [Scoped state/contract and assertions](../../Verus_verification/consolidated/iter2/registry.rs); [actual provider observations](../../uplift/consolidated/iter2/native/receipt.json). These are source-backed observations and a suspected provider-dependent defect, not a registry prevalence claim.

### repo.oras.integrity.policy

- agent
- uncertain
- Expands: [ORAS integrity](#repo.oras.integrity).

No authorized policy decision. The parent states source authority, alternatives, preservation conditions and remaining premises.

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.oras.integrity.policy). The recorded assumption is provisional; no new maintainer approval was found.

### repo.remote.download

- agent
- user-facing-behavior, security
- Expands: [Repositories](#repo).
- Children: [redirects](#repo.remote.download.redirect), [integrity](#repo.remote.download.integrity), [install token](#repo.remote.download.token), [credential question](#repo.remote.download.credentials).

Direct decision: `Repo/Remote/PackageService.cls::GetModule`; request builder `Repo/Http/PackageService.cls::GetHttpRequest`. Caller: `Utils/Module.cls` remote branch of the module-reference loader (`tClient.GetModule`, then `LoadModuleFromArchive`, then `CollectAnalytics`). History: commit `d6cc4204` (2022-07-07). The parsed `path` variable and its comment are dead code. Verus [registry.rs](../../Verus_verification/consolidated/iter4/registry.rs) `{metadata_request,bytes_request,repaired_bytes_request,lemma_cross_origin_carries_credentials}`; controls `bytes-sent-to-configured-origin`, `bytes-drop-credentials-as-built` and `repair-still-forwards` rejected. Native R-cross: a two-origin provider recorded Basic and `apikey` headers on `127.0.0.1:18502` after metadata from `localhost:18501`. Redirect handling (`FollowRedirect=1`), TLS and production registries are outside this observation.

### repo.remote.download.redirect

- agent
- user-facing-behavior, security
- Expands: [Remote download](#repo.remote.download).

Direct decision: `Repo/Http/PackageService.cls::GetHttpRequest` (`FollowRedirect = 1`, `apikey` header and `Username`/`Password` set once on the request). Caller: `Repo/Remote/PackageService.cls::GetModule`. Iteration 5 challenge of iteration 4's request boundary, which had left redirects open. Native X1: metadata named `localhost:18501/redirect/cross`, which answered 302 to `127.0.0.1:18502`; the provider recorded Basic and `apikey` on all three requests. Verus [redirect.rs](../../Verus_verification/consolidated/iter5/redirect.rs) `{as_built_sends,s14_sends,s14b_sends,lemma_s14_insufficient,lemma_s14b_never_leaks}`; controls `redirect-drops-credentials-as-built`, `s14-checks-every-hop`, `s14b-checks-first-hop` rejected. Redirects on metadata and listing requests share the same builder but were not exercised. CHANGELOG 0.10.11 `#1115` fixed the same redirect leak for ORAS token checks.

### repo.remote.download.integrity

- agent
- user-facing-behavior, security
- Expands: [Remote download](#repo.remote.download).

Direct decision: `Repo/Remote/PackageService.cls::GetModule` lines 130-151. Readers: `Utils/Module.cls` (download error escapes before analytics; accepted bytes always treated as an archive), `General/Archive.cls::Extract` (C:D18). Verus `{integrity_ok,get_module,install,registry_neighbors}`; controls `hash-compared-case-insensitively`, `empty-hash-rejected`, `mismatch-sends-analytics`, `junk-refused-at-download` and `bytes-failure-reports-status` rejected. Native R-mismatch, R-upper, R-nohash, R-dl404, R-meta404, and full `install` I1 (installed), I2 (hash mismatch, no report), I3 (empty hash with non-archive bytes, manifest error, report sent). The leftover temporary file after a mismatch was not checked.

### repo.remote.download.token

- agent
- Expands: [Remote download](#repo.remote.download).

Writer: `Repo/Remote/PackageService.cls::GetModule` (`^IRIS.TempZPMRInst`). Reader: `::SendAnalyticsToRegistry`. Verus `token_stored`; control `token-stored-before-integrity` rejected. Native R-same/R-cross store, R-mismatch/R-upper do not, R-analytics sends `tok-same` once and the entry is gone afterwards.

### repo.remote.download.credentials

- agent
- uncertain
- Expands: [Remote download](#repo.remote.download).

No authorized policy decision. Register entries C:Q14 and C:S14; the proposed request is proved only at the abstract request boundary and was not applied.

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.remote.download.credentials). The recorded assumption is provisional; no new maintainer approval was found.

### repo.publish-cache

- agent
- user-facing-behavior
- Expands: [Repository management](#repo.manage).

Sources: `Repo/Remote/PublishManager.cls`, `Repo/Oras/PublishManager.cls`, `General/Singleton.cls`. [State and capability projections](../../Verus_verification/consolidated/iter2/registry.rs); [paired native manager observations](../../uplift/consolidated/iter2/native/receipt.json). Probe transport, manager lifetime and competing configuration writers remain explicit premises.

### repo.filesystem.concurrent-refresh

- agent
- uncertain
- Expands: [Filesystem cache](#repo.filesystem).

Source owner and constructor binding: [boundary ledger](../../uplift/consolidated/iter2/boundaries/bindings.json). The prior lock correspondence is withdrawn; other cache properties retain their evidence.

Findings audit: [sources, alternatives, implications and settlement gate](../../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.filesystem.concurrent-refresh). The recorded assumption is provisional; no new maintainer approval was found.

### repo.search.version-list

- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [repo](#repo).

Sources and readers: `Repo/Filesystem/PackageService.cls::ListModules` → `Repo/Utils.cls::moduleSqlToList` → `Repo/Manager.cls::SearchRepositoriesForModule` → `Repo/Utils.cls::SearchRepositoriesForModule`. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/catalog.rs): `eligible`, `ordered`, `grouped`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

### repo.oras.platform-selection

- agent; user-facing-behavior; native-tested; source-inspected; Verus-scoped
- Expands: [repo](#repo).

Sources and readers: `Storage/ModuleInfo.cls::{IsCompatibleWithPlatform,FormatPlatformVersions}` → `Repo/Manager.cls::SearchRepositoriesForModule`, `Utils/Module.cls::GetModuleListExecute` and `Repo/Oras/PackageService.cls::TagForReference`; aggregation/ordering at `AggregatePlatformVersions` and `PreferCompatibleVersions` remain source-backed. [Iteration 8 index](../../uplift/consolidated/iter8/INDEX.md). [Contract](../../Verus_verification/consolidated/iter8/catalog.rs): `compatible`, `catalog_visible`, `install_candidate`, `reported_platform`, `tag_for`. The [trust inventory](../../Verus_verification/consolidated/iter8/TRUST.md) names provider and input boundaries; direct ObjectScript implementation proofs: 0.

### repo.oras.tag-decode

- agent; source-inspected; native-tested; Verus-scoped
- Expands: [ORAS](#repo.oras).

[Codec model](../../Verus_verification/consolidated/iter15/oras.rs): `replace`, `encode`, `decode`, `parse`, `tag_for`, `codec_roundtrip`, `raw_neighbors`. Sources: Common.inc macros Semver2Tag/Tag2Semver/OrasTagPlatformSeparator and Oras.PackageService::TagForReference. No direct body proof or generic invalid-tag parser claim.

### repo.oras.selection-modes

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Platform selection](#repo.oras.platform-selection).

[Producer/reader model](../../Verus_verification/consolidated/iter15/oras.rs): `get_all`, `aggregate`, `prefer`, `service`, `manager`, `display`, `normal_consumer`. Sources: Oras.PackageService::{GetAllTags,AggregatePlatformVersions,PreferCompatibleVersions,AddModulesFromTags,ListModules}, EmbeddedPython::SortOCITags, Repo.Manager::SearchRepositoriesForModule, Utils.Module::GetModuleListExecute. The two actual consumer fixtures and callable dependency payload check final effects; normal-load composition retains inherited successful-pull/compile and rollback premises. Metadata's first-platform witness is conditional input-order behavior, not a registry-order guarantee. [Evidence](../../uplift/consolidated/iter15/HANDOFF.md).

### repo.oras.build-readers

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [ORAS selection](#repo.oras.selection-modes).

[Two-literal projection](../../Verus_verification/consolidated/iter15/oras.rs): `deep_fields`, `deep_aggregate`, `deep_oci`, `deep_service_first_version`, `same_base_insert`, `deep_manager_version`, `deep_pull_tag`. Sources: SemanticVersion::FromString dot/plus branches, EmbeddedPython::{CompareVersionTotalOrder,SortVersions,SortOCITags}, PackageService aggregate/first-row decision, Manager's Build-omitting key and SQL display reader. Numeric casts are not generalized; equality follows identical raw core fields for these literals and is natively calibrated. The release manager projection assumes distinct base keys; the deep same-base overwrite is separate. [Review](../../uplift/consolidated/iter15/model-review.md).
