# moderefresh · iter65

Across eight resource kinds, normal-mode reload/compile module actions refresh nothing while load <dir> refreshes everything; developer mode refreshes through the stored root. Native s0, a, b1, b2 for md65/mn65.

- Across eight resource kinds, normal-mode reload/compile module actions refresh nothing while load <dir> refreshes everything; developer mode refreshes through the stored root. Native s0, a, b1, b2 for md65/mn65.
- Sync refreshes CLS, INC, MAC, LOC and DFI; GBL, FileCopy and WebApplication have no sync support and are named only with -verbose; normal-mode sync refused. Native c, v.
- Phase at which each kind becomes observable: INC text, GBL data and DFI items at Reload; CLS, MAC and LOC at Compile; FileCopy and WebApplication at Activate, so the compile chain leaves them stale. Native b1, b2 (developer mode).

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter65/moderefresh.rs)

[Product correspondence](../../../uplift/consolidated/iter65/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter65/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter65/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter65/project-map.json)
