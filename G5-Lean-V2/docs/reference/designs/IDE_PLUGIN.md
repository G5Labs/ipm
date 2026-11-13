# G5 IDE Plugin — Design & Tradeoffs

> **Extension-native integration of G5 into VS Code (and compatible editors).**
> Cross-references: AGENTS.md (agent architecture), WORKER_INFRASTRUCTURE.md (session model), FRONTEND_DESIGN.md (web UI).

---

## 0. Motivation

The web UI (WorkspacePageV2) provides a self-contained environment for ontology management, agent observation, and spec editing. However, the core developer workflow lives in the IDE — reading code, navigating call hierarchies, running tests, committing changes. An IDE-native plugin meets developers where they already work, eliminating context-switching between the G5 web app and their editor.

**Key insight:** Many G5 concepts map directly to VS Code primitives — tree views become sidebar panels, specs become virtual documents, agent sessions become integrated terminals, and the task DAG becomes a custom view. The question is not *whether* to build a plugin, but *what to keep native vs. what to delegate to webviews*.

---

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        VS Code Host                          │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────┐ │
│  │  G5 Sidebar  │  │   Editors    │  │   Bottom Panel     │ │
│  │  (TreeView)  │  │  (TextDoc +  │  │  (Agent Chat +     │ │
│  │              │  │   Webview)   │  │   Task Terminal)   │ │
│  │ • Ontology   │  │              │  │                    │ │
│  │ • Tasks      │  │ • Code files │  │ • ReAct trace      │ │
│  │ • Agents     │  │ • Spec view  │  │ • Build output     │ │
│  │ • Workers    │  │ • Diff view  │  │ • Agent commands    │ │
│  └──────┬───────┘  └──────┬───────┘  └─────────┬──────────┘ │
│         │                 │                     │            │
│  ┌──────┴─────────────────┴─────────────────────┴──────────┐ │
│  │                  G5 Extension Host                       │ │
│  │  • WebSocket client → g5-api session stream              │ │
│  │  • REST client → Jobs/Tasks/Specs API                    │ │
│  │  • Virtual filesystem provider (g5-spec://)              │ │
│  │  • CodeLens provider (spec ↔ code annotations)           │ │
│  │  • SCM provider (task branch awareness)                  │ │
│  │  • Language server client (spec validation)              │ │
│  └──────────────────────┬──────────────────────────────────┘ │
└─────────────────────────┼────────────────────────────────────┘
                          │ HTTPS + WSS
                          ▼
                   ┌──────────────┐
                   │   g5-api     │
                   │  (backend)   │
                   └──────────────┘
```

### 1.1 Extension Components

| Component | VS Code API | Purpose |
|-----------|-------------|---------|
| **Ontology TreeView** | `TreeDataProvider` | Browse spec hierarchy, source files, staleness indicators |
| **Task TreeView** | `TreeDataProvider` | Active jobs, task states, progress bars |
| **Spec Document Provider** | `TextDocumentContentProvider` / `CustomEditor` | Open `.g5/specs/*.md` with rich rendering |
| **CodeLens Provider** | `CodeLensProvider` | Annotate source files with spec status, uplift/downlift actions |
| **Agent Chat Panel** | `WebviewViewProvider` | Chat with agents, observe ReAct traces |
| **DAG Webview** | `WebviewPanel` | Task dependency graph visualization |
| **SCM Integration** | `SourceControl` | Task branch awareness, dirty draft indicators |
| **Status Bar** | `StatusBarItem` | Connection status, active agent, cost accumulator |
| **Diagnostics** | `DiagnosticCollection` | Spec validation errors, ontology inconsistencies |
| **Commands** | `commands.registerCommand` | Uplift, downlift, review, attach to agent, etc. |

---

## 2. Feature Mapping: Web UI → IDE

### 2.1 Native VS Code Primitives (No Webview Needed)

These features map cleanly to VS Code's built-in API and feel native:

| Web UI Feature | VS Code Mapping | Notes |
|----------------|-----------------|-------|
| **Ontology sidebar tree** | `TreeDataProvider` with `TreeItem` | Supports expand/collapse, icons, context menus, drag-and-drop. Staleness shown via `TreeItem.iconPath` (colored dot overlay). |
| **Spec file editing** | Standard markdown editor + `TextDocumentContentProvider` | VS Code's markdown preview + custom frontmatter decorations via `DecorationProvider`. |
| **Source code viewing** | Native editor | Already the user's primary workflow. G5 adds CodeLens overlays. |
| **Terminal / build output** | Integrated terminal | Agent's `run_in_terminal` maps to `vscode.window.createTerminal()`. |
| **Git branch display** | SCM API + `SourceControl` | Task branches shown in source control panel. |
| **Search** | `vscode.commands.executeCommand('workbench.action.quickOpen')` | ⌘P already searches files; G5 adds a custom QuickPick for spec search. |
| **Notifications** | `vscode.window.showInformationMessage` | Task completions, agent blocks, build failures. |
| **Settings** | `contributes.configuration` in `package.json` | API URL, default model, budget limits. |
| **Keyboard shortcuts** | `contributes.keybindings` | ⌘⇧U = uplift, ⌘⇧D = downlift, ⌘⇧G = open ontology. |
| **Diff view** | `vscode.diff` command | Spec changes shown in native diff editor. |
| **Comment threads** | `CommentController` API | Spec review comments attached to line ranges — same UX as PR review comments. |

### 2.2 Webview Required

These features need custom rendering beyond what VS Code's native API offers:

| Web UI Feature | Why Webview | Implementation |
|----------------|-------------|---------------|
| **Task DAG visualization** | No native graph/SVG rendering in tree views | `WebviewPanel` with the same SVG DAG renderer from the web UI |
| **Neighborhood graph** | SVG mini-graph for spec relationships | Embedded in spec CustomEditor webview |
| **Rich spec editor** | Notion-like block editing exceeds what markdown preview offers | `CustomEditorProvider` with a webview rendering the block editor |
| **Agent ReAct trace** | Structured tool-call/result rendering with syntax highlighting | `WebviewViewProvider` in the bottom panel |
| **Cost dashboard** | Charts, spend breakdown, budget visualization | `WebviewPanel` (opened on demand) |
| **Worker status** | VM connection indicators, resource gauges | Small `WebviewViewProvider` in sidebar |

### 2.3 Features That Don't Translate

| Web Feature | Problem | Alternative |
|-------------|---------|------------|
| **Multi-user cursors** | VS Code has no multi-cursor collaboration API (Live Share is separate) | Show collaborator list in status bar; link to Live Share session |
| **Mobile layout** | VS Code doesn't run on mobile | Punt to web UI; add "Open in browser" command |
| **Drag-and-drop block reordering** | Webview-only; no native support for custom editor drag semantics | Support in CustomEditor webview, or use move-up/move-down commands |
| **Full-page dashboard** | VS Code editors are document-centric, not dashboard-centric | Use webview panel for dashboard; feels slightly foreign |

---

## 3. Tradeoffs

### 3.1 IDE Plugin vs. Web UI

| Dimension | IDE Plugin | Web UI |
|-----------|-----------|--------|
| **Context switching** | ✅ None — developer stays in editor | ❌ Must switch between browser and editor |
| **Code navigation** | ✅ Full language server (go-to-def, find refs, rename) | ⚠️ Limited (mock editor, no LSP) |
| **File editing** | ✅ Real files on disk, real undo/redo, real git | ⚠️ CRDT-backed virtual documents |
| **Terminal access** | ✅ Real integrated terminal with shell state | ⚠️ Remote terminal via WebSocket (latency) |
| **Rich UI flexibility** | ❌ Constrained to VS Code panel/webview boundaries | ✅ Full control over layout, animations, transitions |
| **Custom visualization** | ⚠️ Webviews feel slightly foreign; limited integration | ✅ SVG, canvas, arbitrary DOM — first-class |
| **Collaboration** | ❌ No real-time multi-user editing (without Live Share) | ✅ CRDT-backed live collaboration built-in |
| **Onboarding** | ❌ Must install extension + configure connection | ✅ Just open a URL |
| **Cross-platform** | ⚠️ VS Code only (or compatible: Cursor, Windsurf) | ✅ Any browser, any device |
| **Offline capability** | ✅ Local files always accessible; agent features degrade gracefully | ❌ Fully depends on backend connection |
| **Extension ecosystem** | ✅ Composes with user's existing extensions (linters, formatters, themes) | ❌ Standalone; can't leverage user's tool stack |
| **Update cycle** | ❌ Marketplace review, version compatibility | ✅ Deploy anytime, all users get update instantly |
| **Performance** | ✅ Native editor performance; only G5 overlays add overhead | ⚠️ Full app in browser; large bundle; memory pressure |

### 3.2 Native TreeView vs. Webview Sidebar

| Approach | Pros | Cons |
|----------|------|------|
| **Native TreeView** | Keyboard-navigable, theme-consistent, accessible, fast | No custom rendering (can't embed SVG, badges are limited) |
| **Webview Sidebar** | Full HTML/CSS control, can match web UI exactly | Breaks keyboard navigation, slower, accessibility gaps, doesn't respect VS Code theme automatically |

**Recommendation:** Use native TreeView for the ontology and task lists (most interactions are expand/collapse/select). Use webview only for the DAG panel, agent chat, and rich spec editor where custom rendering is essential.

### 3.3 Virtual Filesystem vs. Real Files

| Approach | Pros | Cons |
|----------|------|------|
| **`g5-spec://` virtual FS** | Specs loaded from API; no local `.g5/` dir required; clean workspace | Can't use grep/find on specs; no offline access; VS Code treats them as read-only by default |
| **Real `.g5/specs/` on disk** | Searchable, version-controlled, works offline, standard markdown files | Must sync with server; merge conflicts possible; clutters file explorer |

**Recommendation:** Real files on disk (`.g5/specs/`). The ontology specs *are* markdown files — they should live in the repo alongside code. The extension watches for changes and syncs bidirectionally with the server. This aligns with Principle P0 (ontology is ground truth) and means `rg` / `grep` / `git log` all work on specs natively.

### 3.4 Agent Interaction: Chat Panel vs. Inline

| Model | Description | Tradeoff |
|-------|-------------|---------|
| **Side panel chat** | Dedicated panel (like Copilot Chat) for agent interaction | Familiar pattern; always accessible; doesn't obstruct code view |
| **Inline annotations** | Agent suggestions appear as ghost text / CodeLens in the editor | Tighter integration; can feel intrusive if noisy |
| **Terminal-based** | Agent runs in a visible terminal; user watches tool calls scroll by | Maximum transparency; overwhelming for non-power-users |

**Recommendation:** All three, layered. Chat panel as the primary interaction surface (matches Copilot Chat muscle memory). CodeLens for contextual actions (uplift this file, view spec). Terminal for power users who want to watch the raw agent trace. Let the user choose their depth of engagement.

---

## 4. Extension Surface Area

### 4.1 Activity Bar & Sidebar

The extension registers a **view container** in the activity bar with the G5 icon:

```jsonc
// package.json (contributes)
{
  "viewsContainers": {
    "activitybar": [{
      "id": "g5",
      "title": "G5",
      "icon": "resources/g5-icon.svg"
    }]
  },
  "views": {
    "g5": [
      { "id": "g5.ontology", "name": "Ontology", "type": "tree" },
      { "id": "g5.tasks",    "name": "Tasks",    "type": "tree" },
      { "id": "g5.agents",   "name": "Agents",   "type": "webview" },
      { "id": "g5.workers",  "name": "Workers",  "type": "tree" }
    ]
  }
}
```

**Ontology tree items:**

```
📦 g5-api                                    (repo root)
  📁 task_executor/                           (module — yellow dot = stale)
    📄 task_scheduler          ✅ synced       (spec — green = up-to-date)
    📄 task_state_machine      ⚠️ stale        (spec — yellow = needs uplift)
    📄 worker_allocator        ✅ synced
    📄 job_bootstrap           ✅ synced
      └ 📎 bootstrap.rs                       (source file — click to open)
  📁 sessions/
    📄 session_store           ✅ synced
    📄 websocket_stream        🔄 running      (blue = agent actively working)
```

Each tree item supports context menu actions:
- **Spec nodes:** Uplift ↑, Downlift ↓, Review, Open Spec, View Diff, Copy Component ID
- **Source files:** Open File, View Spec, Uplift This File
- **Modules:** Uplift Subtree, Downlift Subtree, Expand All

### 4.2 CodeLens Integration

Source files annotated with G5 CodeLens above function definitions:

```rust
// src/task_executor/scheduler.rs

// ┌─ G5: ✅ Synced with spec task_executor/task_scheduler │ ↑ Uplift │ 📄 View Spec ─┐
pub fn schedule_pending(job_id: &str) -> Vec<TaskId> {
    // Kahn's algorithm for topological sort...
}

// ┌─ G5: ⚠️ Spec drift detected — implementation differs │ ↑ Uplift │ 🔍 Show Diff ─┐
pub fn on_task_complete(task_id: &str) {
    // ...
}
```

CodeLens actions:
- **Synced**: Indicates spec matches code. Click "View Spec" to open side-by-side.
- **Stale/Drift**: Yellow warning. Click "Uplift" to trigger re-uplift. Click "Show Diff" to see what changed.
- **No Spec**: Gray "Create Spec ↑" action for untracked files.

### 4.3 Agent Chat Panel

A webview-based panel in the bottom or sidebar, similar to Copilot Chat:

```
┌─ G5 Agent ──────────────────────────────────────────────┐
│                                                          │
│  🤖 Working on task_scheduler uplift...                  │
│                                                          │
│  ┌─ Tool Call: read_file ────────────────────────────┐  │
│  │ path: src/task_executor/scheduler.rs               │  │
│  │ ✅ 247 lines read                                  │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  ┌─ Tool Call: extract_interface ─────────────────────┐  │
│  │ 3 public functions found                           │  │
│  │ • schedule_pending(job_id) -> Vec<TaskId>          │  │
│  │ • on_task_complete(task_id)                        │  │
│  │ • on_task_fail(task_id, retries_left)              │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  📝 Generated spec for task_executor/task_scheduler      │
│                                                          │
│  ┌──────────────────────────────────────────────────┐   │
│  │ > Ask the agent...                       [Send]   │   │
│  └──────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────┘
```

Users can:
- Watch agent reasoning in real-time (WebSocket stream)
- Inject guidance messages ("Use RS256 for signature verification")
- Expand/collapse tool call details
- Click file references to open in editor
- Interrupt the agent (Stop button)

### 4.4 Status Bar

```
┌────────────────────────────────────────────────────────────────────────┐
│ $(g5-icon) G5: Connected │ 🤖 2 agents │ 📊 $3.42 │ task_scheduler ↑ │
└────────────────────────────────────────────────────────────────────────┘
```

- **Connection indicator**: Green = connected to g5-api, red = disconnected
- **Active agents**: Count of running agent sessions
- **Cost accumulator**: Running total for the current job
- **Current task**: Shows the active task with its direction (↑ uplift / ↓ downlift)

Click actions:
- Connection → Settings (configure API URL)
- Agents → Open agent list panel
- Cost → Open spend breakdown webview
- Task → Open task detail

### 4.5 Commands & Keybindings

```jsonc
{
  "commands": [
    { "command": "g5.uplift",         "title": "G5: Uplift Current File" },
    { "command": "g5.upliftSubtree",  "title": "G5: Uplift Subtree" },
    { "command": "g5.downlift",       "title": "G5: Downlift from Spec" },
    { "command": "g5.review",         "title": "G5: Review Against Spec" },
    { "command": "g5.openSpec",       "title": "G5: Open Spec for Current File" },
    { "command": "g5.viewDiff",       "title": "G5: Show Spec ↔ Code Diff" },
    { "command": "g5.chat",           "title": "G5: Open Agent Chat" },
    { "command": "g5.viewDAG",        "title": "G5: View Task Graph" },
    { "command": "g5.attachAgent",    "title": "G5: Attach to Running Agent" },
    { "command": "g5.searchSpecs",    "title": "G5: Search Ontology Specs" }
  ],
  "keybindings": [
    { "command": "g5.uplift",     "key": "cmd+shift+u", "when": "editorTextFocus" },
    { "command": "g5.downlift",   "key": "cmd+shift+d", "when": "editorTextFocus" },
    { "command": "g5.openSpec",   "key": "cmd+shift+s", "when": "editorTextFocus" },
    { "command": "g5.chat",       "key": "cmd+shift+g" },
    { "command": "g5.searchSpecs","key": "cmd+shift+o" }
  ]
}
```

### 4.6 Diagnostics & Problems Panel

The extension reports issues through VS Code's Diagnostics API, appearing in the Problems panel:

```
PROBLEMS (3)
  📄 scheduler.rs
    ⚠️ Spec drift: function `on_task_complete` signature differs from spec  [g5-drift]
    ℹ️ File not tracked by ontology — create spec with ⌘⇧U  [g5-untracked]
  📄 .g5/specs/task_executor/task_scheduler.md
    ⚠️ Spec references child_spec "retry_policy" which does not exist  [g5-orphan-ref]
```

Severity mapping:
- **Error**: Ontology inconsistency (broken parent/child refs, missing src_files)
- **Warning**: Spec drift, staleness, unresolved dependencies
- **Info**: Untracked files, available actions

### 4.7 SCM Integration

The extension contributes a Source Control group showing task branch state:

```
SOURCE CONTROL
  G5 Tasks
    📌 task/uplift-task-scheduler (current)
      M  .g5/specs/task_executor/task_scheduler.md
      M  src/task_executor/scheduler.rs
    
    🔀 task/downlift-session-store (agent running)
    🔀 task/review-api-handlers (blocked — needs input)
```

Each task branch shows:
- Modified files (changes in the CRDT draft)
- Agent status badge
- Context menu: Switch to branch, Attach to agent, View diff, Merge

---

## 5. Interaction Flows

### 5.1 Uplift a File

```
1. Developer opens scheduler.rs
2. CodeLens shows "⚠️ Stale — code changed since last uplift"
3. Developer clicks "↑ Uplift" CodeLens (or presses ⌘⇧U)
4. Extension calls POST /api/v1/projects/:id/jobs { job_type: "uplift", start_node: "task_scheduler" }
5. Status bar updates: "🤖 Uplifting task_scheduler..."
6. Agent Chat panel opens (if not visible) showing ReAct trace
7. Agent reads file, extracts interface, generates spec
8. On completion:
   a. Notification: "✅ Spec updated for task_scheduler"
   b. Diff view opens: old spec vs new spec
   c. Ontology tree refreshes: node shows ✅ synced
   d. CodeLens updates to "✅ Synced"
```

### 5.2 Observe a Running Agent

```
1. Tasks panel shows "websocket_stream ↓ RUNNING"
2. Developer clicks task → Agent Chat panel shows the active session
3. Real-time: tool calls appear as collapsible blocks
4. Developer sees agent struggling with a type error
5. Developer types: "The WebSocket trait requires `Send + Sync` bounds"
6. Message injected into agent session (POST /api/v1/sessions/:id/messages)
7. Agent reads the hint and adjusts its approach
8. Developer watches the fix propagate through the build
```

### 5.3 Navigate Ontology

```
1. Developer is reading auth/validator.rs
2. Presses ⌘⇧S (Open Spec)
3. Extension resolves file → component_id via IDService rules
4. Spec opens in a split editor (or custom webview with the rich FrontmatterCard)
5. Frontmatter shows parent_spec: "auth" (clickable)
6. Developer clicks "auth" → parent module spec opens
7. Developer sees child_specs list → clicks "token_cache" → navigates to that spec
```

---

## 6. Technical Implementation

### 6.1 Connection Management

```typescript
class G5Client {
  private ws: WebSocket | null = null;
  private apiBase: string;
  
  // REST: Jobs, Tasks, Specs, Projects
  async createJob(projectId: string, opts: JobOptions): Promise<Job>;
  async listTasks(jobId: string): Promise<Task[]>;
  async getSpec(componentId: string): Promise<SemliftSpec>;
  
  // WebSocket: Real-time session streaming
  connectSession(sessionId: string): Observable<SessionMessage>;
  sendMessage(sessionId: string, content: string): Promise<void>;
  
  // Lifecycle
  async connect(): Promise<void>;   // Called on extension activate
  disconnect(): void;               // Called on extension deactivate
  onConnectionChange: Event<boolean>;
}
```

**Reconnection strategy:** Exponential backoff with jitter (1s, 2s, 4s, 8s max). Connection state visible in status bar. All REST calls queue behind connection — if disconnected, they retry when connection restores. WebSocket subscriptions auto-resubscribe on reconnect, replaying missed messages via `GET /api/v1/sessions/:id/messages?after=:lastSeenId`.

### 6.2 Ontology Tree Provider

```typescript
class OntologyTreeProvider implements vscode.TreeDataProvider<OntologyItem> {
  private _onDidChangeTreeData = new vscode.EventEmitter<OntologyItem | undefined>();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;
  
  getTreeItem(item: OntologyItem): vscode.TreeItem {
    return {
      label: item.name,
      collapsibleState: item.children?.length ? Collapsed : None,
      iconPath: this.getIcon(item),        // colored dot for staleness
      contextValue: item.type,             // enables context menu filtering
      command: item.type === 'source'      // click to open file
        ? { command: 'vscode.open', arguments: [item.uri] }
        : { command: 'g5.openSpec', arguments: [item.componentId] },
      description: item.staleness,         // "synced" / "stale" / "running"
      tooltip: new vscode.MarkdownString(`**${item.componentId}**\n\n${item.description}`),
    };
  }
  
  // Refresh on: job completion, spec change, file save, manual refresh
  refresh(): void { this._onDidChangeTreeData.fire(undefined); }
}
```

### 6.3 CodeLens Provider

```typescript
class G5CodeLensProvider implements vscode.CodeLensProvider {
  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const componentId = this.resolveComponentId(document.uri);
    if (!componentId) return [];  // File not tracked by ontology
    
    const spec = this.specCache.get(componentId);
    const staleness = this.computeStaleness(document, spec);
    
    // File-level CodeLens at line 0
    const lenses: vscode.CodeLens[] = [];
    lenses.push(new vscode.CodeLens(new vscode.Range(0, 0, 0, 0), {
      title: staleness === 'synced' 
        ? '$(check) G5: Synced with spec' 
        : '$(warning) G5: Spec drift detected',
      command: 'g5.viewDiff',
      arguments: [componentId],
    }));
    
    lenses.push(new vscode.CodeLens(new vscode.Range(0, 0, 0, 0), {
      title: '↑ Uplift',
      command: 'g5.uplift',
      arguments: [componentId],
    }));
    
    lenses.push(new vscode.CodeLens(new vscode.Range(0, 0, 0, 0), {
      title: '📄 View Spec',
      command: 'g5.openSpec',
      arguments: [componentId],
    }));
    
    return lenses;
  }
}
```

### 6.4 Spec Custom Editor

For the rich spec editing experience (FrontmatterCard, neighborhood graph, block editor), a `CustomEditorProvider` renders specs in a webview:

```typescript
class SpecEditorProvider implements vscode.CustomTextEditorProvider {
  resolveCustomTextEditor(
    document: vscode.TextDocument,
    webviewPanel: vscode.WebviewPanel,
  ): void {
    // Webview renders: FrontmatterCard + block editor + comments
    webviewPanel.webview.html = this.getHtml(document);
    
    // Bidirectional sync: document ↔ webview
    const changeDisposable = vscode.workspace.onDidChangeTextDocument(e => {
      if (e.document.uri.toString() === document.uri.toString()) {
        webviewPanel.webview.postMessage({ type: 'update', content: document.getText() });
      }
    });
    
    webviewPanel.webview.onDidReceiveMessage(msg => {
      if (msg.type === 'edit') {
        const edit = new vscode.WorkspaceEdit();
        edit.replace(document.uri, fullRange(document), msg.content);
        vscode.workspace.applyEdit(edit);
      }
      if (msg.type === 'navigate') {
        vscode.commands.executeCommand('g5.openSpec', msg.componentId);
      }
    });
  }
}
```

**Fallback:** Users who prefer plain markdown editing can set `g5.specEditor: "text"` to bypass the custom editor and use VS Code's built-in markdown editor. The extension still provides CodeLens and diagnostics.

---

## 7. Deployment & Distribution

### 7.1 Marketplace

Published to the VS Code Marketplace as `g5-labs.g5` (and Open VSX for open-source forks).

**Activation events:**
```jsonc
{
  "activationEvents": [
    "workspaceContains:.g5/",          // Auto-activate in G5 projects
    "onCommand:g5.connect",            // Manual activation
    "onView:g5.ontology"               // When user opens the G5 sidebar
  ]
}
```

The extension activates lazily — zero overhead in non-G5 workspaces.

### 7.2 Compatibility Matrix

| Editor | Support Level | Notes |
|--------|--------------|-------|
| VS Code | ✅ Full | Primary target |
| Cursor | ✅ Full | VS Code fork; extension works unchanged |
| Windsurf | ✅ Full | VS Code fork; extension works unchanged |
| VS Code Web (vscode.dev) | ⚠️ Partial | No terminal, no local filesystem; spec viewing + tree work |
| Codespaces | ✅ Full | Remote extension host; same as local |
| JetBrains | ❌ Not supported | Different extension model; would require separate plugin |
| Neovim | ❌ Not supported | Could build a minimal LSP client for spec validation only |

### 7.3 Configuration

```jsonc
{
  "g5.apiUrl": "https://api.g5.dev",           // G5 backend URL
  "g5.apiToken": "",                             // Auth token (or use OAuth flow)
  "g5.specEditor": "rich",                       // "rich" (webview) or "text" (plain markdown)
  "g5.codeLens.enabled": true,                   // Show G5 CodeLens annotations
  "g5.codeLens.staleness": true,                 // Show staleness indicators
  "g5.autoSync": true,                           // Auto-sync specs on save
  "g5.agentChat.position": "panel",              // "panel" (bottom) or "sidebar"
  "g5.notifications.taskComplete": true,          // Notify on task completion
  "g5.budget.warnAt": 5.00,                      // Cost warning threshold ($)
  "g5.budget.maxPerJob": 10.00                    // Max cost per job ($)
}
```

---

## 8. Hybrid Strategy: Web + IDE

The IDE plugin does not *replace* the web UI — they are complementary:

| Use Case | Best Surface |
|----------|-------------|
| Day-to-day coding + spec maintenance | **IDE Plugin** |
| Initial project onboarding + workspace setup | **Web UI** |
| Dashboard / cost overview / admin | **Web UI** |
| Agent observation during long batch jobs | **Either** (IDE chat panel or web task timeline) |
| Collaborative spec editing session | **Web UI** (real-time CRDT collaboration) |
| Code review with spec context | **IDE Plugin** (native diff + CodeLens) |
| Mobile check-in on job progress | **Web UI** (responsive mobile layout) |
| Demo / presentation | **Web UI** (self-contained, no install) |

The shared backend (g5-api) means both surfaces see the same data, same agent sessions, same ontology state. A developer can start an uplift job from the IDE, switch to the web UI to show a colleague the task DAG, then come back to the IDE to review the generated spec — all seamless.

---

## 9. Open Questions

| # | Question | Options |
|---|----------|---------|
| Q1 | Should spec files use a custom language ID (for syntax highlighting/validation)? | `g5-spec` language with markdown base + YAML frontmatter grammar, vs. plain `markdown` |
| Q2 | How to handle agent cost attribution in a team setting? | Per-user API tokens with budget limits, vs. shared workspace budget |
| Q3 | Should the extension bundle the analysis pipeline (Python)? | Remote-only (analysis runs on server), vs. local analysis for offline uplift |
| Q4 | How to handle large ontologies (>500 nodes) in the tree view? | Lazy loading + search, vs. virtual scrolling, vs. flat filtered list |
| Q5 | Should the DAG webview be a panel or an editor tab? | Panel (always visible) vs. editor tab (opened on demand, can be split) |

---

## 10. Cross-Reference Map

| Topic | Location |
|-------|----------|
| Agent architecture & tool set | AGENTS.md §1–§6 |
| Task lifecycle & state machine | AGENTS.md §4 |
| Session streaming (WebSocket) | WORKER_INFRASTRUCTURE.md §10.3, §10.5 |
| Jobs & Tasks API | API_AND_STORAGE.md §3.4 |
| CRDT draft model | AGENTS.md §4 (dirty tree preservation) |
| Web UI (comparison target) | FRONTEND_DESIGN.md §9 |
| Spec format (SemliftSpec) | AGENTS.md §2.1 |
| IDE integration (Open in IDE) | WORKER_INFRASTRUCTURE.md §13 |
| Ontology node types | AGENTS.md §3 |
