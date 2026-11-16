# copy · iter37

Reload copies the package classes from Source; Overwrite=0 keeps classes present in the target; classes owned by another module resource are excluded; copied classes are compiled. Native o1-o4. Export/Load providers and the /checkuptodate flag are native-qualified (a newer target was still replaced). CompileAfter and Flags attributes not assessed.

- Reload copies the package classes from Source; Overwrite=0 keeps classes present in the target; classes owned by another module resource are excluded; copied classes are compiled. Native o1-o4. Export/Load providers and the /checkuptodate flag are native-qualified (a newer target was still replaced). CompileAfter and Flags attributes not assessed.
- Uninstall resolves the Copy package in the installing namespace and deletes every class not owned by another module, including classes the load kept (C:D40). Native u1, u2, u4. Mapped classes and projection ordering in %Clean are not assessed.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter37/copy.rs)

[Product correspondence](../../../uplift/consolidated/iter37/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter37/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter37/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter37/project-map.json)
