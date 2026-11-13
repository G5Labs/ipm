# Experiment overview

Updated 9 October 2026. This index records the questions examined and their main
results. The [comparison study](design_comparison_study.md) contains the results
tables; [PairSoft takeaways](pairsoft-harness-experiment-takeaways.md) covers the
separate domain experiments and the resulting harness decisions.

## Completed comparisons

| Experiment | Question | Main result |
|---|---|---|
| PR #246: conversation recovery | Can native continuity and safe recovery use fewer storage and lifecycle mechanisms? | G5 without TLC chose current file storage and a manifest. Its smaller design survived implementation; both implementations retained defects. |
| PR #339: bounded Stop and fresh Start | Can saving fail without preventing shutdown or corrupting successor state? | Early no-TLC findings were stronger; repeated runs showed a practical tie or possible slight TLC improvement, with substantial variation within each method. |
| PR #186: Git authority and synchronization | Can workspace membership, Git state, provider effects and retries share a coherent design? | All six designs converged on the main architecture. TLC added useful recovery evidence, including a capture-deadlock refinement, without an overall winner. |
| PM-1: collaborative workspaces | Can teammates and independent assistants work concurrently with private credentials and safe contribution integration? | All three designs were credible. Existing Stop behavior, Project Docs identity and repeated partial integration exposed different gaps. |
| PM-2: persistent customer demos | Can published demos remain available independently of development workspaces? | Common architecture: independent hosting, immutable releases and isolated visitor state. Capture guarantees and control-state complexity differed. |
| PM-3: coordinated releases | Can several workspaces publish one reviewed release safely across retries and cancellation? | TLC produced a useful revision-owned publication design and cancellation receipt mechanism, conditional on provider capabilities. |
| Issues #289/#293: Project Docs and agent reviews | How should stable document roots, branch-local editing and generated approval reviews fit together? | Folder designs converged; review execution and publication differed. No overall treatment winner. |
| 8 October harness probes | Does another instruction improve capability checks, outcome-sensitive tests and reuse decisions? | Both conditions found the important issues. Consolidated overlapping guidance; did not adopt the additional paragraph. |
| 9 October harness probes | Do explicit structural-search or brevity clauses improve designs and human readability? | Twelve author runs did not justify adopting any tested clause. Some real design variations appeared without changed architectural guidance. |
| PairSoft comparisons and routing probes | Where do ontology, modeling, review and native implementation help? | Complex work exposed useful contract and integration findings, but did not isolate individual contributions or demonstrate fewer defects. Narrow routing changes were supported. |

The primary candidate suite is complete at the design-comparison level. Most
proposed product designs remain unimplemented. #246 additionally has a two-arm
implementation and requested-review comparison.

## What these results can answer

The comparisons distinguish architectural choices, preserved outcomes, remaining
defects, executed evidence, cost and human readability. A design is meaningfully
smaller when it removes an unnecessary owner, retained fact, lifecycle or coordination
obligation while preserving required behavior. More detail or more passing checks
alone does not establish a better architecture.

The evidence does not establish a general ranking of plain, G5 and G5 with TLC.
Most cases have one or two author observations per condition. Model assumptions,
conditional provider capabilities and incomplete production qualification remain
material limits. Results and those qualifications are summarized in the linked reports.
