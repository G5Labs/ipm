---
name: optimize-ci-e2e-773
description: "CI E2E runtime work (issue #773, branch optimize-ci-e2e): what shipped, what the PR canaries must show, and what stays deferred."
metadata:
  node_type: memory
  type: project
  originSessionId: 15961965-e3c0-40a6-b3f9-a58776087e65
  modified: 2026-10-23T17:43:54.617Z
---

Branch `optimize-ci-e2e` implements the Project Doc `g5-ci-e2e-investigation.md`. On
2026-10-23 the owner said "just follow the doc", although all 24 open questions are
undecided. The plan is `.g5/pr_ontology_diff_design/optimize-ci-e2e/ontology-plan.md`;
the ontology is the "CI E2E runtime" section in `features/operations.g5.md`.

Shipped:
- duration-based whole-file shard planner, with a committed
  `infra/ci/playwright-file-durations.json`;
- the RV7 formal-review test skipped in the broad Rust lane, with a catalog-checked
  receipt-owned list;
- `g5-timings` phase annotations and shard summaries;
- RV9 terminal reuse;
- `expectAssistantReply` fail-fast;
- (the release/frontend-test overlap was canaried in build `c3dcf693` and reverted:
  CPU contention erased its gain).

PR #781 (opened 2026-10-23; Codex CLI review approved). Canaries: c3dcf693 green in
40.8 min (baseline 50.5–52.6); 73d21f25 failed only on the known transient
"materialize workspace repository vector" boot 503 plus 32–93 s capacity waits; e7475044
green on 76ed9edb8. CodeRabbit threads all answered and resolved; the Codex connector
never posted a review. The release overlap was reverted on canary evidence.

The canaries must show:
- shards near 22–25 min;
- RV7 passing once, in the receipt lane;
- no OOM in the resource-sampler;
- any `vm.capacity-wait`, now that all 4 review files run at once, measured against
  shard duration: a short wait is expected; a wait that erodes the shard gain is not.

Deferred per the doc: fixture reuse (Q12/Q13), the RV9 fixture command (Q14), the
image-key split (Q16, amends `ci.image-key`), more workers.

**How to apply:**
- Refresh the duration table from the `G5_SHARD_DURATIONS` log lines with
  `plan_playwright_shards.py durations`; the build-log reader cannot read the GCS
  artifacts.
- Six `run-ci-script-tests.py` suites fail in the VM for environmental reasons, the same
  as on base.

Related: [[g5-ci-cloudbuild-triage]], [[cloudbuild-log-reader-sa]].
