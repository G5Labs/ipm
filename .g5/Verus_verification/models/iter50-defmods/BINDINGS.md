# defmods · iter50

Exactly one of -set/-get/-delete; -set merges the command data (append, same key overwritten); -delete clears all; -get renders every stored node as -D<dotted name>=<value>; non-D modifiers such as -verbose refused by the parser. Native g0, e1, e2, s0-s3, f1, d0. Cross-namespace sharing of the global is owned by P:F-REPO-5 and not re-modelled.

- Exactly one of -set/-get/-delete; -set merges the command data (append, same key overwritten); -delete clears all; -get renders every stored node as -D<dotted name>=<value>; non-D modifiers such as -verbose refused by the parser. Native g0, e1, e2, s0-s3, f1, d0. Cross-namespace sharing of the global is owned by P:F-REPO-5 and not re-modelled.
- Dispatcher merges stored defaults into command data before the parsed command; module <Default> values sit under command data. Stored value beats the manifest <Default>, command-line -D or -quiet beats stored, -delete restores the manifest value. Native l1, l2, v1, v2, d1. Invoke expression expansion (only <Default> names expand) is a provider fact here, not modelled.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter50/defmods.rs)

[Product correspondence](../../../uplift/consolidated/iter50/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter50/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter50/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter50/project-map.json)
