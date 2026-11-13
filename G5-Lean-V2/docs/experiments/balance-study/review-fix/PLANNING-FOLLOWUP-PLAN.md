# Planning comparison: current guidance, A and B on three scenarios

2026-09-29. **Completed: all 27 planners and three independent scenario evaluations.**
Inputs and rubric were frozen before dispatch. [Results and costs](planning-followup-20260929/RESULTS.md).
No implementation, repair or A+B condition.
This broadens the first one-request pilot; it does not rerun its handoff scenario.

## Three different planning problems

The issues were read through authenticated GitHub access on 2026-09-29. Their recorded
gaps were checked against the current source; issue prose is background for preparation,
not an answer packet for planners. Exact worker briefs will be frozen before dispatch.

| Scenario and minimal worker goal | Why this tests a different retrieval path | Supervisor-only evaluation concerns |
|---|---|---|
| **Remove a busy worker host safely.** One ordinary Remove request should drain its work and finish removing the host, with visible progress and recovery after interruption. Keep explicit Force separate; a blocked ordinary drain must not silently discard work. [#155](https://github.com/G5Labs/G5-Lean/issues/155), candidate 5. | Infrastructure lifecycle across placement, workspace Stop, custody, durability and reconciliation. | Stop/flush ownership; exit proof; draining blocks new placement; replay; restart; deadlines and explicit loss authority; current host generation; empty/forced paths. |
| **Make privileged workspace Git capture safe.** Repository-controlled configuration must not execute privileged code or redirect transport secrets, while preserving the user's working state and supported capture/fork/Stop behavior. [#176](https://github.com/G5Labs/G5-Lean/issues/176), candidate 6. | Security and architecture across local Git execution, transport credentials, capture and recovery. | Execution identity versus credential authority; trusted transport configuration; working tree/index preservation; complete capture proof; cleanup/partial failure; existing guest trust boundary. |
| **Show preview state outside its dialog.** Workspace readers and owners should see truthful preview status and history on the workspace page; launching, cancelling and entering the live app keep their existing authority. [#156](https://github.com/G5Labs/G5-Lean/issues/156). | Product/read-model design across durable reading, live authority, task state and UI projection. | Process-running versus serving-ready; read-only readers versus live controls; historical status after Stop; attempt/incarnation freshness; owner transfer; consistent queue/page state; safe URL/credential exposure. |

Current source still has the busy-host 409 guard at `worker_hosts.rs:1122`, preview query
gating on `open && isOwner` at `PreviewControl.tsx:100` and reader exclusion at line 137.
The flush path uses `GitRuntime::provider_command`; that helper clears the environment
without setting a different identity. The security path was inspected, not exploited.
Recheck these starting facts and freeze the repository revision when execution is authorized.
If a chosen gap has meanwhile been implemented, disclose that before substituting a task.

These tasks deliberately span lifecycle, security and UI. We are not reusing the old
retry/fork handoff or the previously exercised stale-save/allocation/recovery tasks.
The preview brief does not copy the issue's suggested “only while VM running” query rule:
the planner must resolve durable history and current readiness from governing ontology.

## Conditions and controls

Run **3 scenarios × 3 conditions × 3 fresh repetitions = 27 Sol/xhigh planners**.
Each gets a fifteen-minute maximum for source-free ontology planning and ordinary final
design review. Each has the same request, ontology meaning, metadata exclusions and
authoring guidance. No code, tests, old findings, issue acceptance checklist, peer output,
supervisor hints or evaluator requirements enter planner inputs.

- **Current:** frozen current planning guidance and ontology navigation.
- **A:** the same single retrieval-challenge checkpoint tested in the pilot.
- **B:** current guidance plus direct canonical rule/exception/state links, without A.

Prepare B with the same general navigation rule, not a request-specific list of answers.
A fresh Astra preparer sees the frozen ontology and navigation rule, but not these briefs
or evaluation obligations. Apply the rule across the ontology corpus, with a bounded
preparation budget; log incomplete areas. A separate check verifies that edits only add
navigation, preserve IDs and canonical meaning, and do not insert missing requirements.
No revision of B after planner results arrive. Save the old pilot variant separately.

Astra freezes each request's required behaviors, material governing obligations and allowed
counterexamples before dispatch. Use roughly 8–12 independently meaningful obligations
per scenario; do not pad to twelve or count the same failure several times as separate
benefits. The exact denominator is fixed for each scenario before any planner starts.
Separate required preservation from legitimate new design choices. An explicit alternative
policy is labelled as an amendment; it is not falsely described as failed retrieval.
Whether an amendment satisfies the request is judged against this frozen rubric.

Run matched waves of three planners on the same scenario, one per condition. Rotate
condition/launch order across repetitions. Do not overlap repair/build jobs with these
planning waves. Record host load, all timings and cache/input/output receipts. Isolation
is separate private directories and fresh contexts, with observed access audited; do not
claim directories alone enforce filesystem secrecy.

## What we measure

Keep two distinct questions: **did the planner retrieve the material rule text?** and
**did the final proposed design apply it correctly?** Record supported, omitted,
contradicted, uncertain and explicit-amendment dispositions, with source quotes. Report
new-request coverage separately from retention of existing constraints. Do not treat
an unresolved implementation question as a design failure merely because code was withheld.

Preserve initial and final ontology/plan snapshots. Record the initial-save time, any
explicit A checkpoint, final-review interval and final delivery. Measure final-review
gains and regressions, plus tool activity and artifact size as descriptive diagnostics.
These traces help explain another fast or low-count result; they do not reveal private
reasoning or turn token count into a quality score.

Fresh Astra evaluation uses masked case IDs and frozen requirements, with the same
evidence standard for every plan. Histories can reveal variants, so masking is not perfect
blinding. Evaluate all 27 once; no selective reruns, test feedback or extra time.
Report all three scenarios separately and aggregate with equal scenario weight. Show
per-run counts and ranges; do not present nine correlated samples as population reliability.

## Decision rules to freeze before running

The first pilot's undefined word “new” will not be reused as a stopping rule. Run all
three scenarios; no outcome-based expansion or early winner selection.

For a provisional adoption recommendation, A or B must:

1. Add at least **two supported obligation instances across its three runs** versus
   current guidance in at least two scenarios, with no supported-count loss in the third.
2. Have **no more final plans with a serious violation than current guidance in any
   scenario**. Classify seriousness before decoding arms: authority escalation, data loss,
   secret exposure, or blocking an essential permitted user action. A serious failure
   type absent from all control plans for that scenario blocks adoption pending diagnosis.
   Define a type by actor, operation, condition and wrong outcome, not wording. Shared
   control failures are reported and counted; they are not intervention-exclusive failures.
3. Stay within **20% of control mean active time, output tokens and uncached input tokens**
   on each scenario. Report navigation preparation/maintenance separately; do not assume
   an arbitrary number of future uses to make it cheap.

Compare A and B directly using the same outcome/severity/cost tables. If their differences
are small or inconsistent, do not declare B superior; prefer the lower observed total
effort or retain current guidance. Any serious uncertainty must accompany the recommendation.
These practical thresholds select a candidate for supervised use; they do not establish
statistical significance or an implementation benefit. Code generation remains untested.

## Budget

- Hard planning allowance: **27 × 15 minutes = 6h 45m summed Sol time**. Three concurrent
  planners means nine waves, at most about 2h 15m of worker-wave elapsed time, plus setup
  and evaluation. This is not a guarantee about app scheduling or service latency.
- At the previous C/A/B means: approximately **4 summed Sol hours and 0.99 million output
  tokens**, plus about **2.73 million uncached input tokens**. Different scenarios can cost
  more; these are estimates, not caps or billing quotes.
- Bound Astra preparation/audit to thirty minutes and independent evaluation to fifteen
  minutes per scenario: **75 summed Astra minutes maximum**, plus root orchestration and
  concise reporting. Record actual cost, including failed preparation and adjudication.
- Preserve local raw evidence; update the existing review-fix README with a short scenario
  table and decision. No implementation, repair workers, services or review/fix loops.

## Quota interruption and continuation

Save each attempt and its effort continuously in `planning-followup-20260929/manifest.json`
and local raw evidence. Preserve completed runs without repeating them. Pause dispatch on
a quota/authentication failure. Retain interrupted attempts and their costs; only infrastructure
interruptions may restart from identical frozen inputs under a new attempt ID. Never restart
on the basis of quality. Record account/model/configuration boundaries and any mixed wave.
A timeout is an experimental result, not an infrastructure retry. Account changes require the
same requested models/configurations; reset credits are not automatically redeemed.
