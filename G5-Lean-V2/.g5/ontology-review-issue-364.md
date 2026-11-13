# Issue 364: remove the task "Stop and hold" action and the bottom drawer — review rounds

PR: [#371](https://github.com/G5Labs/G5-Lean/pull/371), branch `kaihui/remove-bottom-drawer` from main `14a94c21`.
Governing blocks: `g5.agents.lifecycle.chat-stop`, `g5.agents.lifecycle.chat-held-turn`,
`g5.story.continue-interrupted-work` in `ontology/features/agents.g5.md`.
Reviewer: Codex `gpt-6-astra`, reasoning effort xhigh, read-only sandbox on the branch; CodeRabbit
inline comments folded into the same round.

### Round 1 (2026-10-08, `4d92f4c6` against `14a94c21`)

Verdict: changes requested. No retained path references a deleted export, hook, test id or the
pause endpoint; chat Stop keeps its hold and cancellation delivery; ontology pairing and links
resolve. Six Codex findings and two CodeRabbit comments:

| # | Finding | Resolution |
|---|---|---|
| 1 | P2 (Codex and CodeRabbit) a `/job/:jobId[/task/:taskId]` route hydrated from a Home tile, a cross-workspace link or a reload selects the task but opens no content; the drawer used to be that content | The route effect opens the destination once per route key when the route was not reached by a click (`openedJobRouteKeyRef`): a task route through `handleSelectTaskAndOpenOutput`, a job route through the Job Output tab, an explainer job through its Explainer Output tab. Two `WorkspacePageV2` tests cover the direct task and job routes. |
| 2 | P2 held job turns lose their only frontend Continue (`useContinueTask` removed with the drawer) | Not restored, by decision: the issue states that chat Stop ends turns and offers no Continue. This change provides no release for held turns; that is expected from #363 (open), which returns every `owner_stop` hold to the queue on Start. Until it merges, a held job turn continues only through `POST /tasks/:id/continue`. The gap is recorded in the `continue-interrupted-work` metadata. |
| 3 | P2 the Base task-output test asserted only the center container; its `task-chat-badge` negative targeted a deleted element | Asserts the routed `job-output-view` and keeps the no-Base-chat assertion. |
| 4 | P3 orphans: `DAG_TASK_STATUSES`, `TaskActionResponse` (type, schema, import), `TaskCheckpoint` with its mock and fixture re-exports; `HilEscalationPanel`, `HilResponseComposer` and the escalation hooks reachable only through tests | The first group is removed. The escalation panel, composer and hooks are left for #362 / #368, which delete them; `OptionCard` is still imported by `QuestionCard` and `StructuredQuestionEditor`. |
| 5 | P3 `lifecycle.g5.md` `g5.lifecycle.start.held-work` still reasons about a deliberate task pause | Left to #363, which answers that question, rewrites the paragraph without the pause and records the owner's decision; editing it here would only conflict. |
| 6 | P3 the preview-rejection test accepted any error where the route test asserted 409 | Asserts the error renders as `Conflict: only agent tasks can be paused`. |
| CR | Major: `jobs-panel` lookups in `job-merge-back.spec.ts` and `jobs.spec.ts` run before the jobs sidebar section is activated, so the soft checks skip | `activateSidebarSection(page, 'sidebar-jobs-tab')` precedes each lookup. |

While repairing finding 1, adding the handler callbacks to the route effect's dependency list made
the effect re-run often enough that an unrelated restored-Terminal test failed in sequence; the
effect keeps its original dependency discipline (the callbacks are captured from the render in which
the route resolves, and `jobsData` is already a dependency).

### Round 2 (2026-10-08, `763d25c0` against `14a94c21`)

Verdict: changes requested. The omitted callback dependencies cannot call a stale task index
(a task route resolves only once its task is indexed); ordinary task clicks are deduplicated;
explainer routing, Base chat exclusion and the pending/stale checks remain connected; the
backend hold path and the round-1 repairs match the record. One finding:

| # | Finding | Resolution |
|---|---|---|
| 1 | P2 clicking a completed singleton job whose task list is already cached: `handleOpenJobDetails` navigates to `/job/:id`, the activation effect opens the task-scoped Job Output in the same effect flush, and the job-route hydration then reopened Job Output without the task, dropping the task's result and timeline; when the task route caught up, the click guard skipped reopening it | A job route carrying the `taskQueueActivationJobId` router state counts as reached by a click, as a task route recorded in `pendingSelectedTaskIdRef` already did. Regression test `WorkspacePageV2` "keeps a clicked singleton job's task-scoped output when its job route hydrates" clicks the job twice (the second time with its tasks cached) and asserts the task result nav; it fails without the fix. The round-1 record's "ordinary clicks are deduplicated" claim now covers job-row clicks. |

CodeRabbit's pass over round 1 added two comments, folded in here: the record now says the
Start release of `owner_stop` holds is expected from #363 and not provided by this change (the
`continue-interrupted-work` metadata says the same); and a direct task route ran the click
handler, whose URL synchronisation dropped the link's `session` and `askUserQuestion` hints, so a
direct task route now opens its output through `openJobOutputForTask` alone and leaves the URL to
the chat effects (test "keeps a direct task route's session and question hints while opening its
output", which fails under the click-path hydration).

### Round 3 (2026-10-08, `b427200c` against `14a94c21`)

Verdict: changes requested. The cached-singleton repair and its test, Home links without
activation state, click deduplication, the omitted callback dependencies, pending/stale handling,
the preview-list gating, explainer destinations and Base chat exclusion were confirmed. Two
findings:

| # | Finding | Resolution |
|---|---|---|
| 1 | P2 reloading a clicked multi-task job: the retained `taskQueueActivationJobId` history state made hydration skip the route, while the activation effect opens only singleton jobs, so the reload showed Chat over an unopened Job Output | The activation effect now owns every job route carrying that state, on the click and on a reload: a singleton job opens its task-scoped output, any other job its Job Output. Test "opens Job Output for a multi-task job route that retains its activation state" navigates with the state without a click. |
| 2 | P2 task-route hydration discarded deep-link hints | Already repaired in the working tree when the round ran (CodeRabbit's comment above). |

Repairing finding 1 surfaced a latent race in the tab-to-URL `view` mirror: with a restored
Terminal tab, the mirror ran before the workspace detail had loaded, while `workspaceChatAvailable`
was still false, wrote `?view=terminal`, and the hint consumer then opened the Terminal over the
Chat that becomes the center view once the detail loads. Main passed the restored-Terminal test only
by render timing; this branch's render count hit it deterministically. The mirror now waits for the
workspace detail.

### Round 4 (2026-10-08, `89b4fc39` against `14a94c21`)

Verdict: changes requested. The omitted callback dependencies, cached-singleton deduplication,
multi-task activation hydration, explainer destinations, preview and Base chat exclusion, the
`view` mirror gate and the backend scope were confirmed. Two findings, both follow-through on the
round 3 route work:

| # | Finding | Resolution |
|---|---|---|
| 1 | P2 a direct task link to a task that has its own session: the chat auto-open backstop called `handleViewSession` with `resetParams`, which replaced a different explicit `session` hint and dropped `askUserQuestion`; the round 3 test used a task without a session and so missed it | The backstop leaves a URL that already names a session alone (the route hydration applies the hint). The hint test now gives the task its own session and a different `session` hint in the link; it fails without the guard. |
| 2 | P2 click a multi-task job, switch workspace from the selector, press Back: the mounted page still held `handledTaskQueueActivationRef === jobId`, so the activation effect returned early, and hydration skipped the route for its retained activation state, leaving Chat over no Job Output | Leaving the job route, and switching workspace, release the activation. Test "reopens an activated job's output after a workspace switch and Back"; it fails without the release. |

Four rounds reach the loop's usual limit. Rounds 2 to 4 each found only an edge of the route
hydration that replaced the drawer (a cached singleton click, a retained activation on reload or
Back, deep-link hints), no wrong deletion and nothing in the backend or ontology; one confirming
round follows.

### Round 5 (2026-10-08, `4279f177` against `14a94c21`)

Verdict: changes requested. Task clicks and Chat history selections, singleton deduplication,
multi-task activation, the workspace-switch/Back reset, task-index readiness, pending/stale
resolution, preview and Base chat exclusion, explainer destinations, the `view` mirror gate, the
removal itself and the backend hold path were confirmed. One finding, a follow-through on round 4:

| # | Finding | Resolution |
|---|---|---|
| 1 | P2 the round 4 session guard returned before recording the auto-open key; when the user then opened workspace content, which drops the `session` hint while the task stays selected, the backstop fired and rewrote the URL back to the task | The key is recorded before the guard, so a hinted session counts as consumed. Test "leaves content navigation alone after a hinted task route" (a hinted task route, then navigation to the workspace root; the URL stays there); it fails with the round 4 order. |

Five rounds: the last four each found one edge of the route hydration that replaced the drawer,
each narrower than the last, and none touched the removal, the backend or the ontology. The loop
stops here as converged; the branch merges on green Rust and e2e lanes.

### CI follow-through (2026-10-08, `846aac8f` and `a934ac32`)

After merging main (#368 had edited the two BottomDrawer files this branch deletes; both stay
deleted), the Rust lane's clippy rejected the round 1 test's `.err().expect()` on a `Result`
(`expect_err` now), and the e2e lane's frontend unit step failed once on
`WorkspacePageAlignment.test.tsx` asserting a ribbon action enabled synchronously after the
chevrons appear, while the served configuration can land a render later; the assertion now waits.
Neither is in this branch's scope; both are test-only.
