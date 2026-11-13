# Alignment-only UI — design

Request: [chat intent](../../chat_intent/alignment-only-ui/request.md), 2026-10-23.

## Governing blocks

- New in `features/ui.g5.md`: `g5.ui.workbench-shell` — the workbench is the only workspace
  shell; the classic layouts, their setting, their surfaces and their instance gates are
  retired. Main's rebuilt ontology no longer carries `g5.workbench.gate` or the classic
  shell/status-band contracts (dropped in the 2026-10-13 rebuild), so nothing is amended
  there; the new block records the retirement so later work does not reintroduce a gate.
- Amended: `g5.workbench.header-styles` (no longer "beside Workspace panels, offered only
  under the alignment layout"), `g5.ui.home-workspaces`, `g5.workbench.tabs`,
  `g5.workbench.landing`, `g5.ui.activity-rail` and `issues.g5.md#g5.workbench.issues.create`
  lose their "under the alignment layout" qualifiers: there is one layout.
- Unchanged consumers: `g5.ui.guided-toggle` (the Guided section still has its own
  `guided_journey` kill-switch and setting), `g5.workbench.gates`, the phone layout.

## Design

- **One shell.** `WorkspacePage` renders the workbench header (phase chevrons, action
  ribbon with the workspace controls), the tab strip carries the global controls, and the
  rail's Workspace row the Activity section. The `workspaceLayout` and `statusBandCollapsed`
  settings, `resolveWorkspaceLayout`, `useResolvedWorkspaceLayout`, the `top_panel`,
  `alignment_ui` and `g5_ui_v3` instance features and `defaultWorkspaceLayout` are removed
  end to end (client, `instance.rs`, compose, helm, CI, Justfile, the gate inventory).
  Settings stored under the removed keys are ignored by the existing normalizer (user
  settings live in `localStorage` only).
- **Removed surfaces.** `WorkspaceStatusBand` (its shared `RosterAvatar` and
  `shipApprovalRoster` move to `components/workspace/approvalRoster.tsx`, which
  `PrepareToShipModal`, `WorkspaceApprovalsPageContent`, `GuidedJourneySection` and
  `PhaseChevrons` keep importing), `ReposBar` with the `workspace-toolbar` row (`WorkspaceVmStatusStrip` stays as the
  phone's lifecycle control), `OrganizationHomeTopBar`, Home's `RecentWork` and classic
  signal reads, the settings select and the desktop workspace `TopBar`. The workspace
  `TopBar` stays only for the phone's Base page, which keeps its title bar (the strip and
  ribbon controls are a desktop arrangement).
- **Tabs.** The strip shows whenever a signed-in person is on a shell route; the fork
  picker always relies on it.
- **Open-question total.** The status band was the only workspace-page reader of the
  open-question metric; the Home Workspaces tile keeps its per-row read. The live-refresh
  browser flow that watched the band's badge is retired with it.
- **Removed with them (implementation discovery).** The standalone Create PR dialog and
  the Generate modal were reachable only from the classic title bar and band: the ribbon's
  Create PR opens the Prepare-to-Ship surface ([ship actions](../../ontology/features/ui.g5.md#g5.workbench.ship-actions))
  and Generate code is a chat template. They go, with their page tests. `ReviewChangesModal`
  and `HomeIssueSearch` (the classic Home's full issue search; the Issues tile keeps its
  compact search) likewise.
- **Session hint under the landing (fix).** With the timeline landing writing
  `?view=activity`, a chat started from the composer (or a resolution chat after Get
  latest from Base) lost its `?session=` hint: the view-param mirror cleared the stale
  `view` from a render that predated the session's own URL write and wrote the old query
  back without the hint. The mirror now leaves the URL alone while a session selection's
  write is in flight (`pendingSessionSelectionRef`). A reload after starting a chat
  returns to the conversation, as the [landing](../../ontology/features/ui.g5.md#g5.workbench.landing) expects of a
  conversation hint. The classic layout never exercised this path, which is why the moved tests
  exposed it.
- **Download code (re-homed).** The ReposBar was the only caller of the page's source
  download. The Workspace Details header now offers "Download code" (the page keeps
  the request and its in-progress state; the panel renders the button only when the
  page passes the action), so the one shell loses no capability.
- **Workbench configuration payload (fix).** A malformed `GET …/workbench` body (the
  mocked chat flows answered it with an empty list) crashed the page in the ribbon's
  action lookup. The hook now validates the payload shape and reads anything else as the
  error state, which keeps the ribbon on the complete built-in fallback
  ([workbench-config](../../ontology/features/ui.g5.md#g5.ui.workbench-config)); the
  action lookup tolerates a missing phase list. The classic layouts never rendered the
  ribbon, so the crash was unreachable before.
- **Source Control in the rail.** The workbench's Source Control section (since the
  Phase 8 rail) selects file views and offers "Manage current changes" and the Diff
  Graph instead of the classic Changes/History tabs with dirty-file rows. Flows that
  clicked a dirty row now open the Changes view through that action
  (`openWorkspaceChanges`). The classic History tab's paginated Git commit list with
  commit tabs has no workbench equivalent; its flow
  (`source-control-history.spec.ts`) is retired and the gap is recorded for a decision.
- **Chrome surface.** With no title bar, the workspace tab strip carries
  `data-surface="chrome"` so themes that paint chrome (Aero Glass) paint it; the glass
  flow reads that surface.
- **Browser flows.** Flows that opened Preview, Draft requirements or the Workspace Details
  page through the band or title bar use the ribbon's `ribbon-preview-preview`,
  `ribbon-action-*` and `workspace-info-button` controls; the alignment-shell and
  guided-journey flows no longer switch the layout or skip on the gate. CI lanes drop the
  `G5_ALIGNMENT_UI_ENABLED`/`G5_DEFAULT_WORKSPACE_LAYOUT` environment. Flows that drove
  the toolbar's controls move to the shell's: the terminal opens from the rail's Files
  header (`railTerminalButton`/`openRailTerminal`, which also carries the stopped-VM
  reason), the VM state is read from the ribbon's lifecycle button title (`VM: Stopped`),
  forking goes through the strip's "+" and the fork picker's row, and the composer is
  opened explicitly (`openChatComposer`: the ribbon's chat toggle) because a workspace
  route lands on the timeline.

## Retired inventory

- Settings and resolution: `UserSettings.workspaceLayout`, `statusBandCollapsed`,
  `lib/topPanelFlags.ts` (`resolveWorkspaceLayout`), `hooks/useWorkspaceLayout.ts`,
  `lib/uiV3Flags.ts`, `useSettings` hooks `useUiV3Enabled`/`useTopPanelEnabled`/
  `useAlignmentUiEnabled`/`useDefaultWorkspaceLayout`; the normalizer drops stored values
  under the removed keys.
- Instance: `top_panel`, `alignment_ui`, `g5_ui_v3` features and `defaultWorkspaceLayout`
  in `routes/instance.rs`; `G5_ENABLE_TOP_PANEL`, `G5_ALIGNMENT_UI_ENABLED`,
  `G5_UI_V3_ENABLED`, `G5_DEFAULT_WORKSPACE_LAYOUT` in compose (dev, test), helm (prod,
  qa, staging), `cloudbuild-ci-e2e.yaml`, the Justfile and the deployment gate
  inventory/audit; the `G5_NATIVE_APPROVAL_LAYOUT` switch of `e2e/approvals-native.mjs`.
- Surfaces: `WorkspaceStatusBand`, `ReposBar` and the `workspace-toolbar` row,
  `OrganizationHomeTopBar`, Home's `RecentWork` and `HomeIssueSearch`, the settings
  layout select, the desktop render of `workspace/TopBar`, `CreatePullRequestDialog`,
  `GenerateModal`, `ReviewChangesModal`.
- Browser flows: `openWorkspaceLayoutSelect`, `ALIGNMENT_UI_ENABLED`, the
  `open-question-metrics-live-refresh` flow and the band-driven Draft requirements test.

## Assumptions and acceptance

- `G5_GUIDED_JOURNEY_ENABLED`, `G5_ENABLE_PROVIDER_ACCOUNTS` and the other instance
  features are untouched.
- The responsive phone layout and Base's phone title bar are unchanged.
- Browser flows are retargeted to the ribbon and rail controls but were not run locally
  (the dev stack was not started for this change); CI's e2e lanes verify them.
- Tests: the classic-only component tests are deleted; `WorkspacePageV2.test.tsx`,
  `HomePage.test.tsx`, `DataHookWiring.test.tsx`, `WorkspaceSettingsContent.test.tsx`,
  `useUserSettings.test.ts`, `topPanelFlags.test.ts` and `instance.rs` unit tests are
  reworked to the one shell; `e2e/flows/alignment-shell.spec.ts` keeps covering it.
