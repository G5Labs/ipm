# Preview in a top-level G5 app tab (issue #786, second half)

> **Status (2026-10-26):** implemented in commit 9f652a8b2 on `preview-ux-improvement`
> (route, tab surface, `PreviewTabPage`, tests, Playwright flow, `g5.preview.app-tab`)
> and reverted the same day at the owner's request ("revert the tab change for now").
> The design below and the owner's answers stand; the ontology block is withdrawn with
> the code and returns with it. Reapplying the reverted commit is the shortest path back.

Request: [chat intent](../../chat_intent/preview-ux-improvement/request.md);
[issue #786](https://github.com/G5Labs/G5-Lean/issues/786) "Open the application in a
new top-level G5 app tab". Mock: [docs/mocks/preview-dialog-and-tab](../../../docs/mocks/preview-dialog-and-tab/README.md).
The first half (the dialog) shipped as [`g5.preview.dialog-controls`](../../ontology/features/ui.g5.md#g5.preview.dialog-controls).

## Governing intent

- [`g5.workbench.tabs`](../../ontology/features/ui.g5.md#g5.workbench.tabs): the strip is a
  per-user list of route identities; an arrival at a route opens its tab; closing is
  cross-browser-tab; tabs reorder; memory of where a tab was last shown is per session.
- [`g5.workspace.preview-origin-security`](../../ontology/software_architecture/workspace_vm.g5.md#g5.workspace.preview-origin-security):
  previews are served only from an isolated per-preview origin reached through a
  single-use handoff that sets an origin-scoped cookie; platform credentials never
  reach the app.
- [`g5.preview.trigger-state`](../../ontology/features/ui.g5.md#g5.preview.trigger-state) and
  [`g5.preview.dialog-controls`](../../ontology/features/ui.g5.md#g5.preview.dialog-controls):
  the owner learns the preview's state on the trigger and in the dialog; Stop preview is
  the only action that stops the application.
- New: [`g5.preview.app-tab`](../../ontology/features/ui.g5.md#g5.preview.app-tab).

## Owners and the change

**Tab identity (`lib/workspaceTabs.ts`, owner of the stored list).** A tab gains an
optional `surface: "preview"`. Identity (`workspaceTabKey`) includes the surface, so a
workspace and its preview are two tabs of the same route family; the path is the
workspace's path plus `/preview`; the label is "Preview · name". Storage stays the
tolerant list: older entries have no surface and read as workspace tabs. A rename
rewrites both tabs of the workspace. Nothing else in the list's rules changes (cap,
order, eviction, reorder, cross-browser-tab close).

*Rejected alternative:* a separate preview-tab list or session-only state. The owner
chose persistence like other tabs, and one list keeps one set of rules (order, cap,
storage events) with no second lifecycle.

**Route (`lib/shellRoutes.ts`, `App.tsx`).** `<workspace root>/preview` for each route
family (`/:org/:workspace/preview`, `/:org/me/preview`, `/:org/users/:handle/preview`)
renders `PreviewTabPage`, not `WorkspacePage`. The classification reports a workspace
route with `surface: "preview"`, so the strip's arrival rule opens the preview tab
exactly as it opens a workspace tab: the one opening path the issue asks for.

**Opening (`useWorkspaceTabs`, `PreviewControl`).** The dialog's Open preview asks the
tab context to open the active workspace's preview surface: a navigation to that route.
An already-open preview tab is simply shown again (same identity). The session mint
moves from the dialog to the tab page, which is the consumer of the URL. Where the
strip is not rendered (`enabled` false: layouts without tabs) the dialog keeps today's
browser-tab path, since there is no app tab to open.

**The tab page (`pages/PreviewTabPage.tsx`).** Resolves the route the same way the
workspace page does (`useResolveWorkspaceRoute`), reads the workspace's preview and VM
state through the queries the dialog already uses (same keys, so the trigger, the
dialog and the tab agree), and renders the shell with the slim bar over the application:

- owner, VM running, preview serving and no stop in flight → mint
  `POST /api/v1/preview/session/:taskId` once per run (task id, generation) and per
  Reload, and frame the returned URL;
- starting → the bar says so, no frame yet (the mint would be refused: `require_ready`);
- stopping → the frame is removed and the bar says Stopping…;
- no live preview (none, failed, cancelled), VM stopped, or not the owner → the stopped
  state: what happened, "Start preview again" (the last run's repository and profile,
  through the same start route the dialog uses, offered only when the VM runs and a
  last run is known) and "Back to workspace";
- Stop preview in the bar is the dialog's cancel; Open in browser is the dialog's old
  path (mint, then a `noopener` browser tab) for an application that still refuses
  framing; Reload re-mints, since a handoff ticket is single-use.

The tab never stops a preview by being closed, and a stop never closes the tab: the
owner's answers. The frame is an ordinary cross-origin `<iframe>` (no `sandbox`: the
application must run as it does in a browser tab; the isolation is the origin's).

## Why no backend change

The relay already normalizes anti-framing headers to `frame-ancestors 'self'
<platform origin>` (routes/preview [D16], `normalize_anti_framing_headers`), a leftover
of the earlier embedded Preview Browser, and the QA preview hosts
(`*.preview.qa.g5labs.ai`) are same-site with the app, so the `SameSite=Lax`
`__Host-g5_preview` cookie set by the in-frame handoff navigation is sent on the
frame's requests and WebSocket upgrades. Checked against `preview.rs`,
`middleware/preview_host.rs::build_host_session_cookie` and `values-qa.yaml`.

**Assumption to keep:** if previews ever move to a separate registrable domain (the
deferred choice in `g5.workspace.preview-origin-security`), the framed handoff becomes
third-party and the cookie needs `SameSite=None; Secure; Partitioned`; Open in browser
is the fallback until then. Recorded in the ontology block's metadata.

## Acceptance

- Open preview in the dialog lands on `/:org/:workspace/preview`, a new tab beside the
  workspace's, workspace tab still open; the strip switches between them (click,
  arrows, reorder, middle-click close).
- The tab shows the application in a frame whose `src` is the minted preview URL;
  Reload mints again; Open in browser opens the minted URL in a `noopener` browser tab.
- Stop in the bar cancels through the existing route; the tab stays and shows the
  stopped state with Start preview again (last run's profile) and Back to workspace.
- Closing the tab leaves the preview serving; the dialog's Open reopens it.
- A reload of the route restores the tab (stored list) and re-mints; with no live
  preview it shows the stopped state.
- The centre panel gets no preview tab; the default action never opens a browser tab
  while the strip is shown.
- Tests: `shellRoutes.test.ts`, `workspaceTabs.test.ts`, `WorkspaceTabStrip.test.tsx`
  (preview tab label, glyph, close), `PreviewTabPage.test.tsx` (the states above, the
  mint, Reload, Open in browser, Stop, Start again), `PreviewControl.test.tsx` (Open
  navigates through the context; the browser-tab fallback without the strip), and the
  Playwright preview flow, which now reads the marker inside the frame and uses Open in
  browser for its isolated-origin probe.
