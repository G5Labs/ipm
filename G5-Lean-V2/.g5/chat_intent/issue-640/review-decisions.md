# Owner review decisions, October 19, 2026

Source: [PR 642 owner review, part 1](https://github.com/G5Labs/G5-Lean/pull/642#issuecomment-6261547790), posted by `rahul-g5` on October 19, with [implementation and ontology details in part 2](https://github.com/G5Labs/G5-Lean/pull/642#issuecomment-6261548231). The owner identifies these as approved amendments to issue #640. The following decision table is retained verbatim from that record.

| ID | Decision |
|---|---|
| DEC-1 | Humans may **inspect** but never **change** formal review resources. This amends issue #640's "Humans may neither inspect nor modify". |
| DEC-2 | The inspection audience is **the same as the approval history** (readers of the source workspace). Target-side content additionally requires target read access. |
| DEC-3 | Inspection uses retained evidence: the report, the assignment, and the reviewed commits through **evidence refs** that keep the commits reachable. **No live read-only view** of the temporary workspace. |
| DEC-4 | The agent's transcript is served by a **paginated endpoint**, never embedded in the report. It is **not evidence** and follows **ordinary retention**: task events are trimmed after 30 days, and prompts and messages are not deleted, as for ordinary chats. Nothing new is added for review sessions. Only the report (and its evidence rows) is retained as evidence. |
| DEC-5 | A **new review request stays pending even if its review fails**, so an unaddressed request is visibly unaddressed. A newer request, or its failure, **does not clear an earlier effective verdict**. |
| DEC-6 | If a retry is running and someone requests a review of current inputs (or the reverse), **return Conflict**. Don't silently reuse, and don't abort the running attempt. |
| DEC-7 | Implement **both**: a database-enforced rule that only platform contexts and the review's own runtime may change review resources, **and** a static guard test proving that system-scoped routes bind to their workspace. |
| DEC-8 | **Keep** the PR's change that maps an uncertain commit acknowledgement to 503 (retryable) for ordinary fork and VM operations, and add correct test coverage. |
| DEC-9 | A failed rerun does not clear an earlier verdict. This matches current behaviour (experiment X1), so no change is needed beyond DEC-5. |
| DEC-10 | Ontology edits E1–E9 (§7) are **approved**. |

## Agent interpretation

Apply these amendments in the existing design before dependent implementation. Preserve ordinary source-workspace authorization, provider authority, no live human access to the temporary workspace, closed execution and seal-before-effectiveness. Evidence-ref retention preserves Git objects after temporary-resource cleanup or source-ref removal; it does not itself grant access to a deleted source or authorize a review against one. The reported defects and proposed mechanics require independent verification; the owner's review is not evidence that this checkout implements the amendments.


## Source deletion clarification

On October 19, 2026, the owner answered the direct question about evidence after source deletion: **"Retain evidence; keep ordinary access rules (Recommended)"**. Retain the evidence objects without adding deleted-source inspection or frozen-retry authority. This confirms the existing ordinary source-history authorization; it is no longer an open intent question.

## Additional review budget

On October 19, after the original six rounds, the owner authorized **up to six additional Claude Code reviews** as part of addressing the PR review. These are a limit, not a target. Continue self-verification before review, independent adjudication and escalation of significant unresolved disagreement at the new limit.
