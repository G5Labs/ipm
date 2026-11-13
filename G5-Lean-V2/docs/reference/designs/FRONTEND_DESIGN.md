# G5 Frontend Design

> Architecture document for the G5 frontend. Reflects the **V2 unified workspace**
> implemented in `WorkspacePageV2.tsx` (~10,220 lines) — the single-surface editor
> that subsumes the earlier OntologyPage and ExecutionPage prototypes. Maps the
> current UI to the backend schema in API_AND_STORAGE.md and identifies remaining
> integration gaps.

---

## §0 Technology Stack

| Layer | Choice |
|-------|--------|
| Framework | React 19.2 (strict mode) |
| Build | Vite (rolldown-vite) |
| Language | TypeScript 5.9 (`verbatimModuleSyntax`, strict) |
| Styling | Tailwind CSS v4 (`@import "tailwindcss"` + `@theme` custom properties) |
| Testing | vitest 4 (429 tests passing) |
| Routing | react-router-dom v6 (BrowserRouter) |
| Icons | lucide-react |
| Diagrams | SVG-based DAG renderer (custom), mermaid (theme-aware rendering) |
| Rich Editor | Notion-like block editor (custom `SpecEditor`, see §3.4.3) |
| Code Editor | Custom code viewer with syntax highlighting (Monaco planned) |
| Math | KaTeX (inline `$...$` and block `$$...$$`) |
| CRDT | Yjs (collaborative editing — indicators mocked, full integration planned) |

State management uses local `useState` / `useRef` with custom hooks:
`useEditableContent` (per-document undo/redo with 50-entry stack, dirty tracking),
`useSettings` (localStorage + mock backend persistence), and `useTabPersistence`
(per-workspace tab state in localStorage). The migration path is React Query
hooks backed by the REST + WS APIs documented in API_AND_STORAGE.md §3–§4.

---

## §1 Design System

### §1.1 Theme System

G5 ships **10 named themes**, each with **light and dark modes** (20 total palettes).
Themes are applied via `data-theme` and `data-mode` attributes on `<html>`:

```html
<html data-theme="obsidian-gold" data-mode="dark">
```

**Theme catalog:**

| Name | `ThemeName` Value | Accent | Font (sans) | Font (mono) | Character |
|------|-------------------|--------|-------------|-------------|-----------|
| Editor | `editor` | `#3B82F6` (blue) | system-ui | ui-monospace | Standard dev tool feel |
| Signal Studio | `signal-studio` | `#FF4D2E` (red) | DM Sans | DM Mono | Sharp, 2px radius |
| Obsidian & Gold | `obsidian-gold` | `#D4A54A` (gold) | Georgia (serif) | Fira Code | Noise texture, 0.5px borders |
| Carbon Clay | `carbon-clay` | `#C46B3B` (terracotta) | Nunito Sans | IBM Plex Mono | Warm, rounded 6–16px radius |
| Alpine Graphite | `alpine-graphite` | `#06B6D4` (cyan) | Inter | JetBrains Mono | Tight 3–8px radius |
| Darkroom | `darkroom` | `#FF8C00` (amber) | SF Mono (all-mono) | SF Mono | Film grain texture, 0px radius |
| Ink & Parchment | `ink-parchment` | `#8B6914` (sepia) | Libre Baskerville (serif) | Courier Prime | Paper grain texture |
| Studio Night | `studio-night` | `#EC4899` (pink) | Outfit | Space Mono | Pill-ish 6–20px radius |
| PM Paper | `pm-paper` | `#46B4AA` (teal) | Inter | ui-monospace | Clean doc-centric layout, hidden repos bar |
| Aero Glass | `aero-glass` | `#4DA3FF` (sky blue) | system-ui | JetBrains Mono | Vista-style frosted glass chrome over an aurora desktop; opaque client area; glossy controls |
| Old school | `classic-98` | `#000080` (navy) | Tahoma / MS Sans Serif | Courier New | Windows 98-style skin: silver bevelled chrome on a teal desktop, navy caption, sunken client area, 0px radius; CSS only, no bundled assets |
| Green room | `green-room` | `#1ED760` (green) | Outfit | JetBrains Mono | Streaming-app dark: near-black canvas, black rail, borderless chrome, charcoal pill controls, green accent; CSS only, no bundled assets |

**Per-theme design tokens** (CSS custom properties under each `[data-theme][data-mode]` selector):

| Token | Purpose | Example (Editor dark) |
|-------|---------|-----------------------|
| `--g5-bg0` | Deepest background (page) | `rgb(10 12 16)` |
| `--g5-bg1` | Panel / card surface | `rgb(14 17 24)` |
| `--g5-bg2` | Elevated surface (headers, toolbars) | `rgb(18 22 31)` |
| `--g5-stroke` | Borders, dividers | `rgb(255 255 255 / 0.08)` |
| `--g5-text0` | Primary text | `rgb(236 242 255)` |
| `--g5-text1` | Secondary text | `rgb(236 242 255 / 0.72)` |
| `--g5-text2` | Muted / tertiary text | `rgb(236 242 255 / 0.52)` |
| `--g5-active` | Running / in-progress (gold) | `rgb(235 199 94)` |
| `--g5-ok` | Success / integrated (green) | `rgb(113 201 136)` |
| `--g5-info` | Informational / selected (blue) | `rgb(103 164 255)` |
| `--g5-warn` | Warning / needs review (orange) | `rgb(242 162 73)` |
| `--g5-blocked` | Blocked / error (red) | `rgb(255 107 107)` |
| `--g5-font-sans` | Primary body font | `system-ui, sans-serif` |
| `--g5-font-mono` | Monospace / code font | `ui-monospace, monospace` |
| `--g5-radius-sm` | Small radius (chips, inputs) | `4px` |
| `--g5-radius-md` | Medium radius (cards, panels) | `6px` |
| `--g5-radius-lg` | Large radius (modals, drawers) | `10px` |
| `--g5-radius-xl` | Extra-large radius (hero cards) | `14px` |
| `--g5-border-w` | Border width | `1px` |
| `--g5-texture` | Optional background texture (noise, grain, paper) | `none` |
| `--g5-editor` | Code editor background | `rgb(8 10 14)` |
| `--g5-gutter` | Line number gutter background | `rgb(6 8 12)` |
| `--g5-linenum` | Line number text color | `rgb(60 68 82)` |
| `--g5-terminal` | Terminal panel background | `rgb(10 12 16)` |
| `--g5-codeblock` | Inline code / fenced-block background | `rgb(16 19 26)` |
| `--g5-syn-keyword` | Syntax: keywords (`fn`, `let`, `if`) | `rgb(198 120 221)` |
| `--g5-syn-type` | Syntax: type names | `rgb(86 182 194)` |
| `--g5-syn-string` | Syntax: string literals | `rgb(152 195 121)` |
| `--g5-syn-comment` | Syntax: comments | `rgb(236 242 255 / 0.30)` |
| `--g5-syn-macro` | Syntax: macros / decorators | `rgb(209 154 102)` |
| `--g5-syn-number` | Syntax: numeric literals | `rgb(209 154 102)` |
| `--g5-syn-function` | Syntax: function names | `rgb(97 175 239)` |
| `--g5-syn-operator` | Syntax: operators (`+`, `=`, `::`) | `rgb(236 242 255 / 0.50)` |
| `--g5-syn-attr` | Syntax: attributes / annotations | `rgb(229 192 123)` |
| `--g5-syn-bracket1` | Rainbow bracket depth 1 | `rgb(235 199 94)` |
| `--g5-syn-bracket2` | Rainbow bracket depth 2 | `rgb(198 120 221)` |
| `--g5-syn-bracket3` | Rainbow bracket depth 3 | `rgb(86 182 194)` |
| `--g5-syn-bracket4` | Rainbow bracket depth 4 | `rgb(152 195 121)` |
| `--g5-overlay` | Subtle tinted overlay (repo chips, hover rows) | `rgb(103 164 255 / 0.03)` |
| `--g5-overlay-hover` | Hover-state overlay emphasis | `rgb(103 164 255 / 0.06)` |

Shadow token (per-theme): `--shadow-pane: 0 1px 0 rgb(255 255 255 / 0.06), 0 12px 40px rgb(0 0 0 / 0.35)` (example for Editor dark — each theme defines its own shadow with accent-tinted glow).

**Per-theme accent identity:** Each theme defines its own `--g5-active` and `--g5-info` values
to give each theme a distinct personality. Do NOT standardize accents across themes:

| Theme | `--g5-active` (dark) | `--g5-info` (dark) |
|-------|---------------------|--------------------|
| `editor` | `rgb(235 199 94)` (gold) | `rgb(103 164 255)` (blue) |
| `signal-studio` | `rgb(255 160 60)` (orange) | `rgb(255 77 46)` (red) |
| `obsidian-gold` | `rgb(212 165 74)` (deep gold) | `rgb(212 165 74)` (gold) |
| `carbon-clay` | `rgb(255 190 110)` (warm amber) | `rgb(245 158 66)` (amber) |
| `alpine-graphite` | `rgb(100 230 220)` (teal) | `rgb(70 200 220)` (cyan) |
| `darkroom` | `rgb(255 200 120)` (warm) | `rgb(180 160 100)` (sepia) |
| `ink-parchment` | `rgb(180 155 80)` (muted gold) | `rgb(139 105 20)` (sepia) |
| `studio-night` | `rgb(236 72 153)` (pink) | `rgb(236 72 153)` (pink) |
| `pm-paper` | `rgb(94 200 180)` (teal) | `rgb(70 160 155)` (teal) |
| `aero-glass` | `rgb(70 150 235)` (sky blue) | `rgb(70 145 235)` (azure) |
| `classic-98` | `rgb(80 140 240)` (blue) | `rgb(70 135 235)` (blue) |
| `green-room` | `rgb(30 160 80)` (green) | `rgb(20 150 70)` (green) |

The `useSettings` hook persists the user's `themeName` and `themeMode` to
localStorage and syncs it to `document.documentElement.dataset.theme` and
`document.documentElement.dataset.mode` on change. Light/dark mode toggles
independently of theme selection.

### §1.2 State Enum

The `UI.tsx` component defines a universal `State` type used throughout:

```
type State = "active" | "ok" | "info" | "warn" | "blocked" | "muted"
```

Each state maps to a `{ dot, tint, label }` triple for consistent rendering of
status dots, chip backgrounds, and human-readable labels.

| State | Dot Color | Label |
|-------|-----------|-------|
| `active` | g5-active | Running |
| `ok` | g5-ok | Integrated |
| `info` | g5-info | Info |
| `warn` | g5-warn | Review Needed |
| `blocked` | g5-blocked | Blocked |
| `muted` | white/20 | Idle |

### §1.3 Primitive Components (UI.tsx)

| Component | Props | Purpose |
|-----------|-------|---------|
| `Button` | `kind: "primary" \| "secondary" \| "ghost"`, `disabled`, `onClick` | Standard action button |
| `IconButton` | `onClick`, `title` | Square icon-only button (8×8/9×9) |
| `Chip` | `state: State`, `text` | Status pill with dot + label |
| `SectionTitle` | `children` | Uppercase tracking-wide section label |
| `classNames` | `...args` | Utility: falsy-filtering class joiner |

### §1.4 Layout Components (Layout.tsx)

| Component | Purpose |
|-----------|---------|
| `AppShell` | Full-screen flex column (`h-screen`, `flex flex-col`, bg0) |
| `TopBar` | Fixed top bar with breadcrumbs, "Open in IDE" button, connection indicator |
| `WorkspaceGrid` | Responsive 3-column grid (left 260–280px, center 1fr, right 320–360px). Falls back to flex stack on mobile. |
| `Pane` | Titled card with sticky header, scrollable body, optional footer. `shadow-pane` elevation. |

---

## §2 Routing

Defined in `App.tsx`. BrowserRouter wraps all routes.

**ID convention:** All entities use **path-based identifiers** (`component_id`
style) rather than UUIDs. Identifiers are deterministic, human-readable,
and derived from the filesystem or organizational hierarchy:

- Org: `g5`, `acme`
- Workspace: `task-graph-rework`, `ontology-v2`
- Ontology node: `auth/validator`, `storage/compactor`
- Doc: `system-architecture`, `onboarding-guide`
- Job/Task: `job-1`, `task-2a` (sequential within workspace)

These path-based IDs are the primary key in both the URL and React state.
The backend may use UUIDs internally in Postgres, but the API surface
always accepts and returns the `component_id`. A `component_id ↔ UUID`
lookup table provides the mapping; the frontend never sees UUIDs.

**Routes:**

| Path | Page | Purpose |
|------|------|---------|
| `/` | Redirect | Redirects to default org workspace |
| `/:org/:workspace` | WorkspacePage | Unified editor (default to ontology root) |
| `/:org/:workspace/spec/:specPath` | WorkspacePage | Deep-link to ontology spec node |
| `/:org/:workspace/doc/:docPath` | WorkspacePage | Deep-link to doc |
| `/:org/:workspace/file/:filePath` | WorkspacePage | Deep-link to source file |
| `/:org/:workspace/job/:jobId` | WorkspacePage | Deep-link to job in bottom drawer |
| `/:org/:workspace/job/:jobId/task/:taskId` | WorkspacePage | Deep-link to task |
| `/settings` | SettingsPage | Org/user settings |

> **Tracking branches** are a branch mode property (not a workspace type).
> When a branch is in tracking mode, the `WorkspacePage` adjusts its chrome:
> hides the draft/commit toolbar and disables inline editing. Tracking branches
> follow a remote branch at HEAD and provide read-only ontology browsing.

**Hierarchy:** The URL hierarchy reflects the data model: **org → workspace**.
An org contains workspaces. The route `/:org/:workspace` makes this
hierarchy visible in URLs shared between humans. Deep-links to specs,
docs, and files use wildcard sub-routes to support nested paths
(e.g., `/:org/:workspace/spec/auth/validator`).

---

## §3 Pages

### §3.1 WorkspaceSwitcher (286 lines)

**Layout:** `AppShell` → 3-column flex.

```
┌──────────────────────────────────────────────────────────┐
│ AppShell + TopBar                                         │
├────────────┬────────────────────────────┬────────────────┤
│ Left       │ Center                      │ Right          │
│ TaskQueue  │ Search + Filter Chips       │ Dependency     │
│ (all       │ Workspace Table             │ Graph (SVG)    │
│  workspaces)│                             │                │
└────────────┴────────────────────────────┴────────────────┘
```

**Center panel features:**
- Search bar (text input, uncontrolled)
- Status filter chips: Active, Blocked, Review, Integrated, Idea (toggle on/off)
- Branch mode toggle: shows "tracking" vs "pinned" badges per workspace
- Aggregate stats row: total cost, total active tasks
- "New Workspace" button
- Table columns: Name, Branch Mode (badge), Status (Chip), Cost, Concurrent Tasks, Updated, Actions
  - **Branch mode badge**: "TRACKING" chip (muted, grey) for workspaces with tracking branches; omitted for pinned (default).
  - Tracking-mode rows show "—" for Cost and Concurrent Tasks (always 0).
- Actions: Pause/Abort (for active workspaces), Clone (⧉), more menu (⋯)
  - **Clone** opens `CloneWorkspaceDialog` (§9.19).
  - More menu includes: Delete.
- Row click → `navigate('/w/:workspaceId')`

**Left sidebar:** `TaskQueue` component showing all execution tasks across workspaces,
grouped by workspace → job (compile/uplift) → individual tasks.

**Right sidebar:** Static SVG dependency graph with `GraphNode` foreign-object nodes.
Shows 6 hardcoded workspace nodes with arrow connections.

**Mock data consumed:** `workspaces` array from mock.ts (9 items), `executionTasks` (12 items).

**Backend API mapping:**
- `GET /api/v1/orgs/:orgId/workspaces` → workspace list
- `GET /api/v1/workspaces/:workspaceId/jobs?include=tasks` → task queue sidebar
- `GET /api/v1/workspaces/:workspaceId/dependencies` → dependency graph
- `WS workspace:*` → live status updates

### §3.2 OntologyPage — DEPRECATED (V1)

> **Status:** Superseded by WorkspacePageV2 (§3.4). Retained here for
> historical reference. The OntologyPage was 1558 lines and served as the
> primary editing surface in V1. All its features have been reimplemented
> and significantly enhanced in the V2 unified workspace.

**Layout:** `AppShell` → search bar → 3-column flex.

```
┌──────────────────────────────────────────────────────────┐
│ AppShell + TopBar                                         │
│ Search Bar ("Search ontology, views, files...")           │
├───────────────┬──────────────────────────┬───────────────┤
│ Left Sidebar  │ Center Panel             │ Right Sidebar │
│ ┌───────────┐ │ Tab Bar + Toolbar        │ Dependencies  │
│ │ Ontology  │ │ ┌──────────────────────┐ │ Dependents    │
│ │ Tree      │ │ │ UnifiedContentView   │ │ ────────────  │
│ │           │ │ │ (markdown / code /   │ │ Chat          │
│ │           │ │ │  diff / annotations) │ │               │
│ ├───────────┤ │ └──────────────────────┘ │               │
│ │ Project   │ │                          │               │
│ │ Views     │ │                          │               │
│ ├───────────┤ │                          │               │
│ │ File Nav  │ │                          │               │
│ ├───────────┤ │                          │               │
│ │ TaskQueue │ │                          │               │
│ └───────────┘ │                          │               │
└───────────────┴──────────────────────────┴───────────────┘
```

**Left sidebar sections (top → bottom):**

1. **Ontology Tree** — Recursive tree rendering of `ontologyTree` from mock.ts.
   - Each node shows: expand/collapse chevron, sync status dot (green/yellow/red),
     type icon (folder/file-text/file-code), name, uplift (↑) / downlift (↓) buttons.
   - Sync status is **computed from node ID hash** (deterministic fake).
   - Click selects node and opens in center panel.
   - Indentation via `paddingLeft: depth * 16 + 12`.

2. **Docs** — Collapsible section listing `docs` from mock.ts.
   - Each doc: sync dot, ↑/↓ arrows, file icon, name.
   - Click selects doc as a tab.

3. **File Navigation** — Collapsible section with hardcoded file-tree stubs
   (`src/`, `engine/`, `rust_engine.rs`, etc.). Not data-driven.

4. **Task Queue** — Embedded `TaskQueue` component filtered to workspace `feat-a`.

**Center panel:**

- **Tab bar**: Open items as closeable tabs. Active tab has `border-b-2 border-b-g5-info`.
  Each tab shows icon + truncated name + close (×) button.
- **Toolbar** (in tab bar or standalone): Diff toggle, Annotations toggle,
  base branch selector (dropdown: `main`, `feature-a`, `experiment-b`).
- **UnifiedContentView**: The main content renderer. Handles two modes:
  - **Code files** (`node.type === 'code'`): Extracts code blocks from markdown,
    renders with line numbers, Rust syntax highlighting (regex-based),
    inline diff markers (+/-), and positionally-aligned comment thread panels.
    Uses `CodeViewWithComments` sub-component with `useLayoutEffect` for
    annotation positioning.
  - **Document files** (`node.type === 'document'` or `'node'`): Parses markdown
    into blocks (text/mermaid/code/table), renders each block appropriately.
    Text lines support inline formatting (bold, italic, code, links, emoji).
    Diff overlay: colored left-border + background. Annotations as side panels.

- **AnnotationsSidebar**: Separate component rendering comment threads with
  author avatars, timestamps, line references, replies, and reply input.

**Right sidebar:**
- `DependencyList` — Shows dependencies and dependents of the selected node.
  Collapsible sections with file-code icon, name, and "INH" badge for inherited.
- `Chat` — Chat component with text input, send button, optional pause button.

**Key internal types:**

```typescript
type ContentItem =
  | { type: 'ontology'; node: OntologyNode }
  | { type: 'doc'; id: string; name: string; content: string }
```

**State variables:** `selectedItem`, `openTabs[]`, `expandedNodes` (Set),
`expandedSections` (Set), `leftSidebarOpen`, `rightSidebarOpen`, `showDiff`,
`showAnnotations`, `baseBranch`.

**Mock data consumed:** `ontologyTree`, `views`, `nodeDiffs`,
`nodeAnnotations`, `mockChatMessages`, inline `ontologyDependencies`/`ontologyDependents`.

**Backend API mapping:**
- `GET /api/v1/workspaces/:workspaceId/ontology` → full tree
- `GET /api/v1/workspaces/:workspaceId/ontology/:nodeId/content` → node content
- `GET /api/v1/workspaces/:workspaceId/ontology/:nodeId/diff` → diff data
- `GET /api/v1/workspaces/:workspaceId/ontology/:nodeId/deps` → dependencies/dependents
- `GET /api/v1/workspaces/:workspaceId/ontology/:nodeId/annotations` → comments
- `GET /api/v1/workspaces/:workspaceId/docs` → docs list
- `WS /api/v1/yjs/:workspaceId/:branch` → CRDT sync for draft editing
- `WS workspace:<id>` → live updates

### §3.3 ExecutionPage — DEPRECATED (V1)

> **Status:** Superseded by the Bottom Drawer in WorkspacePageV2 (§3.4.5).
> The ExecutionPage was 221 lines and showed task execution in a standalone
> 3-column layout. The V2 bottom drawer provides the same capabilities
> (DAG view, timeline, terminal, diff, intervention) integrated directly
> into the workspace without a page navigation.

**Layout:** `AppShell` → sub-header → `WorkspaceGrid` (3-column).

```
┌──────────────────────────────────────────────────────────┐
│ AppShell + TopBar                                         │
│ Sub-header: task name, branch chip, abort button          │
├─────────────┬───────────────────────────┬────────────────┤
│ Left        │ Center                     │ Right          │
│ Neighbor-   │ Timeline (thought/tool     │ Chat           │
│ hood Map    │ log entries)               │                │
│ (SVG graph) │ ─────────────              │                │
│ ──────────  │ Diff Viewer                │                │
│ TaskQueue   │ (add/remove/context lines) │                │
└─────────────┴───────────────────────────┴────────────────┘
```

**Left sidebar:**
- **NeighborhoodMap**: SVG mini-graph with 5 hardcoded nodes (parent, upstream,
  current task, downstream dep, downstream dep). Colored by state. Arrow connections.
- **TaskQueue**: Filtered to workspace `feat-a`.

**Center panel:**
- **Timeline**: Renders `executionLogs` array. Each entry has `type` (thought/tool)
  with color coding and icon (Brain for thought, Terminal for tool).
- **DiffViewer**: Renders `activeDiff.content` lines with add/remove/context coloring.
  Shows `+8 −2` stats in header.

**Right sidebar:** Chat component.

**Mock data consumed:** `executionTasks`, `executionLogs`, `activeDiff` from mock.ts.

**Backend API mapping:**
- `GET /api/v1/tasks/:id` → task details
- `GET /api/v1/tasks/:id/timeline` → timeline entries (paginated)
- `GET /api/v1/tasks/:id/diff` → live diff
- `POST /api/v1/tasks/:id/pause` / `resume` / `abort` → intervention
- `WS task:<id>` → streaming timeline + diff updates

### §3.4 WorkspacePageV2 (~10,220 lines) — Unified Editor (PRIMARY)

The V2 workspace is the **primary editing surface** and the only page a user
interacts with after selecting a workspace. It unifies ontology browsing,
spec editing, code viewing, execution monitoring, chat, and settings into a
single VS Code–inspired layout with responsive mobile support.

**Layout:** Top bar → Repos bar → resizable 3-region + bottom drawer.

```
┌──────────────────────────────────────────────────────────────────┐
│ Top Bar: [≡] G5  WorkspaceSwitcher  ConnectedWorkerPill          │
│          ◇─── inline search ───◇  Avatars Workers🔔⚙ Chat      │
├──────────────────────────────────────────────────────────────────┤
│ Repos Bar: [org/repo-a • main] [org/repo-b • feat-x] [+ Add]   │
├──────────────┬──────────────────────────────┬───────────────────┤
│ Left Sidebar │ Center Panel                  │ Right Panel       │
│ (resizable   │ Tab Bar + Toolbar             │ (Chat)            │
│  200–480px)  │ ┌──────────────────────────┐  │ Session picker    │
│              │ │ SpecEditor (rich/source)  │  │ Message stream    │
│ Search       │ │ — or —                   │  │ Tool badges       │
│ Ontology Tree│ │ CodeEditor (highlighted) │  │ Input + pause     │
│  ↑↓ on hover │ │ — or —                   │  │                   │
│ Docs Section │ │ DiffView (side-by-side)  │  │                   │
│ Files Tree   │ │ — or —                   │  │                   │
│ Jobs Panel   │ │ MarkdownRenderer (docs)  │  │                   │
│              │ └──────────────────────────┘  │                   │
│              │ Draft Bar: 3 dirty • [Commit] │                   │
│              │ ┌──────────────────────────┐  │                   │
│              │ │ Bottom Drawer (resizable) │  │                   │
│              │ │ DAG│Changes│Timeline│Term │  │                   │
│              │ │           │Intervene      │  │                   │
│              │ └──────────────────────────┘  │                   │
└──────────────┴──────────────────────────────┴───────────────────┘
```

#### §3.4.1 Top Bar

A fixed 44px strip across the top with all global controls:

| Region | Contents |
|--------|----------|
| Left | Sidebar toggle (☰), "G5" brand text |
| Center-left | `WorkspaceSwitcher` dropdown (searchable, shows repos+branches per workspace, "New workspace" / "Connect repository" actions) |
| Center | `ConnectedWorkerPill` — shows the worker bound to the current workspace: status dot (running/idle/provisioning/offline with pulse animation), CPU%, task name; compact mode on mobile |
| Center-right | Inline search input (always visible, ⌘K hint, opens Spotlight overlay) |
| Right | Connected user avatars (colored dots for active collaborators), Workers dropdown (worker list with CPU/mem bars, status dots, uptime, task assignment), Notification bell (unread badge), Settings gear (opens flyout), Chat toggle |

#### §3.4.2 Left Sidebar

Resizable (200–480px) via drag handle. Absolute-positioned on mobile, relative on desktop.

**Spotlight search** sits above the sections — a real-time text input that filters ontology nodes, file tree, and docs by name/content match. When active, the Ontology section switches to a flat `FilteredTree` with highlighted matches.

**Repos bar** is a horizontal strip below the top bar (separate from the collapsible sections) showing all repos in the current workspace. Each chip shows `org/repo • branch`. "Add repo" button at the end.

The sidebar body contains **4 collapsible, resizable, reorderable, and detachable sections** (`SidebarSectionId = 'project-docs' | 'ontology' | 'tasks' | 'files'`). Default order:

1. **Docs** (`project-docs`, icon: `BookOpen`) — Architecture docs, plans, and guides with category icons, preview text, and last-updated timestamp. Click opens in center panel. Badge shows doc count.

2. **Core Ontology** (`ontology`, icon: `Database`) — Per-workspace, per-repo ontology nodes (`TreeNode`, recursive).
   - Icons by type: Package (repo root), FolderTree (specs), Lightbulb (views), BookOpen (docs), FileCode (source files).
   - Dirty dot: blue for modified spec, green for source file with changes. Color dot stays visible on hover.
   - Hover reveals uplift (↑) / downlift (↓) action buttons (dim `text-g5-text2/50`, highlights green/amber on individual arrow hover). Uses named Tailwind group `group/tree` to scope hover to the hovered row only.
   - Source file nodes also show uplift/downlift arrows on hover.
   - Depth indentation via `paddingLeft: depth * 16 + 12`.
   - Source files are dynamically attached as children of their parent spec node via `attachSourceFiles()`.

3. **Task Queue** (`tasks`, icon: `Layers`) — Expandable job → task hierarchy (`JobsList`). Badge shows running-job count.
   - Each job row shows status icon, task count, progress %, and cost.
   - **Hover actions** (uses named group `group/job`): Running jobs → Pause + Cancel; Paused jobs → Resume + Cancel; Completed jobs → Merge + Cancel. Icons: `Pause`, `Play`, `Square`, `GitMerge`. Cost/progress hidden on hover to make room for action buttons.
   - Tasks show status + name. Clicking a task opens the bottom drawer.

4. **Files** (`files`, icon: `FolderTree`) — Full directory browser per workspace (`FileTreeView`, recursive). Expandable directories, `.g5/` directory highlighted. Files open as tabs. Hidden when `pm-paper` theme is active.

Each section supports: collapse/expand toggle, drag-to-reorder, resize handles between adjacent sections, detach into a floating panel, and pop-out into a separate OS window (`WindowPortal`).

#### §3.4.3 SpecEditor (Notion-like)

The spec editor is the most complex component (~770 lines). It provides a
Notion-style block editing experience for SemliftSpecs:

**Modes:**
- **Rich mode** (default): Block-level editing with formatting toolbar and slash commands.
- **Source mode**: Raw markdown textarea with monospace font.

**Top bar:**
- Rich/Source mode toggle
- Yjs sync indicator ("Synced" / "Syncing…" with colored dot)
- Connected user avatars (colored border matching cursor color)
- Undo/Redo buttons
- Formatting toolbar: Bold (⌘B), Italic (⌘I), Code (⌘E), List (⌘L), Link (⌘K)

**Block types** (parsed from markdown by `parseMarkdownToBlocks()`):
- Frontmatter → `FrontmatterCard` (YAML field table with navigable links for child_specs/src_files/parent_spec)
- Headings (h1–h6)
- Code blocks (with language label and copy button)
- Bullet/numbered lists
- Blockquotes
- Tables (header row + body)
- Callouts (`:::note`, `:::warning`, `:::tip`)
- Horizontal rules
- Paragraphs

**Block interactions:**
- Double-click a block to enter edit mode (raw markdown textarea, Escape to close).
- Drag handle on the left edge of each block.
- "+" button below each block inserts a new block.
- Hover/focus highlights the active block.

**FrontmatterCard:**
- Expandable metadata card showing YAML frontmatter fields.
- `component_id`, `parent_spec`, `child_specs`, `src_files` are rendered as navigable links.
- "Show graph" toggle reveals the `NeighborhoodGraph`.

**NeighborhoodGraph:**
- SVG spec dependency visualization.
- Parent node above, current node centered, children fanning below, src_files on the right.
- Clickable nodes navigate to the target spec/file.
- Arrow edges with labeled connections.

**Slash command menu** (triggered by typing `/`):
- Heading, Code Block, Bullet List, Callout, Table, Diagram (Mermaid), Divider
- Each entry: icon, label, description.

**Right-click context menu:**
- Format section: Bold/Italic/Code/Link (with ⌘ shortcuts)
- Collaborate section: Add Comment

**Comment column:**
- Margin-aligned comment threads on the right side of the editor.
- Each thread: expand/collapse, avatar colors per author, reply input, resolve button.
- Threads anchored to block line ranges.

#### §3.4.4 CodeEditor

Custom code viewer with Rust syntax highlighting (regex-based for keywords,
types, strings, comments, macros, attributes, lifetimes):

- Line numbers with annotation gutter (blue dots for comment threads).
- Breadcrumb bar showing file path.
- Connected user cursors with colored labels.
- Comment margin column: expand/collapse threads, reply, resolve.
- Minimap toggle (shown when annotations are off).
- Code-level comment overlay for inline discussions.
- Status bar: line/col position, language, encoding.

**Planned:** Replace the custom viewer with a full Monaco editor integration
for editing, syntax highlighting, and Yjs CRDT binding.

#### §3.4.5 DiffView

Side-by-side diff viewer for file changes:

- Old/new line numbers in separate gutters.
- +/− symbols in gutter with color-coded lines (green additions, red deletions, gray context).
- Strikethrough on deleted text.
- File header showing comment count and +N/−N stats.
- Inline comment threads: blue dots in gutter, expand below line on click.
- Hover "+" on any line to add a new comment thread.
- Reply and resolve actions on each thread.

#### §3.4.6 DAG Execution View

SVG-based directed acyclic graph visualization:

- Grid-positioned nodes (col/row layout).
- Dependency edges with SVG arrowhead markers.
- Status-colored fills/strokes: completed=green, running=blue+pulse, pending=gray, blocked=red, failed=red.
- Dashed outline for the currently-selected node.
- Responsive sizing based on container.

#### §3.4.7 Bottom Drawer (Execution)

Resizable panel (140–600px) with drag handle. Shows execution details for the
selected task. Five tabs:

| Tab | Content |
|-----|---------|
| **DAG** | `DagView` neighborhood graph for the current task |
| **Changes** | Left sidebar listing changed files with +N/−N stats; right pane renders `DiffView` for selected file |
| **Timeline** | Chronological agent trace: 💭 thought (italic bg), ⚙ tool (blue mono), ⚠ error (red), ✓ output (green). "Agent is thinking…" spinner for running tasks |
| **Terminal** | Multiple terminal session tabs with status dots, active terminal cwd bar, terminal output with colored prompts (success/error), blinking cursor for running tasks, command input "Run command on worker…" |
| **Intervene** | Worker info banner (CPU/mem bars, uptime, status), Pause/Cancel/Resume buttons, "Send Message to Agent" textarea, "Take Over Mode" card explaining manual control |

Task header: name, status icon, job label, Pause/Stop/Run/Open-in-IDE buttons.

#### §3.4.8 Draft Status Bar

Thin bar below the tab row showing CRDT draft state:
- Dirty count (blue badge, e.g., "3 dirty") or "Clean" (green).
- "Commit" button when dirty.

#### §3.4.9 Right Panel (Chat)

Collapsible chat sidebar (`ChatPanel`):

- Session picker dropdown with history icon and session search.
- Message bubbles: user messages right-aligned (blue), assistant messages left-aligned (gray).
- Tool call badges on assistant messages (colored by tool type).
- "Agent is thinking…" spinner during streaming.
- Pause button while agent is running.
- Multi-line textarea input.
- Session switching loads different message histories.
- On mobile: FAB button opens chat; on desktop: thin strip toggle.

#### §3.4.10 Mobile Layout

Below the `lg` breakpoint, the workspace switches to a **chat-only** layout:

- Full-height `ChatPanel` with all features.
- Mobile header: Smartphone icon + "Mobile" badge.
- `ConnectedWorkerPill` in compact mode.
- Search button (opens spotlight).
- Workers dropdown (full-screen backdrop on tap, selectable workers).
- Notification bell.

This provides productive mobile access to the most critical workflow —
chatting with agents and reviewing their output — without attempting to
squeeze the full multi-panel workspace onto a small screen.

#### §3.4.11 Overlays & Modals

**Spotlight Search (⌘K):**
- Backdrop-blurred overlay with large search input.
- Scoped tabs: All, Workspace, Specs, External, Chat.
- Result types:
  - **Workspace Specs**: Sync/stale dot indicators.
  - **Workspace Files**: Language badges.
  - **Symbols**: Kind labels (struct, fn, enum, trait).
  - **External Repositories**: Per-repo file-level results.
  - **Chat History**: Session match results.
- Recent items shown when query is empty.
- Keyboard hints: ↑↓ navigate, ↵ select, # filter specs, @repo filter repos.

**Notifications flyout:**
- Grouped by type: job_complete (✓), ai_input (⚡), mention (@), comment (💬).
- Unread dot indicator.
- "Respond" button for `ai_input` type (links to task intervention).
- "Mark all read" and "View all" actions.

**Workers dropdown:**
- Lists all workers with status dots, CPU/mem progress bars, uptime, task assignment.
- Active worker count badge on the trigger button.

### §3.5 Settings Panel (UserSettingsPanel)

Implemented as a flyout overlay within WorkspacePageV2, accessible via the gear
icon in the top bar. Three tabs:

**Tab 1 — Organizations:**
- Per-org billing cards with plan badge (enterprise/team/free) and role badge (owner/admin/member).
- Spend/budget progress bar with current/limit display.
- Token usage (input/output tokens formatted as 1.2M).
- Task and agent hours statistics.
- Grid layout for stats: total cost, tokens used, tasks run, agent hours.

**Tab 2 — My Usage:**
- Personal cost, tokens, and tasks grid.
- Org membership summary with per-org cost.
- Usage period context.

**Tab 3 — Preferences:**
- **Theme selector**: 9-theme grid with accent color dots, active checkmark. Click to switch instantly.
- **Dark/Light mode toggle**: Independent of theme selection.
- **Default model dropdown**: Model selection for new chat sessions.
- **Budget warning threshold**: Numeric input for cost alert threshold.
- **Notification toggles**: On/off switches for task-complete and agent-blocked notifications.
- **Delivery channels**: Dynamic list with add (+ button) and remove (× button) per channel. Each channel has: type-specific icon (Mail/Hash/Phone/Wifi for email/slack/whatsapp/webhook), label, detail, enabled toggle. "Add channel" flow allows selecting channel type and entering details.
- **Editor settings** (collapsible sub-section):
  - Font size slider (10–24, default 13).
  - Tab size radio (2 / 4, default 4).
  - Toggle switches for: Rainbow brackets, Code folding, Minimap, Word wrap, Indent guides, Bracket pair guides, Ligatures, Sticky scroll.
  - Render whitespace dropdown (`none` / `boundary` / `all`).
  - Each toggle writes to `UserSettings.editor.*` and is consumed by `CodeEditor` and `SpecEditor` via the `useSettings()` hook.

**Persistence model:**

1. **localStorage** (instant, offline-safe): Key `g5:user-settings`. Read on mount, written synchronously on every change for zero-latency theme/mode switching.
2. **Backend API** (async, cross-device sync): `PUT /api/v1/users/me/settings` with the full `UserSettings` payload. Called debounced (500ms) after each change. `GET /api/v1/users/me/settings` fetched on first mount and merged with localStorage cache (server wins on conflict).

> **Note (2026-02-13):** The backend `/api/v1/users/me/settings` endpoint does not yet exist.
> The current `/api/v1/users/me` endpoint only supports `displayName` and `avatarUrl`.
> A new route handler and database column (JSONB `preferences` on `users` table)
> are needed. Until then, persistence is localStorage-only.

3. **Migration logic**: On load, if cached settings contain the legacy `theme` field (string), split it into `themeName` + `themeMode`. If `notifyChannels` is an object (old format), convert to array.

### §3.6 Org Creation & Onboarding Flow

New users and users creating additional orgs go through a lightweight
onboarding flow. This is a **modal sequence** (not a separate page) triggered
from the org switcher or the "Create organization" button in settings.

**Step 1 — Name your org:**
- Text input for org name (validates slug availability on debounce).
- Auto-generated slug preview below the input (e.g., "acme-corp").
- "Create" button → `POST /api/v1/orgs` (triggers the server-side cascade
  documented in API_AND_STORAGE.md §3.12.1: creates org, base workspace,
  docs repo, and owner membership in one transaction).

**Step 2 — Connect repositories (optional):**
- Shows SCM connection status (GitHub/GitLab connected or not).
- If connected: filterable list of accessible repos with checkboxes.
  Import selected repos → `POST /api/v1/scm/:provider/repos/import`.
  Imported repos are automatically wired to the base workspace.
- If not connected: "Connect GitHub" / "Connect GitLab" buttons that
  initiate the SCM OAuth flow (§3.11.2). After callback, the repo list
  populates.
- "Skip" link to proceed without importing repos.

**Step 3 — Invite team members (optional):**
- Email input with role dropdown (`admin` / `member` / `viewer`).
- "Add another" button for batch invites.
- "Send invites" button → `POST /api/v1/orgs/:orgId/members` for each.
- "Skip" link to proceed solo.

**Step 4 — Done:**
- Success summary: org name, N repos imported, N members invited.
- "Open Base Workspace" button → navigates to `/:org/base`.

The entire flow is skippable after step 1 — an org with just a name and
an empty base workspace is fully functional. Repos and members can be added
later from the settings panel.

### §3.7 Org Members & Invite Page

Accessible from the settings panel (Tab 1 → click org → "Manage Members")
or directly via `/:org/settings/members`. Shows the full member list with
role management.

**Layout:**
```
┌─────────────────────────────────────────────────────────────────┐
│  Members — Acme Corp                                [Invite +]  │
│─────────────────────────────────────────────────────────────────│
│                                                                  │
│  ┌────┬──────────────────┬───────────┬──────────┬─────────────┐│
│  │    │ Name / Email      │ Role      │ Joined   │ Actions     ││
│  ├────┼──────────────────┼───────────┼──────────┼─────────────┤│
│  │ 🟢 │ Alice (you)       │ owner  ▾  │ Jan 5    │             ││
│  │ 🟢 │ bob@acme.com      │ admin  ▾  │ Jan 12   │ [Remove]    ││
│  │ 🟡 │ carol@acme.com    │ member ▾  │ Feb 1    │ [Remove]    ││
│  │ ⏳ │ dave@ext.com       │ viewer ▾  │ pending  │ [Resend]    ││
│  └────┴──────────────────┴───────────┴──────────┴─────────────┘│
│                                                                  │
│  Pending Invitations (1)                                        │
│  ┌──────────────────┬───────────┬──────────┬──────────────────┐│
│  │ dave@ext.com      │ viewer    │ expires  │ [Revoke]         ││
│  │                   │           │ in 5d    │                  ││
│  └──────────────────┴───────────┴──────────┴──────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

**Invite dialog** (triggered by [Invite +]):
- Email input (autocomplete from provider contacts if SCM connected).
- Role selector: `admin` / `member` / `viewer` with privilege summary
  tooltip explaining what each role can do (see API_AND_STORAGE.md
  §3.12.2 privilege matrix).
- "Send Invite" button → `POST /api/v1/orgs/:orgId/members`.
- Confirmation toast: "Invitation sent to dave@ext.com as viewer."

**Role change:**
- Click the role dropdown on any member row → selects new role →
  `PATCH /api/v1/orgs/:orgId/members/:userId`.
- Owner role cannot be assigned via dropdown (requires explicit transfer).
- Viewers/members cannot change roles (dropdown disabled).

**Remove member:**
- Confirmation modal: "Remove bob@acme.com from Acme Corp?"
- `DELETE /api/v1/orgs/:orgId/members/:userId`.

### §3.8 Workspace Visibility Controls

Workspace visibility is managed from the workspace settings (accessible via
the gear icon in the workspace top bar, or from the WorkspaceSwitcher context
menu → "Settings").

**Visibility toggle:**
- Radio group: "Org-visible" (default) / "Invite-only".
- Changing to invite-only shows a warning: "Only you and explicitly added
  members will be able to access this workspace."
- `PATCH /api/v1/workspaces/:workspaceId { visibility: 'invite_only' }`.

**Members list (visible only when invite-only):**
- Same table layout as org members but scoped to the workspace.
- Add member: select from org members dropdown + role (`admin`/`editor`/`viewer`).
- `POST /api/v1/workspaces/:workspaceId/members`.

**WorkspaceSwitcher integration:**
- Invite-only workspaces show a 🔒 icon next to the name.
- Users who don't have access simply don't see invite-only workspaces
  in the list (server-side filtering).

---

## §4 Shared Components (V2)

> **Note:** V1 shared components (`Chat.tsx` 49 lines, `TaskQueue.tsx` 231 lines,
> `CommentThreadPanel.tsx` 105 lines, `DependencyList.tsx` 95 lines,
> `MermaidDiagram.tsx` 210 lines, `Sidebar.tsx`/`SidebarItem.tsx`) are **deprecated**.
> All functionality has been reimplemented inline in `WorkspacePageV2.tsx` with
> significant enhancements. The V1 files should be removed.

### §4.1 CommentThread (reusable)

Used by SpecEditor, CodeEditor, and DiffView. Renders a collapsible comment
thread card:
- Expand/collapse toggle.
- Author avatars with per-author color coding.
- Reply input with submit.
- Resolve button.
- "N more" indicator when collapsed showing hidden reply count.
- Anchored to line ranges in code, block positions in specs, or diff line indices.

### §4.2 MarkdownRenderer (exported)

Named export `MarkdownRenderer` — a read-only markdown rendering component:
- Headings (h1–h6), code blocks with language labels, tables, blockquotes.
- Ordered and unordered lists.
- Inline formatting: bold, italic, inline code.
- Used for rendering project docs and read-only document tabs.

### §4.3 ConnectedWorkerPill

Inline component showing the worker bound to the current workspace:
- Status dot: running (green+pulse), idle (gray), provisioning (yellow+pulse), offline (red).
- Worker name, CPU%, and current task name.
- Compact mode for mobile (hides CPU/task text, shows dot only).

### §4.4 ErrorBoundary

Wraps the entire page. Catches React render errors and displays a styled panel
with the error message and stack trace.

### §4.5 Utility Functions

| Function | Purpose |
|----------|---------|
| `highlightRust()` | Regex-based Rust syntax highlighting (keywords, types, strings, comments, macros, attributes, lifetimes) |
| `parseMarkdownToBlocks()` | Parses markdown string into typed block array (frontmatter, heading, code, list, blockquote, table, callout, hr, paragraph) |
| `parseFrontmatter()` | Extracts YAML frontmatter into typed fields (string, array, null) |
| `resolveSpecRef()` | Resolves a component_id or spec name to an OntologyNode (exact → name → content match) |
| `filterTree()` | Recursive ontology tree search by name/content substring |
| `attachSourceFiles()` | Attaches source file children to ontology spec nodes using `NODE_SOURCE_MAP` |
| `formatLargeNumber()` | Format numbers as 1.2M / 3.4K |
| `formatCost()` | Format as $X.XX |
| `renderInlineMarkdown()` | Parses inline bold/italic/code into React elements |

---

## §5 Mock Data Model (V2)

### §5.1 Current Types (WorkspacePageV2.tsx)

```typescript
// Repository — multi-repo workspace support
interface Repo {
  id: string;
  org: string;
  name: string;
  branches: Branch[];
}
interface Branch {
  name: string;
  isDefault: boolean;
}

// Workspace — multi-repo with worker binding
interface Workspace {
  id: string;
  name: string;
  status: 'active' | 'blocked' | 'review' | 'integrated' | 'idea';
  description: string;
  repos: { repoId: string; branch: string }[];
  workerId: string | null;           // bound worker (1 per workspace)
}

// Editor tab — tracks open files in center panel
interface EditorTab {
  id: string;
  nodeId: string;
  type: 'spec' | 'code' | 'document' | 'source';
}

// Chat message — with tool badges
interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  tools?: string[];                  // tool call badges (e.g., ["read_file", "run_test"])
}

// Agent timeline entry — execution trace
interface TimelineEntry {
  time: string;
  type: 'thought' | 'tool' | 'output' | 'error';
  content: string;
}

// Task — with files and timeline
interface TaskItem {
  id: string;
  name: string;
  status: 'completed' | 'running' | 'pending' | 'blocked' | 'failed';
  files: string[];
  timeline: TimelineEntry[];
}

// Job — contains tasks, tracks cost
interface JobItem {
  id: string;
  name: string;
  status: 'completed' | 'running' | 'pending';
  tasks: TaskItem[];
  progress: number;                  // 0–100
  cost: string;                      // e.g., "$12.50"
}

// Notification — typed for routing
interface NotificationItem {
  id: string;
  type: 'job_complete' | 'ai_input' | 'mention' | 'comment';
  title: string;
  body: string;
  time: string;
  read: boolean;
  taskId?: string;                   // for ai_input → intervention link
}

// Diff — line-level
interface DiffLine {
  type: 'context' | 'addition' | 'deletion';
  content: string;
  lineOld?: number;
  lineNew?: number;
}

// DAG node — grid-positioned
interface DagNode {
  id: string;
  name: string;
  status: 'completed' | 'running' | 'pending' | 'blocked' | 'failed';
  col: number;
  row: number;
  deps: string[];                    // dependency node IDs
  isCurrent?: boolean;
}

// Chat session — persistent conversations
interface ChatSession {
  id: string;
  title: string;
  preview: string;
  time: string;
  isActive: boolean;
}

// Organization — billing + usage
interface Organization {
  id: string;
  name: string;
  plan: 'enterprise' | 'team' | 'free';
  role: 'owner' | 'admin' | 'member';
  billing: {
    currentSpend: number;
    budget: number;
    period: string;
  };
  usage: {
    inputTokens: number;
    outputTokens: number;
    tasksRun: number;
    agentHours: number;
  };
}

// User profile — personal stats
interface UserProfile {
  id: string;
  name: string;
  email: string;
  avatar: string;
  usage: {
    totalCost: number;
    totalTokens: number;
    totalTasks: number;
  };
}

// Theme — 9 named themes × 2 modes
type ThemeName =
  | 'editor' | 'signal-studio' | 'obsidian-gold' | 'carbon-clay'
  | 'alpine-graphite' | 'darkroom' | 'ink-parchment' | 'studio-night'
  | 'pm-paper';

// Notification channel — dynamic add/remove
interface NotifyChannel {
  id: string;
  type: 'email' | 'slack' | 'whatsapp' | 'webhook';
  label: string;
  detail: string;
  enabled: boolean;
}

// Editor preferences — nested under UserSettings.editor
interface EditorSettings {
  editorFontSize: number;                          // default: 13
  editorTabSize: number;                           // default: 4
  rainbowBrackets: boolean;                        // default: true
  codeFolding: boolean;                            // default: true
  showMinimap: boolean;                            // default: true
  wordWrap: boolean;                               // default: false
  indentGuides: boolean;                           // default: true
  bracketPairGuides: boolean;                      // default: true
  ligatures: boolean;                              // default: true
  stickyScroll: boolean;                           // default: true
  renderWhitespace: 'none' | 'boundary' | 'all';  // default: 'none'
}

// User settings — persisted via useSettings hook
interface UserSettings {
  themeName: ThemeName;
  themeMode: 'dark' | 'light';
  defaultModel: string;
  budgetWarnAt: number;
  notifyTaskComplete: boolean;
  notifyAgentBlocked: boolean;
  notifyChannels: NotifyChannel[];
  compactSidebar: boolean;
  editor: EditorSettings;
}

// Worker — local (krunvm/process) or cloud (Kata) worker instance
interface Worker {
  id: string;
  name: string;
  status: 'running' | 'idle' | 'provisioning' | 'offline';
  taskId: string | null;
  cpu: number;                       // 0–100
  mem: number;                       // 0–100
  uptime: string;
  taskName: string;
}

// Comment thread — shared across editors
interface CommentThread {
  id: string;
  lineStart: number;
  lineEnd: number;
  resolved: boolean;
  comments: {
    author: string;
    avatar: string;
    text: string;
    time: string;
  }[];
}

// Connected collaborator — cursor position
interface Collaborator {
  id: string;
  name: string;
  avatar: string;
  color: string;
  cursor: { line: number; col: number };
}

// Ontology node — recursive tree
interface OntologyNode {
  id: string;
  name: string;
  type: 'spec' | 'code' | 'document' | 'source';
  content: string;                   // markdown blob
  children?: OntologyNode[];
  filePath?: string;
}

// File tree node
interface FileTreeNode {
  name: string;
  type: 'file' | 'dir';
  children?: FileTreeNode[];
}

// Project document
interface ProjectDoc {
  id: string;
  title: string;
  category: 'architecture' | 'plan' | 'guide';
  content: string;
  preview: string;
  lastUpdated: string;
}
```

### §5.2 Mock Data Instances

| Export | Count | Description |
|--------|-------|-------------|
| `MOCK_USER` | 1 | "Alex Chen" with usage stats |
| `MOCK_ORGS` | 3 | G5 Labs (enterprise), Acme Corp (team), Personal (free) — each with billing & usage |
| `THEME_OPTIONS` | 9 | Theme catalog with name, label, accent color |
| `DEFAULT_SETTINGS` | 1 | Default UserSettings with 2 channels (email + slack) |
| `REPOS` | 4 | common, g5-api, g5-frontend, g5-worker |
| `WORKSPACES` | 3 | Multi-repo workspaces with assigned workerIds |
| `WORKERS` | 4 | Workers with status, CPU/mem, uptime, task assignment |
| `MOCK_JOBS` | 3 | Jobs with detailed tasks and timeline entries |
| `MOCK_CHAT` | 4 | Chat messages with tool badges |
| `MOCK_COMMENT_THREADS` | 3 | Comment threads keyed by ontology node |
| `CODE_COMMENT_THREADS` | 3 | Code-level comment threads with line ranges |
| `CONNECTED_USERS` | 2 | Alice + Agent with cursor positions |
| `MOCK_NOTIFICATIONS` | 6 | Mixed types: job_complete, ai_input, mention, comment |
| `DAG_NODES` | 7 | Grid-positioned with dependencies |
| `DIFF_LINES` | ~28 | HashMap→BTreeMap refactor diff |
| `DIFF_COMMENT_THREADS` | 3 | Comments anchored to diff line indices |
| `TERMINAL_OUTPUT` | ~18 | Cargo build + test terminal lines |
| `CHAT_SESSIONS` | 5 | Session titles, previews |
| Rust source files | 13 | Full source for scheduler, waterfall, id_service, anthropic, provider, compactor, react_loop, bootstrap, state, terminal, store, python, allocator |
| Per-workspace ontology | 3 | Full ontology trees with spec content (YAML frontmatter + markdown body) |
| Per-workspace file trees | 3 | Directory structures |
| `NODE_SOURCE_MAP` | ~15 | Maps spec node IDs → source file paths |
| Per-workspace docs | 3 | Architecture, plan, and guide documents |

---

## §6 Mock ↔ Backend Schema Alignment

This section maps every mock type to the backend schema and flags gaps.

### §6.1 Workspace

| Mock Field | Backend Column | Gap |
|------------|---------------|-----|
| `id` | `workspaces.id` | ✓ aligned — path-based `component_id` (e.g., `task-graph-rework`). Backend may use UUID PK internally but API surface uses `component_id`. |
| `name` | `workspaces.name` | ✓ aligned |
| `status` | `workspaces.status` | Mock capitalizes ("Active"), backend lowercase ("active") |
| `description` | `workspaces.description` | ✓ aligned |
| `divergence` | — | **MISSING in backend.** Must be computed from `workspace_branches.head_commit` vs base. |
| `tasksCompleted` | Computed from `jobs`+`tasks` | **MISSING:** need aggregate query |
| `totalTasks` | Computed from `jobs`+`tasks` | **MISSING:** need aggregate query |
| `lastUpdated` | `workspaces.updated_at` | Mock uses relative string; backend uses TIMESTAMPTZ |
| `runningTime` | — | **MISSING in backend.** Compute from `jobs.started_at` to now. |
| `dependencies` | `workspace_dependencies` table | Mock uses ID array; backend uses junction table |
| `cost` | Computed from `jobs.cost_usd` | **MISSING:** need SUM aggregate |
| `concurrentTasks` | `workspace_settings.max_concurrent_tasks` | Mock is a number; backend is in settings table |
| — | `workspaces.org_id` | **MISSING in mock.** Workspaces belong to orgs. |
| — | `workspaces.created_by` | **MISSING in mock.** |
| — | `workspace_branches[]` | **MISSING in mock.** Multi-repo branches per workspace. |
| — | `workspace_branches.head_commit` | **MISSING in mock.** Pinned SHA concept absent. |
| — | `workspace_settings.*` | **MISSING in mock.** Spend limits, worker config, etc. |

**Verdict:** Mock `Workspace` is significantly underspecified. Needs `branches[]`
with `head_commit` and `repo` info, `org_id`, computed stats.

### §6.2 OntologyNode

| Mock Field | Backend Column | Gap |
|------------|---------------|-----|
| `id` | `ontology_nodes.id` | ✓ aligned — path-based `component_id` (e.g., `auth/validator`). Deterministic, derived from filesystem path. |
| `parentId` | `ontology_nodes.parent_id` | ✓ aligned |
| `name` | `ontology_nodes.name` | ✓ aligned |
| `type` | `ontology_nodes.node_type` | DTO uses `nodeType` (not `type`) to avoid JS reserved word conflicts |
| `filePath` | `ontology_nodes.file_path` | ✓ aligned |
| `content` | Via `GET .../content` | Mock inlines content; backend fetches separately |
| `children` | Via recursive query | Mock uses nested array; backend is flat with `parent_id` |
| — | `workspace_branch_id` | **MISSING in mock.** Nodes belong to a workspace branch. |
| — | `sort_order` | **MISSING in mock.** |
| — | `sync_status` | **PARTIALLY mocked.** OntologyPage fakes sync via hash; backend has real enum (synced/modified/added/deleted/conflict). |
| — | `ahead_of_base` / `behind_base` | **MISSING in mock.** |
| — | `content_hash` | **MISSING in mock.** |

**Verdict:** Mock tree lacks repo-branch context and real sync status. Content
should be lazy-loaded, not inlined.

### §6.3 ExecutionTask ↔ Jobs + Tasks

| Mock Field | Backend Column | Gap |
|------------|---------------|-----|
| `id` | `tasks.id` | ✓ |
| `name` | `tasks.name` | ✓ |
| `status` | `tasks.status` | ✓ aligned |
| `workspaceId` | `jobs.workspace_id` | Mock flattens; backend nests task under job |
| `workspaceName` | `workspaces.name` | Denormalized in mock |
| `job` | `jobs.job_type` | Mock uses `'compile' \| 'uplift'`; backend has more types |
| — | `jobs.id` | **MISSING in mock.** Tasks should be grouped under a Job entity. |
| — | `jobs.status` | **MISSING in mock.** |
| — | `jobs.depends_on_job` | **MISSING in mock.** Job-to-job sequencing is faked in UI. |
| — | `tasks.worker_id` | **MISSING in mock.** |
| — | `tasks.depends_on[]` | **MISSING in mock.** Task dependency DAG. |
| — | `tasks.cost_usd` | **MISSING in mock.** |
| — | `tasks.branch_name` | **MISSING in mock.** |
| — | `tasks.base_commit` | **MISSING in mock.** |
| — | `timeline_entries` | Mock has flat `executionLogs[]`; backend has full `timeline_entries` table. |
| — | `task_checkpoints` | **MISSING in mock.** |

**Verdict:** The biggest structural gap. Mock flattens job→task into a single
`ExecutionTask` type. The TaskQueue component partly reconstructs the hierarchy
by grouping on `job` field, but Job as a first-class entity is absent.
WorkspacePage has its own local `Job`/`Task` types that are closer to the
backend model but not integrated with mock.ts.

### §6.4 Annotations ↔ Comments

| Mock Field | Backend Column | Gap |
|------------|---------------|-----|
| `id` | `comment_index.id` | ✓ |
| `line` | `comment_index.line_number` | ✓ |
| `author.name` | `users.display_name` | Mock inlines; backend joins |
| `content` | `comment_index.content` | ✓ |
| `timestamp` | `comment_index.created_at` | Mock uses string; backend TIMESTAMPTZ |
| `resolved` | `comment_index.resolved` | ✓ |
| `replies[]` | `comment_replies` table | ✓ conceptually aligned |
| — | `comment_index.scope` | **MISSING in mock.** `'line' \| 'node'` |
| — | `comment_index.node_id` | **MISSING in mock.** Annotations are keyed by node in `nodeAnnotations` map but no FK. |
| — | `comment_index.file_path` | **MISSING in mock.** |
| — | `comment_index.commit_sha` | **MISSING in mock.** |
| — | `comment_index.commit_range_base/head` | **MISSING in mock.** |
| — | `comment_index.origin_branch` | **MISSING in mock.** Cross-branch comments. |
| — | `comment_index.resolved_at` | **MISSING in mock.** |

**Verdict:** Mock annotations are line-level only and lack commit-anchoring,
branch tracking, and node scope. The CommentThreadPanel component works but
needs enrichment.

### §6.5 Chat

| Mock Field | Backend Column | Gap |
|------------|---------------|-----|
| `role` | `chat_messages.role` | Mock has user/assistant; backend adds system/tool |
| `content` | `chat_messages.content` | ✓ |
| — | `chat_sessions.*` | **ENTIRELY MISSING.** No session concept in mock. |
| — | `chat_messages.artifacts` | **MISSING.** |
| — | `chat_messages.citations` | **MISSING.** |
| — | `chat_messages.status` | **MISSING.** Streaming state. |
| — | `chat_sessions.scope_type/scope_ref` | **MISSING.** Chat is unscoped in mock. |
| — | `chat_sessions.linked_commits/linked_nodes` | **MISSING.** |
| — | `chat_sessions.forked_from_id` | **MISSING.** Fork/resume. |

**Verdict:** Chat is a shell. The Chat component renders only empty state.
Needs full session lifecycle, streaming, artifacts, and scoping.

### §6.6 Draft Sessions

**Entirely absent from mock data.** The CRDT / Yjs draft session concept
(`draft_sessions` table, `WS /api/v1/yjs/:workspaceId/:branch`) has no frontend
representation. The OntologyPage content viewer is read-only (no editing).
WorkspacePage's `DocumentContent` is also read-only.

### §6.7 Workers

**Entirely absent from mock data.** The `workers` table and worker poll model
have no frontend representation. No worker status indicators, no
provisioning/idle/terminated lifecycle.

### §6.8 Docs

| Mock Type | Backend Table | Gap |
|-----------|--------------|-----|
| `DocNode` | `docs` + `doc_node_links` | Mock `linkedTo[]` maps to junction table. Content is inlined vs. file-based. Docs have staleness columns (`last_uplifted_at`, `last_downlifted_at`, `content_updated_at`). Linkedness is determined by the `doc_node_links` table indexing YAML frontmatter. |

**Verdict:** Unified model. Old `FeatureNode` and `PlanNode` collapsed into `DocNode`.

---

## §7 V2 Editor Revision

WorkspacePage represents the direction for v2. This section identifies what
needs to change to align the mock prototype with the backend architecture.

### §7.1 Design Direction

The WorkspacePage correctly unifies three concerns:
1. **Ontology browsing** (left sidebar tree)
2. **Document editing** (center tabbed editor — dual-mode, see §11)
3. **Execution monitoring** (bottom drawer + left sidebar jobs panel)

Specs and docs open in the `DocumentEditor` (§11): rich (Tiptap/ProseMirror)
by default, with a toolbar toggle to source (Monaco) mode. Code files open
directly in `MonacoEditor`. Both paths share the same Yjs CRDT document for
real-time collaboration.

This is the right approach. OntologyPage and ExecutionPage should be **retired**
in favor of WorkspacePage, with WorkspacePage promoted to the primary editing
surface at route `/:workspaceId/*`.

### §7.2 Structural Changes Required

#### 7.2.1 Route Restructure

**Current (legacy mock):**
```
/                           → WorkspaceSwitcher
/workspace                  → WorkspacePage
/:repo/:workspace/ontology/* → OntologyPage
/:repo/:workspace/job/...    → ExecutionPage
```

**Target (implemented in §2):**
```
/                                          → Redirect to /:org/:workspace
/:org/:workspace                           → WorkspacePage (unified editor)
/:org/:workspace/spec/:specPath            → WorkspacePage (deep-link to spec)
/:org/:workspace/doc/:docPath              → WorkspacePage (deep-link to doc)
/:org/:workspace/file/:filePath            → WorkspacePage (deep-link to file)
/:org/:workspace/job/:jobId                → WorkspacePage (job in bottom drawer)
/:org/:workspace/job/:jobId/task/:taskId   → WorkspacePage (task detail)
/:org/settings/members                     → OrgMembersPage (§3.7)
/settings                                  → SettingsPage
/create-org                                → CreateOrgDialog (§3.6, modal over current page)
```

All navigation uses **path-based `component_id`** identifiers directly in
the URL — no UUID indirection, no slug resolver. The URL *is* the
identifier (e.g., `/g5/task-graph-rework/spec/auth/validator`). The
backend API also accepts `component_id` values, with an internal
`component_id ↔ UUID` lookup table in Postgres.

#### 7.2.2 Workspace Model Enrichment

The `Workspace` display type needs to expand:

```typescript
interface WorkspaceSummary {
  id: string;               // component_id (e.g., "task-graph-rework")
  orgId: string;
  name: string;
  isBase: boolean;            // true for the org's base workspace
  status: WorkspaceStatus;    // 'idea' | 'active' | 'blocked' | 'review' | 'integrated'
  visibility: 'org' | 'invite_only';  // access scope (§3.8)
  description: string;
  createdBy: UserRef;
  updatedAt: string;        // ISO 8601
  // Computed aggregates
  branches: BranchSummary[];
  stats: {
    totalJobs: number;
    runningJobs: number;
    totalTasks: number;
    completedTasks: number;
    activeTasks: number;
    blockedTasks: number;
    failedTasks: number;
    totalCostUsd: number;
  };
  dependencies: string[];   // workspace IDs
}

interface BranchSummary {
  id: string;               // workspace_branch UUID
  repoId: string;
  repoName: string;
  branchName: string;
  baseBranch: string;
  headCommit: string;       // SHA
  hasDraft: boolean;
  draftDirtyCount: number;
}
```

#### 7.2.3 Job/Task Model Promotion

Replace the flat `ExecutionTask` with a proper hierarchy:

```typescript
interface JobSummary {
  id: string;
  workspaceId: string;
  name: string;
  jobType: 'compile' | 'uplift' | 'downlift' | 'test' | 'review' | 'chat' | 'custom';
  status: 'pending' | 'bootstrapping' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled' | 'rolled_back';
  dependsOnJob: string | null;
  totalTasks: number;
  completedTasks: number;
  failedTasks: number;
  costUsd: number;
  startedAt: string | null;
}

interface TaskDetail {
  id: string;
  jobId: string;
  nodeId: string | null;
  name: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'blocked';
  branchName: string;
  baseCommit: string;
  workerId: string | null;
  dependsOn: string[];
  costUsd: number;
  timeline: TimelineEntry[];     // lazy-loaded, paginated
  checkpoints: TaskCheckpoint[]; // lazy-loaded
}

interface TimelineEntry {
  id: string;
  entryType: 'thought' | 'tool_call' | 'tool_result' | 'output' | 'error'
           | 'user_message' | 'agent_message' | 'checkpoint';
  content: string;
  metadata: Record<string, unknown>;
  createdAt: string;
}

interface TaskCheckpoint {
  id: string;
  commitSha: string;
  message: string;
  filesChanged: string[];
  stats: { additions: number; deletions: number };
  createdAt: string;
}
```

#### 7.2.4 Ontology Node Enrichment

```typescript
interface OntologyNodeDTO {
  id: string;
  workspaceBranchId: string;
  parentId: string | null;
  name: string;
  nodeType: 'node' | 'document' | 'code';
  filePath: string | null;
  sortOrder: number;
  syncStatus: 'synced' | 'modified' | 'added' | 'deleted' | 'conflict';
  aheadOfBase: number;
  behindBase: number;
  childCount: number;        // for lazy loading
  // Content loaded separately via GET .../content
}
```

Tree should be **lazy-loaded** (children fetched on expand) rather than
fully materialized in a single payload.

#### 7.2.5 Comment Model Enrichment

```typescript
interface CommentDTO {
  id: string;
  authorId: string;
  author: UserRef;
  scope: 'line' | 'node';
  nodeId: string | null;
  filePath: string | null;
  lineNumber: number | null;
  commitSha: string;
  commitRangeBase: string | null;
  commitRangeHead: string | null;
  content: string;
  resolved: boolean;
  resolvedAt: string | null;
  originBranch: string;
  replyCount: number;
  createdAt: string;
}
```

#### 7.2.6 Chat Enrichment

```typescript
interface ChatSessionDTO {
  id: string;
  workspaceId: string;
  sessionType: 'chat' | 'task';
  scopeType: 'workspace' | 'node' | 'task';
  scopeRef: string;
  model: string;
  title: string;
  summary: string;
  status: 'active' | 'completed' | 'archived';
  messageCount: number;
  totalCostUsd: number;
}

interface ChatMessageDTO {
  id: string;
  sessionId: string;
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  artifacts: Artifact[];
  citations: Citation[];
  status: 'pending' | 'streaming' | 'complete' | 'error' | 'stopped';
  messageIndex: number;
  createdAt: string;
}
```

### §7.3 WorkspacePageV2 Resolved & Remaining Gaps

Features ported from V1 pages and newly implemented in V2:

| Feature | V1 Source | V2 Status | Notes |
|---------|-----------|-----------|-------|
| Diff overlay | OntologyPage | ✅ `DiffView` (§3.4.5) | Side-by-side with inline comments |
| Annotations / comments | OntologyPage | ✅ `CommentThread` (§4.1) | Shared across SpecEditor, CodeEditor, DiffView |
| Syntax highlighting | OntologyPage (regex) | ✅ `highlightRust()` | Regex-based, covers keywords/types/strings/macros |
| Tab bar with toolbar | OntologyPage | ✅ | Tabs + mode toggle + sync indicator |
| View/Docs sidebar sections | OntologyPage | ✅ | Project Docs section |
| File Navigation | OntologyPage (stub) | ✅ `FileTreeNode` | Full recursive directory browser |
| Search bar | OntologyPage | ✅ Inline + Spotlight (⌘K) | Always-visible input + full overlay search |
| Mobile responsive | OntologyPage (slide-in) | ✅ Chat-only mobile layout | Responsive `lg:` breakpoint |
| Task execution view | ExecutionPage | ✅ Bottom Drawer (§3.4.7) | 5-tab drawer: DAG, Changes, Timeline, Terminal, Intervene |
| DAG neighborhood graph | ExecutionPage (5-node) | ✅ `DagView` (§3.4.6) | Full 7-node grid-positioned SVG |
| Worker status | None | ✅ `ConnectedWorkerPill` | Top bar + workers dropdown |
| Settings | None | ✅ `UserSettingsPanel` | 3-tab flyout: orgs, usage, preferences |
| Theme system | None | ✅ 8 themes × 2 modes | Full CSS variable system |
| Notification channels | None | ✅ Dynamic add/remove | Email, Slack, WhatsApp, Webhook |
| Chat sessions | None (shell only) | ✅ `ChatPanel` | Session picker, tool badges, pause button |
| Spec editing (Notion-like) | None | ✅ `SpecEditor` (§3.4.3) | Block editor, slash commands, frontmatter card, neighborhood graph |
| Multi-repo workspaces | None | ✅ Repos bar | Per-workspace repo composition |

Remaining gaps for backend integration:

| Feature | Status | Action |
|---------|--------|--------|
| Draft editing (CRDT) | Mocked (sync indicator only) | Wire Yjs + Monaco for real-time editing |
| Monaco code editor | Custom viewer | Replace with full Monaco integration |
| Streaming chat | Mocked | Wire WebSocket chat streaming |
| Cost tracking (live) | Static mock | Connect to jobs/tasks cost APIs |
| Base branch selector | Not implemented | Add diff-against-base dropdown |
| Dependency list in sidebar | Not ported from V1 | Add or integrate into spec frontmatter card |
| Mermaid diagrams | ✅ Implemented | Theme-aware mermaid rendering with CSS var integration |

### §7.4 Remaining Migration Steps

The V2 prototype has completed the UI layer. The remaining work is backend
integration:

1. **Data layer:** Create React Query hooks for all API endpoints (§9.8).
   Replace mock data with hooks. Keep mock data as fallback via MSW
   (Mock Service Worker) for development.

2. **CRDT integration:** Wire Yjs sync for real-time document editing in
   SpecEditor and CodeEditor. Connect the draft status bar to real dirty state.

3. **Monaco integration:** Replace the custom CodeEditor with Monaco wrapper
   (§9.13) for syntax highlighting and Yjs CRDT binding.

4. **Chat streaming:** Wire WebSocket chat streaming for real-time agent
   responses. Add artifact rendering and citation linking.

5. **Cleanup:** Remove V1 pages (OntologyPage, ExecutionPage), legacy shared
   components, and the separate mock.ts data file. Extract WorkspacePageV2
   components into individual files for maintainability.
   and Sidebar/SidebarItem components. Add SettingsPage.

---

## §8 Component Hierarchy (V2 Implemented)

```
App
├── WorkspaceSwitcher
│   ├── TopBar
│   ├── WorkspaceTable (search + filter chips + branch mode toggle)
│   │   └── WorkspaceRow (name, status Chip, cost, tasks, actions)
│   ├── TaskQueue (all workspaces, grouped workspace → job → task)
│   └── DependencyGraph (SVG)
│
├── WorkspacePageV2 (unified editor — PRIMARY surface)
│   ├── ErrorBoundary
│   ├── TopBar (44px fixed)
│   │   ├── Sidebar toggle (☰)
│   │   ├── WorkspaceSwitcher (dropdown)
│   │   │   ├── SearchInput
│   │   │   └── WorkspaceRow (repos, branches, status, actions)
│   │   ├── ConnectedWorkerPill (status dot, CPU%, task name)
│   │   ├── Inline SearchBar (⌘K hint → Spotlight overlay)
│   │   ├── Connected user avatars
│   │   ├── Workers dropdown (worker list with CPU/mem bars)
│   │   ├── NotificationBell (unread badge → NotificationsFlyout)
│   │   ├── Settings gear (→ UserSettingsPanel flyout)
│   │   └── Chat toggle
│   │
│   ├── ReposBar (horizontal repo chips with branch labels)
│   │
│   ├── LeftSidebar (resizable 200–480px)
│   │   ├── SearchFilter (text input, filters all sections)
│   │   ├── OntologyTree (recursive, per-workspace per-repo)
│   │   │   └── OntologyTreeNode (icon, dirty dot, ↑↓ hover buttons)
│   │   ├── OntologySearchResults (flat filtered list)
│   │   ├── SidebarSection ("Docs")
│   │   │   └── ProjectDoc (category icon, preview, timestamp)
│   │   ├── SidebarSection ("Files")
│   │   │   └── FileTreeNode (recursive directory browser)
│   │   └── SidebarJobsSection ("Jobs")
│   │       ├── JobItem (status, progress, cost, expand → tasks)
│   │       └── TaskRow (status icon + name, click → bottom drawer)
│   │
│   ├── CenterPanel
│   │   ├── TabBar (closeable tabs, dirty dots, persistent via localStorage)
│   │   ├── ContentView (dispatches by tab type)
│   │   │   ├── SpecEditor (Notion-like block editor — §3.4.3)
│   │   │   │   ├── Mode toggle (Rich / Source)
│   │   │   │   ├── Yjs sync indicator + connected user avatars
│   │   │   │   ├── Formatting toolbar (Bold/Italic/Code/List/Link)
│   │   │   │   ├── FrontmatterCard (YAML table + navigable links)
│   │   │   │   │   └── NeighborhoodGraph (SVG spec dependency viz)
│   │   │   │   ├── SpecBlock[] (heading, code, list, table, callout, etc.)
│   │   │   │   │   └── Block edit mode (double-click → markdown textarea)
│   │   │   │   ├── SlashCommandMenu (7 block types)
│   │   │   │   ├── ContextMenu (format, collaborate, create actions)
│   │   │   │   └── CommentColumn (margin-aligned threads)
│   │   │   │       └── CommentThread (expand, reply, resolve)
│   │   │   │
│   │   │   ├── CodeEditor (syntax highlighted viewer)
│   │   │   │   ├── Line numbers + annotation gutter
│   │   │   │   ├── Breadcrumb bar (file path)
│   │   │   │   ├── Connected user cursors (colored labels)
│   │   │   │   ├── CommentColumn (margin-aligned threads)
│   │   │   │   ├── Minimap (toggle)
│   │   │   │   └── StatusBar (line/col, language, encoding)
│   │   │   │
│   │   │   ├── DiffView (side-by-side)
│   │   │   │   ├── Line numbers (old + new)
│   │   │   │   ├── Gutter symbols (+/−)
│   │   │   │   ├── Inline comment threads
│   │   │   │   └── Hover "+" to add comment
│   │   │   │
│   │   │   └── MarkdownRenderer (read-only docs)
│   │   │
│   │   ├── DraftBar (dirty count + Commit button)
│   │   │
│   │   └── BottomDrawer (resizable 140–600px, 5 tabs)
│   │       ├── TaskHeader (name, status, job, Pause/Stop/Run/Open-in-IDE)
│   │       ├── Tab: DAG (DagView — SVG execution graph)
│   │       ├── Tab: Changes (file list + DiffView)
│   │       ├── Tab: Timeline (agent trace entries)
│   │       ├── Tab: Terminal (multi-session, command input)
│   │       └── Tab: Intervene (worker banner, message agent, take-over)
│   │
│   ├── RightPanel (ChatPanel — collapsible)
│   │   ├── SessionPicker (dropdown + search)
│   │   ├── MessageList (user/assistant bubbles, tool badges)
│   │   ├── "Agent is thinking…" spinner
│   │   ├── Pause button
│   │   └── MessageInput (multi-line textarea)
│   │
│   ├── SpotlightSearch (⌘K overlay)
│   │   ├── Scoped tabs (All/Workspace/Specs/External/Chat)
│   │   ├── Spec results (sync/stale dots)
│   │   ├── File results (language badges)
│   │   ├── Symbol results (kind labels)
│   │   ├── External repo results
│   │   └── Chat history results
│   │
│   ├── NotificationsFlyout (grouped by type, actions)
│   │
│   ├── UserSettingsPanel (flyout, 3 tabs)
│   │   ├── Tab: Organizations (billing cards, spend/budget, usage stats)
│   │   ├── Tab: My Usage (personal cost/tokens/tasks)
│   │   └── Tab: Preferences
│   │       ├── Theme grid (8 themes × accent dots)
│   │       ├── Dark/Light mode toggle
│   │       ├── Default model dropdown
│   │       ├── Budget warning input
│   │       ├── Notification toggles
│   │       └── Channels list (add/remove/toggle per channel)
│   │
│   └── MobileLayout (lg:hidden — chat-only)
│       ├── Mobile header (Smartphone icon, "Mobile" badge)
│       ├── ConnectedWorkerPill (compact)
│       ├── Search button (→ Spotlight)
│       ├── Workers dropdown (full-screen)
│       ├── NotificationBell
│       └── ChatPanel (full-height)
│
├── CreateOrgDialog (modal — §3.6)
│   ├── Step 1: Name your org (text input + slug preview)
│   ├── Step 2: Connect repos (SCM list + import checkboxes)
│   ├── Step 3: Invite members (email + role dropdown)
│   └── Step 4: Done (summary + "Open Base Workspace")
│
├── OrgMembersPage (/:org/settings/members — §3.7)
│   ├── MemberTable (role dropdowns, remove buttons)
│   ├── PendingInvitationsTable (resend/revoke)
│   └── InviteDialog (email + role picker)
│
├── WorkspaceVisibilitySettings (within workspace settings — §3.8)
│   ├── Visibility radio (org-visible / invite-only)
│   └── WorkspaceMembersPanel (add/change/remove members)
```

---

## §9 Component APIs

This section defines the props contract for every data-driven component.
These are the interfaces that React Query hooks (or mock providers) must satisfy.
Shared DTO types from §7.2 are referenced by name; callbacks follow the
`on<Event>` convention.

### §9.1 Shared Callback & Utility Types

```typescript
/** Lightweight user ref embedded in DTOs */
interface UserRef {
  id: string;
  displayName: string;
  avatarUrl: string | null;
  initial: string;        // first letter, for avatar fallback
  color: string;          // g5 token name for avatar tint
}

/** Pagination envelope returned by list endpoints */
interface Page<T> {
  items: T[];
  cursor: string | null;  // opaque; pass to next request
  hasMore: boolean;
}

/** WebSocket event pushed to the client */
interface WSEvent<T = unknown> {
  channel: string;         // e.g. "project:uuid", "task:uuid"
  event: string;           // e.g. "status_changed", "timeline_append"
  payload: T;
}

/** Common "loading / error / data" wrapper for hook consumers */
type AsyncState<T> =
  | { status: 'loading' }
  | { status: 'error'; error: Error }
  | { status: 'ok'; data: T };
```

---

### §9.2 WorkspaceSwitcher Components

#### WorkspaceSwitcher (top-level)

```typescript
// No explicit props — reads orgId from route or context.
// Internal hooks:
//   useWorkspaces(orgId)           → AsyncState<WorkspaceSummary[]>
//   useOrgJobs(orgId)              → AsyncState<JobSummary[]>   (for TaskQueue)
//   useWorkspaceDependencies(orgId)→ AsyncState<DepEdge[]>      (for graph)
```

#### WorkspaceTable

```typescript
interface WorkspaceTableProps {
  workspaces: WorkspaceSummary[];
  /** Currently active status filters (empty = show all) */
  activeFilters: Set<WorkspaceStatus>;
  searchQuery: string;
  onSelectWorkspace: (workspaceId: string) => void;
  onPauseWorkspace: (workspaceId: string) => void;
  onAbortWorkspace: (workspaceId: string) => void;
  onCloneWorkspace: (workspaceId: string) => void;
  onNewWorkspace: () => void;
}
```

#### DependencyGraph

```typescript
interface DepEdge {
  from: string;  // workspace ID
  to: string;    // workspace ID
}

interface DependencyGraphProps {
  workspaces: WorkspaceSummary[];
  edges: DepEdge[];
  /** Highlighted workspace (hovered row in table) */
  highlightId: string | null;
  onSelectWorkspace: (workspaceId: string) => void;
}
```

#### TaskQueue

```typescript
interface TaskQueueProps {
  /** Filter to one workspace, or show all */
  workspaceId?: string;
  jobs: JobSummary[];
  /** Nested tasks per job — keyed by job ID */
  tasksByJob: Record<string, TaskSummary[]>;
  onTaskClick: (taskId: string, jobId: string) => void;
}

interface TaskSummary {
  id: string;
  name: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'blocked';
  workerId: string | null;
  costUsd: number;
}
```

---

### §9.3 WorkspacePage Components (v2 Editor)

#### WorkspacePage (top-level)

```typescript
// No explicit props — reads workspaceId from route.
// Internal hooks:
//   useWorkspace(workspaceId)       → AsyncState<WorkspaceDetail>
//   useOntologyTree(branchId)       → AsyncState<OntologyNodeDTO[]>
//   useWorkspaceJobs(workspaceId)   → AsyncState<JobSummary[]>
//   useChatSession(scopeType, scopeRef) → ChatSessionState
//   useWSChannel("workspace:<id>")  → live events
//
// Manages local UI state:
//   openTabs: TabItem[]
//   selectedTab: string | null
//   leftSidebarMode: 'ontology' | 'jobs'
//   bottomDrawerOpen: boolean
//   selectedTaskId: string | null

interface WorkspaceDetail extends WorkspaceSummary {
  branchMode: 'pinned' | 'tracking';
  clonedFromId: string | null;
  /** Active branch ID — the branch the UI should show by default.
   *  Set to the first workspace_branch when workspace is opened.
   *  Updated when the user switches branches in the BranchSelector.
   *  Used by useFileTree(activeBranchId) and useOntologyTree(activeBranchId). */
  activeBranchId: string;
  branches: BranchDetail[];
  settings: WorkspaceSettings;
}

interface BranchDetail extends BranchSummary {
  draftSession: DraftSessionSummary | null;
}

interface DraftSessionSummary {
  id: string;
  dirtyFiles: string[];
  dirtyCount: number;
  persistedAt: string;
  participants: DraftParticipant[];
}

interface DraftParticipant {
  userId: string;
  user: UserRef;
  role: 'editor' | 'reviewer';
  cursorData: { file: string; line: number; col: number } | null;
}
```

#### OntologyTree

```typescript
interface OntologyTreeProps {
  /** Top-level nodes (roots). Children loaded lazily. */
  roots: OntologyNodeDTO[];
  selectedNodeId: string | null;
  expandedNodeIds: Set<string>;
  onSelectNode: (nodeId: string) => void;
  onToggleExpand: (nodeId: string) => void;
  /** Lazy load children. Returns children for the given node. */
  onLoadChildren: (nodeId: string) => Promise<OntologyNodeDTO[]>;
  /** Trigger downlift (spec → code) from this node down */
  onDownlift: (nodeId: string) => void;
  /** Trigger uplift (code → spec) from this node up */
  onUplift: (nodeId: string) => void;
}
```

#### TabBar

```typescript
type TabItem =
  | { type: 'ontology'; nodeId: string; name: string; nodeType: 'node' | 'document' | 'code' }
  | { type: 'doc'; docId: string; name: string }
  | { type: 'diff'; nodeId: string; name: string }

interface TabBarProps {
  tabs: TabItem[];
  activeTabId: string;       // derived from TabItem via getTabId()
  onSelectTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  /** Toolbar state */
  showDiff: boolean;
  onToggleDiff: (show: boolean) => void;
  showAnnotations: boolean;
  onToggleAnnotations: (show: boolean) => void;
  baseBranch: string;
  availableBranches: string[];
  onChangeBranch: (branch: string) => void;
}
```

#### ContentView

```typescript
interface ContentViewProps {
  /** What to render */
  item: TabItem;
  /** Loaded content */
  content: AsyncState<string>;
  /** Yjs document for collaborative editing */
  yjsDoc: Y.Doc;
  /** Whether editing is enabled (draft mode) */
  editable: boolean;
  /** Diff overlay */
  showDiff: boolean;
  diffData: DiffData | null;
  /** Annotations */
  showAnnotations: boolean;
  annotations: CommentDTO[];
  onAddComment: (params: { anchorId: string; content: string }) => void;
  onResolveComment: (threadId: string) => void;
  onReplyToComment: (threadId: string, content: string) => void;
}
```

Routing logic: if `item.filePath` starts with `.g5/specs/` or `.g5/docs/`,
render `DocumentEditor` (§11.7). Otherwise render `MonacoEditor` for code
files. See §11.8 for the full routing function.

#### MonacoEditor (code files)

```typescript
interface MonacoEditorProps {
  /** File path for language detection */
  filePath: string;
  /** Initial content */
  content: string;
  /** Whether editing is enabled (draft mode) */
  readOnly: boolean;
  /** CRDT-aware change handler — sends ops to Yjs doc */
  onEdit?: (changes: monaco.editor.IModelContentChange[]) => void;
  /** Diff decorations to overlay */
  diffDecorations?: DiffDecoration[];
  /** Comment gutter markers */
  commentMarkers?: { line: number; count: number; resolved: boolean }[];
  onCommentClick?: (line: number) => void;
  /** Theme — always 'g5-dark' */
  theme?: string;
}

interface DiffDecoration {
  startLine: number;
  endLine: number;
  type: 'add' | 'remove' | 'modify';
}
```

#### DocumentEditor (spec + doc files)

See §11.7 for `DocumentEditorProps`, `TiptapEditorProps`, and
`DiagramNodeViewProps`. The `DocumentEditor` is the dual-mode wrapper
that manages the rich ↔ source toggle and CRDT binding for all
`.g5/specs/` and `.g5/docs/` files.

#### BottomDrawer (task execution panel)

```typescript
interface BottomDrawerProps {
  open: boolean;
  onToggle: () => void;
  /** Expand to full center panel (replaces content view) */
  onMaximize?: () => void;
  task: AsyncState<TaskDetail> | null;
  /** Live-streamed timeline entries appended via WS */
  timelineStream: TimelineEntry[];
  /** Intervention controls */
  onPauseTask: (taskId: string) => void;
  onResumeTask: (taskId: string) => void;
  onAbortTask: (taskId: string) => void;
  /** Inject a user message into the agent's context */
  onInjectMessage: (taskId: string, content: string) => void;
}
```

#### JobsPanel (left sidebar, jobs mode)

```typescript
interface JobsPanelProps {
  jobs: JobSummary[];
  tasksByJob: Record<string, TaskSummary[]>;
  /** Currently selected task (shown in bottom drawer) */
  selectedTaskId: string | null;
  onSelectTask: (taskId: string) => void;
  onPauseJob: (jobId: string) => void;
  onResumeJob: (jobId: string) => void;
  onCancelJob: (jobId: string) => void;
}
```

#### DependencyList

```typescript
interface NodeDependency {
  nodeId: string;
  name: string;
  nodeType: 'node' | 'document' | 'code';
  depType: 'direct' | 'inherited';
  syncStatus: 'synced' | 'modified' | 'added' | 'deleted' | 'conflict';
}

interface DependencyListProps {
  dependencies: NodeDependency[];
  dependents: NodeDependency[];
  onSelectNode: (nodeId: string) => void;
}
```

---

### §9.4 Chat Components

#### Chat (container)

```typescript
interface ChatProps {
  /** Current session (null = no session yet, show empty + create prompt) */
  session: ChatSessionDTO | null;
  messages: ChatMessageDTO[];
  /** Whether the assistant is currently streaming a response */
  isStreaming: boolean;
  /** Scoping — determines what the chat "knows about" */
  scopeType: 'workspace' | 'node' | 'task';
  scopeRef: string;            // node ID, task ID, or workspace ID
  scopeLabel: string;          // human-readable ("HashTable Aggregate", "Task #3")
  /** Actions */
  onSendMessage: (content: string) => void;
  onStopStreaming: () => void;
  onForkSession: (atMessageIndex: number) => void;
  onResumeSession: () => void;
  onSummarize: () => void;
  onCreateSession: () => void;
  /** Optional intervention (for task-scoped chat) */
  showPause?: boolean;
  onPause?: () => void;
}
```

#### ChatMessage

```typescript
interface ChatMessageProps {
  message: ChatMessageDTO;
  /** Whether this message is currently being streamed */
  isStreaming: boolean;
  /** Artifact rendering */
  onArtifactClick?: (artifact: Artifact) => void;
  /** Citation links */
  onCitationClick?: (citation: Citation) => void;
}

interface Artifact {
  type: ArtifactType;
  title?: string;
  content: string | object;    // String for text-based, object for structured (chart, dataframe)
  language?: string;
  filePath?: string;
  actions?: ArtifactAction[];
}

type ArtifactType =
  | 'code'       // Syntax highlighted code block → Monaco (read-only)
  | 'diff'       // Unified diff → Monaco diff viewer
  | 'table'      // Markdown or JSON table → custom table component
  | 'chart'      // Chart.js specification → react-chartjs-2
  | 'diagram'    // Mermaid diagram → MermaidDiagram component
  | 'image'      // Base64 or URL image → <img> with lightbox
  | 'file'       // Downloadable file content → download button + preview
  | 'terminal'   // Command + output → terminal-styled block
  | 'math'       // LaTeX equation → KaTeX
  | 'json'       // Formatted JSON with folding → react-json-view
  | 'dataframe'; // Tabular data with sorting/filtering → AG Grid or similar

interface ArtifactAction {
  label: string;
  icon: string;
  action:
    | { type: 'copy' }
    | { type: 'download'; filename: string }
    | { type: 'apply'; targetFile: string }    // Apply diff/code to file
    | { type: 'run'; command: string }         // Execute in terminal
    | { type: 'open'; nodeId: string };        // Open in editor
}

interface Citation {
  nodeId?: string;
  filePath?: string;
  lineRange?: [number, number];
  commitSha?: string;
  label: string;
}
```

#### ChatSessionList (session picker)

```typescript
interface ChatSessionListProps {
  sessions: ChatSessionDTO[];
  activeSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  onCreateSession: () => void;
  onSearchSessions: (query: string) => void;
}
```

#### Artifact Protocol

Artifacts are embedded in assistant responses using fenced blocks with a special syntax:

```markdown
Here's the analysis:

:::artifact{type="table" title="Performance Comparison"}
| Method | Time (ms) | Memory (MB) |
|--------|-----------|-------------|
| HashMap | 12.3 | 45.2 |
| BTreeMap | 18.7 | 32.1 |
:::

The HashMap is fastest.

:::artifact{type="chart" title="Latency Distribution"}
{ "type": "bar", "data": { "labels": ["p50", "p90", "p99"],
  "datasets": [{ "label": "ms", "data": [12, 45, 120] }] } }
:::
```

**Frontend parsing:** The `ChatMessage` component splits the response into alternating `text` and `artifact` blocks using a regex parser (`:::artifact{...}...:::`). Each artifact block is rendered by a type-specific renderer (Monaco for code/diff, MermaidDiagram for diagrams, Chart.js for charts, KaTeX for math, etc.).

**Artifact actions** appear in the artifact header. Common actions: copy content, download as file, apply diff to workspace, open in editor.

---

### §9.5 Comments / Annotations

#### CommentThreadPanel

```typescript
interface CommentThreadPanelProps {
  /** Anchor position — line number or node scope */
  anchor: { type: 'line'; lineNumber: number } | { type: 'node'; nodeId: string };
  comments: CommentDTO[];
  /** Positioning (absolute top offset, calculated by parent) */
  topOffset: number;
  onClose: () => void;
  onReply: (commentId: string, content: string) => void;
  onResolve: (commentId: string) => void;
  onUnresolve: (commentId: string) => void;
  /** Current user for "you" indicators */
  currentUserId: string;
}
```

#### AnnotationsSidebar (aggregate view)

```typescript
interface AnnotationsSidebarProps {
  comments: CommentDTO[];
  /** Group by: line, node, or flat list */
  groupBy: 'line' | 'node' | 'none';
  /** Filter */
  showResolved: boolean;
  onToggleResolved: () => void;
  onSelectComment: (commentId: string) => void;
  onReply: (commentId: string, content: string) => void;
  onResolve: (commentId: string) => void;
}
```

---

### §9.6 Settings Components

#### SettingsPage

```typescript
// Reads workspaceId from route.
// Internal hook: useWorkspaceSettings(workspaceId) → AsyncState<WorkspaceSettings>

interface WorkspaceSettings {
  workspaceId: string;
  maxConcurrentTasks: number;       // 1–16
  maxSpendUsd: number | null;       // null = unlimited
  spendPeriod: 'hourly' | 'daily' | 'weekly' | 'monthly' | 'total';
  spendPeriodStart: string | null;
  maxInputTokens: number | null;
  maxOutputTokens: number | null;
  allowCloudWorkers: boolean;
  maxCloudWorkers: number;          // 1–32
  workerIdleTimeoutMin: number;     // 1–60
  updatedBy: UserRef | null;
  updatedAt: string;
}
```

#### SettingsForm

```typescript
interface SettingsFormProps {
  settings: WorkspaceSettings;
  onSave: (patch: Partial<WorkspaceSettings>) => Promise<void>;
  saving: boolean;
  /** Validation errors keyed by field name */
  errors: Record<string, string>;
}
```

#### SpendBreakdown

```typescript
interface SpendBreakdownProps {
  /** Current period spend (from GET /api/v1/workspaces/:workspaceId/settings/spend) */
  spend: {
    periodStart: string;
    periodEnd: string;
    totalUsd: number;
    limitUsd: number | null;
    byJob: { jobId: string; jobName: string; jobType: string; costUsd: number }[];
    byDay: { date: string; costUsd: number }[];
  };
}
```

---

### §9.6.1 Org Onboarding Components

#### CreateOrgDialog

```typescript
interface CreateOrgDialogProps {
  /** Whether the dialog is open */
  open: boolean;
  onClose: () => void;
  /** Called after org is successfully created — navigates to base workspace */
  onOrgCreated: (org: CreateOrgResponse) => void;
}

interface CreateOrgResponse {
  org: { id: string; name: string; slug: string; createdAt: string };
  baseWorkspace: { id: string; name: string };
  docsRepo: { id: string; name: string; gitUrl: string };
}
```

#### OrgOnboardingWizard

Multi-step modal after org creation (§3.6 steps 2–4):

```typescript
interface OrgOnboardingWizardProps {
  orgId: string;
  orgSlug: string;
  /** Step 2: repos to import */
  scmConnections: Array<{ provider: string; connected: boolean }>;
  /** Called when user finishes or skips */
  onComplete: () => void;
}
```

### §9.6.2 Org Members Page

#### OrgMembersPage

```typescript
// Route: /:org/settings/members
// Internal hooks:
//   useOrgMembers(orgId) → AsyncState<OrgMemberDTO[]>
//   useOrgInvitations(orgId) → AsyncState<OrgInvitationDTO[]>
```

#### MemberTable

```typescript
interface MemberTableProps {
  members: OrgMemberDTO[];
  currentUserId: string;
  currentUserRole: 'owner' | 'admin' | 'member' | 'viewer';
  onChangeRole: (userId: string, newRole: string) => void;
  onRemove: (userId: string) => void;
}
```

#### InviteDialog

```typescript
interface InviteDialogProps {
  open: boolean;
  onClose: () => void;
  orgId: string;
  onInviteSent: (invitation: OrgInvitationDTO) => void;
}
```

#### PendingInvitationsTable

```typescript
interface PendingInvitationsTableProps {
  invitations: OrgInvitationDTO[];
  onResend: (invitationId: string) => void;
  onRevoke: (invitationId: string) => void;
}
```

### §9.6.3 Workspace Visibility Components

#### WorkspaceVisibilitySettings

```typescript
interface WorkspaceVisibilitySettingsProps {
  workspaceId: string;
  visibility: 'org' | 'invite_only';
  isBase: boolean;                    // Base workspace is always 'org', non-editable
  onChangeVisibility: (v: 'org' | 'invite_only') => void;
}
```

#### WorkspaceMembersPanel

Only rendered when `visibility === 'invite_only'`:

```typescript
interface WorkspaceMembersPanelProps {
  workspaceId: string;
  members: WorkspaceMemberDTO[];
  orgMembers: OrgMemberDTO[];         // For the "add member" dropdown
  onAdd: (userId: string, role: string) => void;
  onChange: (userId: string, role: string) => void;
  onRemove: (userId: string) => void;
}

interface WorkspaceMemberDTO {
  userId: string;
  displayName: string;
  avatarUrl: string | null;
  role: 'admin' | 'editor' | 'viewer';
  invitedBy: UserRef;
  addedAt: string;
}
```

---

### §9.7 Search

#### SearchBar

```typescript
interface SearchBarProps {
  placeholder: string;
  value: string;
  onChange: (query: string) => void;
  /** Results fed from a debounced hook */
  results: SearchResult[] | null;  // null = not searching
  isSearching: boolean;
  onSelectResult: (result: SearchResult) => void;
  onClear: () => void;
}

interface SearchResult {
  type: 'node' | 'doc' | 'file' | 'comment';
  id: string;
  title: string;
  /** Snippet with match highlighting */
  snippet: string;
  /** Breadcrumb path to the result */
  breadcrumb: string;
  /** Relevance score */
  score: number;
}
```

---

### §9.8 Data Hooks (React Query)

Every component above is a pure presentation component. Data fetching lives in
hooks that map 1:1 to backend endpoints:

```typescript
// §3.1 Workspaces
useWorkspaces(orgId: string): AsyncState<WorkspaceSummary[]>
useWorkspace(workspaceId: string): AsyncState<WorkspaceDetail>
useWorkspaceStats(workspaceId: string): AsyncState<WorkspaceStats>
useWorkspaceDependencies(workspaceId: string): AsyncState<DepEdge[]>

// §3.2 Ontology
useOntologyRoots(branchId: string): AsyncState<OntologyNodeDTO[]>
useOntologyChildren(nodeId: string): AsyncState<OntologyNodeDTO[]>
useNodeContent(nodeId: string): AsyncState<string>
useNodeDiff(nodeId: string, baseBranch: string): AsyncState<DiffData | null>
useNodeDeps(nodeId: string): AsyncState<{ deps: NodeDependency[]; dependents: NodeDependency[] }>

// §3.3 Docs
useDocs(workspaceId: string): AsyncState<DocNode[]>

// §3.4 Jobs & Tasks
useWorkspaceJobs(workspaceId: string): AsyncState<JobSummary[]>
useJobTasks(jobId: string): AsyncState<TaskSummary[]>
useTask(taskId: string): AsyncState<TaskDetail>
useTaskTimeline(taskId: string, cursor?: string): AsyncState<Page<TimelineEntry>>
useTaskCheckpoints(taskId: string): AsyncState<TaskCheckpoint[]>

// §3.5 Comments
useNodeComments(workspaceId: string, nodeId: string): AsyncState<CommentDTO[]>
useLineComments(workspaceId: string, filePath: string, branch: string): AsyncState<CommentDTO[]>

// §3.6 Chat
useChatSessions(workspaceId: string, scopeType?: string): AsyncState<ChatSessionDTO[]>
useChatSession(sessionId: string): AsyncState<ChatSessionDTO>
useChatMessages(sessionId: string): AsyncState<ChatMessageDTO[]>

// §3.7 Drafts
useDraftSession(branchId: string): AsyncState<DraftSessionSummary | null>
useDraftParticipants(branchId: string): AsyncState<DraftParticipant[]>

// §3.9 Settings
useWorkspaceSettings(workspaceId: string): AsyncState<WorkspaceSettings>
useWorkspaceSpend(workspaceId: string): AsyncState<SpendBreakdownProps['spend']>

// §3.10 Search
useSearch(workspaceId: string, query: string, types?: string[], mode?: 'keyword' | 'semantic' | 'hybrid'): AsyncState<SearchResponse>

// §3.11 Auth & Users
useCurrentUser(): AsyncState<CurrentUserDTO>
useUser(userId: string): AsyncState<UserRef>

// §3.12 Org Management
useOrgs(): AsyncState<CurrentUserDTO['orgs']>
useOrgMembers(orgId: string): AsyncState<OrgMemberDTO[]>
useOrgInvitations(orgId: string): AsyncState<OrgInvitationDTO[]>

// §3.12.3 Workspace Visibility & Members
useWorkspaceMembers(workspaceId: string): AsyncState<WorkspaceMemberDTO[]>

// §3.13 Workers
useWorkspaceWorkers(workspaceId: string): AsyncState<WorkerSummaryDTO[]>

// §3.14 File Tree
useFileTree(branchId: string, path?: string, depth?: number): AsyncState<FileTreeEntry[]>
useFileContent(branchId: string, path: string): AsyncState<string>

// §3.15 Spend History
useSpendHistory(workspaceId: string, periods?: number): AsyncState<SpendHistoryDTO>

// Mutations (return { mutate, isLoading, error })
useCreateWorkspace(orgId: string): MutationHook<CreateWorkspaceInput, WorkspaceSummary>
usePatchWorkspace(workspaceId: string): MutationHook<Partial<WorkspaceSummary>, WorkspaceSummary>
useUpdateSettings(workspaceId: string): MutationHook<Partial<WorkspaceSettings>, WorkspaceSettings>
useCloneWorkspace(workspaceId: string): MutationHook<CloneWorkspaceInput, CloneWorkspaceResponse>
useCreateJob(workspaceId: string): MutationHook<CreateJobInput, JobSummary>
usePauseJob(): MutationHook<string, void>       // jobId
useResumeJob(): MutationHook<string, void>
useCancelJob(): MutationHook<string, void>
usePauseTask(): MutationHook<string, void>       // taskId
useResumeTask(): MutationHook<string, void>
useAbortTask(): MutationHook<string, void>
useInjectMessage(): MutationHook<{ taskId: string; content: string }, void>
useSendChatMessage(): MutationHook<{ sessionId: string; content: string }, ChatMessageDTO>
useStopStreaming(): MutationHook<{ sessionId: string; messageId: string }, void>
useForkSession(): MutationHook<{ sessionId: string; atIndex: number }, ChatSessionDTO>
useCommitDraft(): MutationHook<{ draftId: string; message: string }, void>
useDiscardDraft(): MutationHook<{ draftId: string }, void>
useCommitAll(): MutationHook<{ workspaceId: string; message: string }, void>
useAddComment(): MutationHook<CreateCommentInput, CommentDTO>
useReplyToComment(): MutationHook<{ commentId: string; content: string }, void>
useResolveComment(): MutationHook<string, void>
useInviteOrgMember(): MutationHook<{ orgId: string; email: string; role: string }, OrgMemberDTO>
useUpdateMemberRole(): MutationHook<{ orgId: string; userId: string; role: string }, void>
useRemoveMember(): MutationHook<{ orgId: string; userId: string }, void>
useCreateOrg(): MutationHook<{ name: string; slug?: string }, CreateOrgResponse>
useResendInvitation(): MutationHook<{ orgId: string; invitationId: string }, void>
useRevokeInvitation(): MutationHook<{ orgId: string; invitationId: string }, void>
useUpdateWorkspaceVisibility(): MutationHook<{ workspaceId: string; visibility: 'org' | 'invite_only' }, void>
useAddWorkspaceMember(): MutationHook<{ workspaceId: string; userId: string; role: string }, WorkspaceMemberDTO>
useUpdateWorkspaceMemberRole(): MutationHook<{ workspaceId: string; userId: string; role: string }, void>
useRemoveWorkspaceMember(): MutationHook<{ workspaceId: string; userId: string }, void>

// §3.16 Permissions / Scopes
useMyScopes(): AsyncState<string[]>
useOrgDefaultScopes(orgId: string): AsyncState<string[]>
useUserScopes(orgId: string, userId: string): AsyncState<string[]>
useCheckScope(scopes: string[], workspaceId?: string): AsyncState<{ allowed: boolean; missing: string[] }>
useSetUserScopes(): MutationHook<{ orgId: string; userId: string; scopes: string[] }, void>

// §3.17 Instance
useInstanceInfo(): AsyncState<InstanceInfoDTO>
useInstanceHealth(): AsyncState<HealthStatus>
useChatModels(): AsyncState<string[]>

// Task dialog (open/close task detail overlay)
useTaskDialog(): { openTask: (taskId: string) => void; closeTask: () => void; activeTaskId: string | null }

// WebSocket subscriptions
useWSChannel(channel: string): void  // auto-invalidates relevant queries

// Data-channel hooks (binary WS streams from worker)
useDataChannel(sessionId: string, channel: string): AsyncState<ArrayBuffer[]>
useTerminalStream(sessionId: string): { output: string[]; sendInput: (data: string) => void }
useBuildLog(taskId: string): AsyncState<string[]>  // streamed build output

// Port-forwarding hooks (see WORKER_INFRASTRUCTURE.md §1.4.2)
useForwardedPorts(workerId: string): AsyncState<ForwardedPort[]>
usePortForwardUrl(workerId: string, port: number): string  // returns proxy URL
useFileWatch(branchId: string): { changed: string[]; clear: () => void }
useGitStatus(branchId: string): AsyncState<{ ahead: number; behind: number; dirty: string[] }>
```

#### §9.8.1 Shared Types

```typescript
/** Wrapper for all paginated API responses */
interface PaginatedResponse<T> {
  data: T[];
  cursor: string | null;        // Opaque cursor for next page
  hasMore: boolean;
  total?: number;                // Only when cheaply computable
}

/** Uniform API error shape (matches backend §4B error catalog) */
interface ApiError {
  code: string;                  // Machine-readable (e.g., 'FORBIDDEN')
  message: string;               // Human-readable
  details?: Record<string, unknown>;
  requestId: string;
}

/** A port detected or manually forwarded on a worker (§1.4.2 of WORKER_INFRASTRUCTURE.md) */
interface ForwardedPort {
  port: number;
  pid: number;
  processName: string;
  label?: string;                 // Auto-detected (e.g., 'Vite dev server')
  proxyUrl: string;               // e.g., '/api/v1/port-forward/worker-abc/3000'
  status: 'detected' | 'forwarding' | 'closed';
}

/** Worker summary for sidebar status bar */
interface WorkerStatusBarProps {
  workers: Array<{
    id: string;
    status: 'idle' | 'executing' | 'provisioning';
    taskName?: string;
    memory: string;               // e.g., '2.1 GB / 4 GB'
    uptime: string;               // e.g., '12m'
  }>;
  activeCount: number;
  totalCount: number;
}
```

---

### §9.9 Component ↔ Hook Wiring Summary

| Component | Reads (hooks) | Writes (mutations) |
|-----------|--------------|-------------------|
| **App (auth wrapper)** | `useCurrentUser`, `useOrgs` | — |
| **WorkspaceSwitcher** | `useWorkspaces`, `useWorkspaceJobs` (all), `useWorkspaceDependencies`, `useOrgRepos` | `useCreateWorkspace`, `usePauseJob`, `useCancelJob`, `useCloneWorkspace` |
| **WorkspaceTable** | — (props from parent) | `onPauseWorkspace` → `usePauseJob` for all active jobs |
| **TaskQueue** | — (props from parent) | `onTaskClick` → navigation |
| **DependencyGraph** | — (props from parent) | `onSelectWorkspace` → navigation |
| **WorkspacePage** | `useWorkspace`, `useOntologyRoots`, `useWorkspaceJobs`, `useDocs`, `useWorkspaceWorkers` | — |
| **OntologyTree** | `useOntologyChildren` (on expand) | `onDownlift` → `useCreateJob`, `onUplift` → `useCreateJob` |
| **ContentView** | `useNodeContent`, `useNodeDiff`, `useNodeComments` | `useAddComment`, `useReplyToComment`, `useResolveComment` |
| **MonacoEditor** | — (props from parent) | `onEdit` → Yjs CRDT ops → WS |
| **BottomDrawer** | `useTask`, `useTaskTimeline`, WS `task:<id>` | `usePauseTask`, `useResumeTask`, `useAbortTask`, `useInjectMessage` |
| **JobsPanel** | — (props from parent) | `usePauseJob`, `useResumeJob`, `useCancelJob` |
| **DependencyList** | `useNodeDeps` | — |
| **Chat** | `useChatSession`, `useChatMessages`, WS `chat:<id>` | `useSendChatMessage`, `useStopStreaming`, `useForkSession` |
| **SettingsPage** | `useWorkspaceSettings`, `useWorkspaceSpend`, `useSpendHistory` | `useUpdateSettings` |
| **SearchBar** | `useSearch` (debounced) | — |
| **FileNavigation** | `useFileTree` | — |
| **CloneWorkspaceDialog** | — (props from parent) | `useCloneWorkspace` |
| **OrgSettings** | `useOrgMembers` | `useInviteOrgMember`, `useUpdateMemberRole`, `useRemoveMember` |
| **OrgMembersPage** | `useOrgMembers`, `useOrgInvitations` | `useInviteOrgMember`, `useUpdateMemberRole`, `useRemoveMember`, `useResendInvitation`, `useRevokeInvitation` |
| **CreateOrgDialog** | — | `useCreateOrg` |
| **OrgOnboardingWizard** | `useScmConnections`, `useScmRepos` | `useImportRepos`, `useInviteOrgMember` |
| **WorkspaceVisibilitySettings** | — (props) | `useUpdateWorkspaceVisibility` |
| **WorkspaceMembersPanel** | `useWorkspaceMembers`, `useOrgMembers` | `useAddWorkspaceMember`, `useUpdateWorkspaceMemberRole`, `useRemoveWorkspaceMember` |
| **TaskTimelineView** | `useTaskTimeline`, `useTaskDialog`, WS `task:<id>` | `useInjectMessage`, `usePauseTask`, `useResumeTask`, `useAbortTask` |
| **DraftEditor** | `useDraftSession`, `useDraftCRDT` (Yjs), `useDraftParticipants` | `useCommitDraft`, `useDiscardDraft` |
| **SessionAttachmentView** | `useChatSession`, `useChatMessages` (task session), WS `chat:<id>` | `useSendChatMessage`, `useStopStreaming` |
| **SCMSettings** | `useScmConnections`, `useScmRepos` | `useConnectScm`, `useDisconnectScm`, `useImportRepos` |
| **WorkerStatusBar** | `useWorkspaceWorkers`, `useForwardedPorts` | — |
| **PortForwardPanel** | `useForwardedPorts` | — (links open in browser) |

---

### §9.10 Intervention & Session Attachment UI

Per WORKER_INFRASTRUCTURE.md §10.9, users interact with running task agents
in two modes. Both share the underlying session API (`/api/v1/sessions/:id/*`).

#### 9.10.1 Mode 1 — Message Injection (Indirect)

The user types in the main project chat. The main chat agent decides to route
a message to a task agent using the `send_message` tool. The frontend
doesn't need special handling — it's a normal chat message. The Chat component
renders the assistant's `send_message` tool call as a status chip:

```
┌─────────────────────────────────────────────┐
│ Chat (workspace scope)                       │
│                                              │
│  You: "also handle timeout errors in T-456" │
│                                              │
│  Agent: I'll pass that to the task agent.    │
│  ┌────────────────────────────────────────┐  │
│  │ 📨 Sent to T-456 (SIMD Opt)           │  │
│  │    "also handle timeout errors"        │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  Agent: Done. The task agent will pick it   │
│  up on its next iteration.                   │
└─────────────────────────────────────────────┘
```

**Component impact:** Chat component (§9.6) needs a `ToolCallBubble` sub-
component that renders tool calls with structured payloads. For
`send_message`, it shows a link to the target task session.

#### 9.10.2 Mode 2 — Session Attachment (Direct)

User clicks "Attach" on a running task → browser switches to the task's
chat session. Now the user is directly in the agent's conversation loop.

```
┌─────────────────────────────────────────────────────────────────────┐
│  WorkspacePage                                                       │
│  ┌──────────┬──────────────────────────────────┬──────────────────┐ │
│  │ Sidebar  │  Center (editor)                  │  Right (chat)    │ │
│  │          │                                    │                  │ │
│  │ Ontology │  Document content                  │ ┌──────────────┐│ │
│  │ Jobs     │  (or task diff when attached)      │ │Session:T-456 ││ │
│  │          │                                    │ │ ● Live       ││ │
│  │          │                                    │ │──────────────││ │
│  │          │                                    │ │Agent: I'll   ││ │
│  │          │                                    │ │ examine the  ││ │
│  │          │                                    │ │ matrix code  ││ │
│  │          │                                    │ │              ││ │
│  │          │                                    │ │🔧 read_file  ││ │
│  │          │                                    │ │ src/matrix.rs││ │
│  │          │                                    │ │ (142ms)      ││ │
│  │          │                                    │ │              ││ │
│  │          │                                    │ │Agent: Found  ││ │
│  │          │                                    │ │ the naive... ││ │
│  │          │                                    │ │              ││ │
│  │          │                                    │ │[type here]   ││ │
│  │          │                                    │ └──────────────┘│ │
│  │          │ ┌──── Bottom Drawer ──────────┐   │ [Back to Chat]  │ │
│  │          │ │ Timeline: T-456 (streaming)  │   │ [Detach]        │ │
│  │          │ └──────────────────────────────┘   │                  │ │
│  └──────────┴──────────────────────────────────┴──────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

**Component: `SessionAttachmentView`**

```typescript
interface SessionAttachmentViewProps {
  sessionId: string;
  taskId: string;
  taskName: string;
  // Callbacks
  onDetach: () => void;                  // Return to project chat
  onInjectMessage: (content: string) => void;
}
```

When attached:
- Right sidebar switches from workspace chat → task session chat
- Messages stream in real-time via WS `chat:<sessionId>`
- Tool calls render with structured metadata (tool name, args, duration, result)
- User can type directly into the session (same as project chat input)
- "Back to Chat" returns to the workspace-scoped chat session
- Bottom drawer auto-opens and shows the task's timeline (§9.13)
- Center editor can optionally show the task's live diff

**Tool call rendering in chat:**

```typescript
interface ToolCallBubbleProps {
  toolName: string;
  args: Record<string, unknown>;
  status: 'running' | 'completed' | 'error';
  result?: string;                       // Truncated to 500 chars
  durationMs?: number;
  // Expandable: click to see full args/result
}
```

Rendered as a collapsible card within the message stream:

```
 🔧 read_file  src/matrix.rs                    142ms  ✓
 ▸ Show result (47 lines)

 🔧 write_file  src/matrix.rs                   28ms   ✓
 ▸ Show diff (+32 -8)

 🔧 run_in_terminal  cargo test                     3,421ms ✓
 ▸ Show output (12 lines)

 🔧 run_in_terminal  cargo bench                    8,102ms ✗
 ▸ Show error
```

---

### §9.11 Draft / CRDT Editor

The draft editor integrates Yjs for real-time collaborative editing. It
replaces the mock's static content display in the center panel.

#### 9.11.1 Integration Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     CRDT Editor Integration                          │
│                                                                      │
│  Browser                              Server (g5d editor role)      │
│  ┌──────────────────────┐             ┌──────────────────────┐     │
│  │ MonacoEditor         │             │ Yjs Authority        │     │
│  │  └─ Yjs binding      │◄──────────►│  └─ Yjs doc per      │     │
│  │     └─ y-monaco      │  y-websocket│     (project,branch) │     │
│  │        └─ YDoc       │  binary     │  └─ Persists to PG   │     │
│  │           └─ getText │  protocol   │     every 100ms       │     │
│  └──────────────────────┘             │     (debounced)       │     │
│                                        └──────────────────────┘     │
│  Awareness (cursors, presence):                                     │
│    Travels on the same Yjs WS channel.                             │
│    Rendered as colored cursor labels in Monaco.                     │
│                                                                      │
│  Connection:                                                         │
│    WS /api/v1/yjs/:workspaceId/:branch?token=<jwt>                  │
│    One WS per open branch. Multiple tabs share the same YDoc.      │
└─────────────────────────────────────────────────────────────────────┘
```

#### 9.11.2 Component API

```typescript
interface DraftEditorProps {
  workspaceId: string;
  branchId: string;
  filePath: string;
  language: string;                      // Monaco language ID
  readOnly: boolean;                     // true when no draft session active
  // Callbacks
  onDirtyChange: (isDirty: boolean) => void;
  onSave: () => void;                   // Cmd+S → no-op (CRDT auto-saves)
}
```

**Hooks:**

```typescript
// Manages Yjs connection lifecycle. Returns the shared YDoc + provider.
useDraftCRDT(workspaceId: string, branchId: string): {
  yDoc: Y.Doc | null;
  provider: WebsocketProvider | null;
  syncState: 'synced' | 'syncing' | 'desynced';  // CRDT sync status
  lastSyncAt: Date | null;               // Timestamp of last server ack
  participants: DraftParticipant[];      // From Yjs awareness
}

// REST metadata about the draft session.
useDraftSession(branchId: string): AsyncState<DraftSessionSummary | null>

// Commit / discard actions.
useCommitDraft(): MutationHook<{ draftId: string; message: string }, void>
useDiscardDraft(): MutationHook<{ draftId: string }, void>
```

#### 9.11.3 Draft Status Bar

Shown in the WorkspacePage bottom status area (below the bottom drawer):

```
┌─────────────────────────────────────────────────────────────────────────┐
│ ● Synced     │  feat/streaming @ abc1234  │  3 dirty files             │
│ ⚙ worker-a3f │                             │  ↓2 behind base            │
│ Participants: 🟢 Alice  🟡 Agent (T-456)  │  [Commit] [Discard]        │
└─────────────────────────────────────────────────────────────────────────┘
```

The **worker indicator** (`⚙ worker-a3f`) shows the short ID of the worker currently assigned to this task's branch. It serves as a diagnostic anchor — if something goes wrong, the user knows which worker to inspect. When no worker is assigned (task is `PENDING`, `BLOCKED` past grace period, or `COMPLETED`), the indicator shows `⚙ No worker`. Clicking the worker ID opens the worker detail panel (logs, resource usage).

**Sync indicator states:**

| Icon | Label | Meaning |
|------|-------|---------|
| 🟢 ● | **Synced** | All local changes acknowledged by server. CRDT state is durable. |
| 🟡 ◐ | **Syncing** | Outbound deltas in flight, not yet acknowledged. Normal during typing bursts. |
| 🔴 ● | **Desynced — 12s** | WebSocket disconnected or server unreachable. Counter shows wall-clock time since last successful round-trip. Increments every second. |

The desynced state shows elapsed time since the last confirmed sync (e.g., "Desynced — 3s", "Desynced — 2m 14s"). This gives the user a clear signal of how stale their view might be. On reconnect, Yjs resolves any divergence via its built-in CRDT merge, and the indicator returns to **Synced**.

Edits made while desynced are **not lost** — they accumulate in the local Yjs document and merge on reconnect. The indicator is informational: it tells the user whether their changes are durable on the server yet, not whether they are at risk of loss (Yjs handles that).

When `readOnly` (no draft session): the status bar shows "No active draft"
with a [Start Draft] button that calls `POST /api/v1/branches/:id/drafts`.

#### 9.11.4 Dirty Files & Revert

The draft editor needs to show *what changed* relative to the base commit and let users (or agents) revert individual files.

**Dirty file list:**

The left sidebar shows a **Changed Files** section when a draft has modifications. Files are grouped by change type:

```
Changed Files (5)
  ● Modified
    src/auth/validator.rs          [↻] [⇔]
    src/auth/claims.rs             [↻] [⇔]
  + Added
    src/auth/refresh.rs            [↻] [⇔]
  − Deleted
    src/auth/legacy_auth.rs        [↻]
```

- `[↻]` — **Revert**: Restore this file to its `baseCommit` version. For modified files, this replaces the CRDT content with the base version. For added files, this removes the file from the draft. For deleted files, this restores the file from the base commit.
- `[⇔]` — **Diff**: Open the side-by-side diff view for this file.

Revert is a CRDT operation — it writes the base commit content into the Yjs document, which propagates to all participants (including the worker). It does **not** shell out to `git checkout`. The agent can also revert files programmatically via a `revert_file` tool that performs the same CRDT write.

**Diff display:**

Monaco's built-in diff editor renders changes between the `baseCommit` version (left, read-only) and the current CRDT state (right, editable):

```
┌─────────────────────────────────────────────────────────────────┐
│ ◄ base (abc1234)              │  draft (live)               ►  │
├───────────────────────────────┼─────────────────────────────────┤
│  fn validate_token(           │  fn validate_token(             │
│    token: &str                │    token: &str,                 │
│                               │+   cache: &TokenCache,          │
│  ) -> Result<Claims> {        │  ) -> Result<Claims> {          │
│    verify_signature(token)?;  │    if let Some(c) = cache.get() │
│                               │+     return Ok(c.clone());      │
│                               │+   }                            │
│                               │+   verify_signature(token)?;    │
└───────────────────────────────┴─────────────────────────────────┘
```

The diff is **live** — because the right side is the Yjs document, edits by any participant (human or agent) update the diff view in real time. The left side is static (fetched once from the base commit via `GET /api/v1/branches/:id/files/:path?ref=<baseCommit>`).

**Inline gutter indicators** are also shown in the normal (non-diff) editor mode: green bars for additions, blue for modifications, red triangles for deletions — the same convention as VS Code's SCM gutter decorations. These are computed by diffing the base commit content against the current CRDT state client-side.

---

### §9.12 WebSocket Strategy

The browser uses a **single multiplexed WebSocket** per the API_AND_STORAGE.md
§4 design. The per-session `WS /api/v1/sessions/:id/stream` endpoint described in
WORKER_INFRASTRUCTURE.md §10.13 is an **internal API** used by workers
connecting to the editor process; it is NOT used by browser clients.

#### 9.12.1 Client Connection Model

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Browser WebSocket Strategy                         │
│                                                                      │
│  1. Main event channel (multiplexed):                               │
│     WS /api/v1/ws?token=<jwt>                                      │
│     → subscribe/unsubscribe to channels:                            │
│       workspace:<id>, job:<id>, task:<id>, chat:<sessionId>,         │
│       draft:<workspaceId>:<branch>                                   │
│     → receives all server-pushed events (§4.2 of API doc)          │
│                                                                      │
│  2. CRDT channel (per-branch, separate):                            │
│     WS /api/v1/yjs/:workspaceId/:branch?token=<jwt>                 │
│     → y-websocket binary protocol                                   │
│     → one connection per open branch (usually 1)                    │
│                                                                      │
│  Total WS connections for a typical session: 2                      │
│    (1 event channel + 1 CRDT channel)                               │
│                                                                      │
│  Reconnection: exponential backoff (1s, 2s, 4s, 8s, 16s max)      │
│  On reconnect: re-subscribe all active channels,                    │
│                 invalidate React Query caches for subscribed topics  │
└─────────────────────────────────────────────────────────────────────┘
```

#### 9.12.2 Hook

```typescript
// Singleton hook — one instance at App root.
function useWebSocket(): {
  connected: boolean;
  subscribe: (channel: string) => void;
  unsubscribe: (channel: string) => void;
}

// Per-component subscription hook — auto-subscribes on mount,
// unsubscribes on unmount. Invalidates relevant React Query keys
// when events arrive.
function useWSChannel(channel: string): void
```

**Event → Query Invalidation Mapping:**

| WS Event | React Query Key Invalidated |
|----------|----------------------------|
| `task.status` | `['task', taskId]`, `['jobs', workspaceId]` |
| `task.timeline` | `['task-timeline', taskId]` |
| `task.diff` | `['task-diff', taskId]` |
| `job.cost` | `['jobs', workspaceId]`, `['spend', workspaceId]` |
| `chat.chunk` / `chat.done` | `['chat-messages', sessionId]` |
| `draft.update` | `['draft-session', branchId]` |
| `draft.commit` | `['draft-session', branchId]`, `['ontology', workspaceId]` |
| `comment.new` | `['comments', nodeId]` |
| `workspace.settings_updated` | `['settings', workspaceId]` |
| `workspace.spend_limit_reached` | `['spend', workspaceId]` — also shows toast |
| `notification` | `['notifications', workspaceId]` — also shows toast/banner (§9.17) |

---

### §9.13 Monaco Editor Integration

Monaco (VS Code's editor core) is the primary code and content viewer. It handles syntax highlighting, diff viewing, and extensible decorations.

#### 9.13.1 Editor Wrapper

```typescript
/** Thin wrapper over monaco.editor that exposes a G5-specific API */
interface G5EditorConfig {
  container: HTMLElement;
  language?: string;
  readOnly?: boolean;
  theme?: 'g5-dark';          // Only dark theme for now
}

interface G5Editor {
  // Content
  setContent(content: string, language: string): void;
  getContent(): string;

  // Diff mode
  enableDiffMode(original: string): void;
  disableDiffMode(): void;

  // Decorations (coverage, errors, highlights)
  setDecorations(decorations: EditorDecoration[]): void;

  // View zones (inline comment threads)
  addViewZone(afterLine: number, domNode: HTMLElement, heightInLines: number): string;
  removeViewZone(zoneId: string): void;

  // Gutter markers (coverage, breakpoints)
  setGlyphMargins(markers: GlyphMarker[]): void;

  // Diagnostics (from build/test/lint output)
  setDiagnostics(uri: string, diagnostics: Diagnostic[]): void;

  // Events
  onCursorChange(callback: (line: number, col: number) => void): void;
  onContentChange(callback: (changes: TextChange[]) => void): void;
  onSave(callback: () => void): void;
}

interface EditorDecoration {
  startLine: number;
  endLine: number;
  className?: string;              // Line background
  glyphMarginClassName?: string;   // Gutter icon
  inlineClassName?: string;        // Text styling
  hoverMessage?: string;
}

interface GlyphMarker {
  line: number;
  className: string;   // 'coverage-hit' | 'coverage-miss' | 'breakpoint' | 'has-annotation'
}
```

#### 9.13.2 Open in IDE

G5 does **not** run LSP in the browser. Provide an **Open in IDE** action that
opens the local workspace path (the g5d checkout) so users get IntelliSense,
refactors, and diagnostics from their IDE's native language servers.

#### 9.13.3 Theme

```typescript
monaco.editor.defineTheme('g5-dark', {
  base: 'vs-dark',
  inherit: true,
  rules: [
    { token: 'comment',  foreground: '6b7280' },
    { token: 'keyword',  foreground: 'c084fc' },
    { token: 'string',   foreground: '86efac' },
    { token: 'number',   foreground: 'fcd34d' },
    { token: 'type',     foreground: '67e8f9' },
    { token: 'function', foreground: '93c5fd' },
  ],
  colors: {
    'editor.background':                '#0f0f0f',
    'editor.foreground':                '#e2e8f0',
    'editor.lineHighlightBackground':   '#1a1a2e',
    'editor.selectionBackground':       '#3b82f633',
    'editorGutter.background':          '#0a0a0a',
    'editorLineNumber.foreground':      '#4b5563',
    'editorLineNumber.activeForeground':'#9ca3af',
  },
});
```

#### 9.13.4 Decoration CSS

```css
.coverage-hit  { background-color: #22c55e; width: 4px !important; margin-left: 3px; }
.coverage-miss { background-color: #ef4444; width: 4px !important; margin-left: 3px; }
.has-annotation { background-color: #3b82f6; width: 4px !important; margin-left: 3px; cursor: pointer; }
```

#### 9.13.5 Extension Points

| Feature | Monaco API | Notes |
|---------|-----------|-------|
| Coverage (hit) | `deltaDecorations` + `.coverage-hit` gutter | Green gutter bar |
| Coverage (miss) | `deltaDecorations` + `.coverage-miss` gutter | Red gutter bar |
| Error squiggles | `setModelMarkers` | From build/test diagnostics |
| Inline comments | `changeViewZones` | Custom DOM node injected at line |
| Diff add/remove | Built-in diff editor | Green/red backgrounds |
| Search matches | `findMatchHighlightBackground` | Theme color |

---

### §9.14 Task Timeline View

The task timeline is the operational activity log shown in the bottom drawer
when a task is selected. It streams in real-time during active tasks and is
browsable as a historical artifact after completion.

#### 9.14.1 Layout (matches mock bottom drawer)

```
┌─────────────────────────────────────────────────────────────────────┐
│ Task Execution: T-456 SIMD Optimization         [⏸ Pause] [■ Stop]│
│ ─────────────────────────────────────────────────────────────────── │
│                                                                      │
│  14:32:01  💭  Examining existing matrix multiply implementation    │
│                                                                      │
│  14:32:03  🔧  read_file  src/matrix.rs                    142ms ✓ │
│                ▸ Show result                                        │
│                                                                      │
│  14:32:05  💭  Found naive O(n³) loop, will add AVX2 intrinsics   │
│                                                                      │
│  14:32:08  🔧  write_file  src/matrix.rs                    28ms ✓ │
│                +32 -8 lines                                         │
│                ▸ Show diff                                          │
│                                                                      │
│  14:32:12  🔧  run_in_terminal  cargo test                   3,421ms ✓ │
│                12 tests passed                                      │
│                ▸ Show output                                        │
│                                                                      │
│  14:32:42  📌  Checkpoint: abc1234                                  │
│                3 files changed (+48 -12)                            │
│                ▸ View commit                                        │
│                                                                      │
│  14:33:01  👤  User injection: "also handle timeout errors"         │
│                via Alice (web client)                               │
│                                                                      │
│  14:33:05  💭  Got it, I'll add timeout handling to the batch...   │
│                                                                      │
│  ─── streaming ─── ● ─── ─── ─── ─── ─── ─── ─── ─── ─── ─── ── │
│                                                                      │
│ Files changed: [src/matrix.rs] [src/batch.rs] [tests/bench.rs]     │
│ Cost: $0.12  │  Duration: 4m 32s  │  Session: S-789 [View Dialog]  │
└─────────────────────────────────────────────────────────────────────┘
```

#### 9.14.2 Component API

```typescript
interface TaskTimelineViewProps {
  taskId: string;
  taskName: string;
  taskStatus: 'pending' | 'running' | 'completed' | 'failed' | 'blocked';
  jobName: string;
  sessionId: string | null;              // Link to full dialog in chat
  // Controls (shown in header)
  onPause: () => void;
  onResume: () => void;
  onAbort: () => void;
  onAttach: () => void;                  // Switch to Mode 2 (§9.10.2)
  onClose: () => void;                   // Collapse bottom drawer
}
```

#### 9.14.3 Timeline Entry Rendering

Each `TimelineEntryDTO` (API §3.4.2) maps to a visual row:

```typescript
interface TimelineEntryRowProps {
  entry: TimelineEntryDTO;
  isStreaming: boolean;                  // true for the latest entry if task is active
}
```

**Rendering rules by `entryType`:**

| `entryType` | Icon | Background | Content |
|-------------|------|------------|---------|
| `thought` | 💭 | `bg-g5-bg2` | `content` as-is |
| `tool_call` | 🔧 | `bg-g5-info/10` | `metadata.toolName` + truncated args + duration badge |
| `tool_result` | — | (merged with tool_call row) | Expandable: full result |
| `output` | 📤 | `bg-g5-ok/10` | `content` (stdout) or `bg-g5-warn/10` (stderr) |
| `error` | ❌ | `bg-g5-blocked/10` | `content` + `metadata.errorCode` badge |
| `user_message` | 👤 | `bg-g5-active/10` | `content` + source badge (injection / attachment) |
| `agent_message` | 🤖 | `bg-g5-bg2` | `content` |
| `checkpoint` | 📌 | `bg-g5-ok/5` | Commit SHA link + files changed + additions/deletions |

**`tool_call` + `tool_result` are paired:** When a `tool_result` immediately
follows a `tool_call` with the same `toolName`, they are rendered as a single
collapsible row. The call row shows the tool name + args summary; the expand
shows the full result. This matches the mock's compact rendering.

#### 9.14.4 Data Flow

```typescript
// Paginated historical entries (loaded on mount + scroll-up)
useTaskTimeline(taskId: string, cursor?: string): AsyncState<{
  entries: TimelineEntryDTO[];
  cursor: string | null;
  hasMore: boolean;
}>

// Real-time streaming (active tasks only)
// useWSChannel('task:<taskId>') auto-appends new TimelineEntryDTO
// to the React Query cache for ['task-timeline', taskId]

// Full dialog (linked from "View Dialog" button)
useTaskDialog(taskId: string): AsyncState<ChatSessionDTO>
// → Opens the task's chat_sessions row (session_type='task')
// → ChatMessages are the full LLM conversation
```

**Footer stats** (`Cost`, `Duration`, `Files changed`) come from the
`TaskDTO` fetched by `useTask(taskId)`. The "View Dialog" link navigates
to `/w/:workspaceId/session/:sessionId` which opens the full
conversational view (all chat_messages, not the summarized timeline).

---

### §9.15 SCM Connection Settings

Manages GitHub/GitLab/Bitbucket connections (API §3.11.2).

```typescript
interface SCMSettingsProps {
  connections: SCMConnectionDTO[];
  onConnect: (provider: string) => void;   // → GET /api/v1/scm/:provider/connect
  onDisconnect: (provider: string) => void;
  onImport: (provider: string, repos: ImportReposRequest) => void;
}

// Hooks
useScmConnections(): AsyncState<SCMConnectionDTO[]>
useScmRepos(provider: string): AsyncState<SCMRepoListResponse>
useConnectScm(): MutationHook<{ provider: string }, void>  // initiates OAuth redirect
useDisconnectScm(): MutationHook<{ provider: string }, void>
useImportRepos(): MutationHook<ImportReposRequest, { imported: RepoDTO[] }>
```

---

### §9.16 Auth Flow

All pages require authentication. The frontend uses a top-level `AuthProvider`
context that manages the JWT lifecycle.

```typescript
interface AuthContextValue {
  user: CurrentUserDTO | null;
  isLoading: boolean;
  scopes: string[];              // Effective scopes for current org (from JWT)
  login: (provider: 'github' | 'gitlab' | 'google') => void;  // redirect
  logout: () => void;
  switchOrg: (orgId: string) => void;    // re-issues JWT with new orgId + scopes
  hasScope: (scope: string) => boolean;  // Fast client-side check
  hasAllScopes: (scopes: string[]) => boolean;
}

// Route guard — wraps all authenticated routes
function RequireAuth({ children, requiredScope? }: {
  children: ReactNode;
  requiredScope?: string;        // Optional: redirect to /forbidden if missing
}): ReactElement {
  // If user is null and not loading → redirect to /login
  // If user is present but missing requiredScope → redirect to /forbidden
  // If user is present → render children
}
```

**Login page** lives at `/login` (outside the `RequireAuth` wrapper).
Shows provider buttons (GitHub, GitLab, Google). After OAuth callback,
the server sets the httpOnly refresh cookie and returns a JWT. The client
stores the JWT in memory (`useRef`) and sets up a refresh interval
(every 50 minutes, before the 1-hour expiry).

### §9.17 Notifications

Operational events (blocked agents, build/test failures, budget warnings) are surfaced through a unified notification system. The notification infrastructure is defined in WORKER_INFRASTRUCTURE.md §13.7; this section covers the frontend rendering.

**Notification rendering tiers:**

| Severity | Rendering | Behavior |
|----------|-----------|----------|
| `info` | Toast (bottom-right) | Auto-dismiss after 5s. No sound. |
| `warning` | Toast (bottom-right) + status bar indicator | Sticky until dismissed or resolved. Amber dot in status bar. |
| `error` | Banner (top of workspace) + toast + status bar | Sticky. Red dot in status bar. Banner persists until action taken or dismissed. |

**Components:**

```typescript
// Toast stack — renders in a portal, bottom-right corner
interface NotificationToastStackProps {
  notifications: G5Notification[];  // From useWorkspaceEvents hook
  onDismiss: (id: string) => void;
  onAction: (id: string, action: NotificationAction) => void;
}

// Banner — renders at top of WorkspacePage, below toolbar
interface NotificationBannerProps {
  notification: G5Notification;     // Highest-severity active notification
  onDismiss: (id: string) => void;
  onAction: (id: string, action: NotificationAction) => void;
}

// Status bar indicator — renders in the bottom status bar
interface NotificationStatusProps {
  activeCount: number;              // Number of unresolved warning/error notifications
  highestSeverity: 'info' | 'warning' | 'error' | null;
  onClick: () => void;              // Opens notification panel
}

// Notification panel — slide-out panel showing all recent notifications
interface NotificationPanelProps {
  notifications: G5Notification[];  // All notifications (including auto-dismissed)
  onAction: (id: string, action: NotificationAction) => void;
  onClearAll: () => void;
}
```

**State management:**

Notifications are managed via a `useNotifications` hook that subscribes to the `notification` WS event type (API_AND_STORAGE.md §4.2) via `useWorkspaceEvents` (§9.12). The server emits `{ type: "notification", ... }` events for operational alerts (task blocked, build/test failure, spend warnings, etc.). The hook maintains an in-memory list of recent notifications (last 50), tracks dismissed state per notification ID, and exposes the active (non-dismissed, non-expired) set for rendering.

```typescript
function useNotifications(workspaceId: string) {
  // Subscribes to useWorkspaceEvents 'notification' channel
  // Returns:
  //   notifications: G5Notification[]     — all recent (last 50)
  //   active: G5Notification[]            — non-dismissed, non-expired
  //   dismiss: (id: string) => void
  //   clearAll: () => void
  //   highestSeverity: 'info' | 'warning' | 'error' | null
}
```

**Interaction patterns:**

- **Blocked agent toast**: Shows task name, blocking reason (truncated), and [View Task] / [Send Message] buttons. Clicking [Send Message] navigates to the task's session with the chat input focused (§9.10).
- **Budget warning toast**: Shows percentage used and which limit is approaching. No action buttons — informational only.
- **Client install banner**: On first load in a browser-only session (no desktop client detected), a dismissible info banner prompts: *"Install the G5 desktop client for local code analysis, offline editing, and faster builds."* [Download] links to the platform-appropriate installer. Dismissed state is stored in `localStorage` and re-shown at most once per 30 days. The desktop client is detected via a `g5-client-version` header on WebSocket connections or the absence of a `g5://` protocol handler check.

### §9.18 Billing Report

The billing report provides per-workspace and per-org cost visibility.
It lives at `/w/:workspaceId/settings` (the "Spend" tab) and
uses data from `GET /api/v1/workspaces/:workspaceId/settings/spend/history`
(API_AND_STORAGE.md §3.15) and the per-request usage breakdown from
`GET /api/v1/workspaces/:workspaceId/settings/spend/usage`
(API_AND_STORAGE.md §3.15.1).

#### BillingReportPage

```typescript
interface BillingReportProps {
  workspaceId: string;
}

// Internal data hook: useSpendHistory(workspaceId, { periods, granularity })
// Internal data hook: useUsageBreakdown(workspaceId, { from, to, groupBy })
```

**Layout:** Two-section page: summary cards at top, then tabbed detail views.

**Summary cards row:**
- **Current Period Spend** — `$X.XX / $Y.YY` with progress bar (green < 80%, amber 80–95%, red > 95%).
- **Total Tokens** — input + output token count for the current period.
- **Active Jobs** — count of running/pending jobs.
- **Cost Trend** — sparkline of daily spend for the last 30 days.

**Detail tabs:**

1. **By Day** — Bar chart of daily spend (from `SpendHistoryDTO.daily`). X-axis = date, Y-axis = USD. Stacked by provider. Hover shows breakdown.

2. **By Model** — Grouped bar chart showing cost per model (from `usage_records` grouped by `model`). Useful for identifying expensive model choices.

3. **By Job** — Table: `| Job Name | Type | Tasks | Tokens (in/out) | Cost | Status |`. Sortable columns. Clicking a row navigates to the job's task timeline.

4. **By Provider** — Pie chart showing cost distribution across providers (`openai`, `anthropic`, `openrouter`, etc.). Includes cache savings callout for Anthropic.

#### UsageBreakdownTable

```typescript
interface UsageBreakdownTableProps {
  rows: UsageBreakdownRow[];
  sortBy: string;
  sortDir: 'asc' | 'desc';
  onSort: (column: string) => void;
}

interface UsageBreakdownRow {
  label: string;           // model name, job name, or date
  provider: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  costUsd: number;
  requestCount: number;
}
```

#### CostTrendChart

```typescript
interface CostTrendChartProps {
  /** Daily spend data points */
  data: Array<{ date: string; costUsd: number; provider?: string }>;
  /** Budget limit line (dashed horizontal) */
  limitUsd: number | null;
  /** Chart height in pixels */
  height?: number;
}
```

**Data hooks:**

```typescript
// Spend history (period-level aggregates)
useSpendHistory(workspaceId: string, opts?: {
  periods?: number;          // default 6
  granularity?: 'hourly' | 'daily' | 'weekly' | 'monthly';
}): AsyncState<SpendHistoryDTO>

// Fine-grained usage breakdown (from usage_records)
useUsageBreakdown(workspaceId: string, opts: {
  from: string;              // ISO 8601 start
  to: string;                // ISO 8601 end
  groupBy: 'model' | 'provider' | 'job' | 'day';
}): AsyncState<UsageBreakdownRow[]>
```

---

### §9.19 Clone Components

#### CloneWorkspaceDialog

Modal dialog for cloning a workspace. Opened from the workspace table's clone
action (⧉) or from the WorkspacePage toolbar menu.

```typescript
interface CloneWorkspaceDialogProps {
  /** Source workspace to clone */
  source: WorkspaceSummary;
  /** Called after successful clone — navigates to the new workspace */
  onCloned: (newWorkspaceId: string) => void;
  onClose: () => void;
}
```

**Layout:**
- Title: "Clone `{source.name}`"
- Name input (pre-filled: `"{source.name} (copy)"`)
- Description textarea (pre-filled from source)
- Per-repo branch name overrides (expandable section, one row per branch):
  each row shows `repoName` + text input (pre-filled with auto-generated name).
- Dirty state warning (if source has any dirty drafts):
  yellow info box: "This workspace has uncommitted changes in {N} repo(s).
  The clone will include those changes."
- Clone button (primary), Cancel button.
- On submit: calls `useCloneWorkspace` mutation → on success → `onCloned(newId)`.

**Data hooks:**

```typescript
// Clone
useCloneWorkspace(workspaceId: string): MutationHook<CloneWorkspaceInput, CloneWorkspaceResponse>

interface CloneWorkspaceInput {
  name: string;
  description?: string;
  branchOverrides?: Record<string, string>;
}

interface CloneWorkspaceResponse {
  workspace: WorkspaceSummary;
  clonedBranches: number;
  clonedDrafts: number;
  clonedOntologyNodes: number;
}

// List repos in org
useOrgRepos(orgId: string): AsyncState<RepoSummary[]>
```

---

## §10 Open Design Questions

| # | Question | Status |
|---|----------|--------|
| Q1 | Should the ontology tree be fully materialized or lazy-loaded per-expand? | **Open.** V2 mock fully materializes. Backend supports both. Lazy loading needed for 1000+ node trees. |
| Q2 | Should diff overlay use the backend diff API or client-side Yjs diff? | **Open.** V2 has `DiffView` component with mock data. Backend git diff and Yjs draft diff may both be needed. |
| Q3 | ~~How should multi-repo workspaces display in the ontology tree?~~ | **Resolved.** Repos bar shows all repos in workspace. Ontology tree is per-workspace per-repo with repo-level headers. |
| Q4 | ~~Should WorkspacePage replace or coexist with ExecutionPage?~~ | **Resolved.** WorkspacePageV2 fully replaces both OntologyPage and ExecutionPage. Execution monitoring lives in the resizable bottom drawer (§3.4.7) with 5 tabs (DAG, Changes, Timeline, Terminal, Intervene). The drawer can expand to 600px for deep debugging. |
| Q5 | ~~What is the Monaco integration plan?~~ | **Resolved (architecture).** V2 uses a custom code viewer for now. Monaco integration is planned as the next major step — dual-mode SpecEditor (rich + Monaco source), full Monaco for code files, both with Yjs binding. |
| Q6 | ~~How should chat scoping work in the unified editor?~~ | **Resolved.** Chat is project-level with session management. The `ChatPanel` supports multiple sessions with a session picker, search, and session switching. Node-scoped chat can be implemented as a session creation option. |
| Q7 | ~~How should worker status be visualized?~~ | **Resolved.** `ConnectedWorkerPill` in top bar shows the workspace's bound worker. Workers dropdown shows all workers with CPU/mem/status. Worker info banner in the Intervene tab shows detailed stats. |
| Q8 | Should the TaskQueue in WorkspaceSwitcher show cross-workspace stats or per-org? | **Open.** V2 shows workspace-scoped jobs in sidebar. Cross-workspace view TBD. |

---

## §11 Document Editor Architecture

Specs and docs use a dual-mode editor: **rich (WYSIWYG)** for Notion-like
editing and **source** for raw Markdown. Both modes bind to the same Yjs
CRDT document. The canonical serialization is the Extended Markdown dialect
defined in DESIGN.md §2.5.

### §11.1 Design Goals

| Goal | Mechanism |
|------|-----------|
| Notion-like editing for specs and docs | Tiptap / ProseMirror with custom node views |
| Markdown stays clean and diffable | Strict serialization rules; `serialize(parse(md))` is idempotent |
| Rich features without inventing syntax | Directive pattern (`:::type{attrs}...:::`) for everything beyond GFM |
| Collaborative editing | Both modes share one Yjs document; agents and humans edit the same doc |
| Source mode for power users | Monaco with Markdown syntax highlighting; toggle in toolbar |
| Comments feel like Google Docs | Anchored threads rendered as sidebar UI in rich mode, visible as directives in source mode |

### §11.2 Markdown Dialect

The canonical format is **GFM + YAML frontmatter**, extended with:

- **Math:** `$...$` inline, `$$...$$` block (KaTeX).
- **Diagrams:** Fenced code blocks with language hints: ` ```mermaid `,
  ` ```dot `, ` ```plantuml `.
- **Mentions:** `@{user:james}` — enforced syntax, resolved to user IDs.
- **Directives:** `:::type{key="value"}...:::` for callouts, collapsible
  sections, figures, comments, and custom blocks.
- **Comment anchors:** `^cmt:<id>` inline markers paired with
  `:::comment{anchor="cmt:<id>"}` thread blocks.

**Formatting rules (normalized on save):**

| Rule | Choice |
|------|---------|
| Headings | ATX only (`# Heading`) |
| Code blocks | Fenced only (no 4-space indent) |
| Tables | GFM pipe tables only (no HTML) |
| List indent | 2 spaces |
| Emphasis | `*italic*`, `**bold**` (no underscores) |
| Blank lines | Exactly one between blocks |
| Trailing newline | Always present |

These rules ensure `serialize(parse(md))` produces identical output,
preventing save churn and keeping git diffs clean.

### §11.3 Internal Document Schema

The Tiptap/ProseMirror schema defines the node types that the rich editor
understands. Each node maps bidirectionally to a Markdown construct.

```typescript
// Core nodes (standard Markdown)
type DocNode = 'doc';                    // Root
type BlockNode =
  | 'paragraph'
  | 'heading'          // attrs: { level: 1-6 }
  | 'codeBlock'        // attrs: { language: string }
  | 'blockquote'
  | 'bulletList'
  | 'orderedList'
  | 'listItem'
  | 'taskList'
  | 'taskItem'         // attrs: { checked: boolean }
  | 'table'
  | 'tableRow'
  | 'tableCell'        // attrs: { header: boolean }
  | 'horizontalRule'
  | 'image'            // attrs: { src, alt, title }
  | 'hardBreak';

// Extended nodes (directive-backed)
type ExtendedNode =
  | 'mathInline'       // attrs: { tex: string }
  | 'mathBlock'        // attrs: { tex: string }
  | 'diagram'          // attrs: { language: 'mermaid'|'dot'|'plantuml', code: string }
  | 'callout'          // attrs: { type: 'note'|'warning'|'tip'|'danger' }
  | 'collapsible'      // attrs: { summary: string }
  | 'figure'           // attrs: { src, alt, width?, align? }
  | 'commentAnchor'    // attrs: { anchorId: string } — invisible inline mark
  | 'commentThread'    // attrs: { anchorId, threadId, entries: CommentEntry[] }
  | 'mention';         // attrs: { kind: 'user', id: string, label: string }

// Marks (inline formatting)
type Mark =
  | 'bold'
  | 'italic'
  | 'strike'
  | 'code'
  | 'link'             // attrs: { href, title? }
  | 'commentHighlight'; // attrs: { anchorId } — highlight span linked to a comment
```

### §11.4 Parse / Serialize Pipeline

```
Markdown text
    │
    ▼
┌──────────────────────────────────────────┐
│  Parser (markdown-it + custom plugins)    │
│                                          │
│  GFM tables, task lists, strikethrough   │
│  + math plugin ($, $$)                   │
│  + directive plugin (:::type{attrs})     │
│  + mention plugin (@{user:...})          │
│  + comment anchor plugin (^cmt:...)      │
│                                          │
│  Output: markdown-it token stream        │
└────────────────┬─────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────┐
│  Token → ProseMirror transformer         │
│                                          │
│  Maps each token to the schema node/mark │
│  defined in §11.3.                       │
│                                          │
│  Output: ProseMirror document (JSON)     │
└────────────────┬─────────────────────────┘
                 │
                 ▼
        Tiptap editor state
                 │
                 ▼
┌──────────────────────────────────────────┐
│  Serializer (ProseMirror → Markdown)     │
│                                          │
│  Walks the ProseMirror doc tree.         │
│  Each node type has a serialization rule │
│  that emits canonical Markdown.          │
│                                          │
│  Applies formatting normalization:       │
│  ATX headings, fenced code, 2-space      │
│  indent, single blank lines, etc.        │
│                                          │
│  Output: canonical Markdown string       │
└──────────────────────────────────────────┘
```

**Parser library:** `markdown-it` with plugins (`markdown-it-math`,
`markdown-it-container` for directives, plus custom plugins for mentions
and comment anchors). The parser is **read-only** in rich mode — it runs
only when switching from source to rich mode or when loading a file.

**Serializer:** Custom ProseMirror serializer that enforces the formatting
rules in §11.2. This is the **only code path** that produces Markdown text
from the rich editor. A single serializer ensures deterministic output.

**Idempotency test:** CI enforces `serialize(parse(md)) === md` for a
corpus of example files. Any formatting rule change must update both the
serializer and the corpus.

### §11.5 Mode Switching

The toolbar has a toggle: **Rich** / **Source**. Switching modes runs a
synchronous transform:

```
Rich → Source:  serialize(tiptapDoc) → monacoModel.setValue(md)
Source → Rich:  parse(monacoModel.getValue()) → tiptapEditor.setContent(doc)
```

**Conflict handling:** Both modes bind to the same Yjs document, but
through different bindings:

| Mode | Binding | CRDT integration |
|------|---------|------------------|
| Rich | `y-prosemirror` (Yjs ↔ ProseMirror) | Structural: Yjs XML fragment maps to ProseMirror nodes. Real-time collaborative cursors via Yjs awareness. |
| Source | `y-monaco` (Yjs ↔ Monaco) | Text: Yjs Text type maps to Monaco text buffer. Character-level CRDT. |

**Single-source-of-truth rule:** At any given moment, exactly one mode
is active and authoritative. Switching modes:

1. The outgoing mode serializes its state.
2. The Yjs document is updated with the serialized form.
3. The incoming mode reads from the Yjs document.

This prevents divergence. The Yjs document always holds the latest state
in whichever representation the active mode uses.

**Agent edits:** Agents write raw Markdown via the mutation tools
(`write_file`, `edit_file_replace`). These edits enter the CRDT as text
deltas. If the user is in rich mode when an agent edit arrives, the
Tiptap binding re-parses the affected region and updates the ProseMirror
doc. If in source mode, the Monaco binding applies the delta directly.

### §11.6 Comment System Integration

Comments in the rich editor work like Google Docs:

1. **Creating a comment:** User selects text → clicks "Comment." The
   editor inserts a `commentAnchor` node (invisible `^cmt:<newId>`) at
   the selection end and opens a sidebar thread.

2. **Rendering:** The `commentHighlight` mark highlights the anchored
   range. The `CommentThreadPanel` renders alongside the content,
   positioned relative to the anchor.

3. **Serialization:** On save, the anchor becomes `^cmt:8f2a` and the
   thread becomes a `:::comment{anchor="cmt:8f2a" id="th_123"}` block
   at the end of the document (or after the relevant section).

4. **Source mode:** Both the anchor and directive are visible and
   editable as plain text.

5. **DB indexing:** The `comments` table in Postgres (API_AND_STORAGE.md
   §2.6) indexes comment threads for cross-branch visibility. The
   Markdown directive is the source of truth; the DB enables queries
   like "all open comments in this workspace."

### §11.7 Component Interfaces

#### DocumentEditor (dual-mode wrapper)

```typescript
interface DocumentEditorProps {
  /** File path for CRDT binding (.g5/specs/... or .g5/docs/...) */
  filePath: string;
  /** Yjs document (shared with other collaborators) */
  yjsDoc: Y.Doc;
  /** Initial mode */
  defaultMode: 'rich' | 'source';
  /** Whether editing is enabled (false for read-only/diff views) */
  editable: boolean;
  /** Comment thread data (from DB for cross-branch visibility) */
  comments: CommentDTO[];
  onAddComment: (params: { anchorId: string; content: string }) => void;
  onResolveComment: (threadId: string) => void;
  onReplyToComment: (threadId: string, content: string) => void;
  /** Diff overlay */
  showDiff: boolean;
  diffData: DiffData | null;
}
```

#### TiptapEditor (rich mode)

```typescript
interface TiptapEditorProps {
  /** Yjs XML fragment for ProseMirror binding */
  yjsFragment: Y.XmlFragment;
  /** Awareness for collaborative cursors */
  awareness: awarenessProtocol.Awareness;
  editable: boolean;
  /** Custom node views for extended blocks */
  extensions: TiptapExtension[];
  /** Comment anchor click handler */
  onCommentAnchorClick?: (anchorId: string) => void;
}
```

#### Diagram node views

Diagram blocks (mermaid, dot, plantuml) render as interactive node views
in rich mode:

```typescript
interface DiagramNodeViewProps {
  language: 'mermaid' | 'dot' | 'plantuml';
  code: string;
  /** Toggle between rendered SVG and editable code */
  editMode: boolean;
  onCodeChange: (newCode: string) => void;
}
```

In read mode, the diagram renders as an SVG. Clicking it enters edit mode
(inline code editor). Changes re-render the SVG on debounce.

### §11.8 File Type Routing

The `ContentView` routes files to the appropriate editor:

```typescript
function getEditorForFile(filePath: string): EditorType {
  if (filePath.startsWith('.g5/specs/') || filePath.startsWith('.g5/docs/'))
    return 'document';     // → DocumentEditor (dual-mode: rich + source)
  if (isCodeFile(filePath))
    return 'code';         // → MonacoEditor (code mode)
  if (isImageFile(filePath))
    return 'image';        // → ImageViewer
  return 'code';           // Fallback to Monaco
}
```

Specs and docs always open in `DocumentEditor` (rich mode by default).
Code files always open in `MonacoEditor`. The mode toggle is only
available for document files.

### §11.9 Extension Points

The directive pattern makes the editor extensible without changing the
parser or serializer core:

| Directive | Renders As | MVP? |
|-----------|-----------|------|
| `:::note`, `:::warning`, `:::tip`, `:::danger` | Colored callout box | ✓ |
| `:::details{summary="..."}` | Collapsible section | ✓ |
| `:::comment{anchor="..."}` | Sidebar comment thread | ✓ |
| `:::figure{src="..." width="..."}` | Sized/captioned image | ✓ |
| `:::todo{owner="james" due="2026-03-01"}` | Todo item with assignment | Phase 2 |
| `:::api-schema{method="POST" path="/users"}` | API schema block | Phase 2 |
| `:::experiment{id="exp_42" status="running"}` | Experiment result card | Phase 2 |
| `:::citation{key="smith2025"}` | Footnote/citation | Phase 2 |

Adding a new directive requires: (1) a `markdown-it-container` rule for
parsing, (2) a ProseMirror node type + node view for rendering, (3) a
serialization rule. No changes to the core pipeline.

### §11.10 Open Questions (Editor)

| # | Question | Context |
|---|----------|---------|
| E1 | Should the CRDT bridge use Yjs `XmlFragment` (structural) in both modes, or `Text` in source mode? | `XmlFragment` is better for structural edits but harder to map to Monaco. `Text` is natural for Monaco but lossy for rich mode sync. Current plan: mode switch triggers full re-sync. |
| E2 | Should comment directives be stored inline (after their paragraph) or collected at document end? | Inline is more natural for authoring; end-collected is cleaner for diffs. |
| E3 | How should conflicting edits to the same comment thread resolve? | Yjs CRDT handles text conflicts. Thread-level: last-writer-wins on resolve/reopen. |
| E4 | Should diagram rendering be server-side (for plantuml) or all client-side? | Mermaid is client-side. PlantUML traditionally needs a server. Options: WASM port, or g5d endpoint. |
