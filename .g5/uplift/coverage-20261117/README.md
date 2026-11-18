# IPM ontology obligation coverage uplift (2026-11-17)

Unit `g5-ontology-coverage-20261117-ipm`. Product `intersystems/ipm` `571e04a19aba9f9b4bc0ea23fbe09182038f4e55` (unchanged).
Starting foundation `1510536d6372760dbeb8b6127f8e8d417a8c1dae` (the 2026-11-16 continuation, converged on member reach, plus imported ontology refinements).
Iterations of this run start at 68. Their directories are `iterNN/` here; `.g5/Verus_verification/consolidated/iter68/` is the earlier, unrelated ORAS admission work (#281) and is not this run's iteration 68. This run's formal artifacts live under `.g5/Verus_verification/coverage-20261117/iterNN/`.

The objective is to close consequential gaps in formal modeling of the ontology, measured as obligations, not members.

## Census rules

The census lives in [census/](census/). `census.py inventory` lists every canonical `I`/`D` block, its body hash and its units (paragraphs `P<n>`, list items `B<n>`, table rows `T<n>`). Records in `census/blocks/<page>.json` account for every unit of every assessed block. `census.py count` validates records and writes `counts.json` and `COUNTS.md`; `census.py selftest` runs the seeded controls.

1. **Read the block itself.** Read its lead, bullets, table rows and exceptions. Split a claim when one outcome can fail while another holds; keep a condition with the outcome it qualifies. Every unit maps to obligation IDs or is `excluded: <reason>` (rationale, definition, navigation, undecided alternative, pointer to another owner).
2. **Restatements count once.** A unit that restates a rule owned elsewhere is an obligation with `alias_of` the owner's obligation, or is excluded with the owner named. If the owner is not yet enumerated, record the alias target as the owner block ID in the reason.
3. **Working assumptions and as-built statements are assessable** within their declared scope. "As built, X (defect C:Dnn)" is an obligation about current behavior; a model that represents the as-built behavior and shows the intended promise fails is `check: refuted` for the intended promise or `proved` for the as-built claim. Record which.
4. **Grades.**
   - `full`: an actual formal rule (Verus spec/theorem, or a Verus explorer run with recorded bounds) distinguishes the obligation's required cases through its stated observation, directly or by documented composition. `check` is `proved`, `bounded` or `refuted` with a receipt in `evidence`.
   - `partial`: a formal rule exists but misses a case, condition, outcome, observation or producer the obligation states, or takes the decision as an input. Name the missing distinction in `gap`.
   - `absent`: no formal rule distinguishes it. Native observations, Python twins, tests, inspection, model labels or code locators alone leave it absent (record them in `basis`/`evidence` anyway).
   - `unassessed`: not yet judged. Prefer a judgment.
5. **Reuse grades only while they apply.** Use ontology metadata model links, `.g5/Verus_verification/coverage-map.md`, the consolidated project map (`python3 .g5/uplift/consolidated/iter67/project_map.py --formal F | --member M`), iteration handoffs and receipts. Open the cited declaration and confirm it states the property and both neighbours. A model mentioned in metadata whose property does not cover the clause does not earn credit. Check models even when a block has no registered link.
6. **Parents.** Record what the parent promises beyond its children. Credit an inherited obligation only with `composed: [child obligation ids]` and an entailment sentence; otherwise keep it open. A pure heading or restatement is `structural` with the owners named in `reason`.
7. **Design checks.** `check` is the strongest current check over the obligation's full stated scope: `proved` (Verus verified, receipt), `bounded` (finite exploration complete within recorded bounds), `refuted` (modeled counterexample against the promise), `unchecked` (model exists, no current run), `none`.

## Layout

| Path | Contents |
| --- | --- |
| `census/` | Census tool, inventory, block records, generated counts |
| `iterNN/` | Assignment, handoff, ledger and bindings of each iteration of this run |
| `QUEUE.md` | Product-wide queue of gaps and leads, maintained by the master |
| `HANDOFF.md` | Final handoff of this run |
