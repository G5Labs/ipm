# Live Files tree in one runtime call — implementation review

Governing design: [ontology-plan.md](ontology-plan.md). Independent reviewers:
Codex (`gpt-6-astra`, read-only, `xhigh`) on the branch diff against `origin/main`,
and CodeRabbit on PR #352.

## Round 1 (2026-10-07)

Codex: REQUEST_CHANGES, three P1s. CodeRabbit: one Major on the same defect as
Codex 1 and 2.

| # | Finding | Disposition |
|---|---|---|
| 1 | A `workspace.file_changed` arriving while a tree or Changes fetch is in flight invalidated with `cancelRefetch: false`, which reuses the pre-change fetch; its response clears the invalidation and the change is never read. The forced refresh after the deferral limit did the same. | Fixed. `FileChangeCoalescer.changed` holds a first change pending when such a fetch is in flight and refreshes once it has landed; `close` refreshes with `cancelRefetch: true` only when the fetch has exceeded the deferral limit (hung). A real `QueryClient` test shows a second read after a change that arrived mid-fetch. |
| 2 | In-flight detection covered only the tree and Changes queries while file bodies and document projections were also invalidated with `cancelRefetch: false`, so a long body fetch could keep stale content. | Fixed by removing the hazard rather than tracking every family: only the tree and Changes fetches (the ones a burst starved) keep the in-flight fetch; `workspace-file`, `file-content`, `node-content` and the document families are invalidated the ordinary way, cancelling a stale fetch. |
| 3 | A listing under the 10,000-entry cap can still exceed the API's 4 MiB reply limit when paths are long, so the panel failed with a transport error instead of the labeled fallback. | Fixed. `WalkLimits` adds `max_bytes`, budgeted per entry as path length plus 64 bytes; the API passes 3 MiB; crossing it reports `truncated`, which the API maps to `entry_cap`. Guest unit test with long paths and API test `a_live_tree_too_large_in_bytes_serves_the_committed_cut`. |

Ontology: no block text changed; `g5.documents.files-view.refresh` and
`g5.documents.read.live-tree` metadata updated with the new mechanics and evidence.

## Round 2 (2026-10-07)

Codex: REQUEST_CHANGES, three P1s and one P2. CodeRabbit on the round 1 commit: one
Minor, the same as Codex 4.

| # | Finding | Disposition |
|---|---|---|
| 1 | TanStack's `cancelRefetch: true` leaves a first read without cached data running, so a change during an initial file-body or document read, or a hung initial tree read past the deferral limit, still reused the pre-change read. | Fixed. `ws.ts::readAgain` cancels a first read still out (the existing `invalidateAndReadAgain`) before invalidating, for the ordinary families and for the forced refresh; the synchronous invalidation stays when no first read is out. Real `QueryClient` tests: a file body that changes during its first read, and a hung first tree read cancelled after the deferral limit. |
| 2 | The byte budget counted raw path length; JSON escaping of quotes and control characters can inflate the reply past the 4 MiB limit while the walker reports no truncation. | Fixed. `entry_bytes` budgets the escaped JSON string plus a 64-byte envelope (57 needed for a 20-digit size); a test serializes 200 heavily escaped names and checks the budget never undercounts. |
| 3 | A directory wider than the guest's 10,000-entry listing limit raised `DIRECTORY_LIMIT_EXCEEDED` from `fs.list` inside the walk or from the root `fs.watch`, and the route propagated it instead of the labeled fallback. | Fixed. `TreeSource::list` returns `Listing::{Entries, Missing, TooWide}`; the guest maps the limit error to `TooWide`, which truncates the walk, and the route maps the same error from the watch to `entry_cap`. Guest test on a real directory with 10,001 files. |
| 4 (P2) | The `g5.documents.files-view.refresh` block said the first notification refreshes at once and a fetch in flight is never cancelled, while the implementation waits behind tree/Changes reads and cancels a hung one. | Fixed. Block text now states the waiting case, the hung-read cancellation and the at-once reads of cheap families; the plan's row and diagram say the same. |

## Round 3 (2026-10-07)

Codex: REQUEST_CHANGES, one P1 and one P2, both new kinds. CodeRabbit on the round 2
commit: one Major on workspace scoping.

| # | Finding | Disposition |
|---|---|---|
| 1 (Codex P1) | The one root `fs.watch` is refused (`watching: false`) when the subtree has more than 10,000 directories or 64 levels, and the API discarded that answer, so a repository with `node_modules/` or `target/` (this one has ~97,000 directories, 337 outside them) got a live tree that never refreshed. The old per-directory walk had registered some watches by accident. | Fixed at the source. `FileService::watch_directories` registers the directories the Files tree walks: pruned by the checkout's `.gitignore` files through the same `walk_tree_with` (`Hidden::Keep` so dot-directories stay watched), refused only past the same limits. A build's writes under ignored output now raise no notifications at all. Guest test on a real tree with `node_modules/`, `target/` and `.github/`; the watcher depth test still passes. Tracked files inside ignored directories are listed but not watched (documented gap). |
| 2 (Codex P2) | Every family went through the one gated coalescer, so a slow tree read delayed an open file's refresh by up to the deferral limit. | Fixed. A second, ungated coalescer refreshes file bodies, node content and documents at once (coalesced per window, never waiting behind the tree). Real `QueryClient` test: a body changes while the tree read hangs and is read again at once. |
| 3 (CodeRabbit Major) | `readAgain(["file-content"])` and `["node-content"]` matched queries of other workspaces, cancelling their first reads. | Fixed. Both keys are workspace-scoped (`["file-content", workspaceId, …]`, `["node-content", workspaceId, …]`); the refresh now passes the workspace id. |

Convergence: round 3 findings were new kinds (watch coverage, family independence,
cross-workspace scope), not repeats. A fourth round follows; if it finds only variants
of these shapes, merging on green CI is proposed.

## Round 4 (2026-10-07)

Codex: REQUEST_CHANGES, two P1s, both refinements of the round 3 watch pruning.

| # | Finding | Disposition |
|---|---|---|
| 1 | With watches pruned by `.gitignore`, dropping a rule made a directory visible in the tree but never watched: the watcher rebuilt registrations on directory events only, and a repeated subscription returns early. | Fixed. `event_changes_directories` treats any non-access event on a `.gitignore` file as a topology change, so the watcher re-registers through the same pruned walk and emits its rescan. Watcher test: dropping a rule registers the formerly ignored directory. |
| 2 | The watch walk counted files against the 10,000 bound meant for directories, refusing a watch over a few directories holding many files; the API discarded `watching: false`. | Fixed. `WalkLimits` gains `max_directories` (`WalkLimits::directories` for watches, `WalkLimits::reply` for listings); the walk counts directories separately. The API logs a warning when a watch is refused. Guest test: two hidden directories with 10,002 files register four watches. |

Convergence: both round 4 findings are variants of the round 3 watch-pruning shape,
not new classes. The loop stops here; merging on green CI is proposed to the user.
Known gaps carried in the ontology metadata: tracked files inside ignored directories
are listed but not watched; a listing whose pruned directory set still exceeds the
watch limit is live but unwatched (warned in the API log).

## Follow-up after the QA deploy (2026-10-08)

PR #352 merged and deployed at 02:39 UTC; at 03:29 the Files tab on preview-test still
showed "Loading files…" for both repositories. Load balancer logs: every tree request
of that workspace ran about 20 to 22 s and was aborted by the browser, while another
workspace's G5-Lean tree returned in 0.6 s. API logs: no `fs.tree` or watch warnings,
but `workspace runtime call refused … MODULE_CONCURRENCY_EXHAUSTED` and then
`workspace Changes query exceeded` its 30 s budget. The route still called
`workspace_changes_report` after the listing, only to label the tree with the checkout
HEAD, so a starved `git status` in the VM held the whole Files tab.

Fix (follow-up PR): `fs.tree` reads the checkout's HEAD from `.git` beside the listing
(`file_tree::head_oid`: detached, loose and packed refs, no Git process) and the route
labels the tree with it, falling back to the durable head when the VM could not read
one. The tree no longer starts or waits on a Changes run. API test
`the_live_tree_does_not_wait_for_the_changes_report` with an unresponsive Changes
peer; guest unit test for the three head layouts.

Separate observation, not fixed here: that VM refused `workspace.changes` with
`MODULE_CONCURRENCY_EXHAUSTED` while no task attempt was running, so the Changes
panel itself may still stall there; worth its own look.

### PR #367 review round 1 (2026-10-08)

Codex: REQUEST_CHANGES, three P1s and one P2.

| # | Finding | Disposition |
|---|---|---|
| 1 | `head_oid` read `.git` by path with `std::fs`, which follows symlinks out of the checkout, blocks on a FIFO and reads unbounded. | Fixed. Moved into `files.rs::head_oid` over the pinned member descriptor: every component through `openat` with `O_NOFOLLOW` and `O_NONBLOCK`, regular files only, reads bounded at 8 MiB. Tests: `.git` symlink, `HEAD` symlink, FIFO `HEAD`, symlinked ref component, worktree `.git` file, oversized ref. |
| 2 | A missing live HEAD was labeled with the durable head, which may not describe the checkout (`g5.documents.read.labels`). | Fixed. `FileTreeResponse.repo_oid` is nullable; a live listing without a readable HEAD carries no OID. The fake dispatcher now reports a distinct `LIVE_HEAD_OID` so the tests tell live from durable labels. |
| 3 | Symbolic refs that point at further symbolic refs were not followed, and a packed value was consulted even when a loose symbolic file existed. | Fixed. Chains resolve up to 8 hops with cycle protection; packed refs are consulted only when no loose file exists; peeled `^` lines and comments are skipped. Tests cover the chain, the stale packed value and the cycle. |
| 4 (P2) | The label assertion used the durable head, so a wrong label would pass; no real `FileService::tree` head test. | Fixed by the distinct `LIVE_HEAD_OID` and a real `fs.tree` test over a `.git` directory with every layout. |

### PR #367 review round 2 (2026-10-08)

Codex: REQUEST_CHANGES, one P1 and two P2s.

| # | Finding | Disposition |
|---|---|---|
| 1 | A loose ref that was rejected (symlink, oversized, unreadable) was treated like an absent one, so a stale `packed-refs` value could label the tree. | Fixed. Metadata reads distinguish `Missing` from `Rejected`; only a genuinely absent loose file consults `packed-refs`, and a rejected file or a non-missing failure on a directory component ends resolution with no OID. The refusal tests now plant a stale packed value behind each rejected case, including a symlinked ref directory. |
| 2 (P2) | `repoOid` was typed `string` in the frontend response and no test exercised an omitted or null guest HEAD end to end. | Fixed. `FileTreeResponse.repoOid` is `string \| null`; the fake dispatcher can omit the HEAD (`attach_without_tree_head`) and the API test asserts `repoOid` is null for a live listing; a hook test covers the null label. |
| 3 (P2) | The saved design still named `workspace.changes` as the label producer and included its wait in the requested outcome. | Fixed. The plan's diagram, outcome, acceptance and labels row now specify the guest-provided HEAD, the nullable label and no Changes dependency; the historical evidence stays in this review record. |

### PR #367 review round 3 (2026-10-08)

Codex: APPROVE, no new findings; the round 2 fixes verified by inspection. Converged.
