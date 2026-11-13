# Completed planning-only study

All authorized work is complete: **27 fresh Sol/xhigh planners**, then **three fresh
Astra/xhigh masked scenario evaluations**, with root Astra supervision. There were no
planning or evaluation timeouts and no outcome-based reruns. No implementation, repair
experiment or production harness adoption ran during the experiment. Do not redispatch completed cases.

Read [RESULTS.md](RESULTS.md), [EVALUATION.md](EVALUATION.md) and the updated
[review-fix README](../README.md). Neither A nor B met the frozen adoption thresholds.
The next constraint-application comparison is a proposal only, not authorized execution.

After discussion, the practical recommendation was clarified to **adopt A provisionally**:
its observed coverage/effort tradeoff supports a small reversible improvement despite the
conservative screening failure. Original thresholds, labels and outcomes remain unchanged.
The user subsequently authorized incremental A adoption. On 2026-09-29 the exact frozen
checkpoint was inserted into the current maintained planning skill before ordinary design
review and its three mirrors were regenerated. Existing governance-format edits remain.
`adoption/` retains separate before/after snapshots and hashes. B and the proposed
constraint-application intervention were not adopted; no experiment was rerun.

## Preserved evidence

- Frozen source revision: `10443761bbf911b033992e7940985af8810fbbaa`.
- `manifest.json`: all 27 outcomes, native agent/attempt paths and input identities.
- `frozen/` and `packets/`: original/current/A/B inputs, old pilot navigation, source
  hashes, request briefs and pre-dispatch rubric. Do not modify or regenerate them.
- `runs/`: initial/final snapshots, full private packet copies, public tool histories and
  per-response numerical receipts. Reasoning and opaque encrypted blocks are omitted
  from public exports; native session originals remain unchanged.
- `evaluation/`: original independent results/reports, generated verification artifacts,
  model/usage receipts and public histories. `evaluation-input-hashes.json` records the
  masked inputs; all 2,340 files were unchanged after evaluation.
- `results-summary.json`, `effort-summary.json`, `overhead-effort.json`, quote/retrieval
  checks and `execution-audit.json`: arithmetic and supervision evidence.

Raw evidence is **local and ignored by Git**. It persists across account changes on this
computer. To move computers, copy this entire directory plus
`/Users/ziniuw/Desktop/projects/g5-study-planning-20260929`; Git alone is insufficient.
Never copy account credentials into the study. Concise reports and helper scripts are
tracked candidates; no commit or PR was requested.

## Execution deviations and permissions

The initial CLI endpoint rejected Sol before any planning. W01–W03 startup attempts are
retained; all actual planners used fresh native Sol/xhigh agents. The old `controller.py`
is deliberately disabled. [PROTOCOL-NOTES.md](PROTOCOL-NOTES.md) records the unrelated app
catalog refresh before W07, missing early host-load samples, observed access isolation
rather than an OS sandbox, and the bounded navigation audit.

The user explicitly authorized **one existing reset credit**, and it was redeemed
successfully. That authorization is fulfilled and cannot be reused for the remaining
credit. No account switch or worker interruption occurred. `reset-attempt.json` records
the successful redemption and fresh usage verification.

Helpers can recompute summaries from preserved evidence; they must not launch workers.
`prepare_evaluation.py` was run once and refuses to overwrite existing evaluator packets.
`native_capture.py` and `evaluation_capture.py` captured the completed sessions. Completed
watch loops exit automatically; do not start duplicate watchers for this finished study.
