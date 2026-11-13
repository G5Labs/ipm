# Issue #238 plain PR — merge-readiness review

## Review scope and method

This is a self-review of PR #257 after the focused hardening in the pinned
[ontology plan](ontology-plan.md). It follows the original [issue #238](https://github.com/G5Labs/G5-Lean/issues/238),
the [plain follow-up](../../chat_intent/issue-238-plain/request.md), and the
scope comparison cited there. No fresh independent reviewers were used; their
additional detection value is unmeasured. The implementation deliberately keeps
the plain selected-file operation rather than importing G5's durable coordinator,
receipt journal, database intent protocol, aggregate caps or identity retention.

## Findings and repairs

1. The plain JSONL source Save lacked a Save callback, while HTML answer Apply
   requested publication. The source editor now uses the document Save hook;
   HTML Apply remains a save-only write. A real Chromium test initially showed
   that setting the frontend flag alone did not move the branch: the overlay
   Project Doc backend route still used the ordinary file writer. That route now
   uses the shared selected-file writer with the observed VM and commit flag.
   Recheck: the JSONL browser case passes on the repaired API in Chromium,
   Firefox and WebKit; the first failure remains part of this review record.
2. A review acceptance could write the file and then fail its commit/push,
   leaving an open, rejectable suggestion. Acceptance now settles the applied
   decision and returns `checkpointPending`; the review panel reports it.
   A retry of the selected-file API operation checks the live version. The
   existing Changes button commits all dirty files, so its scope must be
   reviewed before using it for recovery.
   Recheck: injected API push-failure test passes; a live review failure
   injection was not run.
3. An editor Save could use bytes observed before VM replacement; a late
   cache response could also overwrite a newer attachment. File, document
   and ontology Save carry the observed VM; the backend rejects stale VM
   writes, and affected cache updates are fenced. SpecEditor acknowledges
   only submitted bytes while later edits remain dirty. Recheck: API VM
   rejection, editor in-flight unit test and live stale-hash flows pass.
4. The plain selected commit used a private tree but its later public-index
   updates could race native staging. A native index lock now protects the
   head check, index-copy reconciliation and install. It preserves unrelated
   staging and selected-path staging that differs from the old HEAD; unmerged
   selected entries are rejected. Recheck: dispatcher unit tests and live
   selected-file browser cases pass. A deterministic simultaneous native
   staging stress test was not run.
5. Final PR review found that document Save responses echoed the request's
   optional VM ID instead of the VM ID returned by the live writer. Both
   Project Doc and ordinary document responses now return the saved VM ID.
   The JSONL browser case checks that a save-only request without an observed
   VM ID still returns the current one and leaves the branch unchanged.
6. A Git pathspec for a selected directory could return a child's index entry,
   and Git clean filters could make the staged blob differ from the raw
   `fs.write` hash. Selected entries now require one exact NUL-delimited
   stage-zero path; raw worktree bytes must match the saved hash, and Git's
   path-filtered hash must match the staged blob. Parser unit tests and a live
   CRLF-to-LF filter checkpoint pass. This retains the normal Git clean-filter
   effect rather than promising byte-identical checkout content under
   repository attributes.
7. The outdated-review browser test could silently skip an optional toggle.
   It now locates the intended acceptance action, expands the outdated group
   only when needed and requires that action to become visible. File ownership
   fixtures also use a control file to observe the platform's default child
   group, which can differ from either the process group or its parent on
   different hosts.
8. Final feedback found that the JSONL save-only browser request resubmitted
   unchanged content, so an unchanged branch head alone could not prove that
   publication was suppressed after a real write. It now submits a distinct
   JSONL record and checks both live content and the unchanged head. Its
   targeted Chromium run passed on the rebuilt dev stack. The cloud E2E image
   build stopped before feature tests because the external Caddy apt source's
   signing key had expired. Both Edge Dockerfiles now copy the same Caddy
   2.11.4 binary from a digest-pinned official image. Uncached prebuilt and
   source-built images passed Caddy validation; the rebuilt dev Edge is healthy.
9. The broad all-feature API integration run found two structural guards still
   requiring `update_project_doc` to call the old file writer directly. The
   route now calls the selected-file Save helper, which calls that same owner
   writer. The guards now check both the transaction cut before the new helper
   and the helper's use of the owner writer. The original run passed 945 of
   947 cases; only those two guards failed. Their repaired focused suite
   passed 3/3. Cloud logs were initially inaccessible. After access was restored,
   the next head's concrete failures were inspected as described below.
10. Cloud Rust build `a34ec700-29af-41cd-928d-9b3bda45eb9e` on `63e28c75`
    exposed two needless borrows in selected-file push and a 720-second timeout
    in `historical_upgrade_preserves_history_but_cleanup_requires_reset`.
    Its other 985 integration tests and all 4,904 unit/dispatch tests passed.
    SQLx returns early on migration failure without unlocking its session
    advisory lock; the pooled session could then block a retry on another
    session. Checking every pooled session after the deliberate migration
    rejection reproduced one leaked lock locally before repair. The runner
    now closes its migration connection on completion, error or cancellation,
    without changing migration SQL or disabling locking. All eight focused
    fresh-install, populated-upgrade, replay and role-concurrency tests pass.
    The two borrows were removed without changing push behavior. Full-workspace
    Clippy with all targets and warnings denied, Rust formatting and diff
    whitespace checks pass on the repaired source.
11. Cloud E2E build `70c2d21b-38e5-4581-925b-087322bd4245` passed the Project
    Doc Save, durable-read and stale-write rejection checks but failed the
    subsequent editor refresh assertion on all three attempts. The test wrote
    immediately after reload without checking subscription readiness. It now
    waits for the server's workspace subscription acknowledgment before the
    external write, reusing the existing live-metrics test's observer through
    a shared helper. The refresh assertion remains intact. The focused case
    and authentication passed locally (2/2) on the existing PR dev stack.
    Cloud rerun `3c7ded31-1260-486a-ab87-2dba7fa8ef30` on `4269be26` still
    failed this case on three attempts, so subscription readiness was not a
    sufficient explanation or repair. The assertion remains in place; failure
    diagnostics now print observed file-change events, browser document reads,
    editor dirty state and the current server content. Cloud trace artifact
    access was requested because the account can read logs but not the artifact
    bucket. The restricted-role defect found below explains missing delivery;
    the repaired cloud E2E rerun remains pending.
12. Cloud Rust rerun `be6fae89-261e-409c-a4fb-dafdea9fe292` passed all 986
    integrations, including the migration retry, and the prior Clippy errors
    were absent. One of 4,904 unit/dispatch cases failed instead: the native
    Git HTTPS fixture panicked on EOF before TLS after HTTP CONNECT, destroying
    its listener. An explicit abandoned tunnel reproduced the same OpenSSL
    error locally in Linux. The fixture now checks for EOF before accepting
    TLS and continues serving; authenticated push, compensation, exact ref
    assertions and other handshake failures remain enforced. A server-thread
    panic can no longer hide the original Git push error. Both HTTPS cases
    passed 50 Linux repetitions with the abandoned-tunnel regression, and all
    154 SCM tests plus all-target SCM Clippy passed on macOS. The broader Linux
    stress experiment passed 20 runs of 153 cases; its separate credential-helper
    test could not run in the read-only container mount with an unresolved
    worktree Git directory, and is not counted as a Linux pass. No production
    transport behavior changed. Cloud Rust build
    `98680032-cc29-402e-b535-dbb3d0cd3068` on `fa1fb477` subsequently passed,
    including 4,904 unit/dispatch tests, 986 API integrations and 207 dispatcher
    integrations. The browser refresh still fails. A reporter probe confirmed
    that the CI progress/GitHub reporter pair keeps console diagnostics out of
    the build log; refresh diagnostics are now included in the thrown error
    so the inline annotation exposes them while retaining the original cause.
    The CI progress reporter also emits failed/timed-out test messages at test
    completion; the same reporter probe verified delivery before suite completion.

13. The PR #257 CodeRabbit follow-up reproduced the remaining indexed-attribute
    finding through the real dispatcher. With `.gitattributes` absent from the
    worktree, `git add` uses indexed attributes but `hash-object --path` does
    not, even with the same `GIT_INDEX_FILE`; unchanged CRLF bytes were falsely
    rejected. The checkpoint now uses its staged entry as the filtered
    representation and retains the raw saved-version comparison. Its regression
    failed before repair and now verifies filtered commit bytes, preserved
    working/staged state, and rejection of subsequently changed raw bytes.
    All 43 dispatcher module tests passed with two test threads. The initial
    default-concurrency run passed 42/43; its unrelated 1.5-second agent deadline
    case passed alone and in the bounded rerun. All-target Clippy passed for
    `g5d`, dispatcher and API routes. The ontology save-only response now takes
    its VM identity from the actual save result, matching the two document
    handlers; the existing ontology browser case checks changed save-only
    content, returned VM identity and an unchanged branch head. That added
    browser check is pending cloud execution. The other five comments are
    already covered: exact NUL-delimited stage-zero path validation, the
    filesystem's observed child-group fixture, targeted review-toggle
    visibility, CRLF Save coverage and distinct JSONL save-only bytes.

14. The live-refresh investigation found that both host file-change paths
    queried `workspaces` directly through the restricted system pool, whose
    tenant RLS hides the owner row. The existing integration test checked only
    that ephemeral events leave the lifecycle journal unchanged, so it missed
    silent notification loss. Adding real owner/reader WebSocket subscriptions
    reproduced the missing event before repair. Both paths now use the existing
    narrow `authz.lookup_workspace_runtime_authority` function already used by
    lease renewal, with workspace/VM/host organization matching; database grants
    and RLS are unchanged. All 29 worker-host integration tests pass. The expanded
    regression checks ephemeral and legacy journalled delivery, replay suppression,
    wrong-host/stale-VM/stopped/deleted/terminated rejection, nonowner silence,
    and delivery only to the new owner after ownership changes. All-target,
    all-feature Clippy for API core/API and Rust formatting pass. Cloud browser
    verification of this repair remains pending.

## Commitment and consumer checks

| Commitment | Actual path and distinguishing check | Result |
|---|---|---|
| `g5.git.durability.no-implicit` | Ordinary file write and HTML Apply do not request a checkpoint; explicit Save does. | API and Chromium tests pass. |
| `g5.git.checkpoint.selected` | Shared `commit_and_push_files` and guest selected-path tree; acceptance includes a new file, no-op Save leaves head unchanged. | Browser and API checks pass. |
| `g5.git.checkpoint.isolation` | Private Git index plus native lock; exact selected path and unrelated draft/staged content excluded. | Unit and Chromium checks pass; native race stress unexecuted. |
| `g5.git.checkpoint.retry` | Exact-byte Save retry after push failure, pinned selected hash and branch head. | API and Chromium push-failure checks pass; crash mid-Git phase unexecuted. |
| `g5.documents.save.checkpoint` | Code, Markdown/ontology and JSONL source Save callbacks; HTML Apply is excluded. | Browser checks pass across three engines; Chromium JSONL passed after repair. |
| `g5.documents.save`, `.acknowledgment`, `g5.documents.read.live-cache` | Observed VM and hash at read/write boundary; submitted-buffer acknowledgment and cache fence. | API/editor unit and live stale-hash checks pass; VM replacement browser race unexecuted. |
| `g5.governance.accept.checkpoint`, `.applied-pending` | Accept route, settled suggestion and review notice; selected new-file commit. | API failure injection and three-browser success cases pass; live failure injection unexecuted. |

## Verification and limits

The live stack uses this worktree's API/edge images, native host runtime and
dedicated `g5-exp8-plain-pr_*` data volumes. The first Chromium run passed
seven of eight tests, including authentication, acceptance, editor Save,
failed-push retry, stale hash, no-op Save and HTML Apply; the new JSONL case
exposed the two repaired omissions above. Its targeted rerun passed after
repair. The complete suite then passed **8/8 in Chromium, 8/8 in Firefox and
8/8 in WebKit** on the final feature code. The focused frontend suite passed
28 tests, `g5d-dispatch` passed 117, and the review/route-auth API suites
passed 23 and 7 on the previous base. The source-matched edge build includes
the pending-notice copy edit, which has a passing component test.

For the final Chromium run the API image was
`sha256:9f954c03c64d04adfec2e2ff850e3129460be0c172f1d975cd96ee0aae00aa9d`
and the edge image was
`sha256:04762adf0ed6f23e555d20084df8f139d5a10753eb77348775ab8071dbc0c770`;
its embedded guest `g5d` and the native host runtime both hashed to
`6cccf4da0f0a94ca57a869c570a1853addd6191a9b229c3445286ba40e7fe598`.
Both containers' Compose working directory was this PR worktree, and the
Postgres and repository volumes used the `g5-exp8-plain-pr_` prefix.

The branch was then rebased onto `origin/main` at `e386351a`. The upstream
revert of Project Doc review targeting conflicted with the initial plain
commit; resolution kept current main's review policy and the plain commit's
already-applied file hash needed for checkpointing. On the rebased source,
the dispatcher suite passed 118 tests, the review/route-auth API suites
passed 11 and 7, and four focused frontend suites passed 28 tests with
TypeScript clean. The native host and guest runtime were refreshed from this
checkout. The full post-rebase Chromium suite passed 8/8; the changed review
acceptance path passed with authentication setup in Firefox and WebKit (2/2
each). The complete feature suite had passed 8/8 in both engines before the
rebase; their unaffected Save paths were not rerun after the upstream review
revert.

The rebased stack's API and edge images were
`sha256:07803bef26f4d0971f98be5ebc42276c30ab5f66e524ea697bc23419a7fa6d44`
and `sha256:cce94d502456782108352b31a4c30f02c3bca9b213016217b7de323e6d3ba6b2`.
The API's guest `g5d` and the native host runtime both hashed to
`3f31beb260d6a6b68f0d688ffe2f1f070f36cf52f0d8a4edfec84117b53c5463`;
the runtime source stamp matched this checkout.

After final PR feedback, the dispatcher suite passed **119 tests**; the
review/route-auth API suites passed **11 and 7**; TypeScript, the frontend
build and Rust formatting passed. The final plain-only dev stack uses fresh
`g5-exp8-plain-pr-final_*` Postgres and repository volumes, this checkout's
API image `sha256:5d061e3c87171d2ea0b713a1e65098e7fdf85d9331488a165d662af43aa20c22`,
the existing source-matched edge image, and a Linux guest runtime rebuilt
from this checkout. Its guest `g5d` is
`e3ec06a2d688030bac463189da57a4be6bcc79c3e50e1e724c4cb51ea5cfdc20`;
its runtime source stamp matched the current checkout. The initial browser
reruns exposed a missing dev image catalog, then a test-token/OAuth identity
collision, and then an idle seeded VM occupying one of the host's two slots.
Those setup failures were repaired; their runs are not counted as feature
passes. On the repaired stack, the full Chromium feature suite passed **9/9**
(OAuth setup plus eight feature cases). Firefox and WebKit each passed review
acceptance, filtered Save and JSONL Project Doc Save (**3/3 feature cases**
per browser, plus shared OAuth setup). The live filtered-Save case proves a
selected file with a CRLF-to-LF Git attribute commits and survives a durable
fork. The JSONL case proves the save-only response carries its VM ID without
creating a checkpoint.

After the Caddy image rebuild and the stronger JSONL assertion, the full
Chromium issue-238 suite passed again (**9/9**). The strengthened JSONL case
passed in Firefox and WebKit as well (**3/3** with shared OAuth setup).

On 2026-09-30 the PR branch was rebased again onto `origin/main` at
`967bf34c`. Main updated implementation metadata in the same ontology files
without changing the selected-checkpoint commitments, so both sets of
metadata were retained. The only textual conflicts were the two Edge
Dockerfiles: both branches replaced the expired Caddy package source with a
pinned binary source. The newer main source was retained in both files; the
PR's JSONL verification changes from that commit remained. `git range-diff`
shows the other eleven PR commits unchanged. The frontend production build,
the prebuilt Edge image (including Caddy configuration validation), the
selected-file dispatcher test, and all **11 review API tests** passed after
this rebase. The full dispatcher integration binary encountered failures in
agent-turn cases outside the selected-file path and was stopped after several
cases remained running beyond a minute. An isolated agent-turn test reproduced
a failed outcome with no dispatcher stderr; its cause is unconfirmed. The
selected-file dispatcher case passed when rerun alone.

The old-head E2E run was cancelled after a Changes test failed twice when
native `git add` met `.git/index.lock`. Changes polling had used ordinary
`git status`, which may refresh the index under an optional lock; its read-only
status call now disables optional locks. Five focused Changes dispatcher
tests and the selected-file checkpoint test passed locally. The next CI run
passed both Playwright shards, but its receipt gate rejected a flaky JSONL
Save test: the test expanded the last transcript row before the out-of-band
write's new row arrived. The assertion now waits for all three records before
expanding the new row. The retry had passed, and all provider-agent browser
cases passed; a clean CI rerun remains the verification gate.

That rerun passed the Changes and JSONL Save cases on the first attempt, both
Playwright shards, and all Rust checks. Its receipt still failed because the
Explicit Save selector collected a local-only push-rejection test that skips
without the `g5-dev-api` container. CI now excludes that test from collection;
local runs still collect it. The subsequent `df0a270c` CI run passed Rust,
both Playwright shards, provider-agent tests and the final receipt.

On 2026-10-01 the PR was rebased onto `origin/main` at `35f9ad1c`. Main's
project-document suggestion presentation and measured host-sizing commitments
coexist with this PR's explicit Save and accepted-file checkpoint rules. Main
already had a broader SCM transport regression for abandoned proxy tunnels,
so its two-phase, two-provider fixture was retained. Three new suggestion
tests expected the former two-argument Save callback; they now assert the
third, null VM identity in their fixtures. Post-rebase verification passed the
frontend production build and typecheck, all 3,750 frontend unit tests, both
SCM transport regressions, five Changes dispatcher tests and API test
compilation. A new exact-head CI run follows the push.

Later on 2026-10-01 the PR was rebased again onto `origin/main` at `05dddc47`.
Main's bounded, coalesced Changes reads and cancellation now share the RPC
helper with this PR's selected-path commit and pinned-head push inputs. Main's
disposable migration execution session and historical owner context supersede
the PR's narrower retry-lock cleanup; its implementation was retained. The
governing workspace, migration and selected-checkpoint commitments are
compatible. Post-rebase checks passed the frontend production build and all
3,770 unit tests, API test compilation, two Changes cancellation regressions,
all 11 review API tests, five migration schema tests and five Changes dispatcher
tests. Exact-head CI remains the post-push verification gate.

The plain PR has no retained per-action recovery record. A process crash
between file write, Git commit, push and review-status settlement therefore
cannot always be reconciled automatically. This is a known limit of the
minimal approach, not a claimed tested guarantee. No broad lifecycle audit,
all-editor sweep or simultaneous native-index stress run is claimed.
