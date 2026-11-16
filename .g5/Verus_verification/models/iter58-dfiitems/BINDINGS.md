# dfiitems · iter58

Explicit DFI names load through the Document path (non-dev n1, -dev d1, reload r1 updates); a wildcard name searches the dfi folder with UNIX patterns missing a ";" so .dfi and .XML files are skipped (W2, W3). Folder-item import and FindFiles splitting are native-qualified.

- Explicit DFI names load through the Document path (non-dev n1, -dev d1, reload r1 updates); a wildcard name searches the dfi folder with UNIX patterns missing a ";" so .dfi and .XML files are skipped (W2, W3). Folder-item import and FindFiles splitting are native-qualified.
- Uninstall deletes explicitly named DFI items; wildcard-loaded items remain (u1). Keywords filtering in OnResolveChildren not exercised; OnGetUniqueName reached only via resolution (no Keywords).

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter58/dfiitems.rs)

[Product correspondence](../../../uplift/consolidated/iter58/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter58/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter58/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter58/project-map.json)
