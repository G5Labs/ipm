# installer · iter36

With InstallerClass, Configure (nested in Activate) calls OnInstallComponent(DatabaseName "", Verbose, DeveloperMode, IsUpgrade) then OnConfigureComponent($namespace, Verbose, vars); IsUpgrade/DatabaseName come only from Configure.* params (no product writer); a refused OnInstallComponent fails Configure/Activate, skips OnConfigureComponent and the normal-mode load rolls back. Native l1,l2,l3,l5,e1,n1.

- With InstallerClass, Configure (nested in Activate) calls OnInstallComponent(DatabaseName "", Verbose, DeveloperMode, IsUpgrade) then OnConfigureComponent($namespace, Verbose, vars); IsUpgrade/DatabaseName come only from Configure.* params (no product writer); a refused OnInstallComponent fails Configure/Activate, skips OnConfigureComponent and the normal-mode load rolls back. Native l1,l2,l3,l5,e1,n1.
- <Defaults><Parameter> values merge into phase params and reach installer vars; <Default Name Value> values reach Invoke ${...} arguments via GetCustomParameters/EvaluateArgs but not installer vars (%Configure computes customParams and does not use it); -D overrides both. Native l1,l3. LifecycleClass filtering and dotted Parameter names beyond Configure.IsUpgrade not assessed.
- Uninstall Clean runs Unconfigure first: OnUnconfigureComponent($namespace, Verbose, Purge 0, vars) then OnUnInstallComponent(DatabaseName "", Verbose). Purge forwarding itself is owned by uninstall.purge.as-built (iter27 purge.rs) and is not re-modelled. Native u1,u2.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter36/installer.rs)

[Product correspondence](../../../uplift/consolidated/iter36/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter36/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter36/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter36/project-map.json)
