# Quantitative refinement review

The reviewed scope is eight new decisions, two amendments and two unresolved
questions approved after the quantitative audit. Ben’s immutable source, production
code and the harness are unchanged. The fixed 63-group inventory is retained as the
before-measurement; current dispositions live beside it rather than overwriting it.

| Review | What was checked or corrected | Remaining boundary |
| --- | --- | --- |
| Coordinator → workspace | Read ReviewRounds/Discard/VerdictSync, publication/suggestion service1510–1660/2210–2670, discard route3288–3691 and approval sync180–460. Required explicit SQL serialization scope, fixed reviewer/kind partition, native mutation premise and retry reentry semantics. | Composed child-edit matching and all publication/authority writers are not fully modeled. |
| Identity reviewer → platform | Independently inspected both model bodies, source bindings and witness/divergence traces. Corrected native check/rename source line references, permitted-stutter wording and excluded per-workspace badge grouping. | Home candidate producers, grouping/cursors and ordinary Git publication remain outside the checked slice. |
| Workspace reviewer → runtime | Inspected command/event/journal boundaries, deletion and ordered carrier processing. Required post-disconnect cache replay, actual post-reconnect retained-event commit and source-faithful Stop-response/event ordering. Final TerminalFact review corrected an unreachable retry witness and verified all nine focused cases. | Logical event progress does not prove physical VM death, deployed timing or final retention policy. The terminal comparison preserves a durable pending obligation after restart without inventing the lost receipt; reconciliation and spontaneous exits remain unfinished repair boundaries. |
| Runtime reviewer → identity | Independent review is recorded in the runtime refinement note after final source/model inspection. | Database uniqueness, deferred FK/key-lock behavior, cryptography and hypothetical physical child-deletion reachability remain explicit premises. |

Witness review also tightened Abort→Prepare→Commit for review-round retry;
interrupted staging and the same caller’s later commit for keyed creation; actual
waiting-caller replay; loss→configuration edit→replay; stale role-preflight refusal
by the membership constraint; and deletion freeing the subsequently removed owner.
A state containing both “failure occurred” and “success occurred” was not accepted
as evidence that recovery followed the failure. These were model/evidence corrections,
not application fixes.

The new source-bound gaps are ordinary keyed history-fork bypass, event-ledger
cleanup that stalls late replay acknowledgment, and ignored terminal-event
persistence failure after active custody is removed. Physical child deletion is a
schema-conditional limit, review-verdict authority and allowlist alternatives are
conditional policy comparisons, and invitation/native-file divergences retain known
gaps. See [code gaps](../../../../docs/code_gaps.md) for the minimal candidates and
[domain evidence](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative/README.md) for assertions,
bounds, source hashes, configurations and retained traces.

Final execution and receipt validation use one pinned runner, Java21 and the pinned
TLC jar. Every process gets an isolated temporary directory; this avoids sharing
extracted standard-module files during parallel runs. Exact raw TLC logs are retained
and machine receipts are compact. The after-measurement validates those fingerprints
and named outcomes before calculating counts. Syntax failures and the isolated
standard-module parser failure during construction were rejected, repaired/retried
and superseded by final executions; none was counted as an expected counterexample.

Final execution and integrity verification match **170/170 configurations across 13
models**: 32 safety, three conditional liveness, 66 controls, 60 witnesses and nine
classified divergence/alternative cases. The final runtime rerun includes all 55
cases after adding TerminalFact, so domain-wide model fingerprints are current.
Source preservation and ontology structure/link/comparison checks also pass.

The concurrent upstream rebase to `afccd820` was integrated before publication.
Its provider-merge rule and source fixes are preserved; the only conflict was the
two appended plan sections. All 170 receipts still match the integrated bound
sources, and the current ontology-hash summary was regenerated. The upstream change
is outside these models, not additional fresh formal coverage.

No application trajectory tests were run. Model reachability is not execution-path
coverage of Rust/TypeScript, and a passing intended variant is not a delivered repair.
