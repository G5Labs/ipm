# PairSoft experiments and harness takeaways

Updated 9 October 2026. The PairSoft comparisons examined whether ontology,
precise contracts, modeling and review improve application changes. These results
are separate from the [G5 design comparisons](design_comparison_study.md).

## Application findings

| Area | Result | Limit |
|---|---|---|
| Simple changes | Small comparisons showed no clear agent-quality gain from additional G5 workflow | Does not establish that ontology never helps a simple change; human readability remains useful |
| Complex Experiment E | Useful contract clarification, preservation of historical accounting/routing, and a cleaner deterministic core | Both implementations retained defects; ontology, skills, TLA/TLC and review were bundled |
| Design simplification | The core architecture preceded modeling and was similar in plain; callback repair also preceded its model extension | No demonstrated TLC-driven architectural simplification in E |
| Model sensitivity | TLC exposed an insensitive broken-refund control and syntax issues | Expected-failing controls are not discovery of an unknown production bug; correction-component and offboarding defects remained outside the models |
| Runtime evidence | Typed operations and explicit caller obligations made selected boundaries clearer | Model/code links do not establish refinement or complete application correctness |

The evaluated G5 delivery is preserved at [0573478](https://github.com/G5Labs/procurement_system/blob/05734788fe8ba0a85ef147a33eb127c31d0b0024/docs/pr_experiment_E_comparison.md);
the plain delivery was `24464c4`. These were experimental branches, not merged
application changes. The plain session also received G5 context, limiting isolation.

## Cost and interpretation

Experiment E used **1.47× uncached-input-plus-output tokens** and **1.23× elapsed time**
relative to plain. Ontology changed by only +202/−2 lines. The 90.7k-token evidence/
documentation category included 65.3k incoming-context tokens and 25.4k output;
repeated reads and evidence administration contributed substantially. It was not
90.7k tokens of ontology writing. Final expense TLC checks took 20.3 seconds;
authoring and an abandoned concurrent legacy run are separate costs.

More precise specification, executable state exploration and code correspondence
supply different evidence. Repeating the same rule in several forms is not itself
an improvement. These observations do not isolate TLC's marginal cost or benefit.

## Narrow harness results

| Probe | Observation | Decision |
|---|---|---|
| Five review cases varying explicit TLC authorization | Every case found the governing-contract defect; only the explicitly requested formal review ran TLC | Retain compact ontology review and separate review-specific model permission |
| Generic full-workflow request with an existing model | One fresh agent completed design, implementation, tests and review, with seven passing unittest methods, without model work | Explicitly state that generic full-workflow wording does not authorize incremental TLA/TLC |
| Reuse versus a separate delivery lifecycle | Fourteen TLC configurations supported both candidates under the supplied guarantees; removing exclusive claims broke reuse | Inspect actual existing-owner guarantees; keep an extra mechanism when a missing guarantee requires it |
| State-space growth | Instrumentation and larger workload bounds increased states 16× and 14.69× without changing safety | Treat growth as a diagnostic, not an architectural minimality score |
| Deadline and progress-journal workflow | Nine mechanism probes passed, but the user found the workflow counterintuitive | Reverted; no new aggregate execution budget or progress journal |
| Completed-check cache | Twenty-eight probes passed after provenance repairs, but the prototype remained complex and missed routine reuse | Not adopted |

The review cases shared one reviewer, and the design probe had no unguided control.
These results support narrow routing and mechanism decisions, not general agent
reliability, runtime correctness or cost savings. They do not demonstrate eliminating
Experiment G's reported 123-minute cost.

## Retained lessons

- Keep formal expansions beside their readable parent commitments, with stable IDs.
  Link executable evidence instead of repeating it throughout ontology.
- Distinguish implementation, tests, model checks and code proofs. State their scope
  and assumptions; use unassessed coverage when there is no defensible denominator.
- Keep native implementation and one evolving design. Separate review and incremental
  model work require explicit requests; initial uplift/downlift have their own policies.
- Verify relied-on facts through the actual commit boundary. Include unchanged
  retirement/recovery consumers of new obligations and original/cumulative bounds
  for partial corrections.
- Keep project-specific architecture and proof policy in project ontology. Additional
  generic instructions need demonstrated benefit, not merely sensible wording.

## Validation scope

Earlier installation checks recorded matching generated skill copies, three passing
`python3 -m unittest scripts/test_sync_ontology_skills.py` tests, six passing
`cargo test -p g5d-agent-turn harness::tests --lib` installation/update tests, and
valid formats for all five skills. That record used G5 baseline `2bf1c6a5` and
PairSoft baseline `657516f4` with the then-current harness changes.

Those checks establish instruction packaging and local consistency, not future
agent adherence, application correctness or deployed-VM behavior. The later routing
probes above provide separate, limited behavior evidence. Rejected runner/cache
experiments remain rejected; successful mechanism tests did not override the human's
workflow preference.
