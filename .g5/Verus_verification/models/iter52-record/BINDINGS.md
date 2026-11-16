# record · iter52

Lifecycle class resolved before Initialize and after the Reload refresh: an ordinary-resource class runs the default lifecycle on first install and is stored as Module; a preload class or a reinstall uses the named class; an unknown name becomes Module silently. Native l1, l2, p1, n1.

- Lifecycle class resolved before Initialize and after the Reload refresh: an ordinary-resource class runs the default lifecycle on first install and is stored as Module; a preload class or a reinstall uses the named class; an unknown name becomes Module silently. Native l1, l2, p1, n1.
- Loading a changed manifest replaces the stored resources, dependencies and defaults; the dropped class and the dropped dependency module stay installed; LastModifiedTimestamp changes on every save because the exported hash includes LastUpdated. Native c0-c2. Root override from source control in %OnOpen and deployed DeveloperMode reset are not exercised.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter52/record.rs)

[Product correspondence](../../../uplift/consolidated/iter52/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter52/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter52/HANDOFF.md)
