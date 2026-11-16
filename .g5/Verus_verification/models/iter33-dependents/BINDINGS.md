# dependents · iter33

Flat report: roots are installed modules (or -r repository manifests that declare a dependency, installed modules ignored); a root is listed when its graph succeeds and contains the target, with optional filter on the target version resolved inside that root graph; failed graphs are printed as warnings whatever the target. Fixture: single-dependency chains, lower-bound ranges, r33 filesystem repository. Native l0..l3v1, lr, lrv2.

- Flat report: roots are installed modules (or -r repository manifests that declare a dependency, installed modules ignored); a root is listed when its graph succeeds and contains the target, with optional filter on the target version resolved inside that root graph; failed graphs are printed as warnings whatever the target. Fixture: single-dependency chains, lower-bound ranges, r33 filesystem repository. Native l0..l3v1, lr, lrv2.
- Tree report: InvertDependencies walks parent edges recorded by BuildDependencyGraph, which records an edge only for a non-root parent, so the printed tree holds the target and intermediate modules but no graph root (no direct dependent, no top-level dependent). PrintTree name column 1, then 2*depth+2 (depths 0-2 observed). Chain-shaped trees only; sibling order and diamonds unassessed. Native l0t, lbt, ldt; raw tree dump attempts/explore2-tree-dump.log.
- Reader-side consequence of the iteration31 installed-requirement rule: with the target uninstalled, one installed module requiring >=2.0.0 makes every other root graph fail when only 1.0.0 is offered, so all dependents become warnings; removing that module lists them again at the repository version. Native un-a, l2, l2v1, un-d, l3, l3v1.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter33/dependents.rs)

[Product correspondence](../../../uplift/consolidated/iter33/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter33/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter33/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter33/project-map.json)
