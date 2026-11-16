# Class deletion provider calibration

This is an unexecuted native probe recipe, not a confirmed deletion failure. No Docker or solver run was performed by this lane. The parent owns the fresh IRIS environment, final observations, model and receipts.

## Prediction and authority

The ordinary class processor selects the only fixture class. `%Clean` calls `$system.OBJ.Delete`, stores the returned status in `tDelSC`, prints a warning on error, then continues to `StudioDocument.Module.Delete`. Consequently, a reachable provider refusal that leaves the class would be reported as a successful uninstall while metadata disappears, provided all other operations succeed. Whether the chosen dictionary lock produces that refusal on IRIS 2026.1 is unestablished until calibration.

The fixture is an ordinary module with one class returning 73. `Iter11.DeleteProbe.Holder` requests an exclusive `%Dictionary.ClassDefinition.%LockId` for that class in a JOB process. It publishes acquired/release/done markers under a unique run token and has a 120-second lease. The main process waits at most ten seconds for readiness, records the lock premise, and tries direct `OBJ.Delete`. It runs the locked CLI uninstall only after an error, retained class definition and still-held lock were all observed. A failed premise is a retained miss, not a product defect.

After releasing the holder, another JOB process reads source/compiled existence and attempts the method call. The probe then reloads the ordinary fixture and uninstalls without contention as the positive neighbor. Only these run's JOBs and globals are used; no product or provider body is patched. Main waits are bounded, exceptions and holder lease expiry have separate observations, and the direct deletion status/error log are preserved in raw output.

## Commands for the parent native session

```
do $system.OBJ.Load("/tmp/iter11/DeleteProbe.cls","ck")
do ##class(Iter11.DeleteProbe).Run()
```

The complete `native` directory must be copied to `/tmp/iter11` in a fresh product-pinned IRIS container before that session. Do not install a model oracle that requires the locked-delete outcome before seeing this calibration.

Always examine `del-load`, `del-holder-ready`, `del-lock-request-ok`, `del-cal-lock-held-before`, `del-refusal-calibrated`, `del-locked-uninstall-ran`, `del-holder-done`, `del-unlock-ok`, `del-lease-expired`, `del-probe-exception`, the separate observer results, and the valid reload/uninstall results. When calibration succeeds, also examine the direct status/source/compiled fields, lock held after both attempts, locked uninstall status, and final module/manifest/source/compiled fields. Never let an absent field in a skipped branch stand in for a passing observation.

## Exact roles

- Dispatch/selection: `Storage.Module::{Uninstall,ExecutePhases}`, `ResourceReference::{ResolveChildren,GetChildren}`. The fixture uses the ordinary class processor and needs no custom hook.
- Provider call and warning consumer: `Lifecycle.Base::%Clean`, lines 387–390 at the product pin. Projection classes are deferred to pass two, but the fixture is an ordinary class and is visited in pass one.
- Final metadata writer: `StudioDocument.Module::Delete`, lines 205–227. Its successful status replaces the main `tSC` before `%Clean` returns.
- External lock owner: `%Dictionary.ClassDefinition::{%LockId,%UnlockId}`; external deletion provider: `%SYSTEM.OBJ::Delete`. These engine APIs are not part of the 1,216-member IPM production census and are not direct Verus proofs.

The formal boundary must represent provider status and physical effect separately. It may specialize a lock-conflict outcome only after this engine's native behavior establishes it. A generic error does not entail class survival, and an error from a projection hook is not assumed to veto deletion.

## Alias and alternate-fixture audit

Inherited `nc-unin-env-obj-delete-default` and `-e-flag` are successful `-d`/`-de` deletion checks about retained persistent data. They support P:F-UNIN-2 and trust boundary U10; they do not establish refused class deletion. C:D27 concerns the earlier child-resolution returned status, so a later deletion-provider refusal is a distinct trigger, even though both can misreport cleanup.

The local `StudioDocument.Projection` supplies a supported three-argument `RemoveProjection(classname, ByRef parameters, recompile) As %Status` override. However, official current documentation says projection errors are reported without affecting compilation. A returned projection error therefore is not accepted as a deletion-veto premise. Its native calibration could be retained as a separate miss if desired.

Current official API leads, not pinned-engine guarantees: [ClassDefinition lock methods](https://docs.intersystems.com/irislatest/csp/documatic/%25CSP.Documatic.cls?CLASSNAME=%25Dictionary.ClassDefinition&LIBRARY=%25SYS), [OBJ.Delete](https://docs.intersystems.com/irislatest/csp/documatic/%25CSP.Documatic.cls?CLASSNAME=%25SYSTEM.OBJ&LIBRARY=%25SYS), and [AbstractProjection](https://docs.intersystems.com/irislatest/csp/documatic/%25CSP.Documatic.cls?CLASSNAME=%25Projection.AbstractProjection&LIBRARY=%25SYS). An attempted version-specific documentation URL was unavailable. The runtime dictionary and observed statuses must settle this provider boundary.
