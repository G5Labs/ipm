# settings · iter41

config set/delete over the observed fresh-install slots: first write becomes the default, delete writes the stored default (P:F-REPO-4), unknown key and invalid TestReportFormat print and keep state while the shell status stays OK (P:F-REPO-8). Native c0-c6. ModuleRoot validation (iter29 scope) excluded.

- config set/delete over the observed fresh-install slots: first write becomes the default, delete writes the stored default (P:F-REPO-4), unknown key and invalid TestReportFormat print and keep state while the shell status stays OK (P:F-REPO-8). Native c0-c6. ModuleRoot validation (iter29 scope) excluded.
- config list prints exactly the CONFIGURABLE keys (default_registry not listed). Native c9. Value formatting not modelled.
- analytics stored raw by config set; the remote consumer test negates the stored text, so 0 and any text such as yes disable usage data. Native c7-c8 via the getter; the remote PackageService send itself is not exercised (network).

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter41/settings.rs)

[Product correspondence](../../../uplift/consolidated/iter41/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter41/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter41/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter41/project-map.json)
