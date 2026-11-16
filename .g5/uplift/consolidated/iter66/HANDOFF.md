# Iteration 66: the `document` lifecycle phase

Batch 15, a final convergence test of the strongest remaining unbound candidate, `Lifecycle/Base.cls::%Document` (105 lines). Product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter66/docphase.rs), [cases and controls](../../../Verus_verification/consolidated/iter66/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json).

Exposure: the parent thread had read the first 30 lines of `%Document` before the predictions were written; the ledger records this.

| Case | Observed | Owner/help predicted | Source predicted |
|---|---|---|---|
| c1 `m66 document` (dependency d66) | d66's Document hook runs before m66's; two START lines (m66 printed first) | API docs regenerated | dependency first |
| c2 `-DDocument.Recurse=0` | only m66 | unspecified | only m66 |
| c3 `n66 document` (no classes, no hooks) | succeeds, nothing happens | docs regenerated | nothing |
| c4 `f66 document`, dependency hook returns an error | fails, and the dependency hook's global write is rolled back | unspecified | fails and rolls back |
| c5 d66 uninstalled, no repository | fails: "Could not find satisfactory version of d66" | unspecified | resolution failure |
| c6 d66 uninstalled, filesystem repository offers it | fails: "required module 'd66' is not installed"; d66 stays uninstalled | unspecified | refusal |

No IPM code acts on the Document phase except `%Document`, which only recurses into dependencies inside a transaction. The phase therefore generates no API documentation unless a module adds Document-phase hooks. This contradicts the module-action help ("document: regenerates the API documentation for the module"). It is the only new discrepancy: Low, documentation drift.

Evidence: `docphase.rs` 2 verified, 0 errors. Five controls were each rejected: `recurse-ignored`, `no-rollback`, `dependent-first`, `missing-dep-documented` and `phase-writes-own-mark`. [Native receipt](native/20261116T184640249526Z/observations.json): 63 fields, all gated, and 42 replayed in Verus. Flips of `c4.hook.e66`, `c2.hook.d66` and `c6.notinstalled` were each rejected. Command: `python3 .g5/Verus_verification/consolidated/iter66/run.py` (13 s).

Reach: `Lifecycle/Base.cls::%Document` is newly reached. `ExecutePhases` and `InvokeReference.OnAfterPhase` gain this role.

Limits: one-level dependency fixture. The depth-ordered traversal for deeper graphs is source-read only. Deleting a dependency's record directly, without the uninstall command, was not tried.

## Proposed owner text

New statement in `.g5/ontology/concepts/lifecycle.g5.md`, after `lifecycle.chain`:

```
<a id="lifecycle.document"></a>
[D · lifecycle.document]

The Document phase runs Document-phase hooks for the module and, unless `-DDocument.Recurse=0` is given, first for each installed dependency, all in one transaction. A failing hook fails the command and rolls back what the hooks wrote, and a dependency that is not installed stops the command before any hook runs.

- As built, IPM itself generates no documentation in this phase, although the module-action help says `document` regenerates the module's API documentation (defect C:D-next).
```

Metadata: `agent`, `user-facing-behavior`, Expands [Lifecycle](#lifecycle). Sources: `Lifecycle/Base.cls::%Document`; `Main.cls` module-action help. Model: [docphase.rs](../../Verus_verification/consolidated/iter66/docphase.rs) `document`, `neighbors`, `phase_generates_nothing` ([handoff](../../uplift/consolidated/iter66/HANDOFF.md)). Add a Findings line to the entry below.

C register entry, **Low · Decision: Open**: "`document` regenerates no API documentation unless a module adds its own Document hooks".
- Found by: source search (no consumer of the Document phase besides `%Document`), then the iteration66 model `phase_generates_nothing` with control `phase-writes-own-mark`.
- Reproduced by: E2E reproduced, receipt `iter66/native/20261116T184640249526Z`. `n66 document` succeeds and does nothing (c3), and `m66 document` only runs the modules' own Document hooks, dependency first (c1). Passing neighbour: the hooks themselves run in the documented order and roll back on failure (c1, c4).
- Repair, unchecked: either generate documentation (for example a class-reference or DocBook build for the module's classes) or reword the help to "runs Document-phase hooks for the module and its dependencies".
