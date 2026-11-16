# comments · iter44

Comment anchored to the XPath of the next element start; last child discarded; reapplied only where that XPath exists in the export. Native s1/s2 (with %Studio.SourceControl.Base): C1-C3 kept, C4/C5 before resources and C6 trailing dropped, idempotent. Attribute-free duplicate-path counting, whitespace text and non-indented output not assessed; SAX/XSLT and XMLExportToStream are providers.

- Comment anchored to the XPath of the next element start; last child discarded; reapplied only where that XPath exists in the export. Native s1/s2 (with %Studio.SourceControl.Base): C1-C3 kept, C4/C5 before resources and C6 trailing dropped, idempotent. Attribute-free duplicate-path counting, whitespace text and non-indented output not assessed; SAX/XSLT and XMLExportToStream are providers.
- Source-control status gate: no source-control class -> GetStatus #5889 -> arrange fails and nothing is rewritten (a1, a2, n1); with the stock base class arrange rewrites the stored document, sorting resources into <Resources> (s1-s3). module.xml on disk is never written. Publish-phase caller excluded (package/publish scope); LockWithCurrentDependencies has no callers.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter44/comments.rs)

[Product correspondence](../../../uplift/consolidated/iter44/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter44/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter44/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter44/project-map.json)
