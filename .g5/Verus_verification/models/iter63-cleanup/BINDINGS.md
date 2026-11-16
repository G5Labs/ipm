# cleanup · iter63

Per-kind outcome of uninstall (and -purge): CLS, PKG, INC, MAC/INT/OBJ, record, manifest stream and sync baseline removed; name-only GBL data kept (P:F-UNIN-5), Global-attribute GBL deleted; FileCopy removed unless KeepOnUninstall. Native x1, x3. Table data/purge is owned by uninstall.purge.as-built and not re-tested.

- Per-kind outcome of uninstall (and -purge): CLS, PKG, INC, MAC/INT/OBJ, record, manifest stream and sync baseline removed; name-only GBL data kept (P:F-UNIN-5), Global-attribute GBL deleted; FileCopy removed unless KeepOnUninstall. Native x1, x3. Table data/purge is owned by uninstall.purge.as-built and not re-tested.
- Declared <Mapping> GBL and PKG entries added at Reload remain after uninstall and -purge; developer-mode ^Sources("MODULE",name) and ^Sources("cls",pkg) entries remain after uninstall. Native i1/x1/x3, i2/x2. No uninstall caller of RemoveGlobalMapping/RemovePackageMapping or ^Sources kill (source search).

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter63/cleanup.rs)

[Product correspondence](../../../uplift/consolidated/iter63/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter63/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter63/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter63/project-map.json)
