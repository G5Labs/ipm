# settings · iter70

Revises the iteration 41 settings model for the NotaryExecutable key that #281 adds.

- `CONFIGURABLE` now has 13 keys. Iteration 41's "key outside the list" is now index 13.
- `config delete` empties NotaryExecutable and TestReportFormat. Other keys still restore their stored default, the inherited first-write rule.
- `ContentTrust.NotaryExecutable` reads an empty value as notary on the PATH.

[Formal declaration](../../consolidated/iter70/settings.rs)

[Product correspondence](../../../uplift/consolidated/iter70/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter70/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter70/HANDOFF.md)
