# Planning stage 1 screening audit

Recommendation: **stop after request 1; do not automatically dispatch request 2.** B has a useful numerical signal, but the serious-restriction condition is not an unambiguous preregistered pass. Preserve the original assessment and aggregate. This is a conservative disposition under an underspecified gate, **not** a claim that the wording uniquely requires stopping or that B caused the restriction.

## Frozen result and gate

[PROTOCOL.md](PROTOCOL.md), lines 38–42, requires a reduction of at least two final unsupported obligations across three runs versus C, “with no new serious false restriction,” and time/output increases no greater than 20%. It does not define the reference point or unit for “new”: original governing behavior, initial-to-final within a run, or a semantic restriction type absent from C. The earlier [proposal](../RETRIEVAL-PROPOSALS.md) says “without introducing serious false restrictions”; it does not settle that ambiguity. No novelty taxonomy or incidence-based noninferiority rule was frozen.

| Arm | Supported /36 | Missed | Contradicted | Uncertain | Unsupported | Mean seconds | Mean output tokens |
|---|---:|---:|---:|---:|---:|---:|---:|
| C | 28 | 1 | 5 | 2 | 8 | 561.55 | 39,655.67 |
| A | 29 | 2 | 3 | 2 | 7 | 516.77 | 36,771.67 |
| B | 31 | 1 | 3 | 1 | 5 | 520.58 | 33,328.67 |
| AB | 27 | 3 | 3 | 3 | 9 | 475.07 | 33,649.00 |

B alone meets the benefit margin: +3 supported obligations (8 to 5 unsupported), with time at 92.70% and output at 84.05% of C. C is not at ceiling. Counts remain over the frozen 12 obligations × 3 runs; they are not reliability estimates.

## Source reconciliation

B2 is case-03; C3 is case-11. The inspected evaluation-packet artifacts match their frozen hashes; both original assessment files match the delivery receipt. Exact references below are also preserved in the assessment JSON.

- **B2 introduces an accidental contradiction during final review.** Initial-plan line 19 excludes viewers from redemption, which requires child-owner eligibility. Final-plan line 19 adds listing: an eligible member may “list/redeem” and “a viewer ... cannot.” Its final ontology nevertheless explicitly separates reader listing from eligible-owner redemption (final-changes.patch:94–96). The saved plan therefore denies an allowed read-only case. The frozen rubric says to read plan and ontology together and lets explicit contradictory behavior defeat a correct reference; selecting only the favorable ontology text would change the rubric. Durable evidence: B2 candidate patch (`deliveries/p1b2/plan/candidate.patch`), lines 94–96 and 352; initial plan (`deliveries/p1b2/plan/artifacts/initial-plan.md`), line 19. The independent supported→contradicted label is justified.
- **C3 already has the same denied case.** Its initial and final proposed security block requires child-owner eligibility “before listing or forking.” This is the same viewer-listing restriction in outcome, although located in the governing proposal rather than B2's conflicting plan summary. Durable evidence: C3 candidate patch (`deliveries/p1c3/plan/candidate.patch`):86–88; masked initial-changes.patch:86–88. It is not an intervention-exclusive restriction type. Both planners retrieved the read rule; this is an incorporation failure.
- **Deliberate policy amendments are distinct.** C1, C2, A1, B3 and AB2 intentionally replace current source-reader authority with separate handoff grants. They are not evidence of accidentally missing the read rule. The frozen oracle expressly scores intentional weakening as contradicted and records amendment status separately. B3's explicit substitution appears in masked case-10/final-changes.patch:190. Keep those original labels; proposing a different product policy is not inherently invalid outside this preservation experiment.

## Sensitivity and recommendation

| Reading of “no new serious false restriction” | B outcome | Screening consequence |
|---|---|---|
| No final restriction newly imposed on an existing allowed case | Fails: B2 remains; B3 also changes the frozen audience | Stop |
| No serious initial-to-final regression | Fails: B2 adds viewer-listing denial in final review | Stop |
| No additional semantic restriction type versus the control arm | Passes: viewer-listing denial exists in C3; independent-grant narrowing exists in C1/C2 | Advance all four arms |

The third reading is defensible as a **disclosed alternative interpretation**, including the aggregator's own comment about reconciling shared control defects. It is not established uniquely by preregistration. B has fewer final flagged runs than C (2/3 versus 3/3), but three independent runs cannot establish noninferiority or show that B did not cause a harmful restriction. Choosing a post-result type taxonomy to certify passage would overstate the protocol.

aggregate_plans.py (`aggregate_plans.py`):24,30 uses any nonempty restriction list, without filtering final status or comparing C. Thus it is a conservative alarm, not an exact implementation of every reasonable meaning of “new.” It even includes A3's resolved initial restriction. Filtering that resolved entry does **not** change this screen: A misses the benefit margin, while B2's final restriction remains. No script or evaluation was changed by this audit.

A further descriptive sensitivity explains the signal: C has four contradiction records from deliberate amendments (C1/C2); B has two (B3). Thus two of B's three-obligation advantage reflect fewer explicit policy amendments under the preservation rubric. If those records were analytically set aside, residual unsupported counts would be C=4 and B=3, a difference of one. This is **not** a replacement score, denominator, or permission to alter the frozen outcome.

Stop under the existing protocol and report B as promising but screening-ambiguous, with the conservative safety condition unmet. An advance would need to be openly described as a post-result adjudication adopting control-relative semantic novelty, not an unequivocal preregistered pass. Do not repair B, change the request, or rerun planners to obtain passage. Any future experiment should fix the restriction comparison unit and amendment treatment before outputs exist. Time comparisons also retain the recorded concurrency/host-load limitations; none of these planning results establishes coding gain.

Audit only: no private reasoning read, no application/ontology edits, no tests, and no new experiments. The original evaluation is unchanged. Machine-readable evidence and input hashes are in screen-audit.json (`screen-audit.json`).
