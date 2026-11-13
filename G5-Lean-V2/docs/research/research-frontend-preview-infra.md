# G5-Lean research: frontend feature inventory, preview end-to-end, infra/deployment model

Read-only survey of `/Users/zliu/Workspace/G5-Lean` (2026-09-12).

Conventions:
- **Observed** = read in the cited `file:line` during this survey.
- **Inference** = my reading of the code; not verified by execution or by inspecting a deployment.
- Existing audits were used as a starting point and cross-checked, not re-derived.

Status of the existing audits:
- `frontend/FRONTEND_AUDIT.md` is dated 2025-01-15 and describes a WorkspacePage with stubbed hooks
  (no `useDocs`, empty file tree, CRDT never wired). Observed today: `hooks/useDocs.ts` exists
  (`useDocList`, `useDocContent` at `useDocs.ts:79,112`), `useFileTree` is imported by WorkspacePage
  (`WorkspacePage.tsx:125-130`), and `lib/crdt.ts` is consumed by `SpecEditor.tsx`, `CodeEditor.tsx`,
  `useDraft.tsx`, `WorkspacePage.tsx` (grep of `from "../lib/crdt"`). The old audit is stale for
  feature-wiring purposes; only its spec/type drift list is of historical value.
- `docs/dead-code-proposal.md` and `docs/dead-code-retirement.md` are current (2026-09-12). Batches 1-4
  removed 13 frontend files and 21 hooks. Observed the removals landed: `hooks/` has no
  `usePermissions.ts`/`useSpend.ts`; `pages/` has no `LandingPage.tsx`/`WorkspaceSettingsPage.tsx`;
  `components/chat/` has no `SessionPicker.tsx`/`ToolCallBubble.tsx`; `useOntology.tsx` exports only
  `useOntologyRoots/useNodeContent/useNodeDiff/useReadOntology` (`useOntology.tsx:36-130`).

---

## 1. Frontend feature inventory and backend dependencies

### 1.1 Route table and shell (Observed)

`frontend/src/App.tsx:806-1020` defines every route. The workspace shell (`pages/WorkspacePage.tsx`, 485 KB)
hosts nearly all features as center-panel tabs; org-admin pages are also tabs reached via `?view=`.

| Route | Component | Note |
|---|---|---|
| `/login`, `/login/denied` | `LoginPage`, `LoginDeniedPage` (in App.tsx) | OAuth providers from `lib/authProviders.tsx` |
| `/cli/authorize` | `CliAuthorizePage` | `routes.auth.cliApprove` = `/api/v1/auth/cli/approve` (`lib/routes.ts:24`) |
| `/`, `/landing`, `*` | `LandingRedirect` -> `/coming-soon.html` (`App.tsx:740-748`) | public launch page is static |
| `/:org/home` | `pages/HomePage.tsx` | "UI v3 surface" (`HomePage.tsx:1-7`) |
| `/:org/:workspace` (+ `/spec/*`, `/doc/*`, `/file/*`, `/intent-log`, `/job/:jobId[/task/:taskId]`) | `pages/WorkspacePage.tsx` | one shell; URL selects the tab |
| `/:org/:workspace/issues[/:slug]` | `pages/IssuesPage.tsx` | redirector into the workspace tab with `?openIssue=` (`IssuesPage.tsx:1-9`); requires `issue_tracking` feature (`:20-23`) |
| `/:org/settings/{members,workers,approvals}`, `/:org/settings`, `/settings` | redirects (`buildWorkspaceViewRedirect`, `App.tsx:127-140`) into `/:org/base?view=settings|workers|members|approvals|approval-definitions` | org admin surfaces are workspace tabs |
| `/projects` | `pages/ProjectsPage.tsx` | workspace list across orgs (`ProjectsPage.tsx:1-7`) |
| `/create-org` | `CreateOrgPage` + `components/org/CreateOrgDialog.tsx` | |
| `/ide` | `pages/IDEPluginView.tsx` | **dev-mode only** (`isDevMode()`, `App.tsx:1009-1017`); mockup of a VS Code extension (`IDEPluginView.tsx:1-10`) |

Shell composition (Observed):
- Center tab vocabulary: `TabItem` union in `lib/types.ts:4013-4060`: `ontology`, `doc`, `guide_doc`,
  `intent_log`, `pull_requests`, `workspace_plan`, `diff`, `file`, `job_output`, `workspace_draft_diff`,
  `source_control_diff` (the union continues past line 4060). WorkspacePage also mounts
  `WorkspaceSettingsContent`, `WorkerHostsPageContent`, `OrgMembersPageContent`, `OrgApprovalsPageContent`,
  `WorkspaceApprovalsPageContent`, `PreviewBrowserView`, `IssuesList`/`IssueDetailPanel`,
  `WorkspaceDetailsPanel` (imports at `WorkspacePage.tsx:248-330`).
- Left sidebar sections: `SidebarSectionId` = `ontology | project-docs | explainers | all-docs |
  source-control | issues | tasks | files` (`components/tree/TreeNode.tsx:99-107`; `TreeNode.tsx` is 154 KB).
- Top strip: `components/workspace/TopBar.tsx:1-12` (sidebar toggle, workspace switcher, worker status pill,
  inline search, notifications, settings, chat toggle); `ReposBar.tsx:1-11` (Commit/Discard/Sync/Preview,
  Rebase & Merge, PR actions, Download Code, status-band chevron); `WorkspaceStatusBand.tsx:1-22`
  (stage-flow panels Requirements / Plan / [Plan Approvals] / Code / Finalize driven by `workspace.stages`).
- Right panel: `components/chat/ChatPanel.tsx` (290 KB).
- Bottom drawer: `components/execution/BottomDrawer.tsx:72` tabs `dag | changes | timeline | terminal | escalation`.
- Header layout gate: `lib/topPanelFlags.ts` resolves `simple | advanced | both` from the `top_panel`
  instance feature plus the user preference.

### 1.2 Feature-gating mechanisms (Observed)

Build-time Vite flags (`lib/capabilities.ts:40-52`, `vite-env.d.ts`):
- `VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS`, `VITE_G5_ENABLE_INTENT_LOG_VIEWER`, `VITE_G5_CI_WORKFLOW`,
  `VITE_DEV_MODE`, `VITE_MOCK`.
- `Dockerfile.edge:29-36` defaults all three `VITE_G5_*` to `false`; `docker-compose.dev.yml:378-381`
  sets cloud workflows `true`, intent log `false`.
- `applyDeploymentWorkspaceUiCapabilities` (`capabilities.ts:56-66`) forces `cloudWorkspaceWorkflows=false`
  unless `instanceInfo.selfHosted === false`.

Runtime instance features from `GET /api/v1/instance/info` (`backend/g5-api-routes/src/routes/instance.rs:236-281`):
- always: `crdt`, `search`, `chat`;
- conditional: `cloud_workers`, `issue_tracking` (`G5_ISSUE_TRACKING_ENABLED`, default false,
  `g5-api-foundation/src/config.rs:404`), `codex_ui` (`G5_CODEX_UI_ENABLED`), `g5_ui_v3`
  (`G5_UI_V3_ENABLED`), `top_panel` (`G5_ENABLE_TOP_PANEL`, default on), `provider_accounts`
  (`G5_ENABLE_PROVIDER_ACCOUNTS`, default on), `platform_issue_reporting`, `make_remote`
  (`G5_MAKE_REMOTE_ENABLED`), `oauth`;
- client resolvers: `lib/uiV3Flags.ts`, `lib/providerFlags.ts`, `lib/topPanelFlags.ts`,
  `hooks/useSettings.ts:107-154`.

Hosted values:
- dev enables `G5_CODEX_UI_ENABLED`, `G5_UI_V3_ENABLED`, `G5_MAKE_REMOTE_ENABLED` (`infra/helm/g5/values-dev.yaml`);
- prod sets `G5_UI_V3_ENABLED: "false"`, `G5_CODEX_UI_ENABLED: "false"`, `G5_MAKE_REMOTE_ENABLED: "false"`,
  `G5_DEFAULT_WORKSPACE_LAYOUT: "advanced"` (`values-prod.yaml`).
- Inference: governance/approvals "UI v3" surfaces, Codex, and Make-Remote are not user-visible in prod today.

### 1.3 Feature areas: components, hooks, routes (Observed)

Endpoint lists come from a grep of each hook for `/api/v1` literals and `routes.*` constants. WS event
names are from `lib/ws.ts:61-266`; WS channels from `backend/g5-api-core/src/ws/channels.rs:147-159`
(`workspace:`, `job:`, `task:`, `chat:`, `draft:<ws>:<branch>`, `user:`, `session_inbox:`, `org:`).

| # | Feature area | Main components / hooks | Backend routes (all under `/api/v1`) and WS |
|---|---|---|---|
| 1 | Auth, orgs, CLI approve | `useAuth.tsx:602-701`, `LoginPage`, `CliAuthorizePage`, `CreateOrgDialog` | `/auth/login/:p`, `/auth/refresh`, `/auth/logout`, `/users/me`, `/auth/ws-ticket` (`App.tsx:768`), `/orgs`, `/scm/:p/repos[/import]` |
| 2 | Org Home | `HomePage.tsx`, `useHome.ts:43 useHomeAttention`, `lib/recentWorkspaces.ts` | `routes.orgs.homeAttention` |
| 3 | Workspace list / create / clone / delete | `ProjectsPage.tsx`, `WorkspaceTable.tsx`, `CreateWorkspaceDialog.tsx`, `CloneDialog.tsx:1-15`, `RemoveWorkspaceDialog.tsx`, `useWorkspace.ts:117-302` | `/orgs/:id/workspaces[/init]`, `/workspaces/:id`, `/workspaces/:id/clone`, `/resolve` |
| 4 | Ontology tree + spec editor | `OntologyTree.tsx`, `TreeNode.tsx`, `SpecEditor.tsx:1-15` (Notion-like block editor over a Y.Doc), `SemanticRecordFrame.tsx`, `NodeGraphView.tsx`, `useOntology.tsx:36-130` | `/workspaces/:id/ontology[/:node/{children,content,diff,read-ontology}]`; WS `ontology.changed` |
| 5 | Files tree + code editor | `FileTree.tsx`, `WorkspaceFileTree.tsx`, `CodeEditor.tsx:1-10` (Monaco + y-monaco), `useFiles.ts:17-97` | `/branches/:id/files[/path]` GET/PUT/DELETE |
| 6 | Docs / explainers / guide docs | `useDocs.ts:27-197`, `CreateDocDialog.tsx`, `CreateProjectDocModal.tsx`, `CreateExplainerModal.tsx`, `ExplainerOutputView.tsx`, `useExplainers.ts:100-331`, `lib/userGuide.ts` | `/workspaces/:id/docs[?surface=workspace]`, `/docs/by-path`, `/docs/implementation-overview`, `/docs/:id/links`, `/docs/:id/sync` (`useViews.ts:20`), `/workspaces/:id/docs/explainers`, `/branches/:id/explainer-templates`, `/jobs/:id/explainer-merge`; WS `doc.*`, `docs.links_changed` |
| 7 | Drafts / commit / discard (CRDT) | `useDraft.tsx:136-315`, `DraftStatusBar.tsx`, `DraftWorkspaceControls.tsx`, `CommitModal.tsx`, `useAutoCommitBeforeAction.ts:50`, `lib/crdt.ts` | `/branches/:id/drafts`, `/drafts/:id/{commit,discard,pre-commit-check}`, `/workspaces/:id/drafts[/commit-all|/diff]`; **WS `/api/v1/yjs/:workspaceId/:branchId?token&gitSha[&taskId]`** (`crdt.ts:370`, `routes/mod.rs:456-457,539`); WS `draft.update/commit/discard/behind/participant` |
| 8 | Comments | `CommentColumn.tsx`, `CommentThread.tsx`, `DraftCommentInput.tsx`, `useComments.ts:20-217` | `/workspaces/:id/comments[/cross-branch]`, `/comments/:id[/replies]`; WS `comment.new/reply` |
| 9 | Chat | `ChatPanel.tsx`, `ChatMessage.tsx`, `ArtifactRenderer.tsx`, `ChatContextLinks.tsx:1-12`, `BackgroundAgentsCard.tsx`, `SubscriptionUsageChip.tsx`, `AgentLaunchConfigControls.tsx`, `CodexSubscriptionAuthPrompt.tsx`, `useChat.ts:55-1430`, `useAgentLaunch.ts:27`, `lib/harnessPicker.ts`, `lib/claudeModels.ts`, `lib/codexModels.ts`, `lib/chatWorkerConflict.ts` | `/workspaces/:id/sessions[/start|/search]`, `/sessions/:id[/messages|/messages/:m/stop|/resume|/fork|/timeline|/task|/attach|/approvals/:a|/provider-control|/continue-in-merged-workspace|/context-links[/:l]|/attachments/:a/content]`, `/workspaces/:id/config/agent-launch/resolve`, `/tasks/:id/agent-auth`; WS `chat.chunk/message_updated/done/error/context_updated`, channel `chat:<id>` |
| 10 | Jobs / tasks panel ("Task Queue") | `TreeNode.tsx` tasks section, `BottomDrawer.tsx`, `DagView.tsx`, `Timeline.tsx`, `TerminalPanel.tsx:1-6` (renders `output` timeline entries only, **no PTY**), `JobProgressView.tsx` + `job-progress/*`, `JobOutputView.tsx:1-16`, `JobQuestionDocView.tsx`, `TaskWorkspaceSwitchPrompt.tsx`, `useJobs.ts:241-1430` | `/workspaces/:id/jobs[/tasks/query]`, `/jobs/:id[/pause|/resume|/cancel|/tasks|/progress|/output/{diff,files,...}]`, `/tasks/:id[/timeline|/checkpoints|/diff|/pause|/resume|/abort|/retry|/complete|/force-complete|/rollback]`, `/sessions/:id/inject`; WS `task.status/timeline/diff`, `job.cost`; channels `job:`, `task:` |
| 11 | Generate (uplift) / Sync (downlift) | `GenerateModal.tsx:1-8`, `SyncModal.tsx:1-7`, `useWorkspace.ts:356-409` | `/workspaces/:id/sync`, `/workspaces/:id/sync/pending?direction=downlift`, `/workspaces/:id/docs/sync-isolated`; creates `sync`/`uplift`/`downlift` jobs |
| 12 | Preview | `PreviewControl.tsx`, `PreviewBrowserView.tsx`, `previewBranch.ts`, `useJobs.ts:314-578`, `WorkspaceCapacityIndicator.tsx`, `capacity/previewLinks.ts` | see section 2 |
| 13 | Job merge / workspace merge / rebase | `job-merge-workflow/*` (`JobMergeConfirmModal`, `DirectCleanMergeChoiceModal`, `OutstandingMergeJobsModal`, `WorkspaceMergeConfirmModal`, `RebaseChangeGraphPreview`), `useJobs.ts:1113-1160`, `useWorkspace.ts:409-487` | `/jobs/:id/merge[/preflight|/direct-clean]`, `/workspaces/:id/merge`, `/workspaces/:id/rebase-merge[/preflight]`, `/workspaces/:id/rebase-merge/change-graph` |
| 14 | Pull requests / Prepare to Ship | `PullRequestPanel.tsx`, `PullRequestList.tsx`, `CreatePullRequestDialog.tsx`, `PrepareToShipModal.tsx:1-13`, `ReviewChangesModal.tsx:1-11`, `ReviewWorkspaceButton.tsx`, `useWorkspace.ts:488-679` | `/workspaces/:id/pull-requests[?role&status]`, `.../preview`, `.../:pr/{merge,refresh,close}` |
| 15 | Approvals (workspace + org definitions) | `WorkspaceApprovalsPageContent.tsx:1-20`, `OrgApprovalsPageContent.tsx:1-20`, `useWorkspaceApprovals.ts:122-293`, `useOrgApprovals.ts:35-81`, `lib/approvalHistory.ts`, `lib/approvalStages.ts` | `routes.workspaceApprovals.*` (`/workspaces/:id/approvals[/requirements|/decisions|/requests|/reviews]`), `routes.orgs.approvalDefinitions` |
| 16 | Remote tracking / push / make-remote | `RemoteTrackingActions.tsx:1-22`, `BranchTrackingIcon.tsx`, `BranchTrackingPopover.tsx`, `BranchAutoPullControl.tsx`, `MakeRemoteDialog.tsx:1-20`, `CoordinatedRepoSyncModal.tsx`, `useRemoteTracking.ts:63-476` | `/workspaces/:id/push_to_remote`, `/push_mode`, `/base_auto_pull`, `/branches/:b/sync_runs[/retry]`, `/branches/:b/merge_runs[/:r/start|/complete]`, `/branches/:b/repos/:r/{remote_tracking,publish,auto_pull}`, `/remote_tracking/approved_rebase_publish`; WS `workspace.remote_tip_advisory_updated`, `workspace.authoritative_cut_advanced` |
| 17 | Source control graph + history | `SourceControlChangeGraphView.tsx`, `SourceControlChangeGraphWhatChanged.tsx`, `WorkspaceCommitDiffView.tsx`, `WorkspaceDraftDiffView.tsx`, `DiffView.tsx`, `useSourceControlChangeGraph.ts:35-121`, `useJobs.ts:337-467` | `/workspaces/:id/source-control/change-graph[/nodes/:n/details]`, `/branches/:id/commits[/history|/:sha/diff[/files/:p]]`, `/workspaces/:id/drafts/diff` |
| 18 | Issues | `IssuesList.tsx:1-8`, `IssueDetailPanel.tsx:1-13`, `OntologyIssueBadges.tsx`, `useIssues.ts:78-452` | `/orgs/:id/issues...`; WS `issue.created/updated/event` on `org:` channel |
| 19 | Search | `SearchBar.tsx:1-14`, `SearchPanelShared.tsx`, `SearchHighlight.tsx`, `useSearch.ts:22` | `/workspaces/:id/search`, `routes.instance.searchStatus` |
| 20 | Grounding / traceability | `IntentTraceabilityPanel.tsx:1-8`, `SemanticRecordFrame.tsx`, `useGrounding.ts:160-217`, `lib/reviewMetadata.ts` | `/grounding/trace-parents`, `/trace-children`, `/review-metadata`, `/review-actions` |
| 21 | Intent log | `intent-log/*` (`IntentLogView.tsx:1-6`, table/filters/lineage graph), `useIntentLog.ts:112` | `/intent-log` |
| 22 | Human escalations (HIL) | `HilEscalationPanel.tsx:1-8`, `HilResponseComposer.tsx`, `HilPriorityBanner.tsx`, `JobQuestionDocView.tsx:1-14`, `useEscalations.ts:192-348`, `lib/questionBlock.ts` | `/escalations[?qs]`, `/escalations/:id[/respond|/cancel|/mark-in-review]`, job-output answer endpoint; WS `escalation.*` |
| 23 | Notifications | `NotificationPanel.tsx:1-9`, `NotificationToastStack.tsx`, `useNotifications.ts:103`, `useClientToasts.ts` | `/workspaces/:id/notifications[/read-all]`, `/notifications/:id`; WS `notification` |
| 24 | Settings (workspace/user/org config) | `WorkspaceSettingsContent.tsx:1-13`, `RuntimeEnvConfig.tsx:1-14`, `ProviderAccountsSection.tsx`, `ProviderAccountAssignment.tsx`, `SettingsForm.tsx:1-6`, `SpendBreakdown.tsx`, `UsageRing.tsx`, `OrgApiKeyManager.tsx`, `UserSettingsPanel.tsx:1-7`, `useSettings.ts:38-307`, `useUserSettings.ts`, `useProviderAccounts.ts` | `/workspaces/:id/settings`, `/instance/info`, `/users/me/config`, `/users/me/preferences`, `/orgs/:id/config`, `/workspaces/:id/config`, `/orgs/:id/provider-accounts`, `/orgs/:id/api-keys`, `/scm/*` |
| 25 | Org members / invitations | `OrgMembersPageContent.tsx`, `InviteDialog.tsx`, `useOrg.ts:32-257` | `/orgs/:id/members[/:u]`, `/orgs/:id/invitations[/:i/resend]`, `/invitations/:id/{accept,decline}` |
| 26 | Worker hosts admin, worker tokens, capacity | `WorkerHostsPageContent.tsx:1-20`, `AddWorkerHostDialog.tsx`, `OrgWorkerTokenManager.tsx:1-12`, `WorkersDropdown.tsx`, `ConnectedWorkerPill.tsx`, `WorkspaceCapacityIndicator.tsx:1-8`, `useWorkerHosts.ts:96-188`, `useWorkers.ts:20-91`, `useOrg.ts:80,167,188` | `/orgs/:id/worker-hosts[/:h][/runtime-labels/refresh][/image-policy]`, `/orgs/:id/worker-tokens`, `/orgs/:id/workers[/all][/:w/preferred]`, `/workspaces/:id/workers[/capacity]` |
| 27 | Telemetry (UI events) | `lib/telemetry.ts:1-17`, `TelemetryBridge` in `App.tsx` | `POST /telemetry/ui-events` |
| 28 | Platform bug report | `PlatformIssueReportButton.tsx:1-6`, `captureScreenshot.ts`, `usePlatformIssues.ts:19` | `POST /platform/issues` |
| 29 | Self-hosted / license | `SelfHostedPreProductionBannerProvider` (`App.tsx:27,801`) keyed on `instanceInfo.selfHosted` | no license UI in frontend (grep `license` -> no hits); backend `routes/self_hosted.rs` mounts `/api/v1/registry` and `/api/v1/admin` (`routes/mod.rs:362-364`) for installers |
| 30 | Workspace details / read-only / mutation policy | `WorkspaceDetailsPanel.tsx:1-16`, `ReadOnlyBadge.tsx`, `lib/readOnlyNotice.ts`, `useWorkspace.ts:334` | `/workspaces/:id/mutation-policy`, `routes.workspaces.sourceDownload` |
| 31 | Dev / mocks | `mocks/fetch-interceptor` (`VITE_MOCK`), `data/mock.ts`, `test/msw/*`, `IDEPluginView` | none |

Notes on the table:
- Job types the UI creates (grep of `jobType: "..."` incl. tests): `sync` 67, `preview` 48, `chat` 41,
  `uplift` 27, `merge` 14, `downlift` 13, `compile` 10, `explainer_generate` 8, `view_create` 6, `custom` 4.
  `view_create` is rejected server-side ("ViewCreate already retired", `docs/dead-code-audit-coverage.md`).
- Hotlists: the backend channel doc mentions `hotlist.*` events on `Channel::Org` (`channels.rs:70`); no
  hotlist component was found in the `components/` directory listing.
- Waitlist: `routes.waitlist` (`lib/routes.ts:361`) and `routes/waitlist.rs` (mounted at `routes/mod.rs:429`)
  remain, but the only caller (LandingPage) was removed; the dead-code proposal lists it as an API-retirement decision.

### 1.4 Dependency matrix (Observed unless marked)

Columns: **CRDT** = needs the Yjs draft path (`/api/v1/yjs`) or draft rows; **Worker** = needs a live
worker host / microVM to do anything; **Sticky** = touches sticky chat runtimes (`chat_sticky_runtimes`,
`services/sticky_runtime.rs`); **Merge/Appr** = part of the merge / PR / approvals workflow;
**Gate** = flag or status.

| # | Feature | CRDT | Worker | Sticky | Merge/Appr | Gate / status |
|---|---|---|---|---|---|---|
| 1 | Auth/orgs | - | - | - | - | live |
| 2 | Org Home | - | - | - | - | route unconditional; comment says "UI v3" |
| 3 | Workspace list/clone | - | clone warns on dirty draft (`CloneDialog.tsx:8`) | - | governance model chosen at clone (`:10-12`) | live |
| 4 | Ontology/spec editor | **yes** (edits via `useDraftCRDT`) | - | grep hit in `SpecEditor.tsx` | - | live, core |
| 5 | Files/code editor | **yes** (y-monaco) | - | grep hit in `CodeEditor.tsx` | - | live, core |
| 6 | Docs/explainers | yes (doc editing is CRDT) | `explainer_generate` is a job | - | explainer output merges into draft | live |
| 7 | Drafts/commit | **is** the CRDT feature | - | - | commit is the git boundary | live, core |
| 8 | Comments | line anchors on CRDT docs | - | - | - | live |
| 9 | Chat | edits land in branch draft via host `CrdtPeer` (`supervisor.rs:10975-10981`) | **yes** (`chat` job/task on a worker) | **yes** (worker pin; `sticky` grep hits in `ChatPanel.tsx`) | in-chat agent approvals (`lib/agentApprovals.ts`) | live, core; Codex behind `codex_ui` |
| 10 | Jobs/tasks | job output reads the retained job branch (git) | **yes** | - | job merge from here | live, core |
| 11 | Generate/Sync | auto-commits first | **yes** | - | - | live |
| 12 | Preview | auto-commits first | **yes** (own microVM) | - | merge-result preview from Rebase & Merge (`PreviewControl.tsx:14-16`) | live; off on local macOS host (`scripts/worker-host-env.sh:216`) |
| 13 | Merge/rebase | - | `merge` job on worker | - | **is** the workflow | live |
| 14 | Pull requests | - | - | - | **yes** | live (`e2e/flows/workspace-pull-requests.spec.ts`) |
| 15 | Approvals | - | `approval_review` job for agent approvals | - | **yes** | behind `g5_ui_v3` (`uiV3Flags.ts:1-9`); off in prod |
| 16 | Remote tracking | - | push performed by "the worker" asynchronously (`RemoteTrackingActions.tsx:10-13`) | - | approved rebase publish | live; Make-Remote behind `make_remote` |
| 17 | Source control graph | draft diff yes | - | grep hit | feeds Review Changes | live |
| 18 | Issues | - | - | grep hit in `IssuesList.tsx` | - | behind `issue_tracking` (default false) |
| 19 | Search | - | - | - | - | live; indexer is a separate `G5_ROLE=indexer` process |
| 20 | Grounding | anchors in specs | - | - | - | live (`e2e/flows/grounding-traceability.spec.ts`) |
| 21 | Intent log | - | - | grep hit in `IntentLogRowDetail.tsx` | - | **build-flag gated**, default off in edge image and dev compose |
| 22 | HIL escalations | question docs are CRDT / task drafts | **yes** (agent parked on a question) | - | - | live |
| 23 | Notifications | - | - | grep hits | - | live; dev-mode delivery toggles gated by `isDevMode()` (`WorkspaceSettingsContent.tsx:12`) |
| 24 | Settings | - | - | grep hits (`SettingsForm`: concurrent tasks, cloud workers, idle timeout) | - | live; provider accounts behind `provider_accounts` |
| 25 | Org members | - | - | - | - | live |
| 26 | Worker hosts admin | - | **is** the worker control plane | preferred-worker pinning | - | add/remove gated on `cloudWorkspaceWorkflows && !selfHosted && org admin` (`WorkerHostsPageContent.tsx:18-20`) |
| 27 | Telemetry | - | - | - | - | live, best-effort |
| 28 | Platform bug report | - | - | - | - | behind `platform_issue_reporting` (GitHub App config `config.rs:432-473`) |
| 29 | Self-hosted/license | - | - | - | - | operator/CLI surface, not UI |
| 30 | Workspace details | - | - | - | - | live |
| 31 | Dev/mocks | - | - | - | - | dev-only |

### 1.5 Cross-cutting notes (Observed unless marked)

- CRDT is load-bearing for every editing surface. `SpecEditor` (y-prosemirror) and `CodeEditor` (y-monaco)
  bind to a `Y.Doc` per `(workspaceId, branchId, gitSha)` through `WebsocketProvider` to `/api/v1/yjs/...`
  (`crdt.ts:1-15,370`). Commit/discard are draft operations (`useDraft.tsx`). Every job/preview launcher
  first calls `useAutoCommitBeforeAction` to flush the draft to git (`PreviewControl.tsx:20-22,873`).
  Server side: `backend/g5-api-core/src/crdt/{authority,persistence,task_commit}.rs` (196/110/65 KB) and
  the `backend/g5-crdt` crate. Helm pins `api.replicaCount: 1` with the comment "Temporary CRDT/local-event-bus
  mitigation" (`infra/helm/g5/values.yaml:41-44`).
- Sticky runtimes. `services/sticky_runtime.rs:1-20` owns `chat_sticky_runtimes` rows; a parked sticky
  microVM keeps the host's `active_microvm_count > 0` and blocks rotation (`cloud/rotation.rs:29-40`).
  The string `sticky` appears in 16 frontend files (`TreeNode.tsx`, `WorkspaceSettingsContent.tsx`,
  `SettingsForm.tsx`, `ChatPanel.tsx`, `IntentLogRowDetail.tsx`, `SourceControlChangeGraphView.tsx`,
  `IssuesList.tsx`, `DiffFileBrowser.tsx`, `JobOutputView.tsx`, `SpecEditor.tsx`, `NotificationToastStack.tsx`,
  `useNotifications.ts`, `CodeEditor.tsx`, `lib/defaults.ts`, `lib/types.ts`, `WorkspacePage.tsx`); only the
  chat worker-pin and settings semantics were read. Helm flags `G5_STICKY_RUNTIME_DISCARD_FENCE_V2_*`
  (`values.yaml:74-77`).
- "Worker" in the UI is two things: (a) per-org worker hosts (`worker_hosts` control plane,
  `WorkerHostsPageContent`), and (b) `workers` rows = registered worker sessions (the in-VM
  `g5d worker --task` under the borrowed host identity, `cloud/startup_script.rs:27-36`) shown by
  `ConnectedWorkerPill`/`WorkersDropdown` and pinnable per workspace/chat.
- Workspace model (`lib/types.ts:955-993` `WorkspaceDetail`): `branches: BranchSummary[]` (one per repo),
  `docsRepoId/docsBranchId`, `isBase`, `readOnly`, `pushMode`, `visibility`, `stages`, `dependencies`.
  Inference: a workspace today is "one branch per repo across N repos + a docs repo", not 1-1 with a
  single git branch.

---

## 2. Preview end-to-end today

### 2.1 Summary

A preview is a first-class `preview` **job** whose single task boots a **dedicated long-lived libkrun
microVM on a worker host** (not the VM of any agent task), materialized from a **committed ref**
(workspace branch for `unmerged`, base commit for `merged`), running `g5d preview-serve`, which execs the
customer's `command` from `.g5/workspace.yaml` `preview:` or `.g5/preview_profiles/<name>.yaml`.
The browser reaches it through g5-api's **reverse proxy over a host-dialed WebSocket tunnel** (hosts are
outbound-only), at `/api/v1/preview/<jobId>/...` or at `https://<jobId>.<G5_PREVIEW_HOST_SUFFIX>/`.
It is torn down by Stop (job cancel), TTL (default 1 h) via the `preview_reaper`, or host drain.
Citations for each step follow.

### 2.2 Launch (Observed)

- UI entry: `PreviewControl` button in ReposBar (`PreviewControl.tsx:1-33`); also launched from
  Rebase & Merge for job-branch / merge-result previews (`jobs.rs:229-243`: `previewSourceJobId`,
  `previewMergeResult`, `previewPreflightFingerprint`).
- It commits dirty CRDT drafts first (`PreviewControl.tsx:20-22,873`), then `useCreateJob` ->
  `POST /api/v1/workspaces/:id/jobs` with `jobType: "preview"`, `previewMode: unmerged|merged`
  (`jobs.rs:214-216`), optional `previewTtlSecs` (`:219`), optional `previewProfile` (`:224-226`).
- Server validation: `job_service.rs:1525-1600 validate_preview_workspace_config` reads
  `.g5/preview_profiles/<name>.yaml` (top-level `command`/`port`) or the `.g5/workspace.yaml` `preview:`
  block at the committed ref, pinned to the routed repo; a multi-repo workspace without explicit routing
  fails closed.
- Rows: job + one bootstrap task (`job_service.rs:5230-5245`), plus a `job_preview_sessions` row
  `(mode, profile, status='pending', expires_at = now + ttl)`; "An omitted TTL defaults to 1h ... a preview
  microVM must not pin its worker host past the host's own 24h credential lifetime" (`:5278-5330`).
- Schema: `095_preview_job_type.sql` adds `preview` to the job/task CHECKs; `096_job_preview_sessions.sql`
  creates the table (`mode, status, guest_port, host_port, endpoint_url, expires_at, started_at, stopped_at`,
  org RLS); `097` adds `host_id`; `107` adds `profile`; `236` adds `boot_generation`.
- Preview is in `INTERACTIVE_TYPES` (`job_service.rs:299`).

### 2.3 Where the app process runs (Observed)

- Assignment: `task_service.rs:13451-13464 mint_preview_boot_generation` stamps a fresh UUID per claim; the
  assignment carries `preview_mode`, `preview_profile`, `workspace_git_url`, `branch_name`, `base_commit`.
- Admission on the host supervisor (`g5d host-up`): `supervisor.rs:8807 accept_preview_assignment`
  reserves a slot in a **separate `preview_vms` registry** ("held in the supervisor's `preview_vms`
  registry separate from the one-shot `records` map", `supervisor.rs:2795-2800`), enforcing
  `G5D_HOST_MAX_PREVIEWS` clamped to `G5D_HOST_MAX_MICROVMS` plus the shared VM budget
  (`supervisor.rs:8862-8905`; `host_up.rs:362-370,478`). Over capacity -> `LaunchFailed` is reported so
  the task fails instead of stranding.
- Boot (`supervisor.rs:11905 drive_preview_boot`):
  1. materialize `assignment.workspace_git_url` at `branch_name` (unmerged) or `base_commit` (merged) into a
     per-task host directory through `g5d_foundation::workspace::Materializer` (`supervisor.rs:8979-8987,
     2556-2570`);
  2. read the guest port from the materialized config (`:12030`);
  3. lease a host port from the IANA ephemeral range 49152-65535 (`port_map.rs:1-34`; `lease_bindable`,
     `supervisor.rs:12051`);
  4. stamp `port_maps = [(host_port, guest_port)]` (`:12074`), optionally a second lease plus a 32-byte
     token for chat-attach (`:12088`);
  5. `VmLauncher::start` (boot-then-serve; the handle is kept in `PreviewVmRecord` for the VM's lifetime).
- Run spec (`assignment.rs:662-741 build_preview_run_spec`):
  - image = host `base_image` (`G5D_HOST_BASE_IMAGE`) or the workspace's committed `.g5/workspace.yaml`
    `base_image:` override (documented in `infra/self-hosted/README.md`, "Task microVM images");
  - cmd `/opt/g5/bin/g5d preview-serve`;
  - virtio-fs mount of the host workspace dir at guest `/work`; read-only mount of the runtime bin dir;
    optional scratch disk at `/var/lib/docker` (`G5D_SCRATCH_DISK_MIB`);
  - env only `G5D_PREVIEW_WORK_DIR`, `PATH`, `LD_LIBRARY_PATH`, `G5D_PREVIEW_PROFILE`; **no g5-api credential**
    (`assignment.rs:754-775`).
- In-VM (`preview_serve.rs:218-300 execute`): loads the profile or `preview:` block, seeds the npm cache
  (`:301-355`), spawns `g5d preview-attach-listener` if `G5D_PREVIEW_ATTACH_PORT/TOKEN` were stamped
  (`:100-160`, fail-open), scrubs `G5D_PREVIEW_ATTACH*`, `G5D_AGENT_*`, `ANTHROPIC_BASE_URL`,
  `LD_LIBRARY_PATH` from the customer env (`:200-215`), applies `preview.env`, forces `PORT=<preview.port>`
  last, and runs `/bin/sh -c <command>` with inherited stdio.
- Guest networking: TSI by default, or virtio-net + a per-VM `passt` when `G5D_VM_NET=passt`
  (`passt.rs:22-32`); all hosted envs set `worker_host_vm_net = "passt"` (tfvars) and
  `G5_WORKER_VM_NET: "passt"` (helm values). The host reaches the guest listener at
  `(vmm_loopback_addr, host_port)`: Linux uses the PID-derived `127.128.0.0/10` address
  (`vmm_loopback.rs:33-47`), macOS uses `127.0.0.1` (`:54-66`).
- `loopback_forward.rs:1-25` is the symmetric inbound forward for the identity-bearing worker VM and is
  explicitly **not** used for previews (`assignment.rs:715-717`, `guest_loopback_forwards: vec![]`).
  `g5d-worker/src/port_scan.rs:1-7` scans `/proc/net/tcp` and emits `port_open` events; its production
  caller was not verified in this survey.
- Observation: the host reports `TaskObservation { port_map, host_id, preview_boot_generation }`;
  `task_service.rs:15650-15720` projects into `job_preview_sessions` (`host_id, host_port, guest_port,
  status='serving', endpoint_url = <base>||job_id||'/'`) **only when `boot_generation` matches**
  (migration `236`); an HTTP readiness probe emits `host_microvm_ready`, surfaced as `ready`
  (`job_service.rs:7363`, `jobs.rs:488-490`).

### 2.4 How the browser reaches it (Observed)

- Session state: `GET /api/v1/jobs/:id/preview` (`jobs.rs:1071,2570-2578`; `job_service.rs:7232-7256`),
  polled by `usePreviewSession` until `stoppedAt` (`useJobs.ts:314-332`).
- Mint: `POST /api/v1/preview/session/:jobId` (`preview.rs:268-282,312-380`) re-authorizes through
  `scoped_db_for_job`, sets a job-bound `g5_preview` ticket cookie (httpOnly, `Path=/api/v1/preview/<job>`,
  `SameSite=Strict`, `Max-Age=PREVIEW_TICKET_LIFETIME_SECS`, `:383-396`), and returns `previewUrl`:
  `https://<job>.<suffix>/?__g5_preview_ticket=<single-use handoff>` when `G5_PREVIEW_HOST_SUFFIX` is set,
  else `/api/v1/preview/<job>/`.
- UI opens it in a new tab (`PreviewControl.tsx:206-219`) or embeds it in an iframe center tab
  (`PreviewBrowserView.tsx:1-19,77`; sandboxed only in cross-origin mode; URL bar restricted to the preview
  prefix, `:300-303`). `BottomDrawer.tsx:167,425-458` also mints for preview jobs.
- Proxy: `any /api/v1/preview/:jobId[/*path]` (`preview.rs:283-288`):
  1. `resolve_preview_target` reads `(host_id, host_port, expires_at)` from the latest non-stopped session
     row under org RLS (`job_service.rs:7430-7495`; 503 while booting);
  2. `state.preview_tunnels.get(host_id)` (`preview.rs:440-447`; registry field at `services/mod.rs:364`);
  3. opens a mux `Channel::Preview` stream whose first 2 bytes are `host_port`;
  4. `relay_http` (hyper client over a duplex bridged to the mux, `preview.rs:95-130,813+`) or raw
     `relay_websocket` passthrough (`:1218`).
  - Policies: platform `Authorization` stripped, app's own passes ([D9], `:462-466`); sliding ticket renewal
    past half-life (`:643-676`); `Origin: null` dropped only for attested same-origin navigations
    (`:606-630`); anti-framing normalized to allow the workspace origin in subdomain mode (`:479-490`);
    legacy GET/HEAD redirected to the subdomain origin when the suffix is configured (`:424-436,730-760`).
  - Limits: request body 64 MiB, response head 256 KiB, 30 s time-to-first-byte (`:58-77`).
- Tunnel: the worker host dials `wss://<api>/api/internal/hosts/<host_id>/tunnel` with its host JWT
  (`tunnel.rs:1-16,106-125,165-190`); g5-api accepts and registers the mux (`internal.rs:3520-3555
  host_tunnel_attach`; path host id must equal the JWT's). Per stream the host reads the port header, asks
  `PreviewEndpointResolver` (implemented over live `preview_vms`, `supervisor.rs:2895`; "so a compromised
  control plane cannot choose ... an arbitrary loopback endpoint", `tunnel.rs:53-58`), dials the VMM loopback
  endpoint and relays bytes (`tunnel.rs:209-240`). Keepalive ping 15 s, liveness 45 s, reconnect backoff
  1-30 s (`tunnel.rs:41-50`).
- Subdomain mode: `middleware/preview_host.rs:1-21` translates `Host: <job>.<suffix>` requests to the legacy
  path and exchanges the single-use handoff ticket for the `__Host-g5_preview` cookie. The consumed-jti store
  is **process-local**, which is why helm refuses `previewWildcard.enabled` with `api.replicaCount > 1`
  (`deployment-api.yaml:12-13`) and injects `G5_PREVIEW_HOST_SUFFIX` (`:134-139`); the ingress adds a
  `*.<hostSuffix>` all-paths -> g5-api rule and a Certificate Manager certmap (`ingress.yaml:17-27,60-65`);
  Terraform provisions the wildcard DNS/TLS from `preview_subdomain_bases` (`main.tf` networking module;
  tfvars `preview.dev.g5labs.ai`, `preview.staging.g5labs.ai`, `preview.g5labs.ai`).
- Edge proxy: `infra/docker/Caddyfile:37-52` forwards `/api/*`, `/git/*`, `/ws/*`, `/auth/*` to
  `g5-api:7500`; everything else is the SPA. `Dockerfile.edge` builds the Vite app and copies Caddy
  (`:99 CMD`). On GKE the ingress routes `/api/*`, `/git/*`, `/ws`, `/api/v1/yjs/*`,
  `/api/internal/workers/*` **directly** to g5-api and `/*` to edge (`values.yaml` ingress paths).
  Inference: in GKE the edge never sees preview traffic; in compose/self-hosted it proxies it.
- Auth summary: normal Bearer only for the mint call; afterwards a job-scoped preview ticket cookie
  (`token_use = "preview"`, `aud = preview_ticket_audience(job)`, `preview.rs:610-620`), 30-minute lifetime
  with renewal, plus org-RLS re-resolution on every request.

### 2.5 Profiles (Observed)

- `preview_profiles.rs:1-14`: named profiles are committed files `.g5/preview_profiles/<name>.yaml`
  (`command`, `port`, `working_dir`, `env`, `description`).
- Routes: `GET /api/v1/branches/:branchId/preview-profiles[?mode=merged]`; `POST ...` commits the YAML
  through `DraftService::commit_draft` (`:206-215`); `GET .../preview-applications` discovers default +
  named configs at the exact ref a launch would boot (`:150-170`).
- Persisted in `job_preview_sessions.profile` (`107`), delivered in-VM as `G5D_PREVIEW_PROFILE`
  (`assignment.rs:754-775`, `preview_serve.rs:36-41`).
- Frontend: `usePreviewProfiles`, `usePreviewApplications`, `useCommitPreviewProfile` (`useJobs.ts:469-578`);
  the launcher's "New profile" form offers "Commit" / "Commit and preview" (`PreviewControl.tsx:15-19`).
- This repo's own `.g5/workspace.yaml` contains only a `golden.docker` build recipe, no `preview:` block.

### 2.6 "Preview attach to chat" (Observed)

- Meaning: start a chat session whose Claude Code agent runs **inside the already-serving preview VM**,
  editing the same `/work` tree the app serves, so the user can iterate on the running app.
- Frontend: `PreviewBrowserView.onAttachChat` green header button is "the ONLY attach entry point"
  (`PreviewBrowserView.tsx:41-49`); `useStartChatMessage` sends `attachPreviewJobId` on
  `POST /workspaces/:id/sessions/start` (`useChat.ts:583-632`); ChatPanel badges attached sessions
  (`ChatPanel.tsx:7361,7420`); TreeNode suppresses the row Stop for `job.attachPreview` (`TreeNode.tsx:2647`).
- Admission: `chat_service.rs:2084 admit_preview_attach` requires the latest session row with
  `attach_supported = true` and (per the comment at `:2049-2055`) a `claude_code` session provider; it sets
  `chat_sessions.attach_preview_task_id` (migration `233`: tenant-qualified FK `(org_id, task_id) -> tasks`,
  `ON DELETE SET NULL`; partial index `234`; validate `235`). `attach_supported` is stamped by the host start
  observation and reset to false when the probe fails (`237` one-time reset;
  `supervisor.rs:10940-10975 revoke_preview_attach_supported`).
- Host drive: the chat task is claimed by the **host**, which runs `drive_preview_attach_once`
  (`supervisor.rs:10982`): fenced `preview_vms` lookup -> connect to `(vmm_loopback_addr, P2)` with the
  per-VM token -> send the `PreviewAttachSessionConfig` JSON frame (`preview_attach_wire.rs:52-70`, cap
  256 KiB) -> host-side Claude drive with `llm_proxy` and session-store proxy, host `CrdtPeer` scoped to the
  chat's branch draft. The in-guest listener lazily spawns the agent sidecar; relay sockets live in
  `/run/g5-attach/{llm,session-store}.sock` (`preview_attach_wire.rs:37-48`). Timeline entries
  `preview_attach_started/exited` (`237/238`). One attach drive per preview (`supervisor.rs:2860-2866`).
- Availability: attach is provisioned only when the host has agent-carrier deps (VM-capable hosts get them
  by default; `supervisor.rs:8960-8975`).

### 2.7 Teardown and capacity (Observed)

- Stop = `POST /api/v1/jobs/:id/cancel` from `PreviewControl` (`useCancelJob`, `PreviewControl.tsx:876`)
  -> task cancelled -> host `cancelled_task_ids` drives `teardown_previews` (`supervisor.rs:1094,1294,9217`):
  kill VM, drop `PortLease`s (RAII, `port_map.rs:16-20`), wait for any attach drive's durability barrier
  (`supervisor.rs:2853-2859`), clean the workspace dir. `stopped_at` is stamped server-side.
- TTL: `maintenance/preview_reaper.rs:1-11` cancels preview jobs whose `expires_at` passed "exactly as a
  manual Stop would" (spawned from `maintenance/mod.rs:478-489`).
- Host lifecycle: hosts rotate on a 24 h JWT and previews are TTL-bounded so they do not outlive it
  (`job_service.rs:5285-5292`); a preview boot that loses its record (drain/restart) fails closed
  (`supervisor.rs:11873-11902`).
- Capacity: previews share the host's VM budget with one-shot tasks; `GET /workspaces/:id/workers/capacity`
  reports occupants with `kind: "preview"` and deep-links to Stop (`useWorkers.ts:40-66`,
  `capacity/previewLinks.ts`). Defaults: local macOS host `G5D_HOST_MAX_PREVIEWS=0`
  (`scripts/worker-host-env.sh:216`) so previews are off in the native dev loop; external Docker worker host
  `1` (`infra/self-hosted/scripts/g5-workerhost.sh:459-460`); Azure canary `1`
  (`self-hosted-azure-install.yml:472-473`). Preview routing requires the host id to be the `worker_hosts`
  UUID on the host-JWT path (`host_up.rs:205-230`).
- CI: the deploy pipeline runs `preview-isolation-canary` and `canary-preview-screenshots` steps
  (`infra/ci/cloudbuild-deploy.yaml`) and there is a separate `cloudbuild-canary-preview-cli.yaml`.

---

## 3. Infra / deployment model

### 3.1 GCP SaaS: Terraform + Helm (Observed)

`infra/terraform/main.tf` composes these modules (resource types from a grep of `resource "` across
`infra/terraform/modules/*`; counts include both count/for_each variants):

| Module | Resources | Purpose |
|---|---|---|
| `kms` | key ring, 5 crypto keys + IAM, service identities | CMEK for SQL/GCS/GKE/Filestore/Secrets |
| `networking` | `google_compute_network`, 2 subnets (system `10.0.0.0/20`, worker `10.0.16.0/20`), router + Cloud NAT, firewalls, global static IP, DNS records in admin project `g5-admin-1ccc`, Certificate Manager cert/map/entry/dns_authorization for `preview_subdomain_bases` (`main.tf:150-153`) | VPC, ingress IP, wildcard preview TLS |
| `gke` | `google_container_cluster` + one system node pool, GKE Hub features | control plane only; "the in-cluster worker plane is retired - infra ontology D17" (`main.tf:157-159`); `removed {}` blocks forget worker-cluster ESO releases (`:198-210,434-440`) |
| `cloudsql` | PG17 instance (private IP, pgvector), db, sql users, random passwords | `db-custom-8-32768` prod/dev; `REGIONAL` in staging/prod |
| `redis` | Memorystore Redis 7 (`STANDARD_HA` prod), TLS | "job queues and ephemeral state" |
| `gcs` | bucket with lifecycle | caches/artifacts |
| `filestore` | Filestore instance, firewall, optional PV/PVC | NFS for git repos (`G5_STORAGE_PROVIDER=filestore`, mount `/mnt/g5-repos`, `values.yaml:79-82`); active instance `g5-*-repos-ssd` 2560Gi |
| `iam` | SAs `g5-<env>-api`, `g5-<env>-worker`, WI bindings; `enable_api_managed_worker_hosts` grants compute roles to the API SA (`main.tf:291-296`) | |
| `secrets`, `secret-rotation`, `external-secrets` | Secret Manager secrets (JWT/OAuth/LLM keys seeded by `scripts/bootstrap-secrets.sh`, `main.tf:400-402`), Pub/Sub rotation topic, Cloud Function + Scheduler, ESO helm release | |
| `cloudbuild` | Artifact Registry, 37 `google_cloudbuild_trigger` (ci_rust, ci_e2e, deploy, promote_staging, promote_prod, release_train, deploy_cutoff_notify, release, provision_license_key, canary_preview_cli, filestore_migration, terraform_plan/apply x3, libkrunfw_fat, packer worker, ...), 2 private worker pools (C3 for nested virt), Slack notifiers, license infra | CI/CD |
| `observability` | log metrics, alert policies, uptime check, BigQuery sink | |
| `worker-host` (used twice: `worker_host`, `tungsten_worker_host`) | SA + secret access, `google_compute_region_instance_template` (nested virt, no external IP, `g5-worker` tag, startup-script), `google_compute_region_instance_group_manager` (regional MIG, `distribution_policy_target_shape = ANY`, PROACTIVE REPLACE), daily Cloud Scheduler `applyUpdatesToInstances` | standing worker capacity via a `g5w_` registration token (`modules/worker-host/main.tf:1-14,71-88,163-200,240-276`) |
| `bootstrap/` | folder, 2 projects, org policies, DNS zone, AR, state bucket | one-time |

Environments (`infra/terraform/environments/{dev,staging,prod}.tfvars`):
- projects `g5-{dev,staging,prod}-1ccc`, region `us-central1`, external URLs `https://dev.g5labs.ai`,
  `https://staging.g5labs.ai`, `https://g5labs.ai`;
- all three: `enable_api_managed_worker_hosts = true`, `enable_worker_host_instance = true` **with
  `worker_host_target_size = 0`**, `worker_host_rotation_enabled = true`, `worker_host_vm_net = "passt"`,
  `worker_host_scratch_disk_mib = "65536"`;
- dev also: `worker_host_golden_store_enable = "1"`, `enable_worker_image_pipeline = true`,
  `enable_self_hosted_release_infra = true`; staging/prod consume dev's image family
  (`worker_host_image_project = "g5-dev-1ccc"`).
- Inference: the Terraform MIG is dormant everywhere (target 0); API-managed hosts are the live worker plane.

Helm (`infra/helm/g5`):
- `topology.mode: control-plane` in every hosted overlay (`values-control-plane.yaml`, `values-dev.yaml`),
  which leaves `deployment-worker.yaml`/`hpa-worker.yaml` unrendered (`deployment-worker.yaml:5-7`).
- Rendered: `deployment-api` (1 replica; 14-16 CPU / 4-20 GiB in dev and prod), `deployment-indexer`
  (`G5_ROLE=indexer`), `deployment-edge` (Caddy, 2 replicas, `G5_MAINTENANCE_MODE: "true"` even in prod, so
  the public `/landing` shows coming-soon), `job-migrate` (pre-install hook running `g5-db-migrate`),
  `cronjob-host-report`, GCE `ingress` + `managed-certificate` (+ optional certmap for `previewWildcard`),
  `pvc-repos`, `backend-config` (session-affinity cookie), `network-policy`, optional `istio-mtls`.
- Secrets via ESO (`api.secrets.externalSecretsEnabled: true`; key list at `values.yaml:86-97`).

### 3.2 What a "worker host" is and how it is provisioned and scaled (Observed unless marked)

Definition:
- A worker host is a GCE VM (allowlisted nested-virt families `n2/c2/c3-standard-*`,
  `routes/worker_hosts.rs:57-84`) booting the signed `g5-worker` image and running the `g5d host-up`
  supervisor under systemd, which boots one libkrun microVM per task from a customer OCI `base_image`
  with the G5 runtime mount-injected at `/opt/g5/bin` (`infra/packer/g5-worker/README.md` first table and
  note; `scripts/60-layout-dirs.sh`). Default base image `buildpack-deps:resolute`
  (`cloud/startup_script.rs:45-60`); Tungsten hosts use the `g5d-tungsten` image (JDK + Maven + in-guest
  dockerd, `main.tf:544-553`).
- Image contents (Packer `g5-worker.pkr.hcl:1-30`, scripts 10-90): Ubuntu 24.04 autoinstall with XFS
  reflink root, KVM modules, libkrun + libkrunfw built by `scripts/build-libkrun.sh`, `g5d` built
  `--release --locked`, Claude Code native binary, Codex (node wrapper), node/npm/pnpm, `gh`,
  `gcloud`+`kubectl`, cargo/nextest/just, pinned `passt` from Ubuntu 26.04, `/var/g5d-data/{runtime,cache,
  lib/workspaces}` layout, `g5d-host-up.service`, sysctl tuning. Built by `cloudbuild-packer-worker.yaml`
  (qemu build -> boot/libkrun gate tests -> GCS/GAR push -> cosign sign/attest -> GCE image import -> dev rollout).

Path A - API-managed (the live path):
- `POST /api/v1/orgs/:orgId/worker-hosts` (`WorkersAdmin`; UI `AddWorkerHostDialog`) inserts a
  `worker_hosts` row, then after commit `bring_up_cloud_resources` (`cloud/bringup.rs`) calls
  `GcpCloudProvider` (REST via reqwest, `cloud/gcp.rs:1-17`): `ensure_vpc_for_org` (per-org VPC with a fixed
  `/20` per region, `gcp.rs:60-80`), `provision_instance` (`gcp.rs:1186-1311`; `sourceImage` resolved from
  `G5_WORKER_IMAGE`, `startup-script` metadata, nested virt, boot disk `G5_WORKER_HOST_BOOT_DISK_GB` default 150).
- The startup script (`cloud/startup_script.rs:163 build_startup_script`) writes `/etc/g5/host-up.env` with a
  per-host 24 h `host_jwt`, `G5D_ISOLATION_MODE=microvm`, `G5D_HOST_BASE_IMAGE`, `G5D_HOST_MAX_MICROVMS`
  (= `concurrentJobs` or vCPUs/4, `bringup.rs:32-35`), `G5D_LLM_PROXY=1`, `G5D_VM_NET`,
  `G5D_SCRATCH_DISK_MIB`, golden knobs, then restarts `g5d-host-up.service`.
- Instance name `g5-host-<org-slug>-<uuid>` (`cloud/mod.rs:63-72`).
- Background loops (env-gated in helm values): `reconciler` (30 s; provisioning -> ready on heartbeat; orphan
  pass, `cloud/reconciler.rs:1-30`), `rotation` (provision-then-drain replacement when idle + image-stale or
  before the JWT cliff, `cloud/rotation.rs:1-40`; `G5_HOST_ROTATION_MIN_IDLE_SECS` 300 dev / 3600 prod),
  `stale_host` (never-registered reap, silent-ready reap, cloud-orphan sweep, zone-capacity retry with
  fallback regions, `cloud/stale_host.rs:1-35`).
- Deallocate: `DELETE /orgs/:id/worker-hosts/:hostId`.

Path B - Terraform MIG:
- Boots the same image with a `g5w_` registration token from Secret Manager
  (`templates/host-bootstrap.sh.tpl:56-97`), daily recreate; `target_size = 0` in all envs.

Path C - external / self-hosted worker host (Linux + Docker + KVM):
- `infra/self-hosted/install.sh --worker-host --worker-registration-token ...` -> `scripts/g5-workerhost.sh`
  fetches the licensed runtime image and runs `docker run --device=/dev/kvm ... --entrypoint
  /opt/g5-runtime/bin/g5d ... host-up` with `G5D_ISOLATION_MODE=microvm`, `G5D_HOST_MAX_MICROVMS` default 1,
  `G5D_HOST_MAX_PREVIEWS` default 1 (`g5-workerhost.sh:434-480`).
- The bundle's own compose `g5d-worker` service (default `G5D_ISOLATION_MODE=microvm`, `/dev/kvm` via
  `compose.kvm.yml`) is stopped once an external host exists (Azure workflow `:498`).

Path D - local macOS dev host:
- `just worker-host-setup` / `just worker-host` run a **native** `g5d host-up` on Apple Silicon + HVF with a
  G5-patched libkrun (`Justfile:333-431`, "macOS-only (libkrun HVF)"; `scripts/worker-host-env.sh:1-14,
  74-81,206-216`), Linux guest runtime staged through Docker, `G5D_HOST_MAX_PREVIEWS=0`.
- Repo-tracked `g5d-darwin-aarch64` / `g5d-linux-aarch64` binaries under `infra/local-setup/` feed "Local
  Setup bundle generation" (`infra/local-setup/README.md`).
- `infra/self-hosted/install.sh` has no macOS branch (grep `darwin|Darwin` -> none; the registry token
  request hard-codes `"platform": "linux/amd64"`, `install.sh:157`).

Scaling:
- Worker hosts are added/removed per org by admins (UI or API); the loops above replace and rotate hosts but
  do not size the fleet from queue depth.
- `SettingsForm` exposes per-workspace "concurrent tasks, cloud workers, idle timeout" (`SettingsForm.tsx:1-6`;
  server defaults `G5_DEFAULT_MAX_CONCURRENT_TASKS=4`, `G5_DEFAULT_MAX_CLOUD_WORKERS=8`, `config.rs:380-386`).
- Inference: there is no autoscaler for the API-managed fleet; capacity = provisioned hosts x
  `G5D_HOST_MAX_MICROVMS`.

### 3.3 Dev stack, CI/CD, config surface (Observed)

Dev stack:
- `infra/docker/docker-compose.dev.yml`: `postgres` (pgvector pg17, host port 5433), `redis` (6381),
  `oauth-stub` (9876), `db-migrate`, `g5-api` (7500, `G5_E2E_AUTH=true` dev login), `g5-api-indexer`,
  `g5d-worker` (profile `demo`, `G5D_ISOLATION_MODE: process`, non-root uid 999, provider keys from `.env`),
  `edge` (3000, `G5_MAINTENANCE_MODE: "true"`).
- `docker-compose.yml` is a prod-like reference (g5d-worker with `/dev/kvm`); `docker-compose.test.yml` is
  the CI stack.
- `.env.example` documents `DATABASE_URL`/`TEST_DATABASE_URL`, port overrides, `G5_SCM_ENCRYPTION_KEY`,
  LLM keys, `G5_CLAUDE_CODE_*` profile and model pins, `G5D_LLM_PROVIDER/MODEL`, `G5D_ORG_ID`.

GitHub Actions:
- `ci.yml`, `e2e.yml`, `release.yml`, `base-image.yml` are **disabled** ("moved to Cloud Build", file headers).
- Active: `pages.yml` (docs site), `self-hosted-install.yml` (bundle contract + smoke on push),
  `self-hosted-release-assets.yml` (manual publish of installer assets to `G5Labs/Releases`),
  `self-hosted-azure-install.yml` (real Azure canary on push to `main`: `az vm create` control VM + 1
  nested-virt `Standard_D4s_v3` worker VM, ACR digest-pinned task image, Entra login, license key minted
  through the hosted admin API), `slack-notify.yml`.

Cloud Build (`infra/ci`):
- `cloudbuild-ci-rust.yaml`: fmt / clippy / deny / sqlx-check / nextest / integration / g5d integration /
  doctests / KVM+libkrun isolation tests / release artifacts.
- `cloudbuild-ci-e2e.yaml`: frontend lint/build/test, full stack incl. workers + oauth stub, Rust e2e, API
  smoke, Playwright shards incl. provider-agent lane.
- `cloudbuild-deploy.yaml`: builds `api`, `g5d`, `edge`, customer runtime images (`g5d-tungsten`,
  `g5d-pairsoft-*`, `g5d-pnc-*`, `g5d-g5dev`); `helm-deploy`; `verify-deploy`; `smoke-test`;
  `tungsten-microvm-canary`; `preview-isolation-canary`; screenshots; `trigger-next` promotion
  dev -> staging -> prod.
- `cloudbuild-packer-worker.yaml`, `cloudbuild-release.yaml` (self-hosted image release + smoke),
  `cloudbuild-terraform.yaml` (plan/apply per env), `cloudbuild-canary-preview-cli.yaml`,
  `cloudbuild-license-provision.yaml`, `cloudbuild-filestore-migration.yaml`, `cloudbuild-release-train.yaml`
  (weekly).

API config surface (`backend/g5-api-foundation/src/config.rs:62-586`, clap env):
- server: `G5_HOST/PORT/LOG_LEVEL/LOG_FORMAT`, `G5_EXTERNAL_URL`, `G5_GIT_EXTERNAL_URL`,
  `G5_PREVIEW_HOST_SUFFIX`, `G5_CORS_ALLOWED_ORIGINS`, `G5_COOKIE_SECURE`, rate limit;
- data: three DB pools (`G5_DATABASE_URL`, `G5_AUTHZ_DATABASE_URL`, `G5_TENANT_DATABASE_URL` + pool sizes),
  `G5_REDIS_URL` (+CA), storage (`G5_REPOS_BASE_DIR`, `G5_STORAGE_PROVIDER/BASE_DIR/WORKTREE_DIR/
  MOUNT_POINT/TENANT_PREFIX`), `G5_BLOB_SIGNING_KEY`;
- auth: JWT (`G5_JWT_SECRET/PUBLIC_KEY/PRIVATE_KEY/ISSUER/AUDIENCE/TTLs`), OAuth for
  github/google/gitlab/microsoft (+ overridable authorize/token/userinfo URLs, Entra role map), SCM
  (`G5_SCM_*`, `G5_SCM_ENCRYPTION_KEY`), `G5_AUTH_ALLOWED_EMAILS`, `G5_OAUTH_ENABLED_PROVIDERS`;
- background: scheduler weights, CRDT compaction, invitation GC, stale drafts, repo GC, remote tracking,
  base auto-pull, worker defaults, `G5_CHAT_RESPONSE_TIMEOUT_SECS`;
- feature flags: `G5_ISSUE_TRACKING_ENABLED`, `G5_MAKE_REMOTE_ENABLED`, `G5_ALLOW_LOCAL_GIT_REMOTES`,
  `G5_PLATFORM_ISSUE_*`, `G5_GOLDEN_BUILDER_TRIGGERS_ENABLE`;
- self-hosted/license: `G5_LICENSE_ADMIN_TOKEN_SHA256`, `G5_SELF_HOSTED_REGISTRY_*`,
  `G5_SELF_HOSTED_DEFAULT_ORG_SLUG`;
- direct `env::var` reads elsewhere: `G5_WORKER_IMAGE`, `GCP_PROJECT_ID`, `G5_GCP_ZONE_SUFFIX`,
  `G5_WORKER_ALLOW_SSH`, `G5_WORKER_VM_NET`, `G5_WORKER_SCRATCH_DISK_MIB`, `G5_HOST_ROTATION_*`,
  `G5_STALE_HOST_DETECTOR_ENABLED`, `G5_PROVISION_RETRY_ENABLED`, `G5_UI_V3_ENABLED`, `G5_CODEX_UI_ENABLED`,
  `G5_ENABLE_TOP_PANEL`, `G5_ENABLE_PROVIDER_ACCOUNTS`, `G5_TASK_CRDT_PERSISTENCE_ENABLED`,
  `G5_FORK_CHAT_ENABLED`, `G5_CRDT_*`, `G5_PROMOTED_BASE_IMAGES`.

g5d config surface (grep of `env::var("G5D_...")` across `backend/g5d*/src`): `G5D_API_URL`,
`G5D_REGISTRATION_TOKEN`, `G5D_ORG_ID`, `G5D_ISOLATION_MODE`, `G5D_HOST_BASE_IMAGE`,
`G5D_HOST_MAX_MICROVMS/PREVIEWS/CPUS_PER_VM/MEM_MIB_PER_VM`, `G5D_LLM_PROXY`, `G5D_VM_NET`,
`G5D_SCRATCH_DISK_MIB`, `G5D_OCI_CACHE_DIR`, `G5D_ROOTFS_STATE_DIR`, `G5D_HOST_STATE_DIR/CACHE_ROOT/
WORKSPACES_ROOT`, `G5D_GIT_HTTP_ALLOW_HOSTS`, `G5D_GUEST_HOST_ALIASES`, `G5D_STICKY_RUNTIME`,
`G5D_CAPABILITIES`, `G5D_MAX_CONCURRENT_TASKS`, `G5D_PREVIEW_NPM_CACHE_ROOT`.

### 3.4 Deployment targets (Observed)

1. GCP SaaS (dev/staging/prod): GKE control plane (api/indexer/edge), Cloud SQL, Memorystore, Filestore,
   API-provisioned GCE worker hosts, wildcard preview subdomains.
2. Self-hosted Linux (customer bundle under `/opt/g5`, `infra/self-hosted/README.md`): Docker Compose control
   plane (postgres, redis, db-migrate, g5-api, g5-api-indexer, g5d-worker, edge; `G5_STORAGE_PROVIDER=local`),
   optional TLS overlay, `just setup/up/logs/down/backup/restore/upgrade`, license key -> registry token ->
   private images; external worker hosts via `install.sh --worker-host`.
3. Azure canary: the same bundle on Ubuntu VMs with nested virt, exercised by the GitHub workflow; the
   dead-code retirement note says its config was switched to ordinary MicroVm and "has not been deployed
   to Azure" (`docs/dead-code-retirement.md`, Execution boundaries).
4. Local dev: compose stack + either the compose `process`-mode worker (`G5_DEV_WORKER=process just dev-up`)
   or the native macOS libkrun host (`just worker-host*`).

---

## 4. Coupling

- Preview <-> job/task machinery. A preview is a job + bootstrap task + `job_preview_sessions` row, claimed
  through the same host poll/claim/observation loop as agent tasks, fenced by `boot_generation`, cancelled
  through job cancel, reaped by a maintenance loop. Moving the preview onto the workspace VM removes the job,
  the session table, the port-lease/tunnel routing table and the separate `preview_vms` registry, but every
  consumer (`usePreviewSession`, `BottomDrawer`, capacity indicator, chat-attach admission) keys on
  `jobId`/`task_id`.
- Preview <-> committed-ref semantics. Previews boot from a fresh materialization of a committed ref
  (`drive_preview_boot`), never from the live draft; the UI auto-commits first. Under goal 7 (explicit Save,
  no CRDT) "commit before preview" becomes "save", and a preview on the workspace VM could serve the working
  tree directly.
- Preview <-> reverse tunnel <-> single API replica. The tunnel registry (`preview_tunnels`), the handoff jti
  store and the CRDT/local event bus are process-local, forcing `api.replicaCount: 1` (`values.yaml:41-44`,
  `deployment-api.yaml:12-13`). Any "one VM per workspace" design that keeps browser -> VM traffic flowing
  through g5-api inherits this unless routing is externalized (shared registry or a separate ingress).
- Chat attach <-> preview VM <-> host drive. Attach couples a chat session
  (`chat_sessions.attach_preview_task_id`) to a preview VM's in-guest listener and to the host supervisor's
  agent-carrier deps. With tasks as local processes on the workspace VM (goal 1), "attach" collapses to
  "run the agent in the same VM as the app", which becomes the default.
- Frontend <-> CRDT. `SpecEditor`, `CodeEditor`, drafts, comments (line anchors), question docs, explainer
  merge, and the `draft.*` / `yjs` WS all assume Yjs documents with server authority (`crdt/authority.rs`).
  Dropping CRDT (goal 2) touches essentially all editing surfaces and the `draft.*` event family.
- Frontend <-> worker/sticky runtime. ChatPanel and TopBar surface worker pinning, capacity, `workers` rows;
  sticky-runtime strings appear in 16 frontend files (section 1.5). One VM per workspace with concurrent chats
  (goals 1, 8) makes "which worker / sticky runtime holds my chat" moot.
- Workspace <-> multi-repo branches. `WorkspaceDetail.branches[]` + `docsRepoId` and the per-repo
  remote-tracking / auto-pull / publish endpoints (`useRemoteTracking.ts`) assume N repos per workspace;
  "workspace 1-1 with a git branch" needs a decision on multi-repo.
- Governance at the git boundary already exists in pieces: approvals attach to workspaces and gate Finalize /
  PR merge (`PrepareToShipModal.tsx:1-13`, `WorkspaceApprovalsPageContent.tsx`), PR create/merge through
  `workspace_pull_request_provider_lifecycle.rs`, push through `push_to_remote`. They are interleaved with
  job-merge (`/jobs/:id/merge`), rebase-merge change graphs and remote-tracking merge runs.
- Infra <-> host fleet. Everything in `cloud/` (provider trait, bring-up, reconciler, rotation, stale detector,
  startup script), the `worker_hosts` tables/UI, the packer image and the tunnel exist because tasks run on a
  shared pool of hosts the control plane must provision, rotate and reach. One VM per workspace is a different
  fleet shape (many small long-lived VMs keyed by workspace) and changes what "worker host" means.

## 5. Simplification candidates

### 5.1 Frontend

Drop / already dormant (low risk, evidence above):
- `/ide` `IDEPluginView` (dev-only mockup); `data/mock.ts` + `VITE_MOCK` interceptor if not needed for demos;
  `routes.waitlist` + `routes/waitlist.rs`; `view_create` job-type references (server already rejects).
- Intent-log viewer (`intent-log/*`, `useIntentLog`): build-flag-gated off in every shipped edge image; keep
  only if the audit trail is a product requirement.
- Codex-specific surfaces (`CodexSubscriptionAuthPrompt`, `codexModels`, `provider-control`): off in prod
  (`G5_CODEX_UI_ENABLED: "false"`).
- `WorkspaceCapacityIndicator`, `previewLinks.ts`, `WorkersDropdown`/`ConnectedWorkerPill`, worker pinning,
  `SettingsForm` "cloud workers / idle timeout": these describe a shared worker pool; with one VM per
  workspace they reduce to "VM running / stopped / resources".
- Worker-hosts admin page, `AddWorkerHostDialog`, worker registration tokens, image policy: only needed if
  customers still add hosts by hand.

Simplify (needs a product decision):
- Drafts/CRDT (goals 2, 7): replace `useDraft*`, `crdt.ts`, `DraftStatusBar`, `DraftWorkspaceControls`,
  `CommitModal`, `useAutoCommitBeforeAction`, `draft.*` WS with explicit Save (`PUT /branches/:id/files/:path`
  already exists, `useFiles.ts:97`) + a plain "changed files" list + commit. `SpecEditor` and `CodeEditor`
  lose their Yjs bindings; comments need a non-CRDT anchor (path + line / block id).
- Chat (goal 8): keep ChatPanel but drop worker-conflict/pin logic (`lib/chatWorkerConflict.ts`,
  `WorkspacePageChatWorkerPin`), sticky-runtime badges, "continue in merged workspace"; concurrent sessions
  per workspace become ordinary.
- Jobs/tasks panel: if agents are local processes launched by a dispatcher, the DAG/timeline/checkpoint/
  rollback machinery (`useJobs.ts`, 46 hooks) could shrink to "runs" with a log stream; `TerminalPanel` could
  become a real PTY (today it only renders `output` timeline entries).
- Merge/PR/approvals (goal 6): collapse job-merge + rebase-merge + remote-tracking merge runs + PR flows into
  one "workspace branch -> PR -> approvals -> merge" path at the git boundary; `PrepareToShipModal` is
  already the closest UI.
- Remote tracking / auto-pull / make-remote: with native git on the VM (goal 4), pull/push/fetch become git
  operations on the VM; the server-side sync-run / merge-run tables and their UI (`BranchTrackingPopover`,
  `RemoteTrackingActions`, `BranchAutoPullControl`, `MakeRemoteDialog`) can go if the VM is the git client.
- Governance surfaces behind `g5_ui_v3` (status band Finalize / Plan Approvals, org approval library) are
  dev-only today; decide whether they are the "governance at git boundary" UI or a dead branch.

Must survive (used everywhere, not tied to workers or CRDT): routing shell and tabs, ontology/spec/doc/file
viewing, search, issues (if kept), notifications, org/members/settings, auth, telemetry and platform-issue
reporting (optional), preview (in the new form).

### 5.2 Preview (goal 9)

- If the app runs on the workspace VM: no `preview` job type, no `job_preview_sessions`, no `boot_generation`,
  no separate `preview_vms` registry, no `preview_reaper`, no attach admission / listener / token machinery.
  Profiles reduce to "which command/port to run"; the `.g5/preview_profiles/*.yaml` format can stay (it is
  already committed config).
- What must remain: a way for the browser to reach a port on the VM with auth. Options present in the code:
  keep the tunnel + `/api/v1/preview/:id/*` relay (works with outbound-only VMs; requires a single API replica
  or a shared registry), or keep the subdomain handoff (`preview_host.rs`) routed by workspace id instead of
  job id. The proxy (`preview.rs`) is job-id-keyed only in `resolve_preview_target`; the rest is generic
  HTTP/WS relay.
- Port discovery: `g5d-worker/src/port_scan.rs` already detects new listeners; a dispatcher on the VM could
  publish ports instead of the host `port_resolver` reading the config.
- Keep `PreviewBrowserView` (iframe + URL bar + open-in-tab) and the mint endpoint; drop the launcher modal's
  commit / TTL / profile-commit plumbing.

### 5.3 Infra

- Delete: Terraform `worker_host` / `tungsten_worker_host` MIG modules and the daily rotation scheduler
  (target 0 everywhere); Tungsten/PairSoft/PNC customer image build steps in `cloudbuild-deploy.yaml` if those
  tenants are not retained; disabled GitHub workflows; helm worker Deployment/HPA templates (never rendered);
  Istio template (off).
- Re-scope the `cloud/` provider machinery from "pool of task hosts" to "one VM per workspace" (provision on
  workspace create/open, stop on idle, resume on demand). Heartbeat and orphan sweep carry over; the 24 h
  host-JWT design and idle-based replacement do not fit long-lived per-workspace VMs.
- Filestore + the git smart-HTTP server in g5-api exist so hosts can clone from the control plane; with native
  git on the VM (goal 4) the VM could talk to GitHub/GitLab directly and the control plane keeps only metadata,
  which also removes the `/git/*` ingress path and the `G5_STORAGE_*` surface.
- Single-replica API: removing CRDT authority and the process-local tunnel / jti stores is what unlocks
  `api.replicaCount > 1`.
- Self-hosted: keep the compose bundle; the external-worker-host installer becomes "VM provider" configuration.

## 6. Open questions (for the user)

1. Which center-panel surfaces must survive: ontology tree + spec editor (semantic record frames),
   docs/explainers, file editor, search, intent log, source-control change graph, issues, org home, status
   band (stage flow), approvals library?
2. Editing model after CRDT removal: is concurrent multi-user editing of the same file a requirement (then a
   lighter presence/lock scheme is needed), or is per-user branch + explicit Save acceptable? What happens to
   inline comments anchored to CRDT positions?
3. Multi-repo workspaces: keep (`branches[]` per repo + docs repo) or move to one repo/branch per workspace?
   Where do the ontology docs (`.g5/specs`, `.g5/docs`) live: same repo, or a docs repo as today?
4. Preview reachability: is it acceptable that browser -> app traffic transits g5-api (tunnel relay, single
   replica), or should each workspace VM get its own ingress/subdomain? Is the wildcard-subdomain mode
   (`preview.<env>.g5labs.ai`) a requirement for SPA dev servers?
5. Chat-in-preview ("attach"): with agent and app on the same VM, is the remaining requirement just "the
   agent can restart/observe the app", or also "a human chats while the app serves"? Should the app
   auto-reload on Save?
6. Job/task model: keep the DAG / timeline / checkpoint / rollback presentation and the `uplift`, `downlift`,
   `sync`, `explainer_generate`, `approval_review` job kinds, or reduce to "runs" of an agent with logs?
7. Governance at the git boundary: which gate is authoritative, workspace approvals (`approval_configs`), PR
   merge on the SCM side, or both? Should human/agent approvals remain G5 objects or become GitHub reviews?
8. Remote tracking: are G5-managed mirrors (`g5-local://` repos, auto-pull, push modes, coordinated repo
   sync) still needed, or is every workspace a clone of an external remote?
9. Fleet: who provisions workspace VMs (org admin, or automatically on open), what idle/stop policy, and do
   self-hosted customers bring their own KVM Linux hosts (Docker `g5d host-up`) or a cloud account? Is the
   macOS native host (HVF) a supported customer target or dev-only?
10. Tenants: are the Tungsten / PairSoft / PNC customer images and the Azure canary lanes retained products or
    historical?
11. Deploy shape: keep GKE for the control plane, or is a single-VM compose deployment (like the self-hosted
    bundle) the primary target now that the API is single-replica anyway?
