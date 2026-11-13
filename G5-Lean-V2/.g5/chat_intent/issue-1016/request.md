# Issue #1016: GitHub API budget

## Human instructions (owner, 2026-10-31)

1. "Work on G5Labs/G5-Lean#1016: Stop 15-second GitHub API retries for completed provider PRs."
2. "Start with an audit of how often we send API calls to github. Then focus on the highest impact areas to drive reductions. Consider if we should start using webhooks as a mechanism of reducing polling calls."
3. Asked which of the audit's tiers this PR implements, the owner chose **"Tier 1, all four"**:
   - persist completed provider PRs (#1016);
   - reorder or back off merge when ready;
   - bound and back off `g5d pr checks --watch`, plus its guidance and the UI poll;
   - single-read polling-only check reads.

   Separate issues cover the shared check snapshot cache, outbound-call telemetry and webhooks.

## Interpretation (agent)

- The audit (workspace Project Doc `docs/3fe733bd-c230-4d78-96e1-bdc05d04a06f/github-api-audit.md`; its findings are summarized in the [plan](../../pr_ontology_diff_design/issue-1016/ontology-plan.md)) ranks check watching and the merge-when-ready pass above the completed-PR loop. Each `checks=true` read costs about 14 GitHub GETs.
- Webhooks are recommended after Tiers 1–2, not instead of them. This PR adds no webhook route.
- "Single-read for polling-only reads" is implemented without a caller flag. Only a read whose result would settle repeats its observation; that covers the merge gate too. A read that still finds work pending is reported as observed. This keeps the merge gate's confirmed observation.
