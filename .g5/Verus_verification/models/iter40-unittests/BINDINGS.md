# unittests · iter40

Reload loads the test directory only in developer mode; the test phase loads the whole test directory (every package under it), compiles the resource package, checks for package classes after that load and so always passes /nodelete; the Phase="verify" resource does not run. Native n0-n1, d0-d1. %UnitTest.Manager run/delete behaviour is a provider premise; verify phase not run (excluded archive/verify scope).

- Reload loads the test directory only in developer mode; the test phase loads the whole test directory (every package under it), compiles the resource package, checks for package classes after that load and so always passes /nodelete; the Phase="verify" resource does not run. Native n0-n1, d0-d1. %UnitTest.Manager run/delete behaviour is a provider premise; verify phase not run (excluded archive/verify scope).
- Uninstall removes test classes left by the test phase (both packages under the resource directory). Native n2, d2.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter40/unittests.rs)

[Product correspondence](../../../uplift/consolidated/iter40/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter40/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter40/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter40/project-map.json)
