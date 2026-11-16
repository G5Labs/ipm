# Iteration 30: phase selection to executed phase work

Product `571e04a1`, unchanged. Linux x86-64 continuation of the iteration28 foundation. [Model](../../../Verus_verification/consolidated/iter30/phases.rs), [cases and controls](../../../Verus_verification/consolidated/iter30/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json), owners [lifecycle.chain](../../../ontology/concepts/lifecycle.g5.md#lifecycle.chain), [lifecycle.only](../../../ontology/concepts/lifecycle.g5.md#lifecycle.only), [lifecycle.custom](../../../ontology/concepts/lifecycle.g5.md#lifecycle.custom) and the new Q [lifecycle.phase-list-policy](../../../ontology/concepts/lifecycle.g5.md#lifecycle.phase-list-policy).

## Gap and why it was chosen

Iteration6's phase contract (`workflows.rs` `selected_module`, `custom_hook`) took the resolved phase sequence as a boundary input. The producer of that sequence (`GetCompletePhases`, `MatchSinglePhase`, `RunOnePhase`, `%DispatchMethod`) had no binding, so no formal claim distinguished which phases a module action runs.

## Predictions, written before any native run

The source bodies were read before the predictions, so they are source-informed, not blind. The ledger keeps both the ontology and the source prediction for each case.

| Case | Command | Ontology predicted | Source predicted | Observed |
|---|---|---|---|---|
| c1 | `phasecase compile -only` | Compile only | same | Compile only, old value kept |
| c2 | `phasecase compile` | chain, edit loaded | same | Initialize, Reload, Validate, Compile, value 12 |
| c3 | `phasecase compile activate -only` | Compile, Activate | fails, no work | fails `<METHOD DOES NOT EXIST> *%Compile,Activate`, no hooks |
| c4 | `phasecase reload mycustom` | custom runs | custom dropped | Initialize, Reload, custom hook 0, success |
| c5 | `phasecase MYCUSTOM` | runs | runs | custom hook 1 |
| c6 | `phasecase compile bogus` | unspecified | bogus dropped | compile chain, success |
| c7 | `phasecase bogus` | unspecified | fails | fails |
| c8 | `phasecase activate initialize` | fixed order | same | Initialize..Activate, nested Configure |
| c9 | `compile phasecase -only` (RunOnePhase) | Compile only | same | Compile only |
| c10 | `phasecase mycustom compile -only` | both | fails | fails `*%Mycustom,Compile` |
| c11 | `phasecase activate -only` | Activate only | same | Activate with nested Configure, no Compile |

One prediction missed during setup: the first exploratory run loaded the fixture without `-dev`, so module-action reload was a no-op and every value stayed 1. This is the already-owned [reload root rule](../../../ontology/features/load.g5.md) (stored root only in developer mode). The fixture now loads with `-dev`, and both exploratory logs are kept under `attempts/`.

## Evidence

- Formal: `phases.rs` 12 verified, 0 errors. Seven semantic controls were each rejected by an assertion: `only-runs-each-name`, `custom-beside-standard-runs`, `only-runs-chain`, `activate-chain-omits-compile`, `unknown-alone-is-noop`, `reload-keeps-namespace-source` and `activate-has-no-nested-configure`. The first full run was aborted because a control using `Seq::contains` did not finish under `by (compute)`. It is kept in `attempts/proofs-run1-aborted` and was replaced by an indexed mutation.
- Native: [receipt](native/20261116T170433591502Z/observations.json), image `ipmcont20261116-iris:571e04a1-fresh` (`sha256:7da4480b…`), network none, source bytes matched the pin. 276 fields, all equal to pre-run expected gates.
- Replay: 243 fields asserted against model expressions in Verus ([receipt](replay/receipt.json)). Three focused flips were each rejected.
- Commands from the repository root: `python3 .g5/Verus_verification/consolidated/iter30/run.py` (proofs, controls, fresh native run, replay) and `python3 .g5/uplift/consolidated/iter30/project_map.py --check`.

## Findings

- New defect [C:D34](../BUGS.md#c-d34): `-only` with two or more names fails before any phase runs. E2E reproduced.
- New defect [C:D35](../BUGS.md#c-d35): custom and unknown names beside a standard phase are silently dropped. E2E reproduced. The intended order and refusal rule is the new question C:Q24.
- These are discoveries from this search, not disclosed repairs.

## Reach

Project map [iter30](project-map.json): 411/1216 resolved Verus locator members (iteration28: 407, none lost). Newly reached: `Lifecycle/Base.cls::{GetCompletePhases, MatchSinglePhase, %DispatchMethod}` and `Main.cls::RunOnePhase`. These already-linked members gained an active role in this workflow: `ModuleAction`, `GetCompletePhasesForOne`, `ExecutePhases`, `GetCustomPhases`, `Lifecycle/Module.cls::%Activate` and `InvokeReference.OnAfterPhase`. That gives 4 new plus 6 strengthened, 10 distinct.

## Limits

Tokens abstract ASCII command words. `Register`, `*` and an empty action list are outside the modelled domain. Resource processors, source-control hooks and lifecycle bodies other than the effects named in the bindings are not modelled. The runtime METHOD DOES NOT EXIST raise is a native-qualified premise. No ObjectScript body is verified directly.
