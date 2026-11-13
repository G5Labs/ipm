# Planning follow-up: 27 source-free designs

2026-09-29. All 27 planners and three independent evaluations completed. No implementation or repairs.

## Result and decision

**Frozen screening result: neither A nor B passes. Subsequent practical decision: A incrementally adopted with user approval.** A improves capture and preview coverage, but one host plan leaves the scope of destruction/purge proof ambiguous. B has the highest aggregate count, but its improvements are below the agreed cross-scenario margin and its capture cost exceeds the ceiling. No concrete serious violation or initial-to-final regression was demonstrated.

The original “adopt neither” recommendation treated a conservative screening rule as an absolute bar to a useful reversible change. Following discussion of the results, the recommendation became provisional A adoption: it covers 91/99 selected obligation instances versus current guidance's 87/99, with 3.7% less total planner time, 10.7% fewer output tokens and 2.1% less uncached input. B covers 92/99 with 9.3% more time, 7.1% more output and 4.2% more uncached input than current, before navigation preparation/maintenance. Both show a favorable observed coverage signal; A offers the better practical effort tradeoff. B is defensible if the additional observed coverage is worth its cost. Small samples and scope-sensitive judgments leave the size and persistence of either benefit uncertain. This post-study decision does not alter the frozen thresholds, labels or failed screening outcomes.

The user subsequently authorized incremental adoption on 2026-09-29. The exact frozen A checkpoint was inserted before ordinary design review in `.agents/skills/ontology-planning/SKILL.md` and propagated to the Codex, Claude and local documentation-harness mirrors. Existing governance-format edits were retained. Adoption adds one retrieval pass in the existing plan, with no new ledger or repeated loop; it does not add B or the proposed constraint-application intervention. Local `adoption/` receipts preserve the before/after skill and hashes separately from the frozen experiment. No experiment was rerun.

Each cell below lists the three final plans in repetition order. A supported row means the saved plan plus ontology diff concretely specifies the selected obligation or justifies preserving it. These are evidence counts, not tests or implementation success probabilities.

| Scenario | Fixed denominator per plan | Current | A | B |
|---|---:|---|---|---|
| host | 11 | 11, 10, 11 | 10, 10, 11 | 11, 11, 11 |
| capture | 12 | 10, 8, 9 | 10, 10, 10 | 9, 10, 11 |
| preview | 10 | 9, 9, 10 | 10, 10, 10 | 10, 9, 10 |

| Frozen decision rule | A | B |
|---|---|---|
| Gain ≥2 supported instances in ≥2 scenarios; no loss in third | Fail: host −1, capture +3, preview +2 | Fail: host +1, capture +3, preview +1 |
| No worse serious-plan count or intervention-exclusive serious type | Pass in this evidence: zero demonstrated in all arms | Pass in this evidence: zero demonstrated in all arms |
| Each scenario mean time/output/uncached input ≤120% of current | Pass | Fail: capture time 131.0%, output 138.9%, uncached input 119.4% |

A and B add nine and eight supported instances during ordinary final design review, compared with four for current guidance. The initial totals were C83/A82/B84; final totals C87/A91/B92. This shows useful within-run refinement, not a causal proof that a checkpoint or link caused each gain.

**Retrieval is close to complete:** C98/A99/B94 of 99 row-source sets are observed, with the rest partial. Of 27 final incomplete/uncertain row instances, 25 have complete observed source retrieval. All nine capture plans leave part of the drain/callback obligation unapplied; other omissions include diagnostic redaction, deterministic failure retry and checked child publication. Two preview plans leave secret-safe shared fields unresolved, and one leaves passive polling activity unresolved. The host ambiguity concerns whether destruction proof covers separately retained storage. See [the evidence and scope-sensitive interpretation](EVALUATION.md).

The scopes are unequal and overlapping. Final new-request support is C23/A23/B24 out of 24; preservation support is C64/A68/B68 out of 75. Two incomplete new-request rows still specify safe blocking and separate Force but omit the governing repaired-explicit-retry contingency. They are not demonstrated failures to preserve work. Equal-scenario mean coverage is C88.4%, A92.4%, B93.3%; no statistical significance or general reliability inference is made.

**Next experiment proposed, not run:** use A as the provisional baseline and compare it with A plus a bounded constraint-application check on two unseen tasks, three repetitions per condition (12 planning-only runs). Require the planner to identify the retained owner and effects, trace an adverse condition at each changed boundary, and close or precisely bound the resulting gap. Pre-freeze scope so unchanged callback/consumer internals do not become implicit demands to redesign them. Keep the same cost, counterexample and independent evaluation controls. This tests incomplete application after successful reading; it does not add an index system or an implementation/repair loop. Preserve current guidance as the historical control.

## Comparison

Three scenarios × current guidance (C), one retrieval checkpoint (A), or direct ontology links (B) × three fresh Sol/xhigh repetitions. Each planner had fifteen minutes, including ordinary design review. All 27 completed before the cap and saved initial/final plans and proposed ontology changes. No completed run was repeated.

The source-free packets contained the same governing ontology meaning and authoring guidance. Prior findings and 436 implementation-assessment footer bullets were excluded. Requests, rubric and variants were frozen before dispatch. B was prepared without seeing the requests: 491 links added at 291 locations across 28 of 33 files; complete corpus size increased 16.11%. A separate Astra audit verified preserved canonical text, IDs and metadata; its semantic relevance review was selective.

[Approved protocol and adoption thresholds](../PLANNING-FOLLOWUP-PLAN.md). [Evaluation definitions](EVALUATION.md). [Execution deviations and isolation limits](PROTOCOL-NOTES.md).

## Planner effort

Means, with the observed three-run range in parentheses. Token columns are thousands. Output includes reasoning; do not add reasoning tokens again. These are usage receipts, not monetary charges.

| Scenario | Condition | Active minutes | Output tokens, thousands | Uncached input, thousands |
|---|---|---:|---:|---:|
| host | C | 9.0 (8.5–9.7) | 34.2 (31.9–36.1) | 101.2 (90.3–117.5) |
| host | A | 7.4 (6.6–8.4) | 27.3 (20.2–31.6) | 87.9 (76.4–95.5) |
| host | B | 10.1 (9.3–10.7) | 34.5 (28.1–38.0) | 101.8 (92.5–107.4) |
| capture | C | 5.9 (4.7–6.9) | 21.4 (18.4–24.1) | 76.7 (71.9–79.9) |
| capture | A | 6.4 (5.9–7.0) | 23.3 (20.1–24.9) | 85.2 (81.4–89.2) |
| capture | B | 7.7 (6.3–8.8) | 29.8 (24.1–35.8) | 91.5 (80.2–106.2) |
| preview | C | 7.0 (6.6–7.6) | 26.3 (24.5–28.7) | 83.9 (79.0–88.7) |
| preview | A | 7.2 (6.7–7.6) | 22.6 (19.1–25.5) | 83.2 (78.3–88.0) |
| preview | B | 6.1 (5.4–7.4) | 23.5 (20.4–26.3) | 79.4 (76.8–82.9) |

All planners together used **3h 20m 57s summed active time, 728,721 output tokens, 2,372,262 uncached input tokens and 77,892,992 cached input tokens**. Summed active time is not wall-clock duration with three concurrent workers. A satisfies the per-scenario 120% cost ceilings. B exceeds them on capture: time +31.1%, output +38.9%, uncached input +19.4%.

The capture difference is uneven. In repetitions 1/2/3, B took 7.96/6.28/8.83 minutes versus C at 6.86/6.05/4.70. The third pair drives much of the mean difference: B spent 5.40 minutes between initial/final snapshots versus C at 1.83. That interval includes review, revisions and reporting; it is not isolated reasoning time. Without the third pair, the time difference is about 10%, but all three remain in the preregistered result. This is evidence of observed cost variation, not proof that links intrinsically cause slower reasoning.

## Per-run effort

| Run | Scenario | Condition | Repetition | Active min | Output tokens | Uncached input | Initial-to-final min |
|---|---|---|---:|---:|---:|---:|---:|
| W01 | host | C | 1 | 8.83 | 31,948 | 117,468 | 4.97 |
| W02 | host | A | 1 | 7.29 | 30,139 | 91,774 | 3.63 |
| W03 | host | B | 1 | 10.40 | 28,117 | 92,457 | 4.47 |
| W04 | capture | C | 1 | 6.86 | 24,069 | 79,904 | 3.45 |
| W05 | capture | A | 1 | 6.38 | 24,871 | 84,901 | 3.75 |
| W06 | capture | B | 1 | 7.96 | 29,467 | 88,231 | 4.87 |
| W07 | preview | C | 1 | 6.91 | 24,471 | 83,996 | 2.99 |
| W08 | preview | A | 1 | 7.48 | 25,549 | 87,987 | 3.53 |
| W09 | preview | B | 1 | 5.56 | 23,813 | 76,786 | 3.11 |
| W10 | capture | A | 2 | 7.01 | 20,138 | 81,425 | 2.14 |
| W11 | capture | B | 2 | 6.28 | 24,051 | 80,169 | 2.63 |
| W12 | capture | C | 2 | 6.05 | 21,823 | 78,271 | 2.86 |
| W13 | preview | A | 2 | 6.69 | 19,088 | 78,255 | 3.88 |
| W14 | preview | B | 2 | 5.42 | 20,437 | 78,566 | 2.43 |
| W15 | preview | C | 2 | 7.60 | 28,679 | 88,707 | 4.50 |
| W16 | host | A | 2 | 6.64 | 20,238 | 76,414 | 3.42 |
| W17 | host | B | 2 | 9.31 | 37,425 | 105,361 | 4.85 |
| W18 | host | C | 2 | 8.52 | 34,587 | 90,302 | 4.41 |
| W19 | preview | B | 3 | 7.38 | 26,307 | 82,941 | 4.19 |
| W20 | preview | C | 3 | 6.61 | 25,842 | 79,005 | 3.97 |
| W21 | preview | A | 3 | 7.56 | 23,020 | 83,209 | 3.93 |
| W22 | host | B | 3 | 10.71 | 37,962 | 107,440 | 6.02 |
| W23 | host | C | 3 | 9.67 | 36,060 | 95,831 | 6.21 |
| W24 | host | A | 3 | 8.39 | 31,583 | 95,536 | 4.49 |
| W25 | capture | B | 3 | 8.83 | 35,778 | 106,221 | 5.40 |
| W26 | capture | C | 3 | 4.70 | 18,401 | 71,907 | 1.83 |
| W27 | capture | A | 3 | 5.91 | 24,858 | 89,198 | 3.34 |

## Preparation, supervision and evaluation effort

Navigation preparation and its independent audit are potential B adoption costs. Oracle construction, masked evaluation and root orchestration are study overhead. No arbitrary amortization over future uses is assumed. Preparation time is observed session elapsed time, including calibration waiting; evaluation time starts at each first timer action. Root wall time overlaps worker execution and is not added to these summed times.

| Astra activity | Time | Output tokens | Uncached input tokens |
|---|---:|---:|---:|
| B navigation preparation | 5m 15s | 14,390 | 112,499 |
| B navigation audit | 5m 28s | 13,524 | 96,238 |
| Frozen oracle and pre-dispatch calibration | 8m 57s | 19,783 | 57,110 |
| Three masked scenario evaluations | 26m 24s | 68,253 | 470,999 |

<!-- ROOT_EFFORT_START -->
Root Astra supervision/orchestration through **2026-09-29T16:44:43.952Z** used **134,900 output tokens and 494,833 uncached input tokens** (32,468,480 cached input). This includes setup, capture/reporting helpers, monitoring, reset handling, supervision and report writing after the explicit 27-run request. It excludes earlier study discussions and subsequent tool/final-response activity after this saved cutoff. Root wall time overlaps workers and is not an additional summed worker-time measurement.
<!-- ROOT_EFFORT_END -->

The evaluation and preparation agents together used 115,950 output tokens and 736,846 uncached input tokens. All output-token counts include reasoning as a subset. The initial failed CLI launches produced no planning output; their startup errors remain preserved.

## Evidence preservation

Every original packet, harness version, plan snapshot, public tool history, native numerical receipt and evaluation input hash is retained locally under this directory and the persistent private worker root. Raw material is ignored by Git; copying the entire directories preserves it across computers. Git alone preserves the concise reports and helpers. Native session originals remain unchanged. Public exports omit reasoning and opaque encrypted blocks.

The initial CLI model endpoint rejected Sol before planning; three startup failures are retained separately. All actual outcomes used native fresh Sol/xhigh agents. One user-authorized reset credit was redeemed during the study. No account switch occurred; the remaining credit is not authorized. All 1,242 frozen packet files still match their hashes. The final call audit covered 955 tool-call envelopes and found no flagged access outside packets; this is an observed-call audit, not an OS security guarantee.

## Design and ontology alignment

Final design coverage is 87/99 for current guidance, 91/99 for A and 92/99 for B; the corresponding equal-scenario means are 88.4%, 92.4% and 93.3%. This evaluates consistency of proposed plans and ontology changes with selected governing commitments. It does not establish that generated code would implement them. Neither variant meets the frozen adoption rule; A was subsequently adopted incrementally while treating the expected modest benefit as a hypothesis for further evaluation.
