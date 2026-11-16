# Versions

IPM compares module versions to pick what to install and to refuse downgrades.

## Definitions

- **Snapshot**: a version with the build part `+snapshot`, meaning the latest build of that version.
- **Prerelease**: a version with a prerelease part, such as `1.0.0-beta.1`.

## Statements

<a id="version"></a>
[I · version]

Module versions are semantic versions: major, minor and patch numbers, with optional prerelease and build parts.


<a id="version.order"></a>
[I · version.order]

A higher major, minor or patch number makes a later version. A prerelease comes before its release, and a `+snapshot` build comes before the same release. Prereleases of two different versions are not ordered against each other.

- Prerelease parts compare one dot-separated identifier at a time. Numbers compare as numbers and come before words, and words compare by character code.
- Build metadata may contain hyphens. It never turns a version into a prerelease.


<a id="version.range"></a>
[I · version.range]

Dependencies, `install` and `update` accept an exact version or a range, such as `1.x` or `>=1.2.0`. When IPM picks the highest version in a range, it skips prereleases and snapshots.

- Working assumption: keep the special case for IRIS-style versions, because IPM's tests require `2022.2` to match only `2022.2`.


<a id="version.range.partial"></a>
[I · version.range.partial]

A comparison with a partial version fills the missing parts with zero. So `>=1.2` allows `1.3.0` and `2.0.0`, and `<2` allows every `1.x` version.

- A bare partial version, such as `1.2`, means every version that starts with it (`1.2.x`).
- A caret keeps the leftmost nonzero part: `^1.2` allows `1.2.0` up to, but not including, `2.0.0`.
- As built, an operator on a partial version is dropped, so `<2` means `2.x` and `>=1.2` means `1.2.x`, and `^1.2` means `1.2.x`. These are known defects.
- Four-digit years with a one-digit minor, such as `2024.1`, are treated as full versions. Whether that should apply to all partial versions is [open](../../Verus_verification/open_questions.md#version.range.year).

- Working assumption: keep the special case for IRIS-style versions, because IPM's tests require `2022.2` to match only `2022.2`.


<a id="version.range.combine"></a>
[I · version.range.combine]

When several modules require the same dependency, IPM allows only the versions that every requirement allows. The result does not depend on the order in which the requirements are combined.

- As built, two bounds that differ only by a prerelease part keep whichever came first, so the result depends on order. Because IPM visits dependencies by name, renaming a dependency can decide whether a solvable install succeeds. This is a known defect.


<a id="version.range.snapshot-intersection"></a>
[D · version.range.snapshot-intersection]

As built, intersecting exact `1.0.0+snapshot` with `>=1.0.0+snapshot` raises a no-overlap error even though that snapshot satisfies both expressions separately. The exact-version intersection branch drops build metadata before testing the bounds, causing defect C:D33 and the checked [snapshot reinstall refusal](../features/install.g5.md#reinstall.snapshot-dependencies).


<a id="version.post-release"></a>
[I · version.post-release]

When the `SemVerPostRelease` setting is on, IPM treats `x.y.z-w` as a release after `x.y.z` instead of a prerelease before it.

## Metadata





Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md). Version comparison is used by the formal scope through the [downgrade refusal](../features/load.g5.md#load.refusal.downgrade). Consolidated round 2 brought parsing, ordering, ranges and their combination into the modeled scope (`Verus_verification/consolidated/models/semver.py`, native runs `20261114T030608Z-r2-semver`, `20261114T031048Z-r2-resolve`, `20261114T031514Z-r2-resolve-order`).

### version

- human-provided
- user-facing-behavior
- Children: [order](#version.order), [range](#version.range), [post-release](#version.post-release).

Sources: `Main.cls:260`; CHANGELOG 0.9.0 (`#544`, `#557`, `#559`). Inventory: CAT-19.

Findings: [C:D9 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d9). The linked entry qualifies the caller, reader and providers.

### version.order

- design-verified

- human-verified
- user-facing-behavior
- Expands: [Version](#version).

Sources: doc comment `General/SemanticVersion.cls:116-122` (SemVer 2.0.0 item 11 and node-semver prerelease rule, written by the maintainers); SemVer 2.0.0 item 10 for build metadata. Native ordering matrix F0 and F1 (`20261114T030608Z-r2-semver`) and model `Verus_verification/consolidated/models/semver.py` (`follows`, `compare_dot`), 137 matched fields, 30 of 32 wrong variants rejected. Observed defect C:D9: `1.2.3+build-x` is parsed as prerelease `x` (V9).

Extension Verus: [semver.rs::compare_dot](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::fields_string](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::follows](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::parse](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::snapshot](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::validate](../../Verus_verification/consolidated/verus/semver.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Iteration34 index order: [versions.rs](../../Verus_verification/consolidated/iter34/versions.rs) `key`, `cmp_key`, `releases_agree`. The filesystem version index walks versions in this order; the install consumer re-sorts them differently ([search rule](repositories.g5.md#repo.search-order)).

Findings: [A filesystem repository install picks a prerelease over its own release and orders prerelease numbers as text](../../Verus_verification/bugs_issues.md#c-d39)

Findings: [`list-installed -s version` orders prereleases of one version as text](../../Verus_verification/bugs_issues.md#c-d42)

State machines: [versions · iter34](../../Verus_verification/models/iter34-versions/manifest.json)

### version.range

- uncertain
- human-provided
- user-facing-behavior
- Expands: [Version](#version).
- Children: [partial versions](#version.range.partial), [combining](#version.range.combine).

Sources: `wiki/03` L243-250; `Main.cls:409-411`; CHANGELOG 0.9.0 (`#559`). Inventory: CAT-03, CAT-19. Consolidated round 2: a filesystem repository with prereleases enabled offers them, and the resolver then picked `2.0.0-beta.1` for `<2.0.0` (native S4). The skip in this block applies to repositories that do not offer prereleases ([prerelease setting](repositories.g5.md#repo.prereleases)).

Extension Verus: [semver.rs::cmp_holds](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::cmp_text](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::expr_satisfied](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::expr_text](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::expression_set](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::operator](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::range_holds](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::range_text](../../Verus_verification/consolidated/verus/semver.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [C:D6 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d6). The linked entry qualifies the caller, reader and providers.

Findings: [C:D7 — partial reproduced](../../Verus_verification/bugs_issues.md#c-d7). The linked entry qualifies the caller, reader and providers.

Findings: [C:D8 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d8). The linked entry qualifies the caller, reader and providers.

Findings: [C:D10 — partial reproduced](../../Verus_verification/bugs_issues.md#c-d10). The linked entry qualifies the caller, reader and providers.

Open questions: [Should a partial version such as `2024.1` mean exactly `2024.1.0`, as built, while `2024.12` and `1.2` mean a whole minor series?](../../Verus_verification/open_questions.md#version.range.year)

### version.range.partial

- uncertain
- human-verified
- user-facing-behavior
- Expands: [Range](#version.range).

Sources: IPM unit tests `Test/PM/Unit/SemVer/Expressions.cls:174-175` (`>=2022.1` is satisfied by `2022.2`) and `:154` (`^1.2.x` equals `>=1.2.0 <2.0.0-0`); the caret and tilde rules quoted from node-semver in `General/SemanticVersionExpression/Range.cls:110-124`, whose branch for `^1.2` (`:131-136`) is unreachable. P's ontology DEP-D2 uses `>=2.0` as an example range. Native: `<2` resolves to `>=2.0.0 <3.0.0-0` (X27), `>=1.2` to `1.2.x` (X26), `^1.2` to `1.2.x` (X20); at command level a dependency declared `<2` installed `2.5.0` (S1), `>=1.2` was refused with `1.3.0` offered (S2) and picked `1.2.0` over `1.5.0` (S3) (`20261114T030608Z-r2-semver`, `20261114T031048Z-r2-resolve`). Defects C:D6, C:D7.

Extension Verus: [semver.rs::expand](../../Verus_verification/consolidated/verus/semver.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Open questions: [Should a partial version such as `2024.1` mean exactly `2024.1.0`, as built, while `2024.12` and `1.2` mean a whole minor series?](../../Verus_verification/open_questions.md#version.range.year)

### version.range.combine

- human-verified
- user-facing-behavior
- Children: [snapshot intersection](#version.range.snapshot-intersection).
- Expands: [Range](#version.range).

Sources: doc comment `General/SemanticVersionExpression.cls:120-128` ("Combines two version expressions with AND"); [dependency conflicts](../features/dependencies.g5.md#deps.conflict). Native: `^1.0.0` AND `<2.0.0` gives `<2.0.0-0`, the reverse gives `<2.0.0` (A1, A2), and the same for `>=1.0.0-beta` and `>=1.0.0` (A3, A4); a diamond whose parents require `^1.0.0` and `<2.0.0` of one dependency is refused when the `^` parent sorts first by name (S4ab, S4ba) and installs `1.5.0` when it sorts second (S5, `20261114T031514Z-r2-resolve-order`). Code: `Range.cls:349-389` compares bounds without the prerelease and `:270-292` keeps the first of equal bounds; `Storage/Module.cls:1095-1106` combines the current requirement with those recorded earlier. Defect C:D8. This refutes premise D2 of P's resolver proof ("the expression algebra is exact", `playbook-uplift/models/verus/update_steps.TRUST.md` row D2).

Extension Verus: [semver.rs::compare_versions](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::diamond](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::expr_and](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::group](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::intersect](../../Verus_verification/consolidated/verus/semver.rs), [semver.rs::valid_intersection](../../Verus_verification/consolidated/verus/semver.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

### version.range.snapshot-intersection

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Combining ranges](#version.range.combine).

[Iteration 24 composition](../../Verus_verification/consolidated/iter24/snapshot_dependencies.rs) reuses the inherited `parse`, `snapshot`, `expr_satisfied`, `intersect` and `expr_and` definitions for this literal pair. [Native evidence](../../uplift/consolidated/iter24/HANDOFF.md) checks both individual memberships, the thrown intersection error and the resulting reinstall refusal. The old mathematical definitions are preserved; the new consumer witness exposes this exact-build case without claiming a generic parser or ObjectScript-body proof.

Findings: [C:D33 — e2e reproduced](../../Verus_verification/bugs_issues.md#c-d33). The linked entry qualifies the caller, reader and providers.

State machines: [snapshot dependencies · iter24](../../Verus_verification/models/iter24-snapshot-dependencies/manifest.json)

### version.post-release

- human-provided
- user-facing-behavior
- Expands: [Version](#version).

Sources: `wiki/02` config table; CHANGELOG. Inventory: CAT-19, CAT-26.
