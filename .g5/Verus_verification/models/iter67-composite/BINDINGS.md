# composite · iter67

Non-mapped composite: per-item callbacks and type-1 actions reach only the primary with corrected names (incl. /A/B.cls path form); menu actions reach the extension owning the menu; unknown methods dispatch to the primary. Native a1-a3, u1-u3, d1. Mapped source control (MappedSourceControl, FindNamespaceFor*) not exercised.

- Non-mapped composite: per-item callbacks and type-1 actions reach only the primary with corrected names (incl. /A/B.cls path form); menu actions reach the extension owning the menu; unknown methods dispatch to the primary. Native a1-a3, u1-u3, d1. Mapped source control (MappedSourceControl, FindNamespaceFor*) not exercised.
- GetStatus ANDs Editable over [primary, others] (primary consulted twice); IsReadOnly returns only the primary answer and never consults others. Native g1-g3, r1-r3.
- A nonexistent other-extension name is stored raw with OK and makes every composite creation fail; an empty primary name is ignored with OK. Native s1, e1.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter67/composite.rs)

[Product correspondence](../../../uplift/consolidated/iter67/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter67/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter67/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter67/project-map.json)
