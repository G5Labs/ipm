# listing · iter35

Flat report: label "External (name)", IPM pinned first in every sort, name/name-desc by lowercase label, version by numeric triple then prerelease text ("~" for release) then case-sensitive label; developer-mode marker. Native l0-l2, l5. ASCII labels; date sort, search, -description and -showupstream not modelled.

- Flat report: label "External (name)", IPM pinned first in every sort, name/name-desc by lowercase label, version by numeric triple then prerelease text ("~" for release) then case-sensitive label; developer-mode marker. Native l0-l2, l5. ASCII labels; date sort, search, -description and -showupstream not modelled.
- Tree: roots are installed modules no installed module depends on, in lowercase name order; each dependency printed under each parent (shared child twice); uninstalled dependency printed "[missing]" with no children; -s ignored. Native t0-t2. At most one dependency per module, so sibling order is not modelled.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter35/listing.rs)

[Product correspondence](../../../uplift/consolidated/iter35/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter35/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter35/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter35/project-map.json)
