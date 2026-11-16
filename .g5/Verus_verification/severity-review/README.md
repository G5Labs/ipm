# Severity review, 2026-11-16

Applied to the canonical [bug register](../bugs_issues.md) and [proposal register](../design_simplification.md). The input revision is `7b094a0ec2c8b504c18d30698cc7c873eef6aef2`; [rating decisions and input hashes](ratings.json) record the per-ID basis.

## Results

| Register | Entries | Critical | High | Medium | Low | Unranked |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Bugs and issues | 203 | 5 | 19 | 96 | 83 | 0 |
| Proposals | 49 | 3 | 11 | 29 | 6 | 0 |

The 94 previously Unranked bug records are now 2 Critical, 10 High, 38 Medium and 44 Low. These totals include aliases and closed review records; they are not counts of distinct active defects. The 44 previously Unranked proposals are also rated by the consequence they address.

## Rating policy

Critical covers data loss, a security breach or a halt for many users. High covers incorrect results, authorization bypass or work without required review. Medium covers visible misbehavior or administrator repair. Low covers cosmetic, diagnostic or documentation effects. An unconfirmed trigger lowers the supported potential impact by one level, with Low as the minimum. An existing qualified witness can establish a trigger without replay.

Severity is separate from evidence and disposition. Conditional policy findings keep their open questions. Source-only security findings describe potential impact without claiming a demonstrated compromise. Refuted, withdrawn and accepted-behavior references receive Low administrative review priority with an explicit statement that no surviving defect is asserted. Aliases inherit the canonical scope and severity. A proposal inherits the highest consequence it addresses; this does not accept the proposal or establish a simplification.

## Corrections to existing ratings

| Finding | Before | After | Basis |
| --- | --- | --- | --- |
| [G:D1](../bugs_issues.md#g-d1) | Medium | Critical | A parent rollback removes a dependency's durable audit record entirely. The through-product witness establishes audit-data loss, which the G5 severity convention rates Critical. |
| [G:D2](../bugs_issues.md#g-d2) | Medium | Critical | A documented version-part filter can turn a filtered delete into deletion of the entire history. The witness establishes unintended loss of unrelated persisted audit data. |
| [G:D25](../bugs_issues.md#g-d25) | Medium | Critical | A caller rollback removes the action's durable history. Critical is the conditional audit-data-loss impact if caller-survival is required; Q-05 remains unresolved. |
| [C:D32](../bugs_issues.md#c-d32) | Medium | High | An explicit requested-version install succeeds with another version's callable bytes. This is a wrong installation result, not merely stale listing text. |
| [P:F-REPO-4](../bugs_issues.md#p-f-repo-4) | Low | Medium | Config delete restores a prior assigned value instead of the documented default, changing effective configuration. This is administrator-repairable behavior, not merely cosmetic text. |
| [P:F-LOAD-4](../bugs_issues.md#p-f-load-4) | Low | Medium | An ordinary load returns failure after its changes commit, leaving installed code and a failed command result. This can misdirect scripts and recovery even though the trigger is an after-install message. |

The active proposal ratings for C:S17 and P:D1 now follow the High uninstall consequence and Critical audit-data-loss consequence respectively. Their comparison limitations and decisions are retained.

## Validation and scope

All 203 bug IDs and 49 proposal IDs, historical anchors, decisions, reproduction paragraphs, evidence links and existing entry text were preserved. Actionable entries are ordered by severity and remain ahead of review records. The two registers contain no Unranked status lines. No product code, ontology statement, model, frozen audit receipt or runtime evidence was changed. No new security or interrupted-archive execution was performed.

The findings checker passed against the input revision, the verification-link dry run reported no changes, and the ontology checker found zero errors with 44 pre-existing writing warnings. `git diff --check` passed. A separate comparison confirmed that each entry's content outside the severity line and added rationale was preserved.

Active simplification comparisons remain Verus-only correctness repairs with no measured state reduction. Changing their priority does not establish the missing broader comparison or full-product proof.
