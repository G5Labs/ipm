# Iteration 31: installed modules constrain dependency and version choice

Lane of batch 1 (2026-11-16 Linux run). Owner: [deps.installed](../../../ontology/features/dependencies.g5.md#deps.installed), which had no formal link. [Model](../../../Verus_verification/consolidated/iter31/installed.rs), [cases](../../../Verus_verification/consolidated/iter31/cases.json), [bindings](bindings.json), [ledger](working-ledger.json), [proposed owner/register text](findings-proposal.md). Not sealed and not committed; the main thread integrates the results.

## Prediction and exposure

Predictions were written before any native run, in the [ledger](working-ledger.json). They are source-informed: I read GetRequiredVersionExpression, LoadModuleReference, ProcessSingleDependencyIterative, VersionRequirements and Main.Install first. The product test `BuildDependencyGraph.cls` already covers conflicts, the downgrade bound and co-updating exclusion at the API level. It does not cover the CLI choice, unversioned or directory contrasts below.

## Fixture and observations

B (`i31b`) is offered at 1.5.0, 1.7.0 and 2.0.0 in a filesystem repository. A (`i31a`) requires `>=1.0.0 <2.0.0` on B. P (`i31p`) requires `>=1.6.0`. A loose copy of B 2.0.0 sits outside the repository. Readers are the command status, the error text, B's stored VersionString, the compiled `Iter31B.Info.Version()` and the presence of A and P.

| Case | Observed | Ontology prediction | Source prediction |
|---|---|---|---|
| S1: B 1.5.0 installed, no A, `install i31p` | B 2.0.0 | 2.0.0 | 2.0.0 |
| S2: B 1.5.0 and A installed, `install i31p` | B 1.7.0 | 1.7.0 or failure | 1.7.0 |
| S3: `install i31b 2.0.0` | refused, B stays 1.7.0, message lists `i31a: >=1.0.0 <2.0.0; i31p: >=1.6.0` | refused | refused |
| S4: `install i31b` (unversioned) | refused, 2.0.0 chosen, B stays 1.7.0 | not stated | refused |
| S5: `install i31b 1.7.0` | ok | ok | ok |
| S6: `load` of the B 2.0.0 directory, A and P installed | ok, B is 2.0.0 while A requires `<2.0.0` | not stated | ok (no check) |
| S7: uninstall A, then `install i31b 2.0.0` | ok, B 2.0.0 | ok | ok |

Final receipt: `native/20261116T170410450915Z` (107 fields, all gated, image `sha256:7da4480b…`, source pin with no mismatches). The earlier full pass `20261116T170334434989Z` is kept. Its flip controls fell on trivial fields and are superseded (`attempts/run1-trivial-flips/`). The exploratory run `attempts/explore1.log` is not counted, and `native_expected` was copied from it after every value matched the predictions.

## Formal result

`installed.rs` verifies 4 proof units with 0 errors. Six semantic controls are rejected:

- resolution ignores installed requirements
- an unversioned install filters by installed requirements
- a directory load checks requirements
- the reference check ignores installed requirements
- the search lists lowest first (a provider-premise control)
- any installed B is kept and the downgrade floor is dropped

The replay covers 106 fields, and the opposite values of s2-p.b, s4-bany.ok and s6-load20.b are rejected. Command: `python3 .g5/Verus_verification/consolidated/iter31/run.py` from the repository root (21 s). There are no direct ObjectScript-body proofs.

## Bindings

Newly reached: `Utils/Module.cls::GetRequiredVersionExpression` and `Storage/Module.cls::VersionRequirements`. Already-linked members gaining this active role: `Storage/Module.cls::ProcessSingleDependencyIterative`, `Main.cls::Install`, `Utils/Module.cls::LoadNewModule`, and `Utils/Module.cls::LoadModuleReference` as the refusing reader.

## Limits

- The ranges use full x.y.z release versions as naturals. SemanticVersionExpression And and IsSatisfiedBy are provider premises, and partial versions (C:D6), prereleases and builds are excluded.
- Graph-module exclusion is represented but not distinguished, because the parent was never installed beforehand.
- Not assessed: co-updating exclusion, PermitDowngrade, snapshots, deeper graphs, `-prompt` and developer-mode directory loads.

## Discrepancies for the owner

- **S4.** An unversioned install is refused when the latest offer breaks an installed requirement, even though an older satisfying offer exists, here the installed version itself. This combines `install` ("latest available") with deps.installed. It is a policy question, not a confirmed defect.
- **S6.** `load <directory>` installs a version that installed modules' declared requirements exclude, with no warning. deps.installed covers dependency versions only. The rule for directly loaded modules is unstated, so this is a policy question with an as-built fact.
