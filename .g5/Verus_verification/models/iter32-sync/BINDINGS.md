# sync · iter32

Developer-mode PKG and single-CLS resources in conventional layout: modified/deleted against baseline, early "Nothing to sync" exit, -delete removes compilable documents, deletions without -delete reported and kept. Native s0-s4. SHA-1 provider, $system.OBJ.Delete and compiler are premises; non-compilable resources, test routing and retries are owned elsewhere or outside.

- Developer-mode PKG and single-CLS resources in conventional layout: modified/deleted against baseline, early "Nothing to sync" exit, -delete removes compilable documents, deletions without -delete reported and kept. Native s0-s4. SHA-1 provider, $system.OBJ.Delete and compiler are premises; non-compilable resources, test routing and retries are owned elsewhere or outside.
- Changed module.xml reloads the manifest before scanning; a newly declared resource file has no baseline row and syncs as modified. Native s5. Manifest removal of a resource is observed only as its routing disappearing.
- Sync scans {SourcesRoot}/{Directory}; Reload also tries {SourcesRoot} when Directory is undeclared. Flat-layout PKG module loads, its edit/delete are never synced, a reload picks the edit up. Native f1-f3 (C:D36). Other Directory spellings, preload and path-style resources not assessed.
- Developer-mode rows in ID order, edges restricted to the developer-mode set, repeated row-order passes, cycle fallback. Native order run: direct dependency respected, transitive dependency through a non-developer module not (C:D37). SQL row order is a provider premise.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter32/sync.rs)

[Product correspondence](../../../uplift/consolidated/iter32/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter32/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter32/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter32/project-map.json)
