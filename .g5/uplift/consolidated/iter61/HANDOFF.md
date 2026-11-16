# Iteration 61: WebApplication attributes and phases

Batch 13, product `571e04a1` unchanged. Not sealed. [Model](../../../Verus_verification/consolidated/iter61/webappphases.rs), [cases and controls](../../../Verus_verification/consolidated/iter61/cases.json), [bindings](bindings.json), [ledger](working-ledger.json). Owner: [module.webapp](../../../ontology/concepts/modules.g5.md#module.webapp). This reuses iteration5 (takeover, reinstall, duplicate refusal) and C:D23 and C:D26 without re-filing them.

| Case | Observed | Predicted |
|---|---|---|
| w1 load with `Path="{$mgrdir}ipmcont61a"` | path expanded, NameSpace USER, Timeout 900 (IRIS default) | expanded, USER |
| w2 Description removed, Timeout 900 declared | Description keeps "first" | keeps (only declared properties written) |
| w3 Description "second" | updated | updated |
| c1 manual edit, `compile -only` | edit kept | kept |
| c2 `activate -only` | restored to "second" | restored |
| n1 second module declares `/IPMCONT61A` | refused: "already defined as part of module 'm61a'" | source: takeover (refuted); owner: refused |
| t1 `/ipmcont61c/` | one application, reachable with or without the slash | provider-normalized |
| f1 `Timeout="abc"` | load fails "#7207 not a valid number"; no application, no module | fails, nothing kept |
| u1 uninstall m61a | application deleted | deleted |

No new discrepancy was found. Evidence: `webappphases.rs` 1 verified (the neighbours lemma). Five controls were each rejected: `removed-attribute-cleared`, `compile-resets`, `duplicate-takes-over`, `invalid-kept` and `uninstall-any-owner`. The [native receipt](native/20261116T183332131799Z/observations.json) has 171 gated fields. 43 replay in Verus, and flips of `n1.ok`, `c1.a.desc.manual` and `w2.a.desc.first` were each rejected. Run 1's replay failed because the model omitted IRIS's default Timeout for a new application. That run is kept under `attempts/`, the premise was added, and the replay was rerun on the same native receipt.

Newly reached members (CopyAttributes, OnPhase, NameGet among them) are listed in the bindings. Limits: `GetSourceControlInfo` and `OnItemRelativePath` were not exercised; source control is iteration43's lane. IRIS application validation, name normalization and defaults are providers.

## Proposed owner text

Add to `module.webapp`, after the `NameSpace` bullet:

- "Attribute values are expanded like other resource attributes, so `{$mgrdir}` becomes the manager directory. Loading again writes only the attributes still declared: a removed attribute keeps its previous value. `compile -only` does not touch the application, while Activate and Configure update it."

Metadata line: "Iteration61: [webappphases.rs](../../Verus_verification/consolidated/iter61/webappphases.rs) `apply`, `load`, `neighbors` ([handoff](../../uplift/consolidated/iter61/HANDOFF.md))." No register entry.
