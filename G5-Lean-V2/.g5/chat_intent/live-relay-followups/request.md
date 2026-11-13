# Owner request

2026-11-02, after [PR #973](https://github.com/G5Labs/G5-Lean/pull/973) (#966) merged, the
owner asked what follow-ups remained, chose three of the five reported, asked for their
effect and proposed solution, and then: "no need to file issue, just fix them all in this
workspace and file a new PR."

## Human instructions

- Fix all three in this workspace and file one new PR; do not file issues.
- The three, as the owner selected them from the report:
  1. Guest calls pin a pooled database connection for their whole round trip.
  2. One membership query per delivered WebSocket message.
  3. The chat panel re-renders every held row per commit.

## Interpretation

- The owner read the proposed solutions before choosing: for 1, run the authority,
  incarnation and lease checks, commit, then call the guest, and let the stop path drain
  in-flight calls through an in-process counter; for 2, check membership once per bounded
  batch (or once per second), with a denial still discarding the batch and closing the
  socket; for 3, memoize the projection and the turn so only the changed turn renders,
  then window the transcript. The [design](../../pr_ontology_diff_design/live-relay-followups/ontology-plan.md)
  keeps those shapes and records where measurement, not the proposal, decides (whether
  unmounting offscreen turns is needed once per-turn memoization is in place).
- "Fix them all" is read as complete implementations with tests and ontology, under the
  same ship-readiness rules as #973 (checks green, bot findings answered, no conflicts).
