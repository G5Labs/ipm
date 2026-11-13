# FILES source editor — implementation review

## Scope and method

This is a **self-review** of the implementation on branch `zcl/files-source-editor`:
- the design commit is `d2c82e7f`;
- the implementation commit is `d6fd655c`;
- the review repairs come after it.

The author wrote the frontend, and a background agent briefed by the author wrote the backend. The review is therefore not independent. The independent check is the Codex implementation review that follows (`gpt-6-astra`, `xhigh`).

The starting inventory is the pinned plan's [Ontology changes and governance](ontology-plan.md#ontology-changes-and-governance) and its acceptance outcomes. Coverage then widened to the actual diff and its consumers:
- `dirtyEditors` and `MyReviewPanel`;
- the `RequireAuth` principal changes;
- the websocket invalidation paths;
- every Monaco import site;
- `read_workspace_file_for_actor_bounded` callers;
- the API error envelope.

Out of scope: unrelated pre-existing debt, and SpecEditor and PROJECT DOCS internals beyond confirming they are untouched.

## Initial findings

These were recorded before the repairs below. The pre-repair code is commit `d6fd655c`.

1. **Ended drafts lingered in the mounted editor** (defect; `g5.documents.files-view.drafts`, `.refresh`).
   - **Path.** FileView keeps CodeEditor mounted while reading. After Discard or a successful Push review, the store's draft and Edit ended in the same React batch, so CodeEditor saw `draft = null` and never ran its drop path.
   - **Consequences.** Its buffer kept the ended text, and the editor then:
     - registered the file in `dirtyEditors`, which `MyReviewPanel` treats as unsaved;
     - showed a false changed-on-disk notice in read mode when the file next changed;
     - on the next Edit, could start from the old baseline after clearing the deferred newer snapshot.
   - **Check.** New `FileView.test.tsx` cases cover "leaves no edits behind after Discard (owner)" and "after a pushed review (reader)". With the repair reverted, both fail: `dirtyEditorPaths()` still lists `docs/readme.md`.
2. **Changed-on-disk lacked Save my version** (defect; `g5.documents.files-view.compare`). The notice offered Compare and Reload only. The block requires Compare beside Reload and Save my version.
3. **Reader wording** (defect). A reader's changed-on-disk Compare said "Save then writes it over the newer version". Readers cannot Save; they Push review.
4. **Missing evidence for `.annotations`.** No test showed the file's threads and suggestion cards beside FILES Preview.
5. **Lint.** The draft mirror effect runs after every render on purpose, which triggered `react-hooks/exhaustive-deps`. Its deps cannot be listed, because Compare and Save move the baseline in refs.

### Backend deviations from the brief (validated, accepted)

- **Text checks and the diff run before the stale check.** A push that can never be recorded gets its 422 rather than a 409 offering "Push anyway". Fresh and stale valid pushes behave as designed (`.stale`).
- **Owner push refusal.** An owner's push gets 409 `CONFLICT` ("The workspace owner edits files directly"). FILES never offers the owner Suggest changes.
- **Refusal envelope.** Refusals carry `details {code: FILE_SUGGESTION_REFUSED, field, message}`, and the envelope's message repeats `details.message`. This reuses the `BASE_WORKSPACE_REMOVAL` exposure in `errors/mod.rs`, so FileView shows a readable reason.
- **`text_file_diff` forces text.** libgit2's binary guess would otherwise drop hunks of valid UTF-8 text that is rich in control characters. The server's own text rule (`ensure_suggestion_text`) decides what counts as text. Round diffs are unchanged, and a unit test shows the same hunks as a round's tree diff.
- **Blob hash helper.** It reuses `GitRepo::blob_id_for_bytes` instead of adding a new helper.
- **Over-limit read error.** It now says `{field, message}`. The route is its only bounded caller; `workspaces.rs` passes `None`.
- **Obsolete notice.** Without a review workspace it reads "Suggest the change again from FILES". The round wording is unchanged and tested.

## Repairs and rechecks

1. **Ended drafts.** CodeEditor now shows the current file whenever its draft ends, or it stops mirroring one (`showCurrentFile` in the mirror effect). That resets the baseline to the current props and clears the deferred snapshot, notice, conflict, Compare and save state. The two FileView cases now pass, and the whole FileView, CodeEditor and page suites pass.
2. **Save my version.** The owner's changed-on-disk notice now has Save my version, which saves against the newer hash bound to the baseline VM, as the conflict dialog does. Readers keep Compare and Reload. Two new `CodeEditorFilesOptIns.test.tsx` cases pass.
3. **Reader wording.** The reader's notice and Compare text now name Push review.
4. **Annotations evidence.** `WorkspacePageComments.test.tsx` gains "lists a Markdown file's threads and cards beside its FILES Preview". A reader sees the thread anchored at L2 and the suggestion card, gets no moderation controls, and replies. The HTML fixture was parameterized rather than copied.
5. **Lint.** The every-render effect is documented and its lint rule is suppressed with the reason.

## Codex implementation review, round 1

Codex (`gpt-6-astra`, `xhigh`) reviewed `5c4748cf` in its own worktree and returned **CHANGES_REQUIRED** with six blocking findings. It reproduced each with temporary tests, which it then removed. The author re-checked each against the code before fixing it; all six hold. The self-review missed them: it checked lifetimes within one view, not across remounts, sessions, people or registry owners.

| # | Finding (governing rule) | Fix | Regression check |
|---|---|---|---|
| 1 | **A late push deletes newer edits** (`.drafts`). The push lock was local to the view. After leaving and reopening, typing went into the same draft, and the push's completion then deleted it. | The draft carries `pushing`, so every view of it stays read-only until the push settles. Completion drops only the version that was pushed and keeps any other text. | `FileView.test.tsx`: "keeps every view of a draft locked while its push is in flight", and "drops only the version it pushed". |
| 2 | **A Save completing after Discard resurrects text** (`.drafts`, `g5.documents.save.acknowledgment`). The identity check covered only an unmounted editor. | Save completions are bound to their editing session. The session ends when the draft is discarded, dropped, replaced or detached, and a completion after that only acknowledges into the store. A session continues through clean periods, so typing back during a Save still counts as a later edit. | `FileView.test.tsx`: "leaves nothing unsaved when a Save finishes after Discard" and "after a drop from outside the view". |
| 3 | **Push could send under another person's token** (`g5.security.login.principal`). | `usePushFileSuggestions` passes the rendered principal as the request option. The body is unchanged. | `FileView.test.tsx`: "never sends Push review under another person's token, and sends it after the same person's renewal". |
| 4 | **An org switch dropped the same person's drafts** (`.drafts` says "per person"). The self-review had recorded this as a gap; Codex is right that it contradicts the approved block. | Draft keys carry the person (`draftPerson`), and `RequireAuth` retains by person. The test also exposed that retention ran while auth was still loading, which is not signing out. | `useAuthContext.test.tsx`: "keeps a person's unsaved FILES drafts through an org switch, and drops another person's", using the real `AuthProvider` and `switchOrg`. |
| 5 | **A clean editor unlisted another file's retained draft** (`.drafts`, review-copy Discard). `dirtyEditors` held one entry per path. | Registrations are per owner and path. Each editor, and the page for its drafts, clears only its own entry, and Discard drops every owner's edits. | `dirtyEditors.test.ts` shared-path case; `useFileDraftsAsDirtyEditors.test.tsx`: "keeps a retained draft listed while a clean editor shows the same path". |
| 6 | **Preview hid the source badge** (`g5.documents.read.labels`; the plan's governing table). | `SourceBadge.tsx` is shared by the editor toolbar and the FileView header, which shows it while Preview is displayed. | `FileView.test.tsx`: "names the source it shows beside Preview" checks the label, the OID, the observed time and no hidden ancestor. |

Each regression test fails with its fix undone: seven single-fix reversions, each run against its test, and all seven failed.

Codex's non-blocking points are also handled:
- **Oversized current file.** The refusal now has a readable reason: the route maps the over-limit read to `FILE_SUGGESTION_REFUSED`, and the API test now checks the top-level message for every refusal.
- **E2E spec.** The file helper accepts the binary card. Both FileView roots carry `data-file-path`, so the helper waits for the right file. The owner now creates the glyph-margin comment instead of cancelling it, and Save asserts that the branch head moved. A reader's reply in read-only Monaco with the VM stopped is already covered by the updated `comments.spec.ts`.

Follow-ups Codex named as pre-existing are kept out of scope: mixed line endings, and server caches not scoped to a principal.

## Codex implementation review, round 2

Codex reviewed `e0990dee` and returned **CHANGES_REQUIRED** with one blocking finding. The other five round-1 fixes passed its consumer checks.

**The finding: a pending Save could lose later edits across navigation** (`.drafts`, `g5.documents.save.acknowledgment`).

*Scenario.*
1. With the file at B, Save A.
2. While the Save is in flight, type B back.
3. Leave the view, or reopen it before the Save settles.

*Effect.* The draft was deleted as soon as the buffer matched the old baseline, so the text typed back was lost once the Save landed. The pending Save was known only inside the component.

*Fix.*
- A Save in flight marks its draft (`saving`), and every view keeps the draft while the mark is there, whatever its text.
- Settling a Save changes only the draft's baseline and flags in the store, never its text. Views then adopt the baseline but keep their own possibly newer buffer.
- A view whose buffer is clean also keeps it while its session's Save is pending. The file's next version waits and shows no false changed-on-disk notice, since it is usually that Save itself landing.

*Check.* "FileView Saves in flight" runs six cases with the real file read and Save hooks over HTTP and the page's draft mirror:
- typing back during a Save that succeeds, then staying, leaving and returning after the Save, or reopening while it is in flight;
- the same three after a refused Save.

With the committed code restored, leaving and reopening after a successful Save fail; staying and the refused-Save cases pass as controls. All six pass after the fix.

**Self-found while checking what the fix enables.** The round-1 session guard read the session id at Save start. If Save ran before the draft mirror had first run, for example Ctrl+S right after the first keystroke, the id was unset. The completion then looked as if it came from an ended session, and the editor would stay falsely dirty. Save now starts the session, and writes the draft if needed, itself. This is evidenced by inspection only, because the window cannot be opened inside React's test `act`.

## Codex implementation review, round 3

Codex reviewed `ac5fdac3` and returned **CHANGES_REQUIRED** with two blocking findings. Both came from the round-2 fix.

1. **A reopened view kept the text but left Save disabled** (`.drafts`).
   - *Cause.* Adopting a stored baseline updated only a ref, after the dirty state had been rendered, and nothing rendered again.
   - *Fix.* Adopting a changed baseline re-renders the editor, so the unsaved state and Save follow it.
2. **A refused Save hid a change on disk for good** (`g5.documents.save.disk-change`, `.compare`).
   - *Cause.* The pending-Save guard suppressed the notice, and nothing revisited it when the Save settled.
   - *Fix.* When a view's pending Save settles, the view compares the file version that arrived during the Save with the baseline the Save left. A different version reaches a clean buffer, or raises the notice over an edited one. A version matching the baseline (the Save's own landing) is dropped.

**Self-found while checking the fix.**
- A first version compared the props current at settle time. Because queries notify observers on a timer, those props can still be the pre-Save version. That would cause a false notice or a brief revert, and it reverted the buffer after Save my version in an existing test. The comparison now uses only the version that actually arrived during the Save.
- A successful Save now drops the version it saved over, the one waiting when it started, while keeping a different version that arrived meanwhile.

*Check.* "FileView Saves in flight" gains four cases using the real hooks:
- Save is enabled and submits against the saved hash after the saved version renders first, both staying and reopening;
- the notice, with Compare, Reload and Save my version, after a refused Save that a different version reached, both staying and reopening.

The helper waits for each delivered version to render before continuing. Without that wait, the version reached the editor only together with the Save's answer, an ordering the old code already handled. With the committed `CodeEditor.tsx` restored, exactly the three cases Codex reported fail; with the fix all ten pass. Codex's temporary reproduction of these cases, which the committed cases above mirror, also passes.

## Codex implementation review, round 4

Codex reviewed `3d57afbc` and returned **CHANGES_REQUIRED** with two blocking findings about a Save racing with other writers.

1. **A reopened view kept an obsolete notice** (`.compare`, `.drafts`).
   - *Scenario.* After Save my version, typing more and reopening before the Save settled, the reopened view still offered the overwritten agent version as "newer". Reload there would have discarded the person's text. Reopening after the Save's own bytes arrived showed a false notice too.
   - *Cause.* A view learns about a Save settled elsewhere only through the store, which said nothing about which version that Save replaced.
2. **A Save's answer hid a newer version** (`.refresh`, `.compare`).
   - *Scenario.* A Save landed, another writer changed the file, and only then did the Save's answer arrive.
   - *Cause.* `useSaveWorkspaceFile` wrote the saved bytes into the file query anyway, replacing the newer version that had already been read. A clean view then showed stale text, and an edited one lost its notice.

**Fixes.**
- **Save hook.** It writes the saved bytes into the cache only while the cache still holds the version the Save replaced, or the saved version. Otherwise it reads the file again rather than guess which version is newer. This also covers an older cache, such as after Save my version from the conflict dialog.
- **Record of the last Save.** Settling a Save records `lastSave` (the version replaced and the version written) on the draft. Every view then drops a version that a successful Save superseded, and the notice about it. A version the Save neither replaced nor wrote stays a change on disk: a clean buffer takes it, an edited one gets the notice.
- **Notice clears.** Once the file matches the edits' baseline again, the notice clears.
- **Reopened views wait.** A view reopened while its draft's Save is in flight raises no notice until that Save settles, because the version may be the Save itself landing.
- **Simplification.** These rules replace round 3's identity-based "version waiting at start" rule in the originating view. Every FILES view now applies the same supersede rule.

*Check.* "FileView Saves in flight" gains ten cases with the real hooks, five scenarios each staying and reopening:
- after a refused Save, a clean buffer takes the arrived version;
- Save my version supersedes the version it replaced and keeps later typing;
- the Save's own landing is no change;
- a version written after the Save landed is a change on disk, with Compare against it;
- a clean buffer follows that version.

With the round-3 `CodeEditor.tsx`, `useFiles.ts` and `fileDrafts.ts` restored, six of them fail; all twenty Saves-in-flight cases pass with the fix. Codex's temporary reproductions for rounds 3 and 4, which the committed Saves-in-flight cases mirror, also pass.

## Codex implementation review, round 5

Codex reviewed `276f1beb` and returned **CHANGES_REQUIRED** with one blocking finding; both round-4 fixes passed its checks, including mutation controls.

**Compare lost merge edits typed while Monaco loaded** (`.compare`). `FileCompareDialog` shows textareas until the bundled Monaco diff editor loads. Its diff editor then started from the person's original text (`ownContent`), not from what they had typed meanwhile (`merged`), so Apply dropped those edits.

*Fix.* The diff editor's modified side starts from `merged`. Later keystrokes equal the model's own value, which `@monaco-editor/react` 4.7 leaves alone, so typing is not disturbed.

*Check.* The new `FileCompareDialog.test.tsx` delays the Monaco load, types in the fallback, lets the diff editor take over, types again and applies. It fails without the fix and passes with it. Codex's temporary FileView-level reproduction of the same flow also passes.

Round 5 was the default review cap; the user approved a sixth round to confirm the fix.

## Codex implementation review, round 6

Codex reviewed `e1e60ce4` and returned **APPROVE**, with no blocking or non-blocking findings.

- It reviewed `git diff 276f1beb..HEAD` against the approved design and contracts.
- It ran the FILES, Compare, Save-race, refresh, draft and shared-editor suites: 89 tests.
- It confirmed that the Compare test and its FileView reproduction fail with the one-line fix reverted.
- `tsc` and ESLint on the changed files pass. The only warning is a hook-dependency warning in the test's diff-editor mock.

Its only follow-up is the unexecuted Playwright coverage, which now runs in the `g5-ci-e2e` lane of the draft PR [G5Labs/G5-Lean#316](https://github.com/G5Labs/G5-Lean/pull/316).

## PR review on G5Labs/G5-Lean#316

CodeRabbit left four comments. Each was checked against the code and addressed:
- **This record.** It now states round 6's outcome, and it describes the reproductions by the committed tests that mirror them instead of temporary `/tmp` files.
- **Current-file encoding.** The suggestion route matches `base64` explicitly and refuses any other encoding as not text. A reader's read is always durable, so no reachable defect existed; the match keeps a future change from turning this path into a 500.
- **`katex`.** It moved to `dependencies`, because `MarkdownPreview.tsx` imports its stylesheet directly; the lockfile already treated it as a production package.
- **Compare dialog keyboard handling.** The dialog uses the shared `useDialogFocus`: the text to merge takes the focus, Tab stays inside, Escape cancels, and closing returns the focus to the opener. The new `FileCompareDialog.test.tsx` case fails without it.

## CI on G5Labs/G5-Lean#316

The `g5-ci-e2e` lane ran the browser specs that could not run locally.
- **Run 1.** It failed in `auto-commit-review.spec.ts`, a known flake that this PR's added Edit click had shifted to line 236. Its click on the middle of Monaco's lines could land on an inline review card, so the typing never reached the editor. The failed shard then filled the CI host, and every later workspace start timed out. Fix: the spec clicks a text line and checks the typed text before waiting for the Save.
- **Run 2.** The owner FILES flow passed in Chromium. The reader flow pushed its suggestion, but the owner's Accept click on the inline card in Code view was taken by Monaco's `.view-lines` layer. That was a real regression of read-only Monaco, which hosts the cards in view zones that Monaco paints under its text layer; the replaced viewer drew them as page elements. Fix: the card zones carry `g5-suggestion-zone`, which `index.css` lifts above the text layer. Monaco suppresses mouse-down in a zone only when the zone opts in, which ours do not. A probe in real Chromium with the bundled Monaco 0.55.1 showed the click intercepted without the rule; with it, the button takes the click and a text field in the card takes typing.

## Commitment accounting

Each row reads commitment → actual path → counterexample or check → result or gap.

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| `read.labels` in FILES: both views name their source | CodeEditor toolbar and FileView header (Preview) share `SourceBadge.tsx` | FileView badge case | Pass after round 1 |
| `files-view` read-first, line target, in-place Edit | `WorkspacePage.tsx` `case "file"` → `FileView.tsx` `TextFileView` → `CodeEditor` `monacoReadOnly` | `FileView.test.tsx` reading and editing cases; page suites | Pass; browser unexecuted |
| `.preview` GitHub-flavored Markdown, sanitizing, links, images, Mermaid strict | `MarkdownPreview.tsx`, `remarkGithubAlerts.ts`, `repoLinks.ts`, shared `MermaidDiagram` (`securityLevel: "strict"`); `highlightCode` escapes unknown languages | `MarkdownPreview.test.tsx` (8): scripts, `onerror` and `javascript:` removed | Pass |
| `.source-only` | Preview read-only; Edit only in Monaco | FileView Edit/Preview tabs case | Pass |
| `.text-only` (UTF-8, no NUL, ≤ 2 MiB, either source) | `fileText.ts` → `FileUnavailableCard`; server `ensure_suggestion_text` on the current file, base and content | FileView base64, NUL live and durable, too-large cases; API refusal test | Pass |
| `.drafts` survive view, tab, panel and page changes; prompt on close; per person | `fileDrafts.ts` (person keys, `pushing` lock, `saving` mark), CodeEditor `draft` (session-bound Save completions settled in the store), `useFileDraftsAsDirtyEditors.ts` (own registry owner), `RequireAuth` person retention, HTML source restore | `fileDrafts.test.ts` (6); FileView remount, late-Save, push-lock and lingering-draft cases; mirror tests; the org-switch case | Pass after round 4; real `beforeunload` and the Home trip unexecuted (Playwright) |
| `.owner-edit` Start, then live reload before Edit | `ownerCanEditNow` needs live bytes with a VM; `requestWorkspaceStart` | FileView Start case: no Edit on durable bytes after Start, Edit on the live read | Pass |
| `.compare` on Save conflict and changed-on-disk | CodeEditor `openConflictCompare`, notice with Compare, Reload and Save my version (also for a version that arrived during a Save, once it settles); `FileCompareDialog.tsx`; FileView stale Compare | `CodeEditorFilesOptIns.test.tsx` (7); FileView stale and Saves-in-flight cases | Pass after round 4; Monaco diff editor only in the browser |
| `.refresh` reader follows branch, flush and cut | `ws.ts` `invalidateAuthoritativeCut` on all three events → FILES query; CodeEditor clean reload or notice | `ws.workspaceAuthority.test.ts`; CodeEditor disk-change case | Pass |
| `.annotations` threads and cards beside Preview | FileView `SideThreads` with `comments` and `suggestionOverlay` | New page test (repair 4) | Pass |
| `editor.assets` bundled Monaco | `monacoRuntime.ts` `loader.config({ monaco })` with `?worker` imports; `loadMonacoReact` is the only entry; the theme re-apply runs only after it | `vite build` emits `monacoRuntime` and five workers; no CSP or proxy limits found | Build verified; jsDelivr-blocked browser run unexecuted |
| `file-suggestions` reader-only, writable source, any VM, records only, sent under the rendered principal | `usePushFileSuggestions` binds the principal; `publish_file_suggestions`: `scoped_db_for_workspace(workspaces:read)`, `admit_file_suggestions` (owner refused, `ensure_thread_writes_allowed`) before the read and again in the recording transaction; durable read; notify the owner once | API tests: push with no runtime peer; the owner, Base, read-only, freezing and non-reader refusals leave no rows; branch, file and VM unchanged | See [Check results](#check-results) |
| `.additive` | One annotation publication per push | Two pushes plus rounds stay open; the agent's mutation to kind `round` fails the test | Pass |
| `.stale` | 409 `FILE_VERSION_CONFLICT` with the current hash; FileView Compare and Push anyway | API stale test (pure insertion and an ambiguous block keep existing refusals on Accept); FileView stale cases | Pass |
| `.record` annotation without a review workspace; base vector; final-newline refusal | `create_publication(kind annotation, review_workspace_id None, annotation_vector(source_heads))`; `file_suggestion_changes` | API tests | Pass |
| `.base` blob hash vs the reader's durable file; no client-named objects | `blob_id_for_bytes(baseContent)` vs `read_workspace_file_for_actor_bounded` hash | Stale and flush-snapshot API tests | Pass |
| `review-suggestions` second producer; decisions unchanged | Shared `hunk_suggestions`, `insert_generated_suggestions`; Accept and Withdraw untouched except obsolete wording | CRLF Accept and Withdraw API test; round wording unchanged | Pass |
| SpecEditor and PROJECT DOCS unchanged | No SpecEditor source or unit test changed; `speceditor-independent-suggestions.spec.ts` only adds a click to Code view on its FILES step | Full vitest | Pass; Playwright unexecuted |
| Body and size bounds | Route `DefaultBodyLimit` = 2·6·2 MiB + 64 KiB; per-text 2 MiB; note ≤ 10,000 characters | Near-2 MiB small edit returns 201; with the limit removed the agent saw 413 | Pass |

**Bounded pass from ontology text back to the inventory.**
- "Base, read-only and freezing sources accept none": the frontend `workspaceAcceptsThreadWrites` and the server `ensure_thread_writes_allowed` are the reply rule on both sides. Covered.
- "whichever source the bytes came from" (`.text-only`): live and durable reads both pass through `fileTextEligibility`. Covered.
- "relative images load from the same selected source" (`.preview`): images use the same file read API, which picks the source for the actor at read time. A VM start between the page read and an image read can mix sources. This is accepted, not tested.
- "Save keeps its checkpoint" (`.owner-edit`): the owner path still uses `PUT files/:path` with `commitAndPush`. Unchanged.
- "the owner decides them like any other" (`file-suggestions`): Accept still needs the owner's running VM, which `ReviewsPanel` enforces. Unchanged.

## Check results

All evidence below is from this checkout at the repaired head.

- **Frontend:**
  - `npx vitest run`: 262 files, 4,261 tests passed, including those added by the self-review repairs and rounds 1 to 5;
  - `npx tsc -b --noEmit`: exit 0;
  - ESLint on the changed files: 0 errors, and the remaining warnings are pre-existing;
  - Prettier is clean on the new and touched files;
  - `npx vite build`: success, emitting `monacoRuntime` and the editor, JSON, CSS, HTML and TypeScript worker chunks.
- **Backend** (`SQLX_OFFLINE=true`; the API suite uses private Postgres and Redis containers, never port 5432):
  - `cargo fmt --all -- --check`: clean;
  - `cargo clippy --workspace --all-targets -- -D warnings`: exit 0;
  - `cargo test -p g5-scm`: 158 passed;
  - `cargo test -p g5-api-core --lib workspace_review_service`: 21 passed;
  - `cargo test -p g5-api --test route_auth_static`: 11 passed;
  - `cargo test -p g5-api --test api_workspace_reviews -- --test-threads=1`: 61 passed, 7 of them new, rerun after round 1.
- **Mutation evidence:**
  - With repair 1 reverted, both new lingering-draft cases fail.
  - Round 1: each of seven single-fix reversions (push lock, acknowledging only the pushed version, principal binding, Save session, Preview badge, registry owners, person retention) fails its regression test.
  - Round 2: with the committed `CodeEditor.tsx` and `fileDrafts.ts` restored, leaving and reopening after a successful Save fail; staying and the refused-Save cases pass as controls.
  - Round 3: with the committed `CodeEditor.tsx` restored, the reopen case of Save enablement and both refused-Save notice cases fail; the rest pass.
  - Round 4: with the round-3 `CodeEditor.tsx`, `useFiles.ts` and `fileDrafts.ts` restored, six of the ten new cases fail.
  - The backend agent reports that removing the body limit, using kind `round`, or disabling the stale check each failed its targeted test. This is the agent's report, not rerun here.
- **Not executed:** the Playwright specs (see the gaps below).

## Remaining gaps

| Governing ontology (or N/A) | Gap | Proposed action | Uncertainty / alternatives / blockers |
|---|---|---|---|
| `g5.documents.files-view` and children, `editor.assets`, `file-suggestions` | The Playwright flows in `files-source-editor.spec.ts` and the four updated specs are typechecked but not executed. These cover the owner flow, reader push and Accept, the binary card, the jsDelivr block, the glyph margin and the real `beforeunload`. | The user chose the `g5-ci-e2e` lane: the draft PR [G5Labs/G5-Lean#316](https://github.com/G5Labs/G5-Lean/pull/316) runs them on the approved head `e1e60ce4`. | Results pending. This Mac has no `/dev/kvm`, so local `ci-docker` stops at Phase 4b. |
| N/A | Monaco normalizes mixed line endings to the model's EOL, so a mixed-EOL file's edit diff includes ending changes. This is pre-existing for owner Save. | None in this change. | Pre-existing behaviour. |
