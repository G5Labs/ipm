# G5 Client Frontend Audit — Spec vs. Implementation Drift Report

**Audit Date:** 2025-01-15
**Scope:** `/workspaces/g5/frontend/src/` vs. spec files in `/workspaces/g5/generation/.semlift/specs/g5/client/`
**Methodology:** Exhaustive file-by-file comparison of every source file against corresponding spec requirements.

---

## Summary

| Severity | Count | Description |
|----------|-------|-------------|
| **P0 — Broken** | 5 | Type mismatches or missing fields that will crash at runtime |
| **P1 — Behavioral Drift** | 12 | Code works but diverges from spec contracts |
| **P2 — Missing** | 18 | Spec-required functionality not yet implemented (stubs/placeholders) |
| **P3 — Test Gaps** | 11 | Missing or insufficient test coverage per spec |

---

## P0 — Broken (Runtime Crash or Type Error)

### P0-01: `SearchResult` field name mismatch — `entityType`/`entityId` vs `type`/`id`

- **Spec** (data-layer.semlift.md): `SearchResult { entityType, entityId, title, snippet, ... }`
- **types.ts** (L579–590): `SearchResult { type, id, title, snippet, ... }`
- **WorkspacePage.tsx** (L1059–1071): `handleSearchResult` accesses `result.entityType` and `result.entityId`
- **Impact:** Clicking any search result will reference `undefined` properties. Navigation after search is broken.

### P0-02: `OntologyNodeDTO` missing `children` field — recursive traversal crashes

- **types.ts** (L293–305): `OntologyNodeDTO` has `childCount: number` but no `children` field.
- **WorkspacePage.tsx** (L1013–1020): `findNode()` recursively accesses `n.children` to locate nodes.
- **Impact:** `n.children` is always `undefined`; the `findNode` loop silently fails, making node selection from deep-link URLs a no-op. The guard `if (n.children)` prevents an actual crash but the feature is non-functional.

### P0-03: `WorkspaceDetail` has no `repos` field — ReposBar receives undefined

- **types.ts** (L207–214): `WorkspaceDetail` extends `WorkspaceSummary` with `branches: BranchDetail[]`, `settings`, `activeBranchId`, etc. No `repos` field.
- **WorkspacePage.tsx** (L1160): `(workspace.data.repos ?? []).map(...)` — accesses a nonexistent property.
- **Impact:** ReposBar always renders empty (the `?? []` fallback prevents a crash). The repos strip never shows data.

### P0-04: `WorkerSummaryDTO.status` enum mismatch with TopBar/ConnectedWorkerPill

- **types.ts** (L617–629): `WorkerSummaryDTO.status` is `'provisioning' | 'idle' | 'assigned' | 'executing' | 'terminated'`.
- **TopBar.tsx** (L73–82): `connectedWorker.status` expects `'running' | 'idle' | 'provisioning' | 'offline'`.
- **ConnectedWorkerPill.tsx** (L22): Same — `status: 'running' | 'idle' | 'provisioning' | 'offline'`.
- **Impact:** A worker with status `'executing'` will not match any display case in `STATUS_CONFIG` (falls through to `offline`). `'assigned'` and `'terminated'` are similarly unmapped. Workers will always appear offline/broken in the UI.

### P0-05: `WorkerSummaryDTO` field name mismatches — `cpu`/`mem`/`taskName` vs DTO fields

- **types.ts** (L617–629): Fields are `memoryMb`, `cpuCores`, `taskId`, `taskName`, `uptimeSeconds`.
- **TopBar.tsx** (L84–92): `workers` array expects `cpu: number`, `mem: number`, `uptime: string`, `taskId: string | null`.
- **WorkspacePage.tsx** (L1143–1147): Maps `w.cpuPercent`, `w.memPercent`, `w.currentTask`, `w.currentTaskId` — none of these exist on `WorkerSummaryDTO`.
- **Impact:** All worker metrics display as `undefined` or `NaN`. The worker dropdown shows broken data for every worker.

---

## P1 — Behavioral Drift (Works but Diverges from Spec)

### P1-01: React Query `retry` set to 1 instead of spec'd 3

- **Spec** (data-layer.semlift.md §4): "retry: 3 for network errors, no retry for 4xx".
- **main.tsx** (L17): `retry: 1` in `defaultOptions.queries`.
- **Scope:** All queries retry only once on transient failures instead of three times.

### P1-02: `refetchOnWindowFocus: false` globally instead of spec'd per-query

- **Spec** (data-layer.semlift.md §4): "refetchOnWindowFocus enabled for workspace-level queries".
- **main.tsx** (L19): `refetchOnWindowFocus: false` in global defaults.
- **Note:** `useWorkspaces` (useWorkspace.ts L72) individually sets `refetchOnWindowFocus: true`, partially compensating. But all other workspace-level queries (workspace detail, settings, jobs, etc.) inherit the global `false`.

### P1-03: `DiffLine.type` enum values differ from spec

- **Spec** (center-panel.semlift.md §DiffView): `DiffLine.type: 'add' | 'remove' | 'context'`.
- **types.ts** (L692–696): `DiffLine.type: 'context' | 'addition' | 'deletion'`.
- **DiffView.tsx** (renders based on the type): Uses the types.ts values, which won't match any spec-compliant API response.

### P1-04: `DiffData` shape differs from spec

- **Spec** (center-panel.semlift.md §DiffView): `DiffData { oldPath, newPath, lines: DiffLine[] }`.
- **types.ts** (L698–705): `DiffData { file, additions, deletions, hunks: DiffHunk[] }` where `DiffHunk { header, lines: DiffLine[] }`.
- **WorkspacePage.tsx** (L655): Passes `diffData={{ fileName: '...', hunks: [], additions: 0, deletions: 0 }}`.
- **Impact:** All three layers (spec, type, usage) disagree on the shape.

### P1-05: `FileTreeEntry.type` uses `'directory'` instead of spec's `'dir'`

- **Spec** (data-layer.semlift.md §types): `FileTreeEntry.type: 'file' | 'dir'`.
- **types.ts** (L649): `type: 'file' | 'directory'`.
- **Impact:** Any code testing `entry.type === 'dir'` against API data will fail silently.

### P1-06: App.tsx uses `export default` — spec requires named exports only

- **Spec** (routing-and-auth.semlift.md §3): "Every module uses named exports only — no default exports".
- **App.tsx** (L418): `export default App;`.

### P1-07: LeftSidebar uses tab switching instead of spec's collapsible sections

- **Spec** (left-sidebar.semlift.md §2): "Four collapsible SidebarSections stacked vertically, each independently collapsible".
- **WorkspacePage.tsx** (L795–870): Sidebar renders with `activeSidebarTab` state — only one section visible at a time (Ontology, Files, Docs, Jobs as mutually exclusive tabs).
- **Impact:** Cannot see ontology tree and file tree simultaneously as spec intends.

### P1-08: `useDraftSession` query key uses branchId directly, not composite key

- **Spec** (data-layer.semlift.md §hooks): "Query key convention: `['entity', parentId, entityId]`".
- **ws.ts** (L541): WS handler invalidates `['draft-session', '${event.workspaceId}:${event.branch}']` — composite key.
- **useDraft.ts** (L25): Hook queries with `queryKey: ['draft-session', branchId]` — just the branchId.
- **Impact:** WS cache invalidation for draft events will never match the hook's query key. Real-time draft updates are broken.

### P1-09: `useNotifications` never receives WS events

- **useNotifications.ts** (L56): Calls `useWSChannel(\`workspace:${workspaceId}\`)` to subscribe.
- **useNotifications.ts** (L58–59): State is `useState<G5Notification[]>([])` but no WS event listener is ever registered to populate it.
- **createNotificationPusher** (L126–138): Exported but never actually integrated — documented as "In a real integration, the WS manager would call a registered callback".
- **Impact:** Notifications array is always empty. Toast stack never shows anything.

### P1-10: `useOntologyRoots` uses incompatible query key with WS invalidation

- **useOntology.ts** (L16): Query key is `['ontology-roots', branchId]`.
- **ws.ts** (L550): Invalidates `['ontology', event.workspaceId]` on draft.commit.
- **Impact:** WS-driven ontology refresh after commits doesn't invalidate the correct cache key.

### P1-11: Duplicate `classNames` function — exported from both `UI.tsx` and `utils.ts`

- **Spec** (design-system.semlift.md §5): Single `classNames` utility export.
- **UI.tsx** (L72–76) and **utils.ts** (L22–27): Both export identical `classNames` functions.
- **Impact:** No runtime issue, but violates single-source-of-truth principle and will cause confusion during imports.

### P1-12: `useMyScopes` defined in both `useAuth.ts` and `usePermissions.ts`

- **useAuth.ts** (L440–448): `export function useMyScopes(): AsyncState<string[]>` hitting `/api/v1/auth/scopes`.
- **usePermissions.ts** (L19–27): `export function useMyScopes(): AsyncState<string[]>` hitting `/api/v1/auth/scopes`.
- **Impact:** Duplicate exports with identical names. Import resolution depends on which file is imported. Potential tree-shaking and bundle issues.

---

## P2 — Missing (Spec-Required but Unimplemented)

### P2-01: Content never fetched for editors — hardcoded placeholder strings

- **WorkspacePage.tsx** (L620–660): `ContentDispatch` passes `content={'// Loading...'}` and `content={'Loading...'}` as static strings. Has explicit TODO: "Wire useDraftCRDT, useNodeComments, useNodeContent, useFileContent hooks here".
- **Hooks exist:** `useNodeContent`, `useFileContent` are implemented in useOntology.ts and useFiles.ts but are never called from `WorkspacePage`.

### P2-02: CRDT never wired — `yjsDoc` always null, `syncState` always "synced"

- **WorkspacePage.tsx** (L630–660): All editor components receive `yjsDoc={null}`, `syncState="synced"`, `participants={[]}`.
- **crdt.ts**: Full `CrdtDocumentStore` and `useDraftCRDT` hook are implemented — but `WorkspacePage` never calls them.
- **Spec** (bottom-drawer.semlift.md, center-panel.semlift.md): Editors must bind to Yjs doc for collaborative editing.

### P2-03: Collaborative presence never wired — `participants` always empty, `collaborators` always empty

- **WorkspacePage.tsx** (L1140): `collaborators={[]}` hardcoded for TopBar.
- **WorkspacePage.tsx** (L630–660): `participants={[]}` for all editors.
- **Spec** (top-bar.semlift.md §Collaborator Avatars): Derived from Yjs awareness.

### P2-04: MobileLayout renders placeholder instead of actual ChatPanel

- **WorkspacePage.tsx** (L721): `<div className="flex-1 bg-[var(--g5-bg0)]"><div className="p-4...">Chat panel — mobile view</div></div>`.
- **Spec** (workspace-page-shell.semlift.md §Mobile): Full ChatPanel should render in mobile tab view.

### P2-05: Docs sidebar section — placeholder div

- **WorkspacePage.tsx** (L860–870): `<div className="p-4 text-sm text-[var(--g5-text2)]">Project documents...</div>`.
- **Spec** (left-sidebar.semlift.md §DocsSection): Should render `DocNode[]` tree from `useDocs()` hook.
- **Missing hook:** No `useDocs` hook exists at all.

### P2-06: Jobs sidebar section — placeholder div

- **WorkspacePage.tsx** (L870–880): `<div className="p-4 text-sm text-[var(--g5-text2)]">Jobs panel...</div>`.
- **Spec** (left-sidebar.semlift.md §JobsPanel): Should render job summaries with progress bars, status chips, real-time updates.

### P2-07: Lazy loading of ontology children — returns empty array

- **WorkspacePage.tsx** (L1006–1009): `handleLoadChildren` is `async (nodeId: string) => { return []; }`.
- **useOntologyChildren** hook exists (useOntology.ts L20–28) but is never used.
- **Impact:** Expanding any ontology node in the tree will show zero children.

### P2-08: File tree always empty

- **WorkspacePage.tsx** (L852): `fileEntries={[]}` hardcoded.
- **useFileTree** hook exists (useFiles.ts L13–24) but is never wired.

### P2-09: ChatPanel session always null

- **WorkspacePage.tsx** (L1210): `<ChatPanel session={null} messages={[]} ...>`.
- **useChatSessions/useChatSession** hooks exist (useChat.ts) but are never called.
- **Impact:** Chat panel always shows empty "no session" state.

### P2-10: No-op callbacks throughout WorkspacePage

Numerous action callbacks required by the spec are stub functions:
- `onDownlift={() => {}}` (L1087)
- `onUplift={() => {}}` (L1088)
- `onAddRepo={() => {}}` (L1168)
- `onSelectWorkspace={() => {}}` (L1148)
- `onSelectRepo={() => {}}` (L1169)
- `onInjectMessage` — mock/stub

### P2-11: Bottom drawer task data never fetched

- Resolved by removal: the bottom drawer, its DAG, Timeline and Terminal tabs and the task pause/continue/cancel hooks are gone (G5Labs/G5-Lean#364). Tasks open as a preview or a chat.

### P2-12: Comment system not wired to WorkspacePage

- **WorkspacePage.tsx** (L639–646): Editors receive `comments={[]}` and no-op comment callbacks.
- **useNodeComments**, **useAddComment** hooks exist (useComments.ts) but are never called.

### P2-13: SpotlightSearch `onSelectResult` uses wrong field names

- **SpotlightSearch.tsx** (L90–99): Saves `result` to localStorage using `result.id`.
- But **WorkspacePage.tsx** (L1059): Reads `result.entityId` — separate field name incompatibility.
- **Impact:** Even if search results returned correctly, the navigation handler cannot read the right fields.

### P2-14: Settings flyout not wired

- **WorkspacePage.tsx** (L1148): `onOpenSettings={() => {}}` — no settings overlay implemented.
- **Spec** (workspace-page-shell.semlift.md): Flyout panel for workspace settings.

### P2-15: Notifications flyout not wired

- **WorkspacePage.tsx** (L1149): `onOpenNotifications={() => {}}` — no notification panel rendered.
- **NotificationToastStack** component exists but is never mounted in WorkspacePage.

### P2-16: Missing `useDocs` hook — no hook for document tree data

- **Spec** (left-sidebar.semlift.md §DocsSection): Sidebar must display doc tree from `useDocs()`.
- No `useDocs.ts` file exists in `hooks/`.

### P2-17: Missing keyboard shortcut for Cmd+B (toggle left sidebar)

- **Spec** (workspace-page-shell.semlift.md §Keyboard Shortcuts): "⌘B toggles left sidebar".
- **WorkspacePage.tsx**: `handleKeyDown` handler exists but only handles `⌘K` (spotlight search). No `⌘B` binding.

### P2-18: Missing workspace clone dialog

- **CloneDialog.tsx** component exists in `components/workspace/`.
- Never imported or rendered in WorkspacePage or App.tsx.

---

## P3 — Test Gaps

### P3-01: No MSW handlers file

- **Spec** (tests.semlift.md §2): "src/test/msw-handlers.ts should export an array of REST and WS handlers covering all endpoints".
- **Missing:** No `msw-handlers.ts` file exists. Test files exist but no shared MSW handler setup.

### P3-02: No component tests for editor components

- **Spec** (tests.semlift.md §Component Integration): Tests required for SpecEditor, CodeEditor, DiffView.
- **Missing:** No test files for any editor component.

### P3-03: No component tests for tree components

- **Spec** (tests.semlift.md §Component Integration): Tests required for OntologyTree, FileTree, TreeNode.
- **Missing:** No test files for tree/ components.

### P3-04: No component tests for execution components

- **Spec** (tests.semlift.md §Component Integration): Tests required for BottomDrawer, DagView, Timeline, TerminalPanel, IntervenePanel.
- **Missing:** No test files for execution/ components.

### P3-05: No component tests for notification components

- **Spec** (tests.semlift.md §Component Integration): Tests required for NotificationToastStack, NotificationBanner, NotificationPanel.
- **Missing:** No test files for notifications/ components.

### P3-06: No component tests for shared components

- **Spec** (tests.semlift.md §Component Integration): Tests required for ErrorBoundary, MarkdownRenderer, MermaidDiagram.
- **Missing:** No test files for shared/ components.

### P3-07: Existing component tests are legacy V1 tests

- **Chat.test.tsx** (288 lines), **Sidebar.test.tsx** (227 lines), **TaskQueue.test.tsx** (223 lines): These reference legacy V1 component names and patterns not matching the current V2 component implementation.
- **WorkspacePageV2.test.tsx** (338 lines): Exists but needs verification that it tests current implementation rather than outdated V1 structure.

### P3-08: No API integration tests

- **Spec** (tests.semlift.md §API Integration): Tests required for `api.get/post/patch/delete`, error handling, token refresh, 401 retry.
- **Missing:** No `api.test.ts` file exists.

### P3-09: Hook tests may reference outdated hook signatures

- **useChat.test.ts** (401 lines), **useDrafts.test.ts**, **useJobs.test.ts**, **useOntology.test.ts**, **useSearch.test.ts**, **useSettings.test.ts**, **useWorkspace.test.ts**: Files exist but haven't been verified to match current hook signatures and return types. Hook file is `useDraft.ts` not `useDrafts.ts`.

### P3-10: E2E specs exist but likely reference stale selectors

- 10 E2E spec files exist in `e2e/flows/` covering: chat, drafts, jobs, keyboard-shortcuts, login, ontology, search, settings, task-monitoring, workspace.
- Given the extent of unimplemented features (P2 items), most E2E flows will fail immediately due to missing UI elements and dead functionality.

### P3-11: No visual rendering tests

- **Spec** (tests.semlift.md §Visual Rendering): "Storybook or similar visual test for each theme × dark/light mode".
- **Missing:** No Storybook configuration or visual test files exist.

---

## Appendix: Files Audited

### Spec Files (12)
- data-layer.semlift.md, design-system.semlift.md, routing-and-auth.semlift.md, shared-components.semlift.md, tests.semlift.md, workspace-page.semlift.md
- center-panel.semlift.md, left-sidebar.semlift.md, workspace-page-shell.semlift.md, right-panel.semlift.md, bottom-drawer.semlift.md, top-bar.semlift.md

### Source Files (56)
- **Pages:** WorkspacePage.tsx, App.tsx, main.tsx
- **Lib:** types.ts, api.ts, ws.ts, crdt.ts, utils.ts
- **Hooks (16):** useAuth.ts, useChat.ts, useComments.ts, useDraft.ts, useFiles.ts, useJobs.ts, useNotifications.ts, useOntology.ts, useOrg.ts, usePermissions.ts, useSearch.ts, useSettings.ts, useSpend.ts, useWebSocket.ts, useWorkers.ts, hookUtils.ts
- **Components (35):** All files in chat/, comments/, draft/, editor/, execution/, layout/, notifications/, org/, search/, settings/, shared/, tree/, ui/, workers/, workspace/
- **Data:** mock.ts
- **Tests (17):** 7 hook tests, 3 component tests, 2 page tests, 2 integration tests, 10 E2E specs
