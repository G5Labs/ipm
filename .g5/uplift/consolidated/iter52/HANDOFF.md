# Iteration 52: stored module record across changed manifests, and lifecycle class choice

Convergence test, strongest remaining candidate. Product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter52/record.rs), [cases](../../../Verus_verification/consolidated/iter52/cases.json), [bindings](bindings.json), [ledger](working-ledger.json). Not sealed or committed.

## Result for convergence

No new defect. One owner question is proposed: a lifecycle class that is not compiled yet, or does not exist, is silently replaced by the default. Dropped-resource retention is already the open C:D26 policy area.

## Predictions and observations

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| c1 load the identical manifest again | LastModifiedTimestamp and LastUpdated both change | not stated | Modified unchanged (refuted: the exported hash includes LastUpdated) |
| c2 load manifest without R2 and dep52, Default a to b | record 1 resource, 0 dependencies, Invoke gets b; class M52.R2 and module dep52 stay | record updated; old code stays | same |
| l1 first load, LifecycleClass is the module's own ordinary resource | stored `Module`; custom Activate not run | custom lifecycle | default used, name overwritten |
| l2 load the same directory again | stored `L52.Life`; custom Activate runs | custom | custom |
| p1 lifecycle class shipped in preload | stored `L52P.Life`; custom runs on first install | custom | custom (resolved again after the Reload refresh) |
| n1 LifecycleClass `NoSuch52.Life` | load succeeds; stored `Module` | unspecified | silent fallback |
| o1 second module lists `M52.R1.CLS` | refused, already part of m52 | refused (existing owner) | refused |

## Evidence

`record.rs` 3 verified, 0 errors. Four controls were each rejected: `first-install-uses-named`, `preload-not-compiled`, `changed-manifest-deletes-old` and `hash-ignores-lastupdated`. The [native receipt](native/20261116T180626798933Z/observations.json) has 35 gated fields. 17 replay in Verus, and flips of `l1.life`, `c2.r2defined` and `c1.modchanged` were each rejected. Explore1 (without the preload variant) is kept under `attempts/`. Command: `python3 .g5/Verus_verification/consolidated/iter52/run.py` (15 s).

## Reach candidates

Newly reached if not already linked: `Storage/Module.cls::LifecycleGet`, `OverrideLifecycleClassSet`, `%OnAddToSaveSet`, `%OnBeforeSave`, `%OnNew` and `%OnOpen`. Strengthened: `ExecutePhases`.

Excluded with reasons: `CompareWithRelationships` has no caller in src or tests at the pin. `HaveToDeploy` and `%OnOpen`'s deployed DeveloperMode reset are on the deployed-code path (excluded scope). `GetNamespaceConfig` and `GetKnownDependencies` feed packaging and the dependency analyzer (excluded or external). `CalculatedResourcesGet` is a generated sorter, trivial for this scope. `IsLocked`, `Lock` and `ValidateStream` were not reached by these operations and stay unassessed.

## Limits

The `%OnOpen` root override from source control and `ValidateStream` (Studio document validation) are not exercised. No ObjectScript body is verified directly.

## Proposed owner text

- **`modules.g5.md`, new D statement `module.lifecycle-class`:** "A manifest's `LifecycleClass` picks the lifecycle IPM runs, with short names resolved under `%IPM.Lifecycle`. IPM resolves it when a command starts and again after Reload, so a lifecycle class shipped in the module's `preload` folder is used on the first install. A class shipped as an ordinary resource is compiled only later, so the first install runs the default `Module` lifecycle and stores `Module` as the lifecycle class; a later load uses the named class. A class that does not exist is replaced by `Module` without a warning ([question](#module.lifecycle-class.unknown-policy))."
  - Metadata: agent; user-facing-behavior. Sources: `Storage/Module.cls::LifecycleGet`, `::ExecutePhases`. Model: [record.rs](../../Verus_verification/consolidated/iter52/record.rs) `resolve`, `lifecycle_neighbors` ([handoff](../../uplift/consolidated/iter52/HANDOFF.md)).
- **New Q `module.lifecycle-class.unknown-policy`:** "Should a load fail or warn when `LifecycleClass` names a class that is not available when the lifecycle is resolved?" Working assumption: as built, fall back to `Module` silently. Alternatives: fail the load, or warn and keep the manifest's name. Evidence: native n1 and l1. This is not a maintainer decision.
- **`module.resource`, added bullet:** "Loading a manifest that no longer lists a resource or dependency updates the stored record, but leaves the dropped resource's code and the dropped dependency module installed. Whether IPM should remove them is the open upgrade-cleanup policy (C:D26)." Model: `changed_manifest_keeps_old_code`.
- **Fact for `module.identity.document` metadata:** "The module record's LastModifiedTimestamp, which the `.ZPM` document timestamp falls back to, changes on every save, because the hashed export includes LastUpdated." Model: `identical_reload_still_modified`.
- No register entry.
