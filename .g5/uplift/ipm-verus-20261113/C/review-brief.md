# Stage C reviewer brief

Shared brief for every independent reviewer in the uplift's review and repair loop. It follows `G5-Lean-V2/.agents/guidance/uplift-review.md` and `ontology-model-review.md`. The round counter lives in `../uplift-plan.md`.

## Scope

- Product: IPM at `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`.
- Ontology coverage: all of IPM at capability level.
- Formal and native scope: package loading (`load` and the shared `LoadModuleReference` / `LoadNewModule` / `LoadDependencies` path) and transaction history (`%IPM.General.AbstractHistory`, `HistoryTemp`, `History`, its writers and readers).
- Product repairs are not authorized. Defects are recorded with repair proposals.

## Order of work

1. Read the original sources first: CLI help in `src/cls/IPM/Main.cls` `XData Commands`, README, CHANGELOG, wiki (`../ipm-wiki`, rev `781e98fd`), the issues and PRs named in `sources.md` (read-only `gh api`), the code and tests.
2. Derive your own expected behaviors and outcome-changing cases for your domain before reading the artifacts' claims. Write them down in your review file.
3. Then compare them with the actual ontology bodies (`.g5/ontology/**`), the Verus models and bindings (`.g5/Verus_verification/crates/**`, `premises.md`, `cases/cases.json`), the native checks and raw evidence (`.g5/Verus_verification/native/**`, `results/**`) and the coverage maps.
4. Read the authors' reports (`A/*.md`, `B/*.md`) and earlier dispositions (`C/round-*/dispositions.md`) last.

## Questions every round must answer

- Coverage: which important source-supported behaviors are missing or only partly represented in the ontology and models? Look inside covered features for conditions that change the outcome. Challenge exclusions labeled structural, presentation or provisional.
- Fidelity: do statements, model transitions and claimed results reflect governing intent and the implementation? Trace consequential model actions to exact operations, guards, storage and commit boundaries.
- Try at least one false premise, one erroneously permitted step and one faulty implementation against the models and checks, and confirm that a valid outcome survives.
- Evidence: are receipts complete and current for the claims they support? Does each control detect its mutation? Is any "pass" really a timeout, missing marker, zero-test run or stale log?
- Findings and simplifications: is every material bug accounted for, with a repair proposal that names the obligation, affected writers and readers, behavior to preserve and a distinguishing check?

## Finding format

Write each material finding as:

```
### R<round>-<domain>-<nn>: <one-line title>
- Kind: missing intent | incorrect intent | model error | binding error | product defect | unresolved decision | missing or stale evidence
- Source or owner: <governing source and ontology ID>
- Trigger: <concrete input or execution>
- Discrepancy: <what the artifact says vs what the source or code does>
- Limits: <what you did not check>
- Proposed repair: <owning artifact and change>
```

## Exclusions

Do not read anything under `~/runs/`, other directories under `~/experiments/` besides this experiment and `~/experiments/bank-of-ziniu` (library source), `~/gcp-verus/`, `~/docs/`, or other experiment arms' outputs or evaluator material.
