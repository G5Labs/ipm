# cspapp · iter57

Load and reload create or modify the application with every ATTRIBUTES property and copy the source folder into DeployPath. Native a1, a2, a4, b1, c0, c1. Security.Applications and copy/chmod are providers; MatchRoles, delegated auth and DispatchClass are not exercised.

- Load and reload create or modify the application with every ATTRIBUTES property and copy the source folder into DeployPath. Native a1, a2, a4, b1, c0, c1. Security.Applications and copy/chmod are providers; MatchRoles, delegated auth and DispatchClass are not exercised.
- Uninstall deletes the application and purges the files of its Directory via deletePortal; without DeployPath that Directory is the module source folder. Native a3, b2, c2. deletePortal is a provider; protected directories are covered by the product FileCopy test and not re-run.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter57/cspapp.rs)

[Product correspondence](../../../uplift/consolidated/iter57/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter57/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter57/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter57/project-map.json)
