# ORAS raw-tag, platform and reader model lane

The frozen predictions are in the external iteration15 `selection-pre-source.json`. This continues iteration8's recorded raw-tag/platform/dependency gap; it is not source-blind novelty. The old parsed Artifact contract remains unchanged. No product, canonical ontology, shared harness or native runner was edited by this model lane.

## Modeled producer and consumers

`oras.rs` starts with raw tag character sequences. `encode`, `decode`, `parse` and `tag_for` represent the actual '+'/'_' codec, first '__' version/platform split and deployed-tag requirement. `codec_roundtrip` is an induction over underscore-free input, and the native codec literal is exactly `1.2.3-alpha+build.7`. This is a codec proof, not a proof of generic SemVer validity.

`get_all` removes exact duplicate tag names as SortOCITags' dictionary does, then sorts decoded version keys. `aggregate` groups the encoded version names, appends platforms in input order and orders version groups. `prefer` partitions compatible-or-source versions before incompatible versions without changing order within a partition. The declared release comparator domain is exactly 1.0.0 and 2.0.0; raw CSV join/split, generic SemVer and platform parsing are explicit provider boundaries.

The tri-state and its readers are separate:

- Normal (AllVersions=0): compatible-first preference, return one matching version, report its platforms.
- AllTags (AllVersions=1): compatible-first preference, return one matching version, report raw all-tag tokens but omit its PlatformVersions.
- Dependencies (AllVersions empty): enumerate every matching version in descending version order without compatible-first rearrangement.

`manager` filters compatibility and records the current selected platform before a tag is built. This sequence form is scoped to the distinct base versions in the release case. `display` separately implements the SQL list reader's show-all bypass. Consequently Manager(AllTags) can return zero while SQL show-all reports one row. That API distinction is qualified behavior, not a newly invented defect requirement.

`normal_consumer` composes the raw producer with the inherited normal fresh-load boundary: one declared dependency, no existing rows, no other failure, valid metadata/payload providers and successful compilation. Range >=1 selects current-platform v1 and its actual payload. The ^2-only new consumer has no compatible candidate and retains neither row. Payload values are provider projections checked by the parent's actual callable code; no ObjectScript body proof is claimed. Error wording and setup/status observations are native qualification.

For the later same-version case, current `multi()` uses the FULL catalog `triple()` = v1/current, v2/bad, v1/bad and Normal mode, matching the actual native producer. `same()` and `same_current_first()` are separate raw input-order witnesses for metadata-first behavior. They are not assertions about arbitrary OCI response order. Metadata JSON and artifact bodies remain ports; the sampled deployed metadata is true. Generic inconsistent deployment flags, absent metadata, source artifacts and every nonrelease tag form are not claimed closed.

## Deep build-label boundary and retained misses

The original suspicion that any build label necessarily fails Aggregate was not supported: shallow a.9/z.1 tags sorted consistently. The parent's deeper a.1.a/a.1.z calibration exposed a real boundary. Aggregate compares encoded keys directly through FromString; it does not decode them first. For these two raw keys FromString's fourth-dot-piece fallback yields identical fields, both rendered as `1.0.0_a+1`. Encoded field marshalling is not proved; identical source tokens plus the native field-equality gate qualify this literal pair.

`deep_fields`, `pair_order` and `deep_build_neighbors` derive the same-core field comparison. The aggregate's stable sort retains natural encoded-key A,Z order; decoded SortOCITags orders Z,A. Unknown core comparisons are explicit in PairOrder rather than an invented order. No generic integer cast or invalid-version parser claim follows.

The first attempted full producer based only on module-version/package statuses did not publish the intended deep artifact identities. The parent retained that diagnostic and switched to actual scratch source manifests, verifying source versions, packaged versions and both raw registry tags. The initial install-A hypothesis was then refuted: real install chose Z. These misses are retained, not repaired by changing expected observations.

The actual final readers differ:

| Reader | Narrow two-literal result |
|---|---|
| Service normal / SQL normal display | A, the first aggregate group |
| Dependency service | A then Z |
| Manager | Z, because its same-base map key omits Build and the later row overwrites |
| Real install | Z, matching Manager; not an install-misselection finding |

`same_base_insert` models that specific overwrite separately from the distinct-release manager sequence. `deep_service_first_version`, `deep_dependency_versions`, `deep_manager_version` and `deep_pull_tag` expose the reader results. The same platform and major<2 eligibility are explicit premises for this pair. Parent native results confirm actual service/display, manager and installed full VersionString; body/publish/compile success remains native/provider qualification.

Aggregate's own documentation promises latest-first, and CompareVersionTotalOrder deliberately adds alphabetical Build ordering. This supports describing a Low ordering/listing inconsistency if the canonical owner retains that recovered requirement. The authoritative build/latest policy remains C:Q9; do not turn it into a new decided policy or claim install corruption. A decode-only Aggregate change is not a checked complete repair: reversing the dependency rows could make the manager's last-writer collision select A. The owning manager key and all readers must be considered together.

## Exact source roles

| Formal surface | Production source role |
|---|---|
| encode/decode/tag_for | Common.inc Semver2Tag/Tag2Semver/OrasTagPlatformSeparator; Oras.PackageService::TagForReference and PublishService::PublishModule |
| parse, get_all | Oras.PackageService::GetAllTags; EmbeddedPython::SortOCITags |
| aggregate, prefer | Oras.PackageService::{AggregatePlatformVersions,PreferCompatibleVersions}; EmbeddedPython::{SortVersions,CompareVersionTotalOrder} |
| service, produce, metadata_tag | Oras.PackageService::{AddModulesFromTags,ListModulesFromTagString,ListModules}; artifact metadata JSON is a port |
| compatible, manager | Storage.ModuleInfo::IsCompatibleWithPlatform; Repo.Manager::SearchRepositoriesForModule |
| display | Utils.Module::GetModuleListExecute, its actual SQL query reader |
| tag selection / payload projection | Oras.PackageService::{TagForReference,GetModule}; actual native installed class call |
| fresh consumer composition | Utils.Module::{LoadDependencies,LoadNewModule}; inherited normal-load refusal/rollback contract and parent native store/consumer observations |
| deep_fields | General.SemanticVersion::FromString, literal hyphen-free dot/plus branches |
| same_base_insert | Repo.Manager::SearchRepositoriesForModule versions-map key and overwrite; scope is the same-core/same-repository literal pair |

Locators establish scoped correspondence roles, not whole-member coverage or executable verification. Native setup, source-pinning, real registry publication, raw sessions, field gates and final replay belong to the parent iteration harness.

## Proofs and controls

The final baseline verifies 25 units with zero errors, including termination obligations. Twenty-one semantic controls challenge codec substitution, delimiter choice, pull-tag platform, raw version sorting/deduplication, platform order, metadata-first selection, compatible preference, empty AllVersions enumeration, platform-list omission, manager filtering/selected platform, SQL show-all behavior, pinned range, installed payload, encoded build parsing/order and same-base overwrite. Record the final complete-controls receipt as authoritative; initial release snapshots and failed induction/compute-depth attempts remain preserved under model-lane.

A composite deep proof initially exceeded proof-computation recursion depth. It was replaced by explicit intermediate field/codec facts over the same literal domain, with no assumption or admission. The codec induction similarly gained an explicit replace-cons lemma. Compile failures, timeouts, recursion limits and resource limits are not control rejections. Verus's actual false-assertion signatures include both `evaluates to false` and `expression simplifies to false`.

No assume/admit/external_body was added. Direct ObjectScript implementation proofs: zero. Generic XML/JSON/SemVer, raw invalid tags, unmatched credentials/provider errors, arbitrary OCI tie ordering, arbitrary build sorting, partial installation, preexisting consumer state and concurrent registry mutation remain unassessed or conditional. This is material closure of the selected producer/reader boundary, not repository-wide convergence.
