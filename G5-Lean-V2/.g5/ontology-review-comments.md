# Anchored comments and review-suggestion threads: implementation review

Agent review comments, attribution and round tools described in this historical record
were retired by the [October 24 removal](chat_intent/workspace-colab-harden/remove-agent-review-comments.md). They are not supported interfaces.

## Initial self-review, before independent discovery

Compared with `origin/main` at `a1c3e29e`, using the saved [design](ontology-plan-comments.md) and [chat intent](chat_intent/comments-design/request.md). This pass is self-review by the implementing session. Three fresh discovery reviews and the independent Codex CLI review are recorded below before any consolidated repair.

| Commitment | Actual path and counterexample/check | Initial result |
|---|---|---|
| `g5.documents.comments` (anchor from the displayed bytes) | SpecEditor anchors to `currentMarkdownForRichEdit()`, CodeEditor to its local model; both hand the page a complete anchor, and WorkspacePage forwards it with the base OID. Checks: SpecEditor with unsaved lines above the target, CodeEditor with lines deleted above at Monaco's caret, and the page posting the anchor built on unsaved text. Mutating the page to hash the loaded snapshot fails the page test. | Tested at the editor and page boundary. The Playwright flow (comment, Save, reload, reader view) is written but has not run locally. |
| `g5.documents.comments.identity` | WorkspacePage lists by repository and path for document, raw-file and ontology tabs, adding legacy node threads in their node's view. The API ignores the branch filter and refuses new node-scoped creation. The ontology content read returns `repoId`. Checks: API tests for branch labels and node refusal; the page test asserts list queries carry repository and path but no branch. | Tested for file tabs and the API. The ontology-tab identity is source-inspected only. |
| `g5.documents.comments.reanchor`, `.ambiguity` | One resolver in `commentAnchors.ts`, applying ordered evidence with a lazily indexed text per version; editors keep the previous placement until the next is ready. Checks: resolver case table (the four design examples plus unanchored rows) and an editor test that a refetch keeps the moved placement rather than the written line. | Tested. Hunk mapping is out of scope. |
| `g5.documents.comments.authority`, `.replies`, `.controls` | CommentService decides: the current owner starts, resolves, reopens and deletes; the author edits text; any reader replies; read-only workspaces and Base refuse. Routes also require the owner for write scope. The page derives `CommentPermissions` and editors receive only the permitted actions. Checks: API tests for reader, transfer, Base and a direct service-boundary test as a former owner; page test for a reader (reply box only); editor tests for a non-editing owner and a reader. | Tested. Mutation runs show the API tests alone could not tell the service check from the route gate; the service-boundary test was added for that. |
| `g5.documents.comments.failure` | Drafts await the add, keep the text and show why on failure, and send once while in flight; thread replies and the Reviews panel reply box do the same. Checks: editor test (Ctrl+Enter twice, then the button, one call; a rejection keeps the text), CommentThread tests, Reviews panel test. | Tested; each guard is mutation-checked. |
| `g5.documents.comments.live` | Create, reply, update and delete publish through `ScopedDb::on_commit`; the client refetches lists on `comment.updated` and `comment.deleted` and drops a deleted thread's replies. Checks: an uncommitted service call sends no event; the real WebSocket manager invalidates the right keys. | Tested at the API and client boundaries. Two browsers were not exercised end to end. |
| `g5.governance.review-suggestions`, `.display` | Listed suggestion rows name their reviewer, so the file card offers that reviewer Withdraw. SpecEditor lists cards whose block has no row. The read-only gutter shows the suggestion and comment markers together and keeps Add comment. Comment suggestions carry v2 context and a content hash from the source head. Checks: editor tests for Withdraw, unplaced cards and the gutter; API assertions for the reviewer on listed rows and for v2 context. | Tested. |
| `g5.governance.review-thread`, `.access`, `.attribution`, `.notices` | The review owner writes threads through CommentService; generic comment operations return not-found for a suggestion thread; replies record person or agent; notices go after commit, once per other party, never to the actor; resolve and withdraw notify. Checks: two new review API tests and the anchored-route refusal test. Mutations for self-notice, agent kind and suggestion-scope reach are caught. | Tested; the recipient-dedupe mutation is being rechecked against the in-place review test. |
| `g5.architecture.comment_owner`, `g5.architecture.review_owner` | `open_suggestion_thread`, `append_thread_reply`, `retarget_suggestion_thread` and `list_thread_replies` are the only thread writers and readers used by the review owner. | Source inspected. |
| `g5.security.tenant.rls` (mention edges) | Migrations 043 (columns), 044 (backfill under the owner FORCE toggle, the form the runner accepts on populated tables) and 045 (NOT NULL, trigger, forced RLS). Rehearsed with rows from `a1c3e29e` as `g5_migrator` through `g5-db-migrate`: every edge got its workspace's organization, a session without an org sees none, and the rerun was a no-op. `test_mention_edges_are_tenant_scoped` covers runtime isolation. | Tested. |

Findings from validation during implementation, already repaired before review:

1. The first migration mixed DDL with the NO FORCE toggle. The production runner refuses that form on populated tables, which the row rehearsal showed. It is now split into 043 to 045.
2. The editors' resolver read the displayed text even for comments without a line anchor, and crashed on a malformed test fixture. The text is now split on first use, and an empty list is not resolved at all.
3. The API authority tests could not tell the service check from the route write-scope gate. A service-boundary test now covers CommentService on its own.

Open evidence gaps at this point:
- The Playwright flow has not run: the disk is at 99% and no local candidate stack exists, so it awaits Cloud E2E.
- Ontology-tab identity has no page test.
- A two-browser live update is not exercised end to end.

## Independent discovery and consolidation

Three fresh discovery reviewers (A, B, C; separate subagent contexts that did not see this self-review or each other) and the independent Codex CLI review (0.158.0, `gpt-6-astra`, `xhigh`, read-only sandbox) reviewed candidate `72cce3e8`. Their reports are preserved outside the repository with the session record. Codex returned `VERDICT: CHANGES_REQUIRED` (7 P1, 1 P2, 1 P3). The discovery reports found 15 (A: 2 P1), 14 (B: 3 P1) and 11 (C: 3 P1) findings. The implementer's own later observations S1 and S2 are included. Dispositions were decided before repair:

| ID | Finding (sources) | Evidence | Disposition before repair |
|---|---|---|---|
| K1 | The read-only code viewer's selection Comment and right-click Add Comment use a stale caret line, which starts at 1 (Codex 1, A1, B-02) | Executed by A and B: a selection on line 3 or 4 anchored line 1 | Accept. Take the line from the selected or right-clicked row; test both entry points. |
| K2 | The resolver treats an empty recorded context side as matching evidence, and compares fixed two-line windows at file edges (Codex 2, A7, B-01, C-F4) | Executed by A, B and C: last- and first-line twins map without evidence; a line-2 comment orphans after an insertion above | Accept. Compare each recorded side at its own length. An empty, blank-only (including the phantom final line) or size-truncated side is no evidence. Truncation is recorded in the context. |
| K3 | A refused delete, or a refused resolve in the unresolved-only view, loses its reason and any reply draft: the optimistic update unmounts the thread (Codex 3, B-05, C-F1) | Executed by B and C with the real hook. A's probe passed; B's and C's setups match the production path. | Accept. Delete and resolve stop being optimistic: the card stays mounted and shows the refusal. |
| K4 | SpecEditor shows lost or ambiguous suggestions in place with no note, presenting them as matches (Codex 4, A2, B-03) | Executed by A and B | Accept. Carry the display state and show the same note the code view shows. |
| K5 | Review cards offer Reply and decisions on a read-only, freezing or Base source (Codex 5, A5, B-10, C-F8) | Inspection | Accept. Cards read the source's writability and offer only permitted controls. |
| K6 | An owner who is also the reviewer is not offered Withdraw (Codex 6) | Inspection; the API permits it | Accept. Withdraw depends on being the reviewer, not on ownership. |
| K7 | Legacy node threads on a directory node are not shown (Codex 7) | Code history: node threads were only ever created from editor tabs, which directory nodes never render, and no backend or agent path creates node-scoped threads; nothing listed them there before this change either | Reject as a defect in reachable data. Recorded as a metadata gap for rows made by direct API calls. |
| K8 | A CodeEditor draft keeps only a line number, so text that changes before submit moves the thread (A3, B-04, C-F3, S2) | Executed by A, B and C | Accept. Capture the anchor when the draft opens; re-place it at submit, or keep the draft open with a reason. |
| K9 | The agent-facing round read drops reply authorship (B-06, C-F2) | Inspection | Accept. Add the author kind and an agent label to the internal round replies. |
| K10 | Reply read access is enforced by the route helper, not CommentService (B-07, C-F5) | Inspection; no current bypass | Accept. CommentService checks that the replier can read the workspace. |
| K11 | Acceptance evidence gaps (Codex 8, A6, B-08, B-09, C-F6, C-F10) | Missing checks | Accept. Add page tests for document and ontology identity, a `repoId` assertion on ontology content, a filtered unplaced-card test, reader refusal on decision routes, reader mention delivery, an edit event, and a real transfer in the ownership test; extend the Playwright flow (second viewer live, deletion above); record the migration rehearsal. |
| K12 | The document view calls a thread on a line between blocks "Orphaned anchor" while the code view shows its line (A8, B-11, C-F7) | Executed by A, B and C | Accept. Place comments at the nearest block, as suggestions already are. |
| K13 | New comments are labelled `main` whatever the branch; stale comments and a test title describe branch filtering (A9, B-12, C-F9, S1) | Inspection | Accept. The server labels an unlabelled comment with the workspace's branch; correct the stale text. |
| K14 | Five compile-time-checked statements became runtime queries (B-13) | Inspection | Defer. Each statement runs in the integration tests, and the same services already use runtime queries; no behavior depends on it. |
| K15 | Whether a former owner may still edit their own thread text (B-14) | Design question | No change. Human question below. |
| K16 | The mention table was kept under forced RLS rather than dropped (C-DQ1, A15) | Choice within the design's two options | Keep; the design records it and the human question stays open. |
| K17 | A socket reconnect never refetches anchored threads (A4) | Executed by A | Accept. The workspace resync invalidates thread lists and open replies. |
| K18 | An empty PATCH writes and publishes an update (A10) | Inspection | Accept. Refuse it as invalid. |
| K19 | `get_comment_workspace_id` lost its only caller (A11) | Inspection | Accept. Remove it. |
| K20 | A twin without published context is shown as anchored at its published line (A12) | Pre-existing placement code; acceptance is decided server-side by context | Defer. Recorded as a display gap. |
| K21 | Review decisions refuse read-only workspaces but not Base, while replies refuse both (A13) | Inspection | Accept. The shared thread writers in CommentService refuse read-only workspaces and Base for every entry path. |
| K22 | The ontology view of a file shows no suggestion cards (A14) | Inspection; the display rule covers any view of the file | Accept. Build the overlay for ontology tabs too. |
| K23 | Design and ontology metadata still describe the pre-change code (Codex 9, A15, B §4, C §4) | Inspection | Accept after repair. |

Questions for the human: whether a former owner may edit their own thread text after a transfer (K15), and whether the mention table should be dropped instead of kept under RLS (K16). Neither blocks this change.

## Repair and verification

The accepted findings were repaired together on top of `72cce3e8`, in the commit that adds this section. Each repair has a test at the boundary where the defect showed. Where noted, a mutation run shows that the test fails once that repair is reverted.

| ID | Repair | Evidence |
|---|---|---|
| K1 | Gutter and code rows carry `data-line-number`. A selection's Comment and a right-click Add comment take the row under the pointer, falling back to the caret. | Editor tests "anchors a selection's Comment to the selected row" and "anchors a right-click Add comment to the row clicked, not an earlier caret"; both mutations caught. |
| K2 | The resolver compares each recorded side at its own length. An empty side, a blank-only side (including the phantom final line) or a side cut to the size bound is not evidence. New contexts record the truncation of each side. The `reanchor` block states this rule (amendment). | Six new resolver cases: last-line and first-line twins, a line-two comment far down, a twin appended after, a shortened side, a blank-only side. Mutations on the evidence rule, the truncation rule and the recorded-length comparison are caught. |
| K3 | Resolve and delete are no longer optimistic. The card stays mounted until the server accepts, and a refusal shows on it, keeping any typed reply. | Hook tests: the thread stays listed while deleting and when deletion is refused; it is unchanged until resolve is accepted. Page test "keeps a typed reply and shows why when deleting the thread is refused"; mutation caught. |
| K4 | Document-view cards receive the text as shown and carry the same moved, lost or ambiguous note as the code view. | Editor test "says so when a suggestion's lines are not in the document as shown"; mutation caught. |
| K5 | `workspaceAcceptsThreadWrites` (not Base, not read-only, not freezing) decides Reply and every decision in the Reviews panel and in-file cards. The page reads the source workspace's detail for a copy's cards. | Reviews panel test "keeps discussions readable but offers no reply or decision on a read-only source"; editor test for a read-only source; mutation caught. |
| K6 | Withdraw depends only on being the reviewer. | Editor test "... and Withdraw to a reviewer who owns it"; mutation caught. |
| K8 | A code draft records the displayed text when it opens. At submit, unchanged text anchors as recorded; changed text re-places the draft through the resolver, or the draft stays open saying the line changed. | Editor tests "keeps a draft on its line when lines are added above before it is sent" and "keeps the draft and says why when its line is gone before it is sent"; mutation caught. |
| K9 | The internal round read returns each reply's author kind, and labels agent replies as the owner's agent. | `owner_agent_replies_are_attributed_and_in_place_reviews_skip_self_notices` asserts `authorKind` and the label for agent and reader replies; mutation caught. |
| K10 | `CommentService::add_reply` checks that the replier can read the workspace (`ensure_user_workspace_access`), independent of the route. | `test_comment_service_checks_a_repliers_read_access`: an invite-only workspace refuses a non-member at the service, and accepts them once they are a member; mutation caught. |
| K11 | Page tests use the document's and the node file's repository and path, and keep legacy node threads. Other additions: a `repoId` assertion on ontology content; a filtered unplaced-card test; a reader refused on reject, resolve and withdraw; mention delivery on a reader's reply; an edit event; a real `/transfer` in the ownership test. The Playwright flow adds a line deleted above with a second comment, and live changes seen by the other browser. | The named tests pass. The ontology-view page test also checks that the suggestion list is requested for that file (K22); mutation caught. |
| K12 | The document view places a comment on a line between blocks at the nearest block, as suggestions already were. | Editor test "places a thread on a line between blocks at its line, not orphaned"; mutation caught. |
| K13 | The server labels an unlabelled comment with the workspace's branch, which is the workspace's name (`g5.workspace` one-name rule). The client sends no label, and stale text about branch filtering is corrected. | `test_branch_label_never_hides_a_files_threads` asserts the label; hook test "sends the file identity on creates, with no branch"; mutation caught. |
| K17 | The workspace channel's reconnect resync invalidates thread lists, line comments, open replies and review replies. | `ws.reconnect.test.ts` "refetches anchored and review threads of a workspace on reconnect"; mutation caught. |
| K18 | An update with neither text nor a resolved flag is refused as invalid. | `test_blank_comment_text_is_refused` expects 422 for `{}`; mutation caught. |
| K19 | `get_comment_workspace_id` and its SQLx cache entry are removed. | Build and `cargo sqlx prepare --check`. |
| K21 | `open_suggestion_thread` and `append_thread_reply` refuse read-only workspaces and Base. The reject, resolve and withdraw routes run the same check before any effect. That check is the only guard for a suggestion whose thread row is gone: `comment_id` is set null on delete, so such a decision writes no note. | `test_thread_writers_refuse_base_for_every_caller`, and `base_review_threads_accept_no_message` with a threaded and a threadless suggestion. With only the threaded case, the reject pre-check mutation survived, because the writer refused the note instead. With the threadless case, all three decision mutations are caught, as are both writer mutations. |
| K22 | The ontology views (code, spec and JSONL) receive the file's suggestion overlay. | Page test "uses the node file's repository and path in the ontology view, and keeps legacy node threads"; mutation caught. |
| K23 | Metadata is updated for the comment, identity, re-anchor, ambiguity, authority, reply, control, failure and live blocks, and for the review story, display, thread, access, attribution and notices. Also updated: `comment_owner`, `review_owner`, commit-notify, `notification_owner`, durable notices and `identity.base`. The design gains an "Implementation decisions" section and the amendment. | This review. |

Validation of the repaired tree:
- **Backend:** `cargo fmt --check` and strict clippy pass, with `-D warnings` on `g5-authz`, `g5-api-core`, `g5-api-routes` and `g5-api`, all targets.
  - Library tests pass: 1014, 242 and 16.
  - Integration binaries pass: `api_comments` 31, `api_ontology` 34, `api_workspace_reviews` 25, `route_auth_static` 7, `tenant_scope_regressions` 3.
  - `cargo sqlx prepare --check` passes. `just validation-preflight` passed 11 of 11 at the candidate, and the repair adds no migration.
- **Backend mutations:** nine runs, each against the test meant to catch it: replier read access, branch label, empty update, the two writer Base checks, the three decision pre-checks, and the round-read author kind. All nine are caught.
- **Frontend:**
  - `tsc` (build config) passes, and ESLint reports 0 errors. The changed files add three warnings in two places, each following a pattern already in its file: a click guard on the thread's error message (two accessibility warnings), and one more exported helper in the overlay module.
  - Prettier is clean on the changed files; the 16 files that fail on `main` are unrelated.
  - vitest passes: 238 files, 3637 tests.
  - Thirteen frontend mutations, each reverting one of the client repairs above, are all caught.
- **Migrations:** unchanged since the candidate, so the rehearsal recorded in the self-review stands.

Remaining gaps:
- The Playwright flow, including the two-browser live checks and the deletion above, has not run locally. Cloud E2E CI runs it.
- K14 (runtime queries) and K20 (placement of context-less twins) are deferred, and K7 is rejected, all with the evidence above. Round 2 reversed K20 and K7 (R2-6, R2-7).
- K15 and K16 are open questions for the human.

## Round 2

The Codex CLI (same configuration) reviewed the repaired candidate `cc05f493` and again returned `VERDICT: CHANGES_REQUIRED`: 1 P0, 6 P1 and 1 P2.
- **Round-1 findings:** K1, K4, K6 and K23 are resolved. K2, K3, K5 and K11 are resolved only in part; their remainders are R2-4, R2-2, R2-5 and R2-8.
- **K7:** Codex accepted the evidence about product history, but disagreed with the rejection because the older public API accepted node comments on any node.
- **K20:** Codex disagreed with the deferral: the display rule forbids presenting a suggestion as a match unless its text is found uniquely.

Each finding was checked against the code before a disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R2-1 | The owner's agent replied after releasing its incarnation fence, and `add_reply_as` did not re-check the actor. A transfer could land between the fence's owner check and the reply. | Inspection. Accept, reject and resolve re-check the owner inside their own transaction; the reply did not. The gap predates this change; it is the review-thread sibling of K10. | Accept. The reply runs inside the fence: it calls no VM, unlike accept, which must release it. `add_reply_as` checks the replier's read access for every entry path. | `owner_agent_replies_inside_the_incarnation_fence`: a table lock pauses the reply at its insert, and `SELECT … FOR UPDATE NOWAIT` on the workspace row fails with 55P03. Releasing the fence before the reply makes the test fail. The read check is defence in depth: every path to it first passes an equivalent check, so no test isolates it. |
| R2-2 | A failed background refetch unmounted threads and cards, losing drafts and errors. | Confirmed. `toAsyncState` drops loaded data on a refetch error; the page substituted an empty list, the overlay returned none, and the Reviews panel replaced its cards. | Accept. `toRefreshingState` keeps loaded data with the refresh error in the comment and review hooks. Other consumers only hid content on error, so they now keep it. A notice appears beside the kept content in the comment column, the Reviews panel and discussions. | A page test (anchored thread: reply and list refresh fail, then recovery), a page test (in-file card) and a panel test. Reverting the adapter or hiding a notice fails them (5 mutations). |
| R2-3 | A threadless artifact could be kept on Base. | Confirmed. `ensure_workspace_mutable_by` checks the owner and mutation policy, not Base; Base defaults to `read_write`. | Accept. Accept applies `ensure_thread_writes_allowed` before any effect, as the other decisions do. | The Base test covers threaded and threadless artifacts: refused, status unchanged, no notices. The existing test that keeps an artifact without a VM still passes. Mutation caught. |
| R2-4 | Contexts from the earlier producer could be shortened without a flag and still count as evidence. | Confirmed from the base producer. It cut 128 UTF-16 units at a time until the JSON fit 4096 bytes, and recorded nothing. | Accept. New contexts always record the three flags. One cut removes at most 768 bytes, so an unflagged context over 3328 bytes gives no evidence. | Hand-written earlier-producer contexts: a large one gives no evidence, a small one still does, and new contexts carry the flags. 2 mutations caught. |
| R2-5 | Accept finding, Review with my agent, and a Reject form opened earlier bypassed the writability gate. | Inspection. | Accept. Both publication controls require a writable source. The form and its submit check the current permission and keep the typed reason. | Panel tests: the read-only source with the VM running, and a writable → read-only → writable transition. 3 mutations caught. |
| R2-6 | Suggestion twins with no usable context were shown as matched at the published line (K20). | Confirmed against `g5.governance.review-suggestions.display`. | Accept, reversing the K20 deferral. Twins that no context singles out are marked ambiguous, and the note no longer claims acceptance follows context. | A unit test and a twin test in each editor. Mutation caught by all three. |
| R2-7 | Legacy threads on a directory node were fetched but never shown (K7). | The older public API accepted `scope = 'node'` with any node id. | Accept, reversing the K7 rejection. A directory node's view lists its node threads beside the graph, marked not anchored to a line; they can be answered but not started there. | A page test: a reader sees the thread and replies. Mutation caught. |
| R2-8 | The Playwright deletion case never saved and reloaded. | Confirmed. | Accept. The flow saves and reloads after the second comment, and the owner and the reader expect lines 4 and 5. | Not run locally; the E2E check on the PR runs it. |

Re-verification at `cc05f493`, in a clone detached from git so the reviewer's tree stayed untouched:
- The ten round-1 backend mutations and the thirteen repair-round frontend mutations still fail their tests.
- The ten round-1 frontend mutations still fail their tests too.
- One frontend control run exited non-zero under load from the concurrent Rust builds. It passed alone, and its mutation was then caught alone.

Validation of the round-2 tree:
- **Backend:** `cargo fmt --check` and strict clippy (all targets) pass.
  - Library tests pass: 1014, 242 and 16.
  - Integration binaries pass: `api_comments` 31, `api_ontology` 34, `api_workspace_reviews` 26, `route_auth_static` 7, `tenant_scope_regressions` 3.
  - `cargo sqlx prepare --check` passes. The repair adds no query macro and no migration.
- **Frontend:** `tsc` passes; ESLint reports 0 errors and no new warnings beyond the three recorded above; Prettier is clean on changed files; vitest passes 238 files and 3647 tests.
- **Mutations:** backend 2 of 2 and frontend 14 of 14 round-2 mutations fail their tests.
- **Ontology:** no block body changes. The existing rules already require this behaviour: unanchored rows "stay readable and answerable where its file or node is shown", a failed action "keeps what the person typed", sides must be "recorded in full", suggestions are "never presented as a match", and controls are offered only when permitted. The metadata of the changed blocks and the design's implementation decisions record the repairs.

Remaining gaps: the Playwright flow has not run locally; K14 stays deferred; K15 and K16 remain open questions for the human.

## Merge with main

`main` moved to `e386351a` during round 3. It added migrations 043/044 (agent attempt limits, #259) and reverted the review Project Doc destinations (#258, reverting #222).
- **Ontology:** main changed only the agents and conversations pages, and the revert carries no ontology. No comment or review block conflicts.
- **Migrations:** this branch's migrations are now `045_comment_thread_authorship`, `046_backfill_comment_mention_orgs` and `047_scope_comment_mentions_to_orgs`. After round 3, #261 added `045_drop_review_document_targets` on `main`. A second merge moved them to `046`, `047` and `048` (see below).
- **Conflict resolutions:**
  - Take the revert's carry-forward branch, and drop `retarget_suggestion_thread`, whose only caller the revert removed.
  - Drop the 15 Project Doc tests and helpers the revert removed.
  - Keep this branch's writability gates without the reverted labels.
- **Migration rehearsal** on the merged tree: rows loaded at `a1c3e29e`, then main's 043/044 and 045–047 applied as `g5_migrator` through `g5-db-migrate`.
  - Every mention edge got its workspace's organization, and `org_id` is NOT NULL.
  - Forced RLS is on `comment_mentions`, and `workspaces` has its FORCE flag restored.
  - A role without org context sees no edges, and the rerun was a no-op.
- The merge commit is `9ec31110`.

**Second merge**, after round 3: `main` moved to `454ace5b`.
- #261 dropped the reverted Project Doc columns in `045_drop_review_document_targets`. It changed only that migration and the checksum list.
- This branch's migrations move to `046_comment_thread_authorship`, `047_backfill_comment_mention_orgs` and `048_scope_comment_mentions_to_orgs`, with their cross-references and checksums updated.
- The rehearsal was repeated with rows from `a1c3e29e`: main's 043–045 and 046–048 applied, and every check above held, with ledger versions 43–48.
- The fixture holds no review suggestions, so it does not exercise main's own 045 backfill.

## Round 3

The Codex CLI reviewed `db6fe50f` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R2-1, R2-3, R2-4, R2-6 and R2-7.
- **Resolved in part:** R2-2 and R2-5.
- **Still open:** the browser run for R2-8.

The new findings, each checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R3-1 | A typed reply or draft is lost when a parent is unmounted by a failed refresh. The workspace page replaces everything with its error screen when a poll of the workspace detail fails. | Confirmed. The page guard renders the error view on `workspace.status === "error"`, and `useWorkspace` polls every 5 s. The error screen predates this change and also discards unsaved file edits. | Accept, for what the comment rules own. `lib/keptDrafts.ts` keeps typed text by thread or file in memory, above every error boundary. This covers anchored and review reply boxes, rejection reasons, and new-thread drafts in both editors (the draft's line or block too). Sending or cancelling forgets it. The page-level error screen is app-wide behaviour, flagged as separate work. | A page test: a reply and an open draft survive the page's error screen and return with it. Remount tests for the document editor's draft and the Reviews panel's reply and reason. The global test setup forgets kept drafts between tests. |
| R3-2 | A failed source-detail refresh re-enables forbidden review controls. | Confirmed. Both expressions treated a non-loaded detail as writable. | Accept. `threadWrites.ts::sourceThreadWrites` offers controls only for a loaded, writable source, and the closed note says why: loading, failed to load, Base or read-only. The panel and a review copy's cards both use it. | A panel test for read-only, Base and freezing sources whose detail refresh fails: controls stay absent, then return once writable. A helper unit test. |
| R3-3 | A review comment without recorded line text was shown as matched. | Confirmed. The source-head check read the line, but only the hash and context were kept, and the client placed empty text at the published line. | Accept. The internal route records the source head's line text when the agent sends none. The client marks a comment with no text, or an insertion no published context pins down, as `unverified`. A recorded blank line is looked for like any other line. | API: a comment sent without `lineText` stores the source line. Unit tests cover no text, blank lines and insertions with and without context. Editor tests cover both editors. |
| R3-4 | The editable fallback placed cards against the saved text. | Confirmed. `FallbackSuggestionList` received `content`, not `activeContent`. | Accept: pass the text being edited. | `CodeEditorFallback.test.tsx`, with Monaco made unavailable: an insertion above moves the card, and deleting the line marks it lost. |
| R3-5 | The HTML document view and the HTML ontology document view showed neither thread kind. | Confirmed. Both return `HtmlPreviewShell` with no source companion. The raw-file and code-node previews show threads in source mode. | Accept. `SideThreads.tsx` lists the file's threads by source line, and its suggestion cards, beside the preview. They can be answered and decided as permitted, but not started there. The directory-node view uses the same component. | Page tests for both routes as a reader: the thread labelled L2, its card, a reply sent, and no owner controls. |
| R3-6 | The identity metadata still said directory-node threads are not shown. | Confirmed. | Accept: metadata rewritten. | Review. |

Remaining: the Playwright flow has not been run for a candidate yet. The E2E lane is triggered on the pushed branch before this round's review ends (see the status below).

## Round 4

The Codex CLI reviewed `2b28fa84` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R3-2 to R3-6.
- **Merges:** both judged correct, with 48 migration files, matching checksums and no duplicate versions.
- **Resolved in part:** R3-1, with its remainder in R4-2.

Each new finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R4-1 (P0) | Kept drafts could cross authenticated users. | Confirmed. The store was module-wide with no principal. The auth provider adopts another principal's token in place (the silent 401-refresh path) and evicts other principal-scoped caches then (`evictReviewMetadata`), but not drafts. The R3-1 repair introduced this. | Accept. `evictReviewMetadata` also calls `forgetKeptDrafts`, which clears kept text and sends in flight on every principal change (logout, org switch, cross-principal adoption), not on a same-principal rotation. An epoch fences a send from before the change, so it settles nothing. | `useAuthContext.test.tsx` through the real provider: a rotation keeps the draft; adopting another principal clears the draft and the in-flight flag; the earlier principal's late completion leaves the new principal's text alone. 3 mutations caught. |
| R4-2 | The document draft's text was keyed by block index, so a moved block lost its words, and a vanished block reopened nothing. | Confirmed. | Accept. Both editors key a draft's text by file only. The document draft reopens on its block; if the block is gone, it opens with its words and a note, and sending asks for the text again. | The remount test covers a block moved by an insertion above (the words follow) and a deleted block (words kept, note shown). 2 mutations caught. |
| R4-3 | A remounted composer could send a pending reply a second time. | Confirmed. The in-flight guard was per component. | Accept. The in-flight state is kept in the store with the draft (`beginKeptSend` and `endKeptSend`) for the review and anchored reply boxes, rejection reasons and new-thread drafts. Completion clears only the text that was sent, even while the composer is hidden. | Panel test: a pending reply is hidden and shown again; Enter sends nothing; newer typing survives the settle. Thread-card and code-draft tests: shown again while pending, still one call. 4 mutations caught; one needed a wait in its test first, because the second call arrives only after hashing. |
| R4-4 | Threads far down the CodeEditor margin, and unplaced ones, could not be reached. | Confirmed by layout. The margin was a non-scrolling sibling of the scrolling text in an `overflow-hidden` row, with threads placed at full-file offsets. This predates the change but blocks the requested margin. | Accept. The margin scrolls, is at least as tall as the text, and follows the text's scroll in the read-only viewer, Monaco (`onDidScrollChange`) and the fallback textarea. Monaco's row height is pinned to the margin's 20 px; it defaulted to about 18 px on Linux. | Editor test: scrolling the read-only viewer and Monaco scrolls the margin to the same offset. Playwright: a thread on row 150 of a 150-row file is in view with its row, leaves at the top and returns at the end. 3 mutations caught. |
| R4-5 (P2) | Browser acceptance is unverified. | Known. A manual Cloud Build run is refused for this trigger, and a local stack does not fit the disk. | The `g5-ci-e2e` lane on the PR head runs both flows (the original one and the long-file one) and must pass before merge. | Pending. |

Validation of the round-4 tree:
- **Frontend:** `tsc` passes, ESLint reports 0 errors with no new warnings beyond the three recorded earlier, and Prettier is clean on changed files; vitest passes 240 files and 3659 tests.
- **Mutations:** round 4, frontend 12 of 12 caught.
- **Backend:** unchanged since `2b28fa84`, so its validation stands.

## Round 5

The Codex CLI reviewed `b33bf192` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R4-1 and R4-3, for the cases reported.
- **Resolved in part:** R4-2 and R4-4.
- **Still open:** R4-5.

Three of the five new defects were in the store added in round 3 to keep drafts above the page. Each repair of that store had opened another path: drafts crossing users, keys lost when blocks moved, duplicate sends, and now lost words and lost errors. The review had reached its five-round default, so the agent asked the user how to proceed. The user chose to fix the root ([decision](chat_intent/comments-design/request.md#review-round-5-decision)): remove the store, and keep the workspace page mounted through short refresh failures, the failure the store had been working around.

The repair is [page refresh](ontology/features/workspaces.g5.md#keep-the-workspace-page-through-a-failed-refresh).
- **What stays:** on a transient failure (no answer, a timeout, rate limiting, a server error), the workspace detail, the open file's content and the approvals state keep their last loaded value, with a status beside them. A refusal still shows the error.
- **Composers:** they are therefore no longer unmounted by a refresh, and keep their text, their send in flight and its error in their own state.
- **Controls:** they stay as last known, and the server decides each action. Approval actions keep their existing rule and wait for current workspace details.
- **R3-2 follow-on:** a review source whose refresh fails keeps its last loaded state. A closed source stays closed, with the reason known then.

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R5-1 (P0) | A late successful create erased a newer draft's words and closed it. | Confirmed. The store's file-level text key was cleared on any success. Both editors closed whichever draft was open, so the close survives the store's removal. | Accept. The store is removed. A new-thread draft is locked while it is sent, and neither Cancel nor Escape closes it until the answer arrives. Success clears only the words sent, and closes the draft only if it is still the one sent. The document editor reads whether its comments panel is open when the send settles, not when it began. | Both editors: another line or paragraph is chosen while the first draft is sent. The first one's success leaves the new draft open and empty, and it sends its own words to its own line. While a draft is sent, its text is read-only and Cancel and Escape do nothing. 7 mutations caught. |
| R5-2 | The editable code margin could not follow its lines. Its extent came from the viewer's line list, which is empty in Monaco and the fallback. Monaco's suggestion cards were not counted. | Confirmed. | Accept. The margin takes each line's top from the active editor. In Monaco that is `getTopForLineNumber`, which counts padding and view zones. It is read again on `onDidContentSizeChange`, on `onDidLayoutChange`, and on `onZonesChanged`, which `MonacoSuggestionZones` now calls after placing, moving, resizing or removing a zone. The read-only viewer keeps its rows and zones. The extent is the content height plus one screen, so the margin never stops scrolling first. The editable fallback wraps lines, so it lists threads in line order, with the draft first. | Editor test with padding and a 300 px card in Monaco's rows: the thread sits at its row, an unplaced one follows the text, the extent follows the content, and the thread moves when the card grows. A zone test for the callback. A fallback test for the list. The Playwright flow adds a thread on row 20 of 150: it sits within 24 px of its row and leaves the view at the end of the file. 6 mutations caught. |
| R5-3 | A failed send's reason was lost when its composer remounted. | Confirmed, for the store's remounts. | Accept, by the root repair. Composers are not remounted by a refresh. A hidden review discussion stays mounted. A new-thread draft cannot be closed while it is sent. | Panel test: a reply refused while its discussion is hidden shows its reason and words when shown again, sent once. Page test: a reply and an open draft through a failed workspace refresh. 1 mutation caught. |
| R5-4 | A document draft could not be reached when every block disappeared. | Confirmed, in the store's recovery path. | Resolved by removing the store: nothing is recovered after the editor unmounts. While it is mounted, the draft stays open where it was. If its block is gone, it says the text changed. | The duplicate-paragraph test's last step: the text changes under an open draft, which keeps its words and asks again. |
| R5-5 | The document editor refused comments on either of two identical paragraphs. | Confirmed. This predates the branch, because the shared resolver refuses twins. | Accept. The draft records the text shown when it opens. While that text is unchanged, the chosen block stands (`locateDraftCommentBlock`). After a change, the block must be the only match. The shared resolver is unchanged. | Editor test: each of two identical paragraphs anchors to its own line; after a change, the draft asks again and keeps its words. 1 mutation caught. |
| R5-6 (P2) | Browser acceptance is unverified. | Known. | The `g5-ci-e2e` lane on the PR head runs both flows, including the new early-row case, and must pass before merge. | Pending. |

Tests that depended on the removed store were deleted with it: reopening a kept draft, one send after a remount, and drafts forgotten on a principal change. The page test that expected the error screen now expects the page to stay. New page tests cover a 403 on refresh, which still shows the error, and failed refreshes of file, document and ontology content. An approvals test covers a failed approvals refresh and a failed detail refresh, each with a review reply being typed.

Validation of the round-5 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file, the branch adds only the three warnings recorded in round 1. Prettier is clean on the 26 changed files, and vitest passes 240 files and 3664 tests. The Playwright flow type-checks, but it only runs in CI.
- **Mutations:** round 5, frontend 26 of 26 caught: 11 for page refresh, 6 for the margin, 1 for twins, 7 for drafts sent once and closed only when sent, and 1 for the hidden discussion. The 16 earlier mutations of the removed store went with it.
- **Backend:** unchanged since `2b28fa84`, so its validation stands.

## Round 6

The Codex CLI reviewed `f85d3f83` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R5-1 (success case), R5-2 (browser check pending), R5-4 and R5-5.
- **Resolved in part:** R5-3, with its remainder in R6-2 and R6-3.
- **Still open:** R5-6.
- **The review's judgment:** keeping the other panels that read the workspace detail at their last known state is acceptable under the page rule.
- **Found by the implementer while the review ran:** the open file's refresh status was a fixed overlay at the window's bottom right, where it could cover the comment margin or the chat. It is now a strip above the content.

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R6-1 (P0) | A periodic token refresh that adopts another person's token keeps the previous person's unsent replies on screen, sendable under the new identity. | Confirmed. The periodic path calls `setUser` with the new user without clearing the old one. `RequireAuth` returns the same children, so mounted composers keep their state. The round-4 store had cleared drafts there, and its removal reopened this. | Accept. `RequireAuth` keys what it guards by user and active org: another principal gets it afresh. A same-principal rotation keeps it. | Auth test through `AuthProvider` and `RequireAuth`: a rotation keeps the typed words in the same field; the next periodic refresh adopting another user gives a new, empty field. 1 mutation caught. |
| R6-2 (P0) | Transient refresh failures of three more queries still discarded page state. A failed name resolution showed "not found". A failed ontology tree changed the document editor's identity, which replaced unsaved text and undo history. A failed VM poll read as a stop and switched the code editor out of Monaco. | Confirmed for all three. The resolver refetches on reconnect after its five-minute stale time. The editor's key fell back to the node id. `ownerHasRunningAttachment` read a failed poll as not running. All predate the branch but are inside the page rule. | Accept. The three hooks keep their last loaded value on a transient failure, and the page status shows when any of them, or the detail, is retained. A document tab's editor takes its file from the tab while the tree cannot name it. Only API errors count as transient now, so a name that resolves to nothing still says not found. | Page tests: a 503 on resolution keeps the page, and a vanished name shows not found; a 503 on the VM poll keeps Monaco and its unsaved text; a failed (503) and a refused (403) tree refresh each keep a document's unsaved text, each variant catching one of the two repairs. 6 mutations caught. |
| R6-3 (P1) | Hiding the comments, or a card changing place, unmounted composers with their words, errors and send guards. | Confirmed. Both editors mounted their panel only while shown. Cards were children of their block, line zone or unplaced list. | Accept. Both editors keep the panel mounted once shown and hide it. Each suggestion card renders once into a DOM node of its own (`useSuggestionCardNodes`, `SuggestionCardPortals`), and each place it can show up (`SuggestionCardSlot`) adopts that node. A card that moves keeps its component, as Monaco's zones already did. | Editor tests: a reply sent, refused while the comments are hidden, shows its reason and words when shown again, with an open draft kept; a document card's reply, sent while a tag filter moves the card to the unplaced list, shows its refusal there with one POST; a code card's typed reply follows its line when lines are added above. 4 mutations caught. |
| R6-4 (P1) | A refused create could be retried against the line chosen while it was sent. | Confirmed. The unkeyed composer kept the first draft's words while the editor's draft moved to the new line. | Accept. While a new thread is sent, choosing another line or block does nothing, in both editors, so a retry goes to its own anchor. | Both editors: another line or paragraph is chosen while the first is sent; it is refused and retried, and the retry goes to the first anchor. Then another line can be chosen. 2 mutations caught. |
| R6-5 (P1) | Review comments recorded by the internal route did not match the file as the browser splits it. Rust's `lines()` drops carriage returns. The route also preferred the caller's text over the confirmed source line, so trailing spaces were lost. | Confirmed by reading `comment_anchor_at_source_head` and `resolveDisplayAnchor`. | Accept. The helper splits with `split_lines`, which keeps carriage returns, as hunks already do. The route records the confirmed source line whenever there is one. | API test: a CRLF file records `target\r` and a context with carriage returns; a line with trailing spaces records them even when the caller left them out. Placement cases for both shapes place the card in place. 3 backend mutations caught, one of them the round-3 check that the source line is recorded at all. |
| R6-6 (P2) | Two consumers of the changed hooks showed retained data without saying so. The explainer view showed its document silently, and the fork dialog could say "No approvals required" after a failed refresh. | Confirmed. The implementer also found the finalize band, which fails closed on unknown approvals, showing retained ones. | Accept. The explainer view shows a status above its document. The fork dialog says the approvals could not be loaded. The finalize band treats retained approvals as unknown. On the approvals page, actions also wait for current approvals. | Explainer and dialog tests. The approvals test now also expects approval actions to be unavailable after a failed approvals refresh. The band was checked by inspection, with its existing tests of unknown approvals. |
| R6-7 (P2) | Browser acceptance is unverified. | Known. | Unchanged. The `g5-ci-e2e` lane on the PR head must pass. | Pending. |

Validation of the round-6 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file, the branch adds only the three warnings recorded in round 1, after the card-node hook moved to its own module. Prettier is clean on the changed files. vitest passes 240 files and 3675 tests.
- **Mutations:** round 6, frontend 16 of 16 and backend 3 of 3 caught. Three controls first misbehaved, and were fixed before counting. Two tree tests checked before the editor had taken the failure in, so one mutation first survived; they now flush pending work first. One test raised a jsdom scrolling error from the editor; it now defines element scrolling.
- **Backend:** `cargo fmt --check` and strict clippy (`-D warnings`, all targets) pass on `g5-authz`, `g5-api-core`, `g5-api-routes` and `g5-api`. Library tests: 1013, 242 and 16. API tests: `api_comments` 31, `api_ontology` 34, `api_task_lifecycle` 43, `api_workspace_reviews` 15, `route_auth_static` 7, `tenant_scope_regressions` 3. The SQLx check passes. No migration changed.

## Round 7

The Codex CLI reviewed `8cde6374` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R6-4, R6-5 and R6-6.
- **Resolved in part:** R6-1, R6-2 and R6-3.
- **Still open:** R6-7.

The review had passed its default round count, and the agent had said it would ask before another round. The human chose to fix all three findings and review once more ([decision](chat_intent/comments-design/request.md#review-round-7-decision)).

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R7-1 (P0) | Another person's words could still be sent as them. A periodic refresh installs the new person's token before loading their profile, and the page stayed keyed by the old profile until it loaded, or for good if loading failed. Separately, a request answered 401 was sent again after a refresh, even if the refresh brought someone else's token. | Confirmed. The periodic path called `setApiToken` and then awaited `/users/me`. The silent path dropped the old user, but the periodic one did not. `request` retried after any refresh. The auth paths predate the branch, and R6-1's repair left them open. | Accept ([`g5.security.login.principal`](ontology/concepts/security.g5.md#act-for-one-principal-per-browser-tab)). The provider names the principal of every adopted token, and `RequireAuth` keys by it, before any profile loads. A periodic refresh that adopts another principal drops the outgoing profile at once, as the silent path does. The API client sends a request again after a refresh only for the same principal, and otherwise fails it with `PRINCIPAL_CHANGED`. | Auth tests: the page is gone while the new person's profile never arrives; an org switch gives a new page although the profile still names the old org; a rotation keeps it. API client tests: another principal's token is never replayed, and the same principal's is. 4 mutations caught. |
| R7-2 (P0) | Some network failures still discarded the page. They did not reach the classification as API errors: an answer cut off while read, or a token refresh that could not connect. | Confirmed. `response.json()` and the refresh's `fetch` ran outside the client's network wrapper. Round 6's narrowing introduced the gap. The implementer found a sibling: the provider's periodic refresh treated no answer as an expired session and signed out, discarding the page. | Accept. The client reads every answer through `readJson`: a cut-off answer is a network failure (status 0), and an answer that is not JSON is `INVALID_RESPONSE`, which is not transient. The refresh's fetch reports no answer as a network failure and leaves the session alone. The provider's refresh tells a refusal from no answer (`requestToken`); after no answer the periodic refresh keeps the session. | API client tests for a cut-off answer, an answer that is not JSON, and a refresh with no answer. A page test where a file refresh is cut off while read keeps the editor and its unsaved text. An auth test where the periodic refresh gets no answer keeps the page. 4 mutations caught. |
| R7-3 (P1) | Two display changes still recreated composers. The Unresolved filter dropped a thread with a reply in flight, and the code view switching between Monaco, its fallback and the viewer made new card instances. | Confirmed. `visibleCommentThreads` filtered the array that was rendered. Monaco's zones and the fallback list each rendered their own cards. The switch was a recorded gap; the filter was not. | Accept. SpecEditor renders every thread and hides those the filter excludes (`hiddenThreadIds`), so the empty-state note no longer replaces the column. The code editor renders every card once, into its node. Monaco's zones and the fallback list adopt the node, as the viewer's rows do. | Editor tests: a reply sent, its thread resolved remotely and hidden by the filter, then refused, shows its reason and words when the filter is turned off. A card's typed reply survives viewer to Monaco and back. A fallback test covers the fallback to the viewer and back. 3 mutations caught. |
| R7-4 (P2) | Browser acceptance is unverified. | Known. | Unchanged: the `g5-ci-e2e` lane runs both flows on the PR head and must pass before merge. | Pending. |

Validation of the round-7 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file, the branch adds only the three warnings recorded in round 1. Prettier is clean on the changed files. vitest passes 241 files and 3687 tests; since then one test changed (the org switch is now triggered by a button, for the lint rule), and the auth suite passes with it.
- **Mutations:** round 7, frontend 11 of 11 caught. Under one mutation, the no-answer auth test kept redirecting to sign-in and never finished, rather than failing. It now runs inside routes, so a sign-out fails it cleanly.
- **Backend:** unchanged since `8cde6374`, so its validation stands.

## Round 8

The Codex CLI reviewed `0408dc6a` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R7-3, for the reported transitions.
- **Resolved in part:** R7-1, with its remainder in R8-1 and R8-2, and R7-2, with its remainder in R8-3.
- **Still open:** R7-4.

The human had asked to be consulted if round 8 still found problems. They chose to fix the findings in the comments and auth paths, and to move the app-wide cache split to its own follow-up task, stated as a gap in the security rule. Round 9 runs with that split out of scope ([decision](chat_intent/comments-design/request.md#review-round-8-decision)).

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R8-1 (P0) | A new thread whose anchor was still being hashed could be sent after another person's token arrived, with the first person's words. | Confirmed. Both editors await hashing before they call the page's handler, and the API client reads the token when the request starts. The keyed page was gone, but its pending send was not. The deferral predates the branch. | Accept ([`g5.security.login.principal`](ontology/concepts/security.g5.md#act-for-one-principal-per-browser-tab)). Every comment- and review-thread write carries the principal the page or card was rendered for: starting, replying to, resolving, reopening and deleting a thread, and replying to, accepting, rejecting, resolving or withdrawing a suggestion. The API client refuses a request made for one principal under another's token (`PRINCIPAL_CHANGED`). The implementer first bound new threads to the token read at submit. They then moved the binding to the rendered principal, so a click that arrives after another principal's token, before the page re-renders, is refused too, and extended it from words to every thread write. Actions outside comment and review threads are not bound; that app-wide gap is recorded for a follow-up task. | Page tests: a thread whose hashing is held is refused when another person's token arrives, and sent after a renewal. Replies to a thread and to a suggestion, and decisions (resolve, reopen and delete a thread, resolve a suggestion), sent after another person's token arrived but before the page re-rendered, are refused, keeping the words. After a renewal they are sent. A Reviews panel test does the same for accepting a finding's changes. An API client test refuses a request made for another principal. 13 mutations caught. |
| R8-2 (P0) | After a principal change, the previous principal's workspace, file and comment data stays in the shared query cache, where the new principal's page can read it until it is fetched again. | Confirmed. The query client lives above the keyed page, and only review metadata and scoped configuration are evicted. This predates the branch and is app-wide. | Deferred by the human to a follow-up task, and recorded as a gap in the metadata of `g5.security.login.principal`. Out of scope for round 9. | None in this branch. |
| R8-3 (P0) | A token refresh answered 408, 429 or 5xx still signed out, discarding the page. | Confirmed. The API client's refresh called `onAuthFailure` on every unsuccessful answer; the provider's own refresh already told them apart. This predates the branch. | Accept. The client's refresh reports those answers as `REFRESH_UNAVAILABLE`, with their status, and leaves the session alone; only a refusal signs out. | API client tests: a refresh answered 408, 429 or 503 keeps the session for plain and raw requests; a refused refresh signs out. 1 mutation caught. |
| R8-4 (P1) | A periodic refresh that only changed the same principal's scopes dropped the page. | Confirmed. Round 7's repair dropped the profile on any claim change, and the silent path had the same shape before the branch. | Accept. Only a change of person or org drops the outgoing profile, on both paths; scopes update in place. | Auth tests on both paths: wider scopes for the same person keep the page and its typed words while the new profile never arrives. 2 mutations caught. |
| R8-5 (P1) | A suggestion decided by someone else removed its in-file card while a reply was being sent, losing the words and the refusal. | Confirmed. The file view asked only for open suggestions, so a decided one's card unmounted. | Accept. The file view asks for every status of the current commit. It shows open cards, plus a decided card whose reply box holds unsent words or a send in flight, with its decision, until those settle (`suggestionComposerActivity.ts`, replaced in round 9 by `useComposerActivity.ts`). | Page tests in the code and document views: a reply is sent, the suggestion is decided, and the refusal shows on the card with its words, sent once; the card leaves once its words are cleared. An overlay test. 3 mutations caught. |
| R8-6 (P3) | The design's inventory of ontology changes missed the two blocks added during the review. | Confirmed. | Accept. The inventory lists `g5.workspace.page.refresh` (round 5) and `g5.security.login.principal` (round 7), and says their metadata follows each round. | Every block whose body or metadata the branch changes was checked against the inventory; all are listed. |
| R7-4 (P2) | Browser acceptance is unverified. | Known. | Unchanged: the `g5-ci-e2e` lane runs both flows on the PR head and must pass before merge. | Pending. |

Validation of the round-8 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 560 warnings. Per file and rule, the branch adds the three warnings recorded in round 1, and one from round 8: `useRenderedPrincipal` is exported beside `useAuth`, as that file already exports hooks. The review discussion cannot use `useAuth` itself, because editor tests render its cards without an auth provider. Prettier is clean on every TypeScript file the branch changes, and vitest passes 241 files and 3708 tests.
- **Mutations:** round 8, frontend 19 of 19 caught: 13 for the principal binding, 1 for the refresh, 2 for scope changes and 3 for kept cards. Three first ran with a test-name pattern that matched nothing, because vitest escapes the apostrophe in "person's"; with the pattern corrected, all three were caught.
- **Backend:** unchanged since `8cde6374`, so its validation stands.

## Round 9

The Codex CLI reviewed `5e0c6702` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R8-1, R8-3, R8-4 and R8-6, and R8-5 for the reported case.
- **Still open:** R7-4.
- **The review's judgment on the implementer-recorded gap:** binding actions outside comment and review threads is acceptable as a separate follow-up for this change. It noted that the deferral was not human-approved.
- **Merge:** `origin/main` had moved to `967bf34c` (#264). It was merged as `9d123eab` with two conflicts. In `g5.governance.review-workspace`'s metadata, the branch's lineage note and main's assessment of the `g5d mcp` round tools are both kept. The HTML preview in `WorkspacePage.tsx` keeps the branch's thread list and gains main's `workspaceId`. The formerly retained agent decision path is now retired by the [October 24 removal](chat_intent/workspace-colab-harden/remove-agent-review-comments.md). Main's review backend changes are doc comments only, and it adds no migration.

The human had asked to be consulted again if round 9 still found problems. They chose to fix all four P1s and the metadata line, merge main, and review once more ([decision](chat_intent/comments-design/request.md#review-round-9-decision)).

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R9-1 (P1) | A late profile answer could overwrite the current principal's profile, so the page showed one person or org while requests went as another. | Confirmed. Initial sign-in, the periodic refresh and the org switch set the user after awaiting `/users/me` without checking that their token was still current, and explicit adoptions never advanced the silent path's epoch. Predates the branch. | Accept. Every path stores a loaded profile through `showProfileFor`, which shows it only while the token it was loaded for names the principal in use. | Auth tests: an older answer for org A arrives after an org switch to B, or after another person's silent adoption, and the profile stays B's or the other person's. In a control, the late answer is shown when nothing else happened. 1 mutation caught. |
| R9-2 (P1) | A thread deleted elsewhere unmounted a reader's reply being sent, with its words and the refusal; deleting a document's only thread also removed the SpecEditor column. | Confirmed. Both editors and the HTML side list render threads only from the fetched list, which the `comment.deleted` refetch empties. The list-driven rendering predates the branch; the live delete makes it immediate. | Accept. Each thread's reply box reports its words or send (`useComposerActivity.ts`). The page keeps a thread that leaves the list as last seen, marked deleted, while its box is active (`useCommentsKeptWhileAnswered`). The thread says it was deleted, keeps the words read-only with Send disabled, stops loading replies, and leaves when dismissed. | Page tests in the code view (a reader) and the document view (its only thread): the reply is held, the thread is deleted, and the refusal shows on the same thread with the words, one POST, Send disabled; Dismiss removes it. 4 mutations caught. |
| R9-3 (P1) | Regrouping in the Reviews panel unmounted replies being written or sent. A superseded round left the list when its last open suggestion closed, earlier rounds were hidden, an outdated group was collapsed, or a competing cluster dissolved. A clustered suggestion also had a second card with separate state. | Confirmed for each transition. Predates the branch. | Accept. Each card is rendered once, into its own node, and its place adopts it, as in the editors. A clustered suggestion is shown in the cluster, and its round points there. A card stays mounted while its round is hidden. A round shown as current stays while one of its cards is being answered or decided. When the source's next commit moves such a round into an outdated group, that group opens. A round the person shows or hides stays theirs to show or hide. | Panel tests: typed words survive hiding and showing earlier rounds, collapsing and opening an outdated group, and a cluster dissolving back into the round. A round keeps its reply being sent, then its refusal, when its last open suggestion is decided elsewhere, and when the source commits. 4 mutations caught. |
| R9-4 (P1) | A rejection reason was lost when the suggestion was withdrawn first. | Confirmed. Only the reply box reported activity, and the reason form showed only while Reject was allowed. Predates the branch; a sibling of R8-5. | Accept. A card reports a decision in flight or refused, or a typed reason. Once the suggestion is decided, the form stays with the reason read-only and Reject disabled, and Cancel releases the card. A refused decision on a decided card can be dismissed. A reason kept while the source is read-only stays hidden until it is writable again, as in round 2. | Page tests: in the code view, a reason typed or being sent when the suggestion is withdrawn stays readable, with the status Withdrawn and no Reject; cancelling releases the card. The document view repeats the case of a reason being sent. 3 mutations caught. |
| R9-5 (P2) | Browser acceptance is unverified. | Known (R7-4). | Unchanged: the `g5-ci-e2e` lane runs both flows on the PR head and must pass before merge. | Pending. |
| R9-6 (P3) | `g5.story.discuss-a-precise-line`'s metadata still listed gaps the branch had closed. | Confirmed; the stale line was written by the branch. | Accept. It now names what implements the story and that browser placement is not yet observed. | Inspection. |

Validation of the round-9 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 560 warnings. Per file and rule against the new merge base `967bf34c`, the branch adds the four warnings recorded before. Two more from the deleted-thread notice were removed by stopping the click at its button. Prettier is clean on every TypeScript file the branch changes. vitest passes 242 files and 3722 tests (main's merge added tests), and after the notice's last edit, the deleted-thread tests pass again.
- **Mutations:** round 9, frontend 12 of 12 caught. One control first matched nothing, because vitest shortens a long `$how` value in the test name; with a pattern on its untruncated start, the mutation was caught.
- **Backend, after the merge:** `cargo fmt --check`, `cargo check --workspace --all-targets` and strict clippy on the four API crates pass. Library tests pass 1012, 242 and 16; main removed two `doc_service.rs` tests and added one. API tests: `api_comments` 31, `api_ontology` 34, `api_task_lifecycle` 43, `api_workspace_reviews` 15, `route_auth_static` 7, `tenant_scope_regressions` 3. The SQLx check passes. No migration changed.
- **Ontology:** the format check reports no problem beyond those at `967bf34c`.

## Round 10

The Codex CLI reviewed `c590bacb` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R9-3, R9-4 for rejection reasons, R9-6, and R9-1 for late answers.
- **Not fully resolved:** R9-2 (the directory view).
- **Still open:** R9-5 (browser acceptance).
- **The merge with `main` was judged correct.**

Three of the four P1s, and the P2, were further cases of a composer losing what was typed when the view changed, after rounds 5–9 had found others. The human had asked to be offered the page-level owner in that case, and chose it ([decision](chat_intent/comments-design/request.md#review-round-10-decision)). The design's composer-lifetime decision was amended first: [one owner of what is typed, per page](ontology-plan-comments.md).

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R10-1 (P1) | An org switch kept the outgoing profile until the new one loaded, or for good if it failed. Through the shared session that profile could belong to another person than the new token. | Confirmed. `switchOrg`'s expected adoption skipped the profile clearing the other paths do. Predates the branch. | Accept. When the shared session answers for another person, the profile is dropped as the token is adopted. For the same person, the profile names the new org at once. | Auth tests: an org switch answered for another person, and for the same person, while the new profile never arrives. 2 mutations caught. |
| R10-2 (P1) | A directory node's thread list skipped the deleted-thread protection. | Confirmed; the directory view passed the node threads directly. The implementer missed it in round 9. | Accept. The directory view shows the node threads through the same keeping as every other view. | Page test: a reader's reply to a directory node's only thread is held, the thread is deleted, and the refusal shows with the words, one POST and Send disabled; Dismiss removes it. 1 mutation caught. |
| R10-3 (P1) | A commit of the source dropped an in-file card that was being answered or decided, because the file view asks only for the current commit's suggestions. | Confirmed. Predates the branch. | Accept. `useFileSuggestions` keeps the last-seen row of an active suggestion that leaves the list, scoped to the file, and the card says it is from an earlier commit of the source. A card with nothing held leaves as before. | Page tests: in the code view, a reply held when the commit lands stays with its refusal, and an untouched card on the same file leaves; the card leaves once its words are cleared. The document view keeps a card with typed words. 2 mutations caught. |
| R10-4 (P1) | An edited replacement was lost when the suggestion was withdrawn. | Confirmed. Predates the branch. | Accept. The edit is held by the page's owner and keeps the card. Once the suggestion is decided, the edit stays readable with Accept unavailable, until dismissed. | Page tests in the code and document views; a code-view test where a resolve is held and then refused on a withdrawn card, with nothing typed. 4 mutations caught, including the held and refused decision. |
| R10-5 (P2) | Switching a file between Source and Rich removed the source editor with its drafts and replies. Codex asked for an explicit decision. | Confirmed: both shells mount the source editor only in Source mode. Predates the branch. | Decided by the human's choice of the page-level owner. Composer state lives in one owner per page (`useComposerStore.ts`, `ComposerOwner`), scoped by workspace and inside the page a principal change replaces. Thread and suggestion replies, rejection reasons, edited replacements, decisions, and each file's new-thread draft are held there, so removing a composer loses nothing. The round-9 activity reports are replaced by the owner's active subjects. | Page test: on an HTML file, a held reply and an open draft with words survive Source to Rich and back; a second Enter sends nothing; the refusal arrives with the words; a cancelled draft leaves nothing behind. Store tests. 4 mutations caught: every composer owning its state again, a cancelled draft keeping its words, and a typed reply not keeping its thread or card. |
| R9-5 (P2) | Browser acceptance is unverified. | Known. | The `g5-ci-e2e` lane runs both flows on the PR head and must pass before merge. | Pending. |

Validation of the round-10 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `967bf34c`, the branch adds the four warnings recorded before, and the new store and owner modules none. Prettier is clean on every TypeScript file the branch changes, after formatting two files the ladder flagged. vitest passes 243 files and 3735 tests.
- **Mutations:** round 10, frontend 13 of 13 caught.
- **Backend:** unchanged since the merge `9d123eab`, so its validation stands.
- **Ontology:** the format check reports no problem beyond those at `967bf34c`.

## Round 11

The Codex CLI reviewed `53e78cdb` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R10-1, the original R10-4 and R10-5 cases, and the original R10-2 omission.
- **Not fully resolved:** R10-2 and R10-3, because a tab switch defeated them (R11-1).
- **Still open:** R9-5.
- **The review's judgment:** the owner's structure is appropriate: keys and scope, principal isolation, and sends that settle after their composer is gone.

These were gaps in completing the design the human chose in round 10, not a new design question, so the implementer repaired them and ran another round.

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R11-1 (P1) | A tab switch made a kept subject unreachable. The record that shows a deleted thread or a card from an earlier commit lived in the view, so a view built again lost it while the page still held the words and the refusal. | Confirmed: the retention hooks kept those records in view state. | Accept. The page's owner also keeps, for an active thread or suggestion, the record a view last listed (`useComposerRecords`). It is kept only while the subject is active and released with it. Each view shows the kept records of its own file or node. | Page tests: a deleted thread, a directory node's deleted thread, and a card a commit took out of the list each have their reply refused while the person is on another tab. On return, each is shown with its label, the words and the refusal, and one POST. A hook test: a kept card shows on its own file only. Store tests: a record is kept only while active and dropped when the subject settles. 6 mutations caught. |
| R11-2 (P1) | A successful Accept edited or Reject cleared words typed while it was sent. | Confirmed: both inputs stayed writable during the decision. | Accept. The edited replacement and the rejection reason are read-only while a decision is in flight; since that state is in the owner, a card built again is locked too. | A panel test for Accept edited and a page test for Reject: newer typing is refused, and only the submitted version is sent. 2 mutations caught. |
| R11-3 (P2) | A cancelled rejection reason kept a decided card shown, with no way to release it. | Confirmed: round 10's reason predicate made the words alone keep the subject, and Cancel kept them. Introduced by round 10. | Accept. Cancel clears the reason with the form. | Page test: a reason typed, then cancelled, and a withdrawal: the card leaves. 1 mutation caught. |
| R9-5 (P2) | Browser acceptance is unverified. | Known. | The `g5-ci-e2e` lane runs both flows on the PR head and must pass before merge. | Pending. |

Validation of the round-11 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `967bf34c`, the branch adds the four recorded warnings. Prettier is clean on every TypeScript file the branch changes. vitest passes 243 files and 3743 tests.
- **Mutations:** round 11, frontend 9 of 9 caught. One first survived: in the page test the other tab never shows a kept card, so dropping the per-file filter went unseen. A hook test now checks the filter, and it catches the mutation.
- **Backend:** unchanged since the merge `9d123eab`, so its validation stands.
- **Ontology:** the format check reports no problem beyond those at `967bf34c`.

## Round 12

The Codex CLI reviewed `675ac5a1` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R11-1 for the reported cases, R11-2 and R11-3.
- **Still open:** R9-5.

Keeping typed words alive had now come up in every round from 5 to 12, so the agent asked the human before repairing. They chose to fix all three and review once more ([decision](chat_intent/comments-design/request.md#review-round-12-decision)).

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R12-1 (P1) | Two lists storing the same thread's record could loop endlessly. A comment from before file identity can carry both a file and a node, so an ontology view lists it twice. When that view's two retention hooks held different copies, each write changed the reader both depended on. | Confirmed by inspection; introduced by round 11. | Accept. `useComposerRecords` returns a stable `remember` and a separate `get` that changes with the records. The writing effects depend only on the writer, so a write never runs them again. The store ignores a record equal to the one it holds. | A page test where the file and the node list return differing records of one thread: typing a reply settles, and a deletion is then kept. A store test for an equal record. 2 mutations caught, the first by the loop it brings back. |
| R12-2 (P1) | A card kept from an earlier commit kept its last-seen status, and still offered decisions after being decided here or elsewhere. | Confirmed: nothing refreshed a row the current list no longer returned. Introduced by round 10. | Accept. While cards are kept away, `useFileSuggestions` reads every round on the file and applies their rows; it does so only when that list is read again, not when a record changes. A decision made here applies its own answer, so it shows even before the list is read again. | Page tests: a kept card decided here, with the list no longer answering, shows Rejected from the answer; decided elsewhere, it shows Withdrawn from the list. Each keeps its typed reply and offers no decision. 3 mutations caught, one of them the old list being applied again. |
| R12-3 (P2) | When the workspace turned read-only or the ownership moved, the box with the words disappeared, though the words were still kept. | Confirmed. Introduced by the branch. | Accept. A thread's and a card's reply words stay readable, read-only, with Dismiss, once replies close. The rejection reason stays readable, read-only, whenever Reject is not offered, which replaces round 2's hiding while the source is read-only. An edited replacement is read-only, with Dismiss, wherever it can no longer be accepted. | Page tests for a thread and a card whose reply is refused after the workspace turned read-only; a panel test for a reason and an edit after the ownership moved. The round-2 panel test now expects the reason readable and Reject disabled. 4 mutations caught. |
| R9-5 (P2) | Browser acceptance is unverified. | Known. | The `g5-ci-e2e` lane runs both flows on the PR head and must pass before merge. | Pending. |

Validation of the round-12 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `967bf34c`, the branch adds the four recorded warnings. Prettier is clean on every TypeScript file the branch changes. vitest passes 243 files and 3750 tests.
- **Mutations:** round 12, frontend 9 of 9 caught. While making the decided-here test rely on the decision's own answer, the implementer found that applying every round's list on each record change overwrote that answer; the effect now runs only when the list is read again.
- **Backend:** unchanged since the merge `9d123eab`, so its validation stands.
- **Ontology:** the format check reports no problem beyond those at `967bf34c`.

## Round 13

The Codex CLI reviewed `8f7f3d01` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R12-1, and R12-3 for the reported cases.
- **Not fully resolved:** R12-2, through R13-1.
- **Still open:** R9-5.

The user chose to fix all three and review again ([decision](chat_intent/comments-design/request.md#review-round-13-decision)). They then allowed five more review rounds, through round 18, before the agent must ask again ([budget](chat_intent/comments-design/request.md#review-budget-from-round-14)).

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R13-1 (P1) | After a decision on a card kept from an earlier commit, the list of every round, read before the decision, put Open back: on a tab return, or when another kept card was released. | Confirmed: the history effect stored whatever its cached list held, and a decision only invalidated the lists. Introduced by round 12. The same cause reaches a card of the current commit: when its list fails to refresh after a decision, the list read before it offers the decision again. That sibling predates the branch. | Accept, at two layers. A decision's answer replaces the suggestion in every cached suggestion list of the source (`useWorkspaceReview.ts::applyDecidedSuggestion`); a list narrowed to open rows, or to another status, drops it. A kept record takes a view only if it is not older (`suggestionRecords.ts::isLaterSuggestion`: a decision never gives way to an open view; of two decisions, the later stands, with times compared as instants); the store applies that rule per record kind. A status leaves open only once (`WHERE status = 'open'`), so an open view of a decided suggestion is always stale. List reads in flight are already cancelled on invalidation and on unmount, because the query function takes the abort signal. | Page tests: a kept card, decided, stays Rejected with its reply through another card's release and a tab switch while every round's list hangs. A card of the current commit shows Rejected, with its reply, when its list then fails; a panel test does the same in the Reviews panel. Hook tests cover a list narrowed to open rows and a later decision a list holds. Store and unit tests cover which view replaces which. Mutations below. |
| R13-2 (P1) | A new-thread draft still offered Comment after the workspace turned read-only or the ownership moved; only the send was refused. | Confirmed; predates the branch. | Accept. `DraftCommentInput` takes `closed`. The words stay readable and read-only, with a note; Dismiss replaces Cancel; there is no Comment button and no keyboard send. The code and document editors pass it once `onAddComment` is withdrawn. | Page tests in the code view (a gutter draft) and the document view (a draft from a block's menu), each through a real read-only refresh: the words stay readable and read-only, with no Comment. Cmd+Enter sends nothing and asks for no refusal. Dismiss closes the draft, and nothing is posted. 6 mutations caught. |
| R13-3 (P2) | A failure to read the earlier rounds, which kept cards depend on, was not shown. | Confirmed; introduced by round 12. | Accept. While cards are kept away, a load or refresh error of that list is the comment column's notice, unless the current list's own refresh error is shown. | Page test: the earlier rounds answer 503, the notice says so and the reply stays. 1 mutation caught. |
| R9-5 (P2) | Browser acceptance is unverified. | Known. The flows resume a workspace VM, so they need the full stack with a worker host. | The `g5-ci-e2e` lane runs both flows on the PR head; it must pass before merge. | Pending. |

A decided card that holds nothing more now leaves the file when the decision's answer arrives, not after the next refresh. One round-11 test checked the card element after the decision; that element is now detached, holding its last render. It checks the page instead.

Validation of the round-13 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `967bf34c`, the branch adds the four recorded warnings. Prettier is clean on every TypeScript file the branch changes. vitest passes 244 files and 3763 tests.
- **Mutations:** round 13, frontend:
  - Before the lists were reconciled, 8 of 9 were caught. The ninth, dropping the line that refuses an open view of a decided suggestion, is equivalent on real data: an open row has no status time, so the time comparison refuses it too. A unit test now catches it.
  - The reconciliation, the remaining draft check and comparing decision times as instants: 7 more caught.
  - With the lists reconciled, removing the records rule, or the overlay's use of it, no longer fails the page test alone, because no list holds the older view. A store test catches the rule, and removing both layers together fails the page test: 2 caught.
- **Backend:** unchanged since the merge `9d123eab`, so its validation stands.
- **Ontology:** the format check reports no problem beyond those at `967bf34c`.

## Round 14

The Codex CLI reviewed `c817699d` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R13-2 and R13-3.
- **Not fully resolved:** R13-1, through R14-2.
- **R9-5:** Codex now records it as the full-stack gate before merge.

Round 14 is the first of the five rounds the human allowed before being asked again, so the agent repaired without asking.

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R14-1 (P1) | A periodic token refresh answered after an org switch installed the token of the context it started in. It compared the answer with the token it had captured, so it saw no change and skipped the profile. Requests then went as the old context while the screen showed the new one. | Confirmed: the comparison used the captured token, never the one in use. A refusal answered late signed out a valid session the same way. The race predates the branch. | Accept. The periodic refresh drops an answer when another token was installed while it was out, whether the answer is a token or a refusal; the next interval refreshes again. The first refresh at sign-in does the same, leaving the newer token and its profile. Both compare against the token in the API client, which a silent adoption sets before the provider's own reference. | Auth tests: a periodic refresh held across an org switch and its profile load, then answered with the old context's token or a refusal; the switch's token, principal and profile stay. A token installed while the first refresh is out stays, with its profile, and loading ends. 2 mutations caught. |
| R14-2 (P1) | Two paths still showed a decided suggestion as open. A list whose first read went out before a decision was answered after it, and TanStack does not cancel a first read on invalidation (`query.ts`: only when data exists). A decision one list read, such as the Reviews panel's, did not reach a sibling list whose own refresh failed. | Confirmed in the installed TanStack 5.90.20 and by inspection. The round-13 rewrite of cached lists covered neither. | Accept. The rewrite is replaced: the client keeps the latest decided view of each suggestion it has read, from any list's read or any decision answer, keeping the later of two (`useWorkspaceReview.ts::noteDecidedSuggestions`). Every suggestion list is read through it (`useReviewSuggestions`, a `select` that is rebuilt when a view is noted), so a list's own state, such as a failed refresh, is untouched. A list narrowed to open rows, or to another status, drops a row decided since. | Page test: a reply typed on a file's card; the Reviews tab reads the withdrawal while the file's list can no longer be read. Back on the file, the card shows Withdrawn with no decision controls, keeping the reply. Hook tests: a list whose first read is answered after a decision shows it; a decision one list read shows on a sibling whose refresh fails, which still reports the failure; the answer reaches every list of the source and leaves a narrowed one; a later decision stands. The round-13 page and panel tests still hold. Mutations below. |
| R9-5 (P2) | Browser acceptance is unverified. | Known. | The `g5-ci-e2e` lane runs both flows on the PR head; it must pass before merge. | Pending, as a merge gate. |

Validation of the round-14 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `967bf34c`, the branch adds the four recorded warnings. Prettier is clean on every TypeScript file the branch changes. vitest passes 244 files and 3770 tests.
- **Mutations:** round 14, frontend: 12 of 12 caught. Two first survived because another layer covered them: the order check when a view is noted, which the reader's own check hid, and the subscription that reads idle lists again, whose list re-rendered for another reason. Each now has a test where only that layer can act.
- **Backend:** unchanged since the merge `9d123eab`, so its validation stands.
- **Ontology:** the format check reports no problem beyond those at `967bf34c`.

## Round 15

The Codex CLI reviewed `3c6fbb14` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R14-2.
- **Not fully resolved:** R14-1, through R15-1.
- **R9-5:** the full-stack gate before merge.

Codex also noted that the round-14 mutation log still showed the two first-run survivors; their reruns had gone to the terminal. They are now logged in `fe-mutations-r14b.log`, both caught.

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R15-1 (P0) | The API client's own refresh, for a request answered 401, installed and stored whatever it was answered, and signed out on a refusal, even after an org switch or a sign-out. A late answer could restore credentials after a sign-out or replace a newer principal, retiring that principal's page. The request was then sent again under the restored principal. | Confirmed: `refreshToken` in `api.ts` had no fence. The race predates the branch; it is the sibling of R14-1. | Accept. `refreshToken` notes the token in use when it goes out. If another was installed, or none is, by the time it is answered, the answer changes nothing, a token, a refusal or a failure alike. The request is then sent again only for the principal in use, through the existing replay check; after a sign-out it fails, saying the person is signed out. Plain and raw requests share the refresh. `switchOrg` is unchanged. | API client tests: a request's refresh, held, is answered with a token or a refusal after another principal's token, a sign-out, or the same principal's renewal. The newer token stays, nothing is stored, nobody is signed out, and the request is sent again only after the renewal, with the renewed token. A raw request is covered too. 4 mutations caught. A third check, after the answer's body is read, only covers a token installed during that read and is not tested alone. |
| R15-2 (P1) | A comment list whose first read was still out when a thread changed elsewhere, or when one was created here, kept that read's older answer. TanStack keeps a first read in flight instead of starting another (`query.ts`: it cancels only when data exists), and comment lists and replies do not poll. | Confirmed by the installed TanStack code. The create and reply race predates the branch; the update and delete handlers inherited it. | Accept. `lib/readAgain.ts::invalidateAndReadAgain` cancels a first read still out, then invalidates, so the next read goes out after the change. Every comment event (`comment.new`, `.reply`, `.updated`, `.deleted`) and every change committed here (create, reply, resolve, reopen, delete) uses it, for file lists, node lists and replies. | `ws.comments.test.ts`, rewritten with a real query client: for each event, a watched file list and node list, and for a reply the thread's replies, are read again after the event though their first read was out. The late first answer changes nothing. Hook tests: a thread created, and a reply sent, while the first read was out show once it answers. 6 mutations caught. |
| R9-5 (P2) | Browser acceptance is unverified. | Known. | The `g5-ci-e2e` lane runs both flows on the PR head; it must pass before merge. | Pending, as a merge gate. |

Validation of the round-15 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `967bf34c`, the branch adds the four recorded warnings. Prettier is clean on every TypeScript file the branch changes. vitest passes 244 files and 3788 tests.
- **Mutations:** round 15, frontend: 10 of 10 caught (`fe-mutations-r15.log` and, for four whose name patterns first matched nothing, `fe-mutations-r15b.log`).
- **Backend:** unchanged since the merge `9d123eab`, so its validation stands.
- **Ontology:** the format check reports no problem beyond those at `967bf34c`.

## Round 16

The Codex CLI reviewed `0a865925` and returned `VERDICT: CHANGES_REQUIRED`.
- **Not fully resolved:** R15-1, through R16-1 and R16-2; R15-2, through R16-3.
- **R9-5:** the full-stack gate before merge.

Each finding was checked against the code before its disposition was set:

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R16-1 (P0) | A request sent as A, answered 401 only after a sign-out, still started a refresh, from no token. A refresh cookie the server failed to revoke then signed the tab back in as A, and the write was sent again. | Confirmed: the refresh fence of round 15 compares with the token in use when the refresh starts, which was already none. Predates the branch. | Accept. A request answered 401 starts a refresh only while the token it was sent with is still in use (`api.ts::refreshForAnswerTo`), for plain and raw requests. Otherwise the existing replay check decides: sent again with the same principal's renewed token, or not at all. | API client tests: the first answer is held until another principal's token, a sign-out or the same principal's renewal. No refresh is asked for, nothing is stored, and nobody is signed out; only the renewal sends it again, with the renewed token. A raw request after a sign-out is covered too. 2 mutations caught. |
| R16-2 (P0) | Provider steps still changed auth state after a newer one. An org switch answered after a sign-out, another person's sign-in or a later switch installed its token. A periodic refusal, a silent adoption and an org switch each wrote after their eviction without checking again. | Confirmed by inspection. Testing the sign-out case also found that sign-in ran again on every navigation: `useNavigate` changes with each route under `BrowserRouter`, and the provider's effect depended on it. After a sign-out, its navigation to the sign-in page re-ran sign-in, which a refresh cookie the server failed to revoke answered. Predates the branch. | Accept. The provider changes auth state only while the token it acts for is installed, and checks again after each wait. An org switch fails, saying the session changed, after a sign-out, another person's sign-in or a later switch; a renewal of the same person's token does not stop it. The periodic refusal, the silent adoption and the org switch stop after evicting if another token arrived. Sign-in and the periodic refresh run once per mount (`navigate` through a ref), and the periodic refresh does nothing while signed out. | Auth tests: an org switch held, then answered after a sign-out, another person's sign-in or a later switch, keeps the newer token, profile, scopes and stored credential, with a renewal control. Tokens installed while a periodic refusal, a silent adoption and an org switch evict all stand. No background refresh runs after a sign-out whose server-side revocation failed. 6 mutations caught. |
| R16-3 (P1) | A reconnect still invalidated the comment lists and replies plainly, so a first read out across the disconnection kept its older answer. | Confirmed; the reconnect recovery came from this branch's first round. | Accept. The reconnect reads the workspace's file and node lists, and the replies, again through `invalidateAndReadAgain`. | `ws.reconnect.test.ts`, rewritten with a real query client: file, node and reply lists whose first reads are out when the socket drops are read again after the reconnect, and the late first answers change nothing. 2 mutations caught. |
| R9-5 (P2) | Browser acceptance is unverified. | Known. | The `g5-ci-e2e` lane runs both flows on the PR head; it must pass before merge. | Pending, as a merge gate. |

A known gap is recorded in the principal block's metadata: an org switch replaced by a later one fails, so its caller says it could not switch to the earlier org while the later switch goes ahead.

Validation of the round-16 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `967bf34c`, the branch adds the four recorded warnings. Prettier is clean on every TypeScript file the branch changes. vitest passes 244 files and 3800 tests.
- **Mutations:** round 16, frontend: 10 of 10 caught (`fe-mutations-r16.log`).
- **Backend:** unchanged since the merge `9d123eab`, so its validation stands.
- **Ontology:** the format check reports no problem beyond those at `967bf34c`.

## Round 17

The Codex CLI reviewed `220f6986` and returned `VERDICT: CHANGES_REQUIRED`.
- **Resolved:** R16-1 and R16-3.
- **Not fully resolved:** R16-2, through R17-1.
- **R9-5:** the full-stack gate before merge.

Codex checked the composer owner, suggestion status, authority loss, live comments and AC1–3 and AC6–9 again, and found no further defect there.

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R17-1 (P1) | A renewal with the same claims while an adoption evicted left identity half reconciled. Round 16's checks compared exact tokens, so the pending silent adoption or org switch stopped. The renewal's own handler only updates the token reference, so requests went as the new principal while profile and scopes stayed the old ones. | Confirmed by inspection; introduced by round 16. | Accept. After evicting, a silent adoption and an org switch continue when the token in use carries the same person, org and scopes as theirs (`useAuth.tsx::sameContextInUse`). They then finish with that renewed token: reference, scopes, stored credential and profile. They still stop after a sign-out or once another context is in use. The periodic refusal keeps its exact check, so any token installed meanwhile prevents the sign-out. | Auth tests, with the eviction held while the same claims are renewed: another person's silent adoption, a silent change of scopes, an org switch, and an org switch answered for another person each end with the renewed token, the new scopes and the new profile. The org switch stores the renewed token. The round-16 tests for another person's sign-in during an org switch's eviction, and for an org switch during a silent adoption's eviction, still hold. 4 mutations caught (`fe-mutations-r17.log`; the first rerun with a pattern that matches both its tests, `fe-mutations-r17b.log`). |
| R9-5 (P2) | Browser acceptance is unverified. | Known. | The `g5-ci-e2e` lane runs both flows on the PR head; it must pass before merge. | Pending, as a merge gate. |

Validation of the round-17 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `967bf34c`, the branch adds the four recorded warnings. Prettier is clean on every TypeScript file the branch changes. vitest passes 244 files and 3804 tests.
- **Backend:** unchanged since the merge `9d123eab`, so its validation stands.
- **Ontology:** the format check reports no problem beyond those at `967bf34c`.

## Round 18

The Codex CLI reviewed `ae511e66` and returned `VERDICT: CHANGES_REQUIRED`, with no P0 or P1.
- **Resolved:** R17-1.
- **R9-5:** the full-stack gate before merge.

Round 18 was the last of the five the human had allowed, so the agent asked. The human chose to fix all three and review again ([decision](chat_intent/comments-design/request.md#review-round-18-decision)). While that repair was under way, they allowed up to fifteen more rounds, through round 33. They also asked the agent to push back with reasons where a finding is out of scope, and to keep to the two cases they named ([budget and scope](chat_intent/comments-design/request.md#review-budget-and-scope-from-round-19)).

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R18-1 (P2) | A margin card that grew by itself, as its replies or an error arrived, was not measured again, so the next card could cover it. | Confirmed: `CommentColumn` measures only after its own renders, and a thread's replies load inside the thread. Predates the branch, but it is the CodeEditor margin this change reworked, one of the two named surfaces. | Accept. The column observes each card's size and measures again when one changes. One stable ref callback for every card, with the thread id in a `data-` attribute and React 19's ref cleanup, keeps a render from detaching and observing the card again. | Column tests, with the geometry stubbed and the size observer notified as a browser would: a card's replies, or a failure to load them, arrive late, and the next card moves below it. 1 mutation caught. |
| R18-2 (P2) | A review discussion whose first read was still out kept that read's older answer after a reply or decision made here, a review notice or a reconnect, until its ten-second poll. | Confirmed: the same first-read case as R15-2. The send path predates the branch; the notice and reconnect paths came from it. | Accept. Each of those paths reads the discussion again through `invalidateAndReadAgain`. | Hook tests for a reply sent and a decision made while the first read was out; a notification test and the reconnect test with a real query client. 4 mutations caught. |
| R18-3 (P3) | Two example cells in the design's table still said readers get no Reply and a 403 on anchored replies. | Confirmed; introduced on this branch, before the human's reader-reply decision was applied to the table. | Accept. The cells now say readers reply (201) in both thread kinds, and are refused (403) starting, resolving, reopening or deleting a thread. | Documentation. |
| R9-5 (P2) | Browser acceptance is unverified. | Known. | The `g5-ci-e2e` lane runs both flows on the PR head; it must pass before merge. | Pending, as a merge gate. |

Validation of the round-18 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `967bf34c`, the branch adds the four recorded warnings. Prettier is clean on every TypeScript file the branch changes. vitest passes 245 files and 3809 tests. Round-18 mutations: 5 of 5 caught (`fe-mutations-r18.log`).
- **Backend:** unchanged since the merge `9d123eab`, so its validation stands.
- **Ontology:** the format check reports no problem beyond those at `967bf34c`.

## Merge of main `ba525f53`

Codex approved `18116059` in round 19, with no blocking finding and no new follow-up. Before the PR could open, `main` had moved twenty commits. The new commits include SpecEditor inline review suggestions ([PR #265](https://github.com/G5Labs/G5-Lean/pull/265)) and host resource slots ([PR #263](https://github.com/G5Labs/G5-Lean/pull/263)). The merge was reconciled ontology first:
- **No contradiction in intent.** `main`'s new blocks, `g5.documents.suggestions.inline`, `.projection`, `.decision-scope` and `.placement`, show the same suggestion cards with the same actions. That agrees with `g5.governance.review-suggestions.display` and this change's composer owner. They sit in other sections of `documents.g5.md`, which merged without conflict.
- **Kept cards in the inline view.** `main` reads open hunks inline and gave every hunk no block card in that view. A decided card kept while someone answers it, or one kept from an earlier commit, would have vanished, or been read as a pending change. Only an open change of the current commit is now read inline; the others keep their block card.
- **Label-only title rows.** `main` shows their cards in the record's frame; this change had listed them as unplaced. Both keep the card visible, and `main`'s placement is the more specific, human-approved one. So title rows show their cards in the frame, and collapsed or filtered blocks keep the unplaced list. AC9 names only those two, so it is unchanged; the design's detail line is updated.
- **The resolver.** `main` replaced the line search with a linear one and removed `contextSurrounds`. This change's twin and insertion rules stay, on `main`'s search and `linePatternAt`, so twins with no telling context remain ambiguous even at the published line (K20, reversed in round 2).
- **Comments from a selection in the inline view** send this change's anchored draft. `main`'s renderer tests expected its block id and a synchronous send. They now wait for the anchor and read the line from it; what they check, the canonical quote, is unchanged.
- **Migrations.** `main` added 046–048, so this change's become 049–051, with unchanged contents and checksums. The design's migration decision and the governance metadata are updated.
- **WorkspacePage.** `main`'s seven changes (the doc tab's review presentation, the navigation origin, the inline prop and PROJECT DOCS opting in) were applied to this change's file. Every line `main` added is present.

New tests: an inline-view test where an open change of the current commit is read inline, while a decided card and one from an earlier commit keep their cards. The label-row test now expects the card in the frame.

Validation of the merged tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `ba525f53`, the branch adds the four recorded warnings. Prettier is clean on every TypeScript file the branch changes. vitest passes 248 files and 3977 tests. Merge mutations: 4 of 4 caught, two after a test for each was added or aimed at the right test (`fe-mutations-merge3.log`, `fe-mutations-merge3b.log`): every hunk read inline, label-row cards left out of the frame, a card with no block dropped, and twins settled by the published line.
- **Backend:** the `sqlx` schema database was rebuilt in the merged order (ledger 1–51). `cargo fmt --check`, `cargo check --workspace --all-targets` and strict clippy pass. Library tests pass: 1012, 242 and 16. API tests pass: `api_comments` 31, `api_ontology` 34, `api_task_lifecycle` 43, `api_workspace_reviews` 15, `route_auth_static` 7, `tenant_scope_regressions` 3, `workspace_search_removal_guard` 6. `cargo sqlx prepare --check` and the migration checksum check pass. The migrations' cross-references to each other were renumbered too.
- **Migration rehearsal:** rows from `a1c3e29e` were loaded, then the merged 043–051 were applied as `g5_migrator` through `g5-db-migrate` (9 applied), and the rerun was a no-op. All 4 mention edges got their workspace's organization, and the tenant role without org context sees none.
- **Ontology:** the format check reports no problem beyond those at `ba525f53`.

## Round 20

The Codex CLI reviewed the merge `3feb21fd` and returned `VERDICT: CHANGES_REQUIRED`. It kept every conflict choice: open changes of the current commit read inline, kept cards as block cards, title-row cards in the frame, the unplaced and no-block lists, the resolver on `main`'s search, and migrations 049–051. It found two places where `main`'s new renderer bypassed contracts the approved branch already met. Both concern review-suggestion cards in the editors, one of the two cases the human named.

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| R20-1 (P1) | The inline cards `main` added (`SpecReviewSuggestions.tsx`) set `viewerIsReviewer={false}` and passed no source writability. In PROJECT DOCS the reviewer could not withdraw, and a closed source still offered decisions and editable words. | Confirmed: every other card takes these from the overlay. Came in with `main`'s renderer. | Accept. The inline card takes the reviewer from `viewerPublishedSuggestion` and the source's writability and closed note from the overlay, as block cards do. That covers the inline, attention, source-edit and empty-document presentations, which share the component. | Editor tests through the real inline renderer: the reviewer sees Withdraw. The owner on a read-only source gets no Accept, Reject or Withdraw. A reply typed before the source closed stays readable, read-only, beside the closed note, with Dismiss. 3 mutations caught. |
| R20-2 (P2) | One `BlockSuggestions` call `main` added, for cards beside an inline change, passed no content, so a card there lost its moved, lost, ambiguous or unverified note. | Confirmed. | Accept. It passes the content and the card nodes, as the other calls do. | An editor test: a comment card whose line is gone, beside an inline change, keeps "not in this file as shown". 1 mutation caught. |

The tests sit at the SpecEditor boundary: the page already builds the overlay with these props, and the fault was in forwarding them to `main`'s renderer.

Validation of the round-20 tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `ba525f53`, the branch adds the four recorded warnings. Prettier is clean. vitest passes 248 files and 3980 tests. Round-20 mutations: 4 of 4 caught (`fe-mutations-r20.log`).
- **Backend:** unchanged since the merge `3feb21fd`, so its validation stands.
- **Ontology:** the format check reports no problem beyond those at `ba525f53`.

## Rounds 21–22 and the PR

The Codex CLI approved `ee491b2c` in round 21 and `8baa205a` in round 22, a merge of `main` that changed only CI. That opened [PR #271](https://github.com/G5Labs/G5-Lean/pull/271). After that approval, the human asked that the Codex CLI not be called again: fixes for CI and for bot findings rated P0, P1, Critical or Major go straight to the PR.

CodeRabbit's review was answered in `7973ecfb`:
- a thread is ambiguous when more than one line carries its whole recorded context, before the nearby step;
- a suggestion card offers controls only when its caller says the source is writable;
- the principal rule's main body states its scope;
- a request refused because nobody is signed in says so.

It deferred one finding, about the trigger in migration 052 (049 when reviewed), with a reply in its thread.

## Browser run on the PR

The `g5-ci-e2e` lane ran the Playwright flows for the first time on `8baa205a` (Cloud Build `31957e42`). It found two defects in this change (E1, E2); the Rust lane passed. Its run on `4cf9a823` (Cloud Build `888e663c`) passed 136 tests and failed one, on a fault in the new flow itself (E3).

| ID | Finding | Evidence | Disposition and repair | Regression evidence |
|---|---|---|---|---|
| E1 | No comment could be sent: both comments flows waited for a create request that never came. | Inferred from the setup, then confirmed by the next run. The CI page is served over plain HTTP from `g5-test-client` (`E2E_BASE_URL`), which is not a secure context, so `crypto.subtle` is missing there and every anchor digest fails before the request. So is a self-hosted install served over plain HTTP, without its TLS overlay. On `4cf9a823` (Cloud Build `888e663c`), with the fallback, the first flow passed and the second sent both of its threads. | Accept. `sha256Utf8` computes the digest in script when WebCrypto is missing (`lib/sha256.ts`, FIPS 180-4). `main`'s inline suggestions hash the shown text through the same function, so they are covered too. | Tests against the FIPS vectors, against WebCrypto at every padding edge and on multibyte text, and `sha256Utf8` with no WebCrypto. The last fails without the fallback. |
| E2 | Creating an org never reached its connect step: the org-creation flows and the PAT connect guidance (I7) timed out. | Confirmed. The wizard switches to the new org and then moves to its next step. `RequireAuth` keyed the page by person and org, so the switch replaced the page, wizard included, before that step. On `4cf9a823` the org-creation flows and I7 passed. | Accept. `RequireAuth` keys the page by person only. Each composer store is keyed by the principal it is rendered for, so comment and review-thread words still clear on an org switch ([`g5.security.login.principal`](ontology/concepts/security.g5.md#act-for-one-principal-per-browser-tab) now says so). This narrows an agent decision, not a human one: the human's choices in rounds 7 and 10 named a change of person. | An auth test with a flow that awaits its org switch, under `RequireAuth`, next to both composer owners. Keying the page by org again fails at the next step; leaving the stores unkeyed fails on the kept words. Five page tests render the page without `RequireAuth` and swap in another person's token. They had expected the refused words to stay on screen with the refusal; they now expect the words cleared at once, as the rule says and as the replaced page did in the app, and nothing sent. Removing the API client's principal check fails all five. |
| E3 | The long-file flow never found row 20 to measure its thread against, though the thread was posted on that line and shown. | Confirmed. Monaco 0.55.1 draws a space in a line as U+00A0, and Playwright tests a pattern against the text as drawn, so `/^row 20$/` matched no line. A probe in Chromium matched nothing with it and the line with `/^row\s20$/`. This part of the flow had never run, because E1 stopped it earlier. | Accept: a fault in the test, not in the margin. The pattern uses `\s` for the space. | The same probe. On `33933ce7` (Cloud Build `d4aaf99f`) both Playwright shards passed, this measurement included, and `11ead7bf` passed both lanes in full (Cloud Builds `2b8c90e1` and `b00e5bd7`). |

## Merge of main `25f4ba15`

`main` gained the remote-tracking display on Workspace Details and automatic completion of proven divergences, with migrations 049–051. Merged into `11ead7bf`:
- **Ontology.** Both sides add stories and rules in different places. `main`'s tracking status comes from the sync-runs feed, and `g5.workspace.page.refresh` keeps the workspace detail, so its unknown-state rule and the refresh rule govern different data.
- **Migrations.** This change's become 052–054, their comments name each other by the new numbers, and the checksum manifest lists `main`'s 049–051 first. The design's migration decision and the governance metadata are updated.
- **Code.** Only the checksum manifest conflicted. `types.ts` and `WorkspaceDetailsPanel.test.tsx`, changed on both sides, merged cleanly and type-check.

Validation of the merged tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `25f4ba15`, the branch adds the four recorded warnings. vitest passes 250 files and 4007 tests. Prettier is clean on this change's lines; `main`'s own unformatted lines in `WorkspaceDetailsPanel.test.tsx` are left as `main` has them.
- **Backend:** the `sqlx` schema database was rebuilt in the merged order (ledger 1–54). `cargo fmt --check`, `cargo check --workspace --all-targets`, strict clippy and `cargo sqlx prepare --check` pass, and so do the migration checksum and version-uniqueness checks (54 versions). Neither side's backend change touches the other's files; the integration tests run in the `g5-ci-rust` lane.
- **Migration rehearsal:** rows from `a1c3e29e` were loaded, then the merged 043–054 were applied as `g5_migrator` through `g5-db-migrate` (12 applied), and the rerun was a no-op. All 4 mention edges got their workspace's organization, the table forces row-level security, and the tenant role without org context sees none.
- **CI:** on `71fda35e` the Rust lane passed (`b3d0bab1`), and the browser lane (`d2bbf711`) passed both Playwright shards. Its fixture seeds lost the base-image lock race in `scripts/ensure-base-images.sh`, which this change does not touch, so the lanes that need those fixtures did not run, and CI was run again.

## Merge of main `72d2b4de`

`main` gained issue 238's selected-file checkpoints (explicit Save and accepted review files are committed and pushed; an acceptance whose publication fails stays accepted), design-principle checks and the migration owner cleanup, with no new migrations. Merged into `6d2dcbc2`:
- **Ontology.** No rule changed in meaning. `main`'s acceptance rules keep an applied change accepted, as this change's decided views require, and its Save rules carry the observed VM, which the principal rule leaves to the server.
- **Code.** Two conflicts. In `ReviewsPanel.tsx` the card keeps this change's accept flow (the edit stays until the decision succeeds) and gains `main`'s notice when publication is pending. In `WorkspacePage.tsx` the HTML preview keeps its comment list and its answer Apply takes `main`'s observed VM and save-only flag.

Validation of the merged tree:
- **Frontend:** `tsc` passes for both configurations. ESLint reports 0 errors and 559 warnings; per file and rule against `72d2b4de`, the branch adds the four recorded warnings. vitest passes 250 files and 4009 tests, `main`'s pending-publication card test included. Prettier is clean on this change's lines; `main`'s own unformatted lines are left as `main` has them.
- **Backend:** `cargo fmt --check`, `cargo check --workspace --all-targets`, strict clippy and `cargo sqlx prepare --check` pass (no new migrations; ledger 1–54). Integration tests pass: `api_comments` 31, `api_ontology` 34, `api_workspace_reviews` 16, `route_auth_static` 7, `tenant_scope_regressions` 3.
