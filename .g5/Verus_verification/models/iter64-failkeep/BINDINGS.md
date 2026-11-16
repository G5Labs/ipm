# failkeep · iter64

Normal-mode load in a transaction failing at Compile or after Activate: record, classes, messages, declared mappings and web application roll back; CPF merge, FileCopy targets and the .LOC export file survive. Native ok/ec/la. Config/Security rollback and the compiler are native-qualified premises.

- Normal-mode load in a transaction failing at Compile or after Activate: record, classes, messages, declared mappings and web application roll back; CPF merge, FileCopy targets and the .LOC export file survive. Native ok/ec/la. Config/Security rollback and the compiler are native-qualified premises.
- External effects (iris merge, file copies, localization export file) are not undone by the load rollback; database-backed configuration (mappings, web application) is. Native ec/la.
- Developer-mode load failing at Compile keeps the record, compiled valid class, messages, declared mappings and ^Sources; Activate effects never happen; history reads committed. Native dv.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter64/failkeep.rs)

[Product correspondence](../../../uplift/consolidated/iter64/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter64/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter64/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter64/project-map.json)
