# nsinit · iter55

Exact name: switch only when %IPM.Main.Shell is visible there, otherwise "IPM is not enabled" with OK status and no switch; unknown namespace fails; switch persists after Shell. Pattern form lists matching namespaces with "(IPM not enabled)" markers then prompts (empty answer keeps namespace). Native z1-z5. Switching into a second IPM namespace not exercised (no second IPM namespace in fixture).

- Exact name: switch only when %IPM.Main.Shell is visible there, otherwise "IPM is not enabled" with OK status and no switch; unknown namespace fails; switch persists after Shell. Pattern form lists matching namespaces with "(IPM not enabled)" markers then prompts (empty answer keeps namespace). Native z1-z5. Switching into a second IPM namespace not exercised (no second IPM namespace in fixture).
- xecute under new $namespace: data effects persist, namespace change does not (help example ineffective); runtime and syntax errors fail. Native e1-e4.
- init -noprompt: dangling class disabled; plain class stored as Composite primary but stays active (messages claim enabled/configured); -extension X makes Composite active. Native i1-i4. Interactive prompts, settings configuration, -zpm and -menuextension not exercised.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter55/nsinit.rs)

[Product correspondence](../../../uplift/consolidated/iter55/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter55/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter55/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter55/project-map.json)
