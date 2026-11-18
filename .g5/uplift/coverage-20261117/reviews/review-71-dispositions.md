# Review-71 dispositions (master)

Reviewer session `8aac6775-a98e-453a-9a7c-a7ef938eab3f`, read-only, inspected commit `69e06436`. Cost $24.79. Raw findings: [review-71.md](review-71.md). Review round counter for this run: 1.

| Finding | Disposition | Assigned |
| --- | --- | --- |
| A1 full credited to constant/table/input deciding values (8/45 sampled; more in part B) | Accepted. New census rule 4a: a rule whose deciding value is a constant, a per-kind table or an input counts as taking the decision as input, so partial. Re-audit every full grade against it; regrade the named obligations. Coverage 64.78% is withdrawn as a headline until the re-audit lands. | iteration-72 |
| A2 grades too strict (build.publish#2, update#2, history.actions#2, lifecycle.custom#2) | Accepted after the builder opens the cited declarations. history.actions#2's refusal-before-row behaviour becomes a refuted obligation. | iteration-72 |
| A3 ontology contradicted (lifecycle.module-action update dispatch; namespace.enable B9; history.committed.marking#1 for uninstall; inspect B1 -versions; settings T3 HistoryRetain) | Accepted as ontology repairs, with the census regrade. | iteration-72 regrades; text repairs in the ontology repair batch |
| B completeness (merged obligations, missed units, wrong aliases in 10 blocks) | Accepted: split, re-alias and add missing units as listed. | iteration-72 |
| C1 generate.resources#2 full -> partial (UnitTest Name/Phase only native) | Accepted; model the UnitTest entry fields. | iteration-73 |
| C2 general theorems do not pin key/test exclusion/extension filter | Accepted: add general theorems (or scope the grades to instances explicitly). | iteration-73 |
| C3 thin flip controls (3/59, none on C:D73 load fields); collapse key order hand-written | Accepted: add flips on the C:D73 load fields; derive or bind the ordering. | iteration-73 |
| C4 C:D72 severity Medium -> High; trigger wording (same group name in two directories) | Accepted: the severity policy rates incorrect results High and the trigger is reproduced end to end. Reword finding and ontology bullet. | iteration-73 |
| C5 C:D73 severity Low -> Medium; `generate -export` -> `load` trigger needs a receipt | Accepted: Medium (visible misbehaviour). Add the export->load native case. | iteration-73 |
| C6 readability of devtools.generate.resources; module.xml backup rename not stated | Accepted. | iteration-73 |
| D1 `<m> activate configure` runs Configure twice | Source-only lead; needs native confirmation before ontology. | queue (discovery) |
| D2 normal-mode parent install replaces a developer-mode dependency and clears DeveloperMode | Source-only lead; likely refutes dev-mode.overwrite-guard B3. | queue (discovery, high) |
| D3 module-version re-imports the whole module record from the .ZPM document | Source-only lead. | queue (discovery) |
| D4 enable -community partial failure reported as success | Same as A3 namespace.enable B9. | ontology repair + queue #15 |
| D gaps 1-5 | Merged into QUEUE (rollback scope as writers, parameter cycle guard, dependency failure after commit, export->load, Committed for uninstall/update). | queue |
