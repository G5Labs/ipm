# Versions

IPM compares module versions to pick what to install and to refuse downgrades.

## Definitions

- **Snapshot**: a version with the build part `+snapshot`, meaning the latest build of that version.
- **Prerelease**: a version with a prerelease part, such as `1.0.0-beta.1`.

## Statements

<a id="version"></a>
[I · version]

Module versions are semantic versions: major, minor and patch numbers, with optional prerelease and build parts.

<a id="version.range"></a>
[I · version.range]

Dependencies, `install` and `update` accept an exact version or a range, such as `1.x` or `>=1.2.0`. When IPM picks the highest version in a range, it skips prereleases and snapshots.

<a id="version.post-release"></a>
[I · version.post-release]

When the `SemVerPostRelease` setting is on, IPM treats `x.y.z-w` as a release after `x.y.z` instead of a prerelease before it.

## Metadata

Source locators follow the conventions on the [root page](../root.g5.md#how-to-read-this-ontology). Inventory IDs refer to [A1](../../uplift/ipm-verus-20261113/A/intent-inventory.md). Version comparison is used by the formal scope through the [downgrade refusal](../features/load.g5.md#load.refusal.downgrade). Version parsing itself is outside it.

### version

- human-provided
- user-facing-behavior
- Children: [range](#version.range), [post-release](#version.post-release).

Sources: `Main.cls:260`; CHANGELOG 0.9.0 (`#544`, `#557`, `#559`). Inventory: CAT-19.

### version.range

- human-provided
- user-facing-behavior
- Expands: [Version](#version).

Sources: `wiki/03` L243-250; `Main.cls:409-411`; CHANGELOG 0.9.0 (`#559`). Inventory: CAT-03, CAT-19.

### version.post-release

- human-provided
- user-facing-behavior
- Expands: [Version](#version).

Sources: `wiki/02` config table; CHANGELOG. Inventory: CAT-19, CAT-26.
