# grouping · coverage iter69

generate resource grouping: one package resource per (directory, top-level package), named by the deepest shared package; leftovers written individually; tests as UnitTest entries, test leftovers written nowhere; module.xml keeps one resource per name (C:D72); load resolution of entries without Directory (C:D73). Native m1-m6, 59 fields.

- One package resource per (directory, top-level package), named by the deepest package its classes share; proved for every file sequence (`group_characterized`). Native m1, m3.
- Leftovers (routines, include files, classes without a package) are written once under their listed names; unsupported, unlisted and tests-folder files are written nowhere. Native m1, m2a, m2e, m4, m5.
- The module.xml writer keeps one resource per (name, extension); with one package in two directories one directory's classes are lost (C:D72). Native m3, with load.
- `load` finds package-layout files; a package-less root class beside a cls folder and a flat-named class file are not found (C:D73). Native m2a, m2b, m2e.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../coverage-20261117/iter69/grouping.rs)

[Product correspondence](../../../uplift/coverage-20261117/iter69/bindings.json)

[Proof and controls](../../../uplift/coverage-20261117/iter69/proofs/models.json)

[Iteration handoff](../../../uplift/coverage-20261117/iter69/HANDOFF.md)
