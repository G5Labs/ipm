# Comments and review-suggestion threads: design request

Branch `zcl/comments-design`; comparison base `origin/main` at `a1c3e29e`.

## Human instructions

First request (2026-09-28):

> please investigate the codebase and find how the `comments` functionality (mostly in the speceditor but may not limitted to it) works in the codebase

After the agent's investigation summary (2026-09-30):

> ok main branch has lots of new commits since your investigate, check if they will change your conclusion above,
> please create a git worktree under folder `~/Workspace/`, land a design to make the following two work (i.e, github issue comment is out out of scope here)
>
> | System | Where users see it | Storage |
> |---|---|---|
> | Anchored comments (your focus) | SpecEditor side panel, CodeEditor margin | comment_index (scope line or node) plus comment_replies |
> | Review-suggestion threads | Suggestion cards in the editors and the Reviews panel | The same tables, scope suggestion |

## Context the request refers to

These are the agent's investigation findings, not user words. They were found at `b02101f4` and re-checked at `a1c3e29e` by source inspection; the anchor cases were also replayed against `frontend/src/lib/commentAnchors.ts`. Nothing was run in a browser or a test suite.

Anchored comments:

1. A comment started while the editor holds unsaved edits takes its line number from the unsaved text but its hashes from the last loaded version. With lines inserted above, creation fails and the typed comment is dropped without a message; with lines deleted above, it attaches to another line.
2. A comment added from a raw file tab is stored but never listed in that tab.
3. A comment added from an ontology tab is stored at node level without hashes and always shows "Orphaned anchor".
4. Re-anchoring accepts a lone line with the same text anywhere in the file, with no context or distance check: after a commented heading was deleted, the comment moved to an unrelated identical heading 61 lines away.
5. Reply, Resolve and Reopen are offered to viewers the server refuses. Only read-only refusals are shown, and a refused reply loses its text. Resolve and delete need the owner and the author, so after an ownership transfer nobody can resolve or delete earlier threads.
6. Resolve, edit and delete send no live event; the new-comment and reply events are sent before the transaction commits.
7. An orphaned or ambiguous thread shows a label without its original line or text.
8. Smaller: list paging is accepted but ignored, an empty comment is accepted, a collapsed thread hides its reply count, the mention-edge table is never read and has no tenant row security, and typed mentions match only one-word display names.

Review-suggestion threads:

1. The generic comment routes accept a suggestion thread's id. The source owner can reply there without the review rules, and can resolve or delete it when also its author; a delete detaches the thread, after which replies fail.
2. The review service writes thread and reply rows itself rather than through the comment owner.
3. Notices are published before the transaction commits. Reply recipients are not deduplicated, accept and reject notify the reviewer even when that is the actor, and resolve and withdraw notify nobody.
4. A reply written by the owner's agent is stored and shown as typed by the owner.
5. In-file cards never offer Withdraw. The spec editor drops a card whose block is not rendered. In the read-only code gutter a suggestion marker hides the comment marker and the add-comment control.
6. An open thread refreshes only by ten-second polling, and Enter can send a reply twice.
7. A comment suggestion stores a context record that neither side reads, so one on a repeated line cannot be placed.
8. The ontology has no block for review suggestions. `governance.g5.md` still describes pull-request hand-back and Finish, which PR #170 replaced with Push review.

## Agent interpretation

"Work" means both thread kinds behave as their governing ontology says, with one clear authority for each action and no silent loss of a person's words. The design covers anchored comments in every editor surface and the threads of review suggestions wherever their cards appear.

Out of scope: GitHub issue comments (stated by the user); the rest of the review product — rounds, accept mechanics, review copies, approval sync and the issue #238 auto-publish work; mention matching; and the unused cross-branch comment endpoint.

## Design amendment and implementation request

The design asked whether people who can read the workspace should be able to reply in anchored-comment threads, noting that a mentioned teammate could not answer and that allowing it would reuse the review-thread rule. The user answered:

> yes, as reply does not need a workspace vm attached i assume

Agent interpretation: anyone who can read the workspace may reply in its anchored-comment threads, under the same rule as review threads; starting, resolving, reopening and deleting threads stay with the current owner. The assumption holds: a reply is a platform record written through the API and needs no VM.

The user then asked:

> please create a commit for these design docs, and then follow /build-to-ship to ship it

## Review round 5 decision

After the fifth independent review still required changes, the agent asked how to proceed (the build-to-ship review loop's five-round default had been reached). The question named one P0 and two P1s in the store added in round 3 to keep drafts above the page, plus two older P1s: code-margin positions in Monaco, and comments on duplicate paragraphs. The option the user chose (2026-09-30), quoted as offered:

> **Fix at the root (Recommended)**: Remove the kept-draft store and fix what it worked around: keep the workspace page mounted during short refresh failures. That pulls the follow-up task into this PR. Also fix the margin positions and duplicate paragraphs, then run review round 6.

Agent interpretation: "short refresh failures" are transient ones (no answer, a timeout, rate limiting, a server error). A refusal still shows the error. The page's open file and its approvals page, which hosts the Reviews panel, stay mounted too, since they unmount the same composers. Choosing it also authorizes continuing past the fifth review round.

## Review round 7 decision

The sixth review was repaired, and the agent ran a seventh. It said it would ask before another round if the seventh found more. The seventh found two P0s:
- an auth refresh adopts another person's token before their profile loads, and a 401 retry can resend a request as them;
- some network failures still discard the page, a regression from round 6.

It also found one P1: the Unresolved filter and code-view renderer switches recreate composers. The option the user chose (2026-09-30), quoted as offered:

> **Fix all, round 8 (Recommended)**: Fix both P0s at their source (retire the old person's page as soon as their token changes; never replay a request after the person changes; treat every network failure as transient) and the P1 (keep filtered threads mounted; one card owner across all code-view renderers). Then run round 8. If round 8 still finds problems, I come back to you before going further.

## Review round 8 decision

Round 8 found three P0s:
- a new comment whose anchor was still being computed could be sent after another person's token arrived;
- the query cache keeps the previous person's workspace data, which predates the branch and is app-wide;
- the API client's token refresh still treated a 429 or 5xx answer as a sign-out.

It also found:
- two P1s: a scope-only refresh clearing the page (a round-7 regression), and a decided suggestion removing its card while a reply is sent;
- a P3: the design's list of ontology changes was out of date.

The option the user chose (2026-09-30), quoted as offered:

> **Fix scoped, defer cache (Recommended)**: Fix 1, 3, 4, 5 and 6, which sit in the comments and auth paths this PR already touches. Move the app-wide cache split by person (2) to its own follow-up task, and state that gap in the new security rule. Then run round 9 with that split declared out of scope; if round 9 still finds problems in scope, I come back to you.

## Review round 9 decision

Round 9 found four P1s, each predating the branch:
- a late profile answer could overwrite the current principal's profile, so the page showed one person or org while requests went as another;
- a thread deleted by its owner unmounted a reader's reply being sent, with its words and refusal;
- regrouping in the Reviews panel unmounted replies being written or sent;
- a rejection reason was lost when the suggestion was decided by someone else first.

It also found a P3: a story's metadata still listed repaired gaps.

The option the user chose (2026-09-30), quoted as offered:

> **Fix all, round 10 (Recommended)**: Targeted fixes for all four plus the metadata line. Merge main, fixing the ontology conflict first. Then run full validation and round 10. If round 10 finds yet more composer-lifetime cases, I come back to you with the page-level owner option below.

The page-level owner option, offered alongside, would give the page one owner of every reply box's words, send and error, keyed by thread or suggestion.

## Review round 10 decision

Round 10 found four P1s and a P2. Three of the P1s, and the P2, were further cases of a composer losing what was typed when the view changed:
- a directory node's threads skipped the deleted-thread protection;
- a commit of the source dropped an in-file card being answered or decided;
- an edited replacement was lost when the suggestion was withdrawn;
- switching a file between Source and Rich removed the source editor with its drafts and replies (the P2).

The fourth P1 was in sign-in: an org switch kept the outgoing profile until the new one loaded, or for good if that failed.

The option the user chose (2026-09-30), quoted as offered:

> **Page-level owner, round 11 (Recommended)**: Fix the org-switch profile (R10-1). Move every reply box's words, send and error into one owner per page, keyed by thread or suggestion and cleared on a person change. That covers replies, rejection reasons, edited replacements and new-thread drafts, so mode switches (R10-5), commits (R10-3), deletion (R10-2) and regrouping lose nothing. Replaces the round 6–9 per-view fixes. Larger change; round 11 reviews it.

## Review round 12 decision

Round 11 had found the page-level owner sound in structure but incomplete; the agent completed it without asking, since that raised no new design question. Round 12 then found:
- a render loop introduced by round 11: two lists of an ontology view kept storing different copies of the same thread;
- a card kept after a commit that kept its last-seen status, still showing Open after being decided;
- words hidden when the workspace became read-only or ownership moved during a reply.

The option the user chose (2026-09-30), quoted as offered:

> **Fix all three, round 13 (Recommended)**: Fix the loop (stable record writer), re-read kept cards' status from the server and apply decision results, and show words read-only with Dismiss when authority is lost. Then round 13. Each is bounded; the loop must be fixed in any case.

## Review round 13 decision

Round 13 found two defects introduced by the round-12 repair, and one older gap:
- a card kept from an earlier commit could show Open again after a decision, from an older cached list;
- a failure to read the earlier rounds was not shown;
- a new-thread draft still offered Comment after the workspace turned read-only or the ownership moved.

The option the user chose (2026-09-30), quoted as offered:

> **Fix all three, round 14 (Recommended)**: Newer decided status always wins in kept records, drafts go read-only with Dismiss when authority is lost, and history refresh errors show as a notice. Then round 14.

## Review budget from round 14

While the round-13 repair was under way, the user wrote (2026-09-30): "i give you 5 more rounds with codex, if codex still did not approve, raise up to me". Rounds 14 to 18 proceed without a decision between rounds; if round 18 still does not approve, the agent stops and asks.

## Review round 18 decision

Round 18, the last of the five allowed, found no P0 or P1 and confirmed the round-17 repair. It reported two P2 findings and a P3:
- the comment margin did not measure a thread again when it grew on its own, as its replies or an error loaded, so a neighbouring card could overlap it;
- review-suggestion replies could show up to ten seconds late when a first read was still out, the pattern repaired for anchored comments in round 15;
- two example cells in the design table contradicted reader replies.

The option the user chose (2026-09-30), quoted as offered:

> **Fix all, round 19 (Recommended)**: R18-2 reuses the read-again helper on the review-reply paths. R18-3 is two doc cells. R18-1 adds a size observer on margin cards. Then one more Codex round; with no P0/P1 left, approval is likely if the fixes are clean.

## Review budget and scope from round 19

While the round-18 repair was under way, the user wrote (2026-09-30):

> please keep working with codex for at most 15 more rounds, feel free to push back to codex with a clear reason, you must keep in mind the original goal I gave to you (make the following two cases work) and avoid over-engineering and out of scope work

The two cases, as the user gave them:

| System | Where users see it | Storage |
|---|---|---|
| Anchored comments (your focus) | SpecEditor side panel, CodeEditor margin | `comment_index` (scope line or node) plus `comment_replies` |
| Review-suggestion threads | Suggestion cards in the editors and the Reviews panel | The same tables, scope suggestion |

Rounds 19 to 33 proceed without a decision between rounds. A finding that does not bear on these two cases working, or that predates this change outside them, is answered with the reason and recorded as a follow-up rather than repaired here.

## Agent decisions

The user did not state who may start or decide a thread, how a comment re-anchors, or how review threads relate to anchored comments. Those choices in the design are agent decisions derived from existing ontology, the v9 design and the merged review PRs (#170, #173, #184, #199, #203 by kaihuiz-g5). Treating review suggestions as retained product is inferred from the user naming their threads as something to make work.
