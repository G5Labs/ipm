/**
 * Vitest unit tests for BranchTrackingPopover, the remote-tracking detail
 * body, mounted the way Workspace Details renders it: through
 * `BranchTrackingInline` with its expander opened.
 *
 * Covers:
 *   - The detail discloses in page flow and stays open after actions.
 *   - Header shows healthy status + last non-noop action.
 *   - Merge-required section renders for active merge runs with the
 *     three SHAs visible.
 *   - "Mark resolved" two-step (click → confirm → POST) calls the right
 *     endpoint.
 *   - Retry section is gated to retryable last statuses.
 *
 * SOT:
 * - aux_files/future_implementation/remote_tracking_gap_closure.plan.md G2
 * - Chat C6 (proceed with G2)
 */

import React from "react";
import { beforeEach, describe, expect, it } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { renderApp } from "../../test/render";
import { createMockUser, createMockWorkspace } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { BranchTrackingInline } from "./BranchTrackingInline";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";
import type {
  AgentLaunchPreferencesV1,
  AgentLaunchSelectionV1,
} from "../../lib/types";

const WORKSPACE_ID = "ws-abc";
const REPO_ID = "repo-1";
const MERGE_RUN_ID = "merge-run-1";
const LAUNCH_PREVIEW_ROUTE = `/api/v1/workspaces/${WORKSPACE_ID}/config/agent-launch/resolve`;
const START_ROUTE = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs/${MERGE_RUN_ID}/start`;

function resolvedLaunch(preferences: AgentLaunchPreferencesV1) {
  const field = <T,>(preference: { source: string; value?: T }) => ({
    value: preference.value,
    provenance: "explicit",
  });
  return {
    schemaVersion: 1,
    resolutionId: `alr_${preferences.provider.source === "explicit" ? preferences.provider.value : "configured"}`,
    configRevision: "cfg-test",
    catalogVersion: "agent-launch-catalog-v1",
    provider: field(preferences.provider),
    providerRoute: "anthropic_direct",
    model: field(preferences.model),
    upstreamModel: field(preferences.model).value,
    permissionMode: field(preferences.permissionMode),
    effort: field(preferences.effort),
    authMode: "anthropic_api_key",
    egressMode: "direct_provider",
    credentialReady: true,
    requiresInteractiveAuth: false,
  };
}

function fakeRun(overrides: Record<string, unknown> = {}) {
  return {
    id: "run-default",
    repoId: REPO_ID,
    actingUserId: "user-1",
    kind: "integration",
    triggerKind: "commit",
    status: "noop",
    startedAt: "2026-05-28T00:00:00Z",
    ...overrides,
  };
}

interface SyncRunsOpts {
  syncPausedReason?: string;
  latestRunStatus?: string;
  remoteTipAdvisorySha?: string;
  remoteTipAdvisoryObservedAt?: string;
  baselineCommit?: string;
  runs?: Array<Record<string, unknown>>;
}

function syncRunsHandler(opts: SyncRunsOpts) {
  const runs = opts.runs ?? [];
  return http.get(
    `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/sync_runs`,
    () =>
      HttpResponse.json({
        data: {
          branch: {
            workspaceId: WORKSPACE_ID,
            repos: [
              {
                repoId: REPO_ID,
                repoName: "demo-repo",
                repoBranchName: "main",
                trackingMode: "remote_tracking",
                headCommit: "deadbeef",
                remoteBranch: "main",
                // Default baseline == head ⇒ healthy. Override per test for
                // "behind" (advisory != baseline) or "ahead" (head != baseline).
                baselineCommit: opts.baselineCommit ?? "deadbeef",
                syncPausedReason: opts.syncPausedReason,
                latestRunStatus: opts.latestRunStatus,
                remoteTipAdvisorySha: opts.remoteTipAdvisorySha,
                remoteTipAdvisoryObservedAt: opts.remoteTipAdvisoryObservedAt,
              },
            ],
          },
          runs: { data: runs, total: runs.length, page: 1, perPage: 20 },
        },
      }),
  );
}

function mergeRunsHandler(items: Array<Record<string, unknown>>) {
  return http.get(
    `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs`,
    () => HttpResponse.json({ data: { items } }),
  );
}

function fakeMergeRun(overrides: Record<string, unknown> = {}) {
  return {
    id: MERGE_RUN_ID,
    workspaceId: WORKSPACE_ID,
    sourceKind: "remote_tracking",
    sourceBranchName: "g5/feat/child",
    status: "waiting_for_human",
    phase: "reset_below_baseline",
    blockingReason: null,
    repoId: REPO_ID,
    sourceHeads: {
      workspace_tip: "aaaaaaaaaaaaaaaa",
      mirror_tip: "bbbbbbbbbbbbbbbb",
    },
    targetHeads: { baseline: "bbbbbbbbbbbbbbbb" },
    startedAt: "2026-05-28T01:00:00Z",
    ...overrides,
  };
}

describe("BranchTrackingPopover", () => {
  beforeEach(() => {
    server.use(
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () =>
        HttpResponse.json({
          data: {
            ...createMockWorkspace({ id: WORKSPACE_ID }),
            orgId: "org-1",
            pushMode: "manual",
          },
        }),
      ),
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/config`, () =>
        HttpResponse.json({
          data: { effectiveSettings: {}, settings: {}, secretKeys: [] },
        }),
      ),
      http.post(LAUNCH_PREVIEW_ROUTE, async ({ request }) => {
        const { preferences } = (await request.json()) as {
          preferences: AgentLaunchPreferencesV1;
        };
        return HttpResponse.json({
          data: { resolved: resolvedLaunch(preferences) },
        });
      }),
    );
  });

  it("offers agent resolution for automatic divergence without a merge run", async () => {
    const starts: Array<{ agentLaunch: AgentLaunchSelectionV1 }> = [];
    let releaseStart!: () => void;
    const startGate = new Promise<void>((resolve) => {
      releaseStart = resolve;
    });
    server.use(
      syncRunsHandler({
        syncPausedReason: "diverged",
        latestRunStatus: "diverged",
      }),
      mergeRunsHandler([]),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs/start`,
        async ({ request }) => {
          starts.push(
            (await request.json()) as { agentLaunch: AgentLaunchSelectionV1 },
          );
          await startGate;
          return HttpResponse.json({
            data: {
              run: fakeMergeRun({
                phase: "workspace_rebase_preflight",
                resolutionTaskId: "task-1",
              }),
            },
          });
        },
      ),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );
    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const start = await screen.findByRole("button", {
      name: "Resolve with agent",
    });
    await waitFor(() => expect(start).toBeEnabled());
    expect(starts).toHaveLength(0);
    fireEvent.click(start);
    fireEvent.click(start);
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0].agentLaunch.preferences.permissionMode).toEqual({
      source: "explicit",
      value: "bypassPermissions",
    });
    expect(starts[0].agentLaunch.confirmedResolutionId).toBe("alr_claude_code");
    expect(start).toHaveTextContent("Starting…");
    releaseStart();
    // The inline detail stays open after the action so its feedback remains.
    await waitFor(() => expect(start).toHaveTextContent("Resolve with agent"));
    expect(screen.getByTestId("branch-tracking-popover")).toBeVisible();
    expect(screen.getByTestId("repo-tracking-expander")).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("confirms explicit agent settings before launching and coalesces duplicate clicks", async () => {
    const starts: Array<{ agentLaunch: AgentLaunchSelectionV1 }> = [];
    let previewed: AgentLaunchPreferencesV1 | undefined;
    let releasePreview!: () => void;
    let releaseStart!: () => void;
    const previewGate = new Promise<void>((resolve) => {
      releasePreview = resolve;
    });
    const startGate = new Promise<void>((resolve) => {
      releaseStart = resolve;
    });
    server.use(
      syncRunsHandler({ syncPausedReason: "diverged" }),
      mergeRunsHandler([fakeMergeRun({ phase: "workspace_rebase_preflight" })]),
      http.post(LAUNCH_PREVIEW_ROUTE, async ({ request }) => {
        const { preferences } = (await request.json()) as {
          preferences: AgentLaunchPreferencesV1;
        };
        previewed = preferences;
        await previewGate;
        return HttpResponse.json({
          data: { resolved: resolvedLaunch(preferences) },
        });
      }),
      http.post(START_ROUTE, async ({ request }) => {
        starts.push(
          (await request.json()) as { agentLaunch: AgentLaunchSelectionV1 },
        );
        await startGate;
        return HttpResponse.json({
          data: { run: fakeMergeRun({ resolutionTaskId: "task-1" }) },
        });
      }),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );
    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const start = await screen.findByTestId(`start-merge-${MERGE_RUN_ID}`);
    await waitFor(() => expect(previewed).toBeDefined());
    expect(start).toBeDisabled();
    fireEvent.click(start);
    expect(starts).toHaveLength(0);
    expect(previewed?.permissionMode).toEqual({
      source: "explicit",
      value: "bypassPermissions",
    });
    releasePreview();
    await waitFor(() => expect(start).toBeEnabled());
    fireEvent.click(start);
    fireEvent.click(start);
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0].agentLaunch).toEqual({
      preferences: previewed,
      confirmedResolutionId: "alr_claude_code",
    });
    expect(start).toHaveTextContent("Starting…");
    releaseStart();
    await waitFor(() => expect(start).toHaveTextContent("Resolve with agent"));
    expect(screen.getByTestId("branch-tracking-popover")).toBeVisible();
  });

  it("uses the selected provider and its freshly confirmed permission mode", async () => {
    let started: AgentLaunchSelectionV1 | undefined;
    server.use(
      syncRunsHandler({ syncPausedReason: "diverged" }),
      mergeRunsHandler([fakeMergeRun({ phase: "workspace_rebase_preflight" })]),
      http.post(START_ROUTE, async ({ request }) => {
        started = (
          (await request.json()) as { agentLaunch: AgentLaunchSelectionV1 }
        ).agentLaunch;
        return HttpResponse.json({
          data: { run: fakeMergeRun({ resolutionTaskId: "task-1" }) },
        });
      }),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );
    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const start = await screen.findByTestId(`start-merge-${MERGE_RUN_ID}`);
    await waitFor(() => expect(start).toBeEnabled());
    await user.selectOptions(
      screen.getByTestId(`resolve-merge-${MERGE_RUN_ID}-agent-select`),
      "codex",
    );
    await waitFor(() => expect(start).toBeEnabled());
    await user.click(start);
    await waitFor(() => expect(started).toBeDefined());
    expect(started?.preferences.provider).toEqual({
      source: "explicit",
      value: "codex",
    });
    expect(started?.preferences.permissionMode).toEqual({
      source: "explicit",
      value: "never/danger-full-access",
    });
    expect(started?.confirmedResolutionId).toBe("alr_codex");
  });

  it("does not launch when the agent settings cannot be confirmed", async () => {
    let startCount = 0;
    server.use(
      syncRunsHandler({ syncPausedReason: "diverged" }),
      mergeRunsHandler([fakeMergeRun({ phase: "workspace_rebase_preflight" })]),
      http.post(LAUNCH_PREVIEW_ROUTE, () =>
        HttpResponse.json(
          { error: { message: "Launch unavailable" } },
          { status: 422 },
        ),
      ),
      http.post(START_ROUTE, () => {
        startCount++;
        return HttpResponse.json({ data: {} });
      }),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );
    await user.click(await screen.findByTestId("repo-tracking-expander"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Launch unavailable",
    );
    const start = screen.getByTestId(`start-merge-${MERGE_RUN_ID}`);
    expect(start).toBeDisabled();
    fireEvent.click(start);
    expect(startCount).toBe(0);
  });

  it("discloses the detail inline, collapsed by default, without a portal", async () => {
    server.use(syncRunsHandler({}), mergeRunsHandler([]));
    const user = userEvent.setup();
    const { container } = renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    const expander = await screen.findByTestId("repo-tracking-expander");
    expect(expander).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("branch-tracking-popover")).toBeNull();

    await user.click(expander);
    const body = await screen.findByTestId("branch-tracking-popover");
    expect(expander).toHaveAttribute("aria-expanded", "true");
    // Rendered in page flow inside the card, not portaled to <body>.
    expect(container).toContainElement(body);
    expect(body.closest('[role="dialog"]')).toBeNull();
    expect(body.className).not.toMatch(/shadow|fixed|absolute/);
    const detail = screen.getByTestId("repo-tracking-detail");
    expect(expander).toHaveAttribute("aria-controls", detail.id);

    // Escape and outside clicks no longer dismiss it; only the expander does.
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.mouseDown(document.body);
    expect(screen.getByTestId("branch-tracking-popover")).toBeVisible();
    await user.click(expander);
    expect(expander).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("branch-tracking-popover")).toBeNull();
  });

  it("surfaces the 'remote moved / behind' state with the pill's wording", async () => {
    // Remote moved (advisory != baseline) but not paused: the popover header
    // AND a dedicated banner must reflect the same "behind" state. C3.
    server.use(
      syncRunsHandler({
        remoteTipAdvisorySha: "cafebabe", // != baselineCommit "deadbeef"
        remoteTipAdvisoryObservedAt: "2026-05-30T12:00:00Z",
      }),
      mergeRunsHandler([]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));

    // Headline lives ONLY in the header status line.
    const statusLine = await screen.findByTestId("popover-status-line");
    expect(statusLine).toHaveAttribute("data-state", "behind");
    expect(statusLine).toHaveTextContent(
      "Remote moved — your branch is behind",
    );
    expect(statusLine).toHaveTextContent(
      screen.getByTestId("repo-status-pill").textContent ?? "",
    );

    // Banner states the CONSEQUENCE and does NOT repeat the header headline.
    const banner = screen.getByTestId("popover-remote-moved");
    expect(banner).toHaveTextContent(/agent.*running workspace/i);
    expect(banner).not.toHaveTextContent(/remote moved/i);
    // Binary signal — no commit count.
    expect(banner.textContent ?? "").not.toMatch(/\d+\s*commits?/i);
  });

  it("surfaces the 'ahead' state for unpushed local commits (not behind)", async () => {
    // head != baseline (local commits), advisory == baseline (the remote has
    // NOT moved) → "ahead", with its own banner, NOT the behind/merge banner. C3.
    server.use(
      syncRunsHandler({
        baselineCommit: "0000", // head "deadbeef" is ahead of baseline
        remoteTipAdvisorySha: "0000", // advisory == baseline ⇒ not behind
      }),
      mergeRunsHandler([]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));

    const statusLine = await screen.findByTestId("popover-status-line");
    expect(statusLine).toHaveAttribute("data-state", "ahead");
    expect(statusLine).toHaveTextContent(/ahead/i);
    // The ahead banner is shown; the behind/merge banner is NOT.
    expect(screen.getByTestId("popover-ahead")).toHaveTextContent(
      /not yet pushed/i,
    );
    expect(screen.queryByTestId("popover-remote-moved")).toBeNull();
  });

  it("renders 'Tracking healthy' header and the last non-noop action", async () => {
    server.use(
      syncRunsHandler({
        latestRunStatus: "pushed",
        runs: [
          fakeRun({
            id: "r-noop",
            status: "noop",
            startedAt: "2026-05-28T03:00:00Z",
          }),
          fakeRun({
            id: "r-pushed",
            status: "pushed",
            startedAt: "2026-05-28T02:00:00Z",
          }),
        ],
      }),
      mergeRunsHandler([]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const statusLine = await screen.findByTestId("popover-status-line");
    expect(statusLine).toHaveTextContent(/Tracking healthy/i);
    const lastAction = screen.getByTestId("popover-last-action");
    expect(lastAction).toHaveTextContent(/Last action: pushed/i);
  });

  it("lists recent pull/push activity with friendly labels and filters background noise", async () => {
    server.use(
      syncRunsHandler({
        latestRunStatus: "pushed",
        runs: [
          fakeRun({
            id: "r-resolved",
            status: "resolved",
            triggerKind: "commit",
            startedAt: "2026-05-28T03:30:00Z",
          }),
          fakeRun({
            id: "r-pushed",
            status: "pushed",
            triggerKind: "push",
            startedAt: "2026-05-28T03:00:00Z",
          }),
          fakeRun({
            id: "r-fetched",
            status: "fetched",
            triggerKind: "manual",
            startedAt: "2026-05-28T02:30:00Z",
          }),
          fakeRun({
            id: "r-imported",
            status: "imported",
            triggerKind: "manual",
            startedAt: "2026-05-28T02:00:00Z",
          }),
          fakeRun({
            id: "r-failed",
            status: "push_failed",
            triggerKind: "push",
            errorDetail: "remote rejected the push",
            startedAt: "2026-05-28T01:00:00Z",
          }),
          fakeRun({
            id: "r-bg-noop",
            status: "noop",
            triggerKind: "background_inbound",
            startedAt: "2026-05-28T00:30:00Z",
          }),
        ],
      }),
      mergeRunsHandler([]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const activity = await screen.findByTestId("popover-recent-activity");
    expect(activity).toHaveTextContent(
      "Divergence resolved — tracking resumed",
    );
    expect(activity).toHaveTextContent("Pushed to remote");
    expect(activity).toHaveTextContent("Pulled new commits");
    expect(activity).toHaveTextContent(
      "Remote refs refreshed; workspace unchanged",
    );
    expect(activity).toHaveTextContent("Push failed");
    expect(activity).toHaveTextContent("remote rejected the push");
    // Automatic background noop pings are filtered out of the feedback list.
    expect(screen.queryByTestId("sync-run-r-bg-noop")).not.toBeInTheDocument();
  });

  it("renders the merge-required card with three SHAs when an active run exists", async () => {
    server.use(
      syncRunsHandler({
        syncPausedReason: "diverged",
        latestRunStatus: "diverged",
      }),
      mergeRunsHandler([
        {
          id: MERGE_RUN_ID,
          workspaceId: WORKSPACE_ID,
          sourceKind: "remote_tracking",
          sourceBranchName: "main",
          status: "waiting_for_human",
          phase: "workspace_rebase_preflight",
          blockingReason: null,
          repoId: REPO_ID,
          sourceHeads: {
            workspace_tip: "aaaaaaaaaaaaaaaa",
            mirror_tip: "bbbbbbbbbbbbbbbb",
          },
          targetHeads: { baseline: "cccccccccccccccc" },
          startedAt: "2026-05-28T01:00:00Z",
        },
      ]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    expect(
      await screen.findByTestId("popover-merge-required"),
    ).toBeInTheDocument();
    const card = screen.getByTestId(`merge-run-card-${MERGE_RUN_ID}`);
    expect(card).toHaveTextContent(/Workspace and remote diverged/i);
    expect(card).toHaveTextContent(/aaaaaaa/);
    expect(card).toHaveTextContent(/bbbbbbb/);
    expect(card).toHaveTextContent(/ccccccc/);
  });

  it("recovers the merge button in one pass when the merge-runs cache lags the diverged state (C1)", async () => {
    // Explicit integration opens a resolution record as it pauses. Unlike
    // automatic pause-only detection, this case has a record server-side.
    // The merge-runs poll can momentarily lag the
    // sync-runs poll, though: model that by returning an EMPTY list on the first
    // read and the run on later reads. The popover must detect
    // diverged-without-run and refetch so the button lands in the same viewing.
    let mergeCalls = 0;
    const run = {
      id: MERGE_RUN_ID,
      workspaceId: WORKSPACE_ID,
      sourceKind: "remote_tracking",
      sourceBranchName: "main",
      status: "waiting_for_human",
      phase: "workspace_rebase_preflight",
      blockingReason: null,
      repoId: REPO_ID,
      sourceHeads: { workspace_tip: "aaaa", mirror_tip: "bbbb" },
      targetHeads: { baseline: "cccc" },
      startedAt: "2026-05-28T01:00:00Z",
    };
    server.use(
      syncRunsHandler({
        syncPausedReason: "diverged",
        latestRunStatus: "diverged",
      }),
      http.get(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs`,
        () => {
          mergeCalls += 1;
          return HttpResponse.json({
            data: { items: mergeCalls === 1 ? [] : [run] },
          });
        },
      ),
    );

    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));

    // The first merge-runs read was empty (cache lagging), but the popover
    // refetched on detecting diverged-without-run, so the card appears without
    // waiting for a background poll cycle.
    expect(
      await screen.findByTestId(`merge-run-card-${MERGE_RUN_ID}`),
    ).toBeInTheDocument();
    await waitFor(() => expect(mergeCalls).toBeGreaterThanOrEqual(2));
  });

  it("shows why automatic completion did not finish a merge run", async () => {
    server.use(
      syncRunsHandler({ syncPausedReason: "diverged" }),
      mergeRunsHandler([
        fakeMergeRun({
          phase: "workspace_rebase_preflight",
          blockingReason:
            "Automatic completion failed: Database error. Mark resolved to retry.",
        }),
      ]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    expect(
      await screen.findByTestId(`merge-run-blocking-${MERGE_RUN_ID}`),
    ).toHaveTextContent(/Automatic completion failed.*Mark resolved to retry/);
    expect(
      screen.getByTestId(`mark-resolved-${MERGE_RUN_ID}`),
    ).toBeInTheDocument();
  });

  it("drops an open Mark resolved confirmation once ownership moves away (g5.workspace.ui.reader-publication)", async () => {
    let completePosted = false;
    server.use(
      syncRunsHandler({ syncPausedReason: "diverged" }),
      mergeRunsHandler([
        {
          id: MERGE_RUN_ID,
          workspaceId: WORKSPACE_ID,
          sourceKind: "remote_tracking",
          sourceBranchName: "main",
          status: "waiting_for_human",
          phase: "workspace_rebase_preflight",
          blockingReason: null,
          repoId: REPO_ID,
          sourceHeads: { workspace_tip: "aaaa", mirror_tip: "bbbb" },
          targetHeads: { baseline: "cccc" },
          startedAt: "2026-05-28T01:00:00Z",
        },
      ]),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs/${MERGE_RUN_ID}/complete`,
        () => {
          completePosted = true;
          return HttpResponse.json({ data: { completed: true } });
        },
      ),
    );
    const gate = (owner: boolean) => ({
      kind: owner ? ("live" as const) : ("reader" as const),
      canWrite: owner,
      live: owner,
      canStart: false,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner,
      workspaceNoun: "the workspace",
    });
    function Host() {
      const [owner, setOwner] = React.useState(true);
      return (
        <WorkspaceLiveGateProvider value={gate(owner)}>
          <button
            type="button"
            data-testid="lose-ownership"
            onClick={() => setOwner(false)}
          >
            transfer
          </button>
          <BranchTrackingInline
            workspaceId={WORKSPACE_ID}
            repoId={REPO_ID}
            repoLabel="acme/demo"
          />
        </WorkspaceLiveGateProvider>
      );
    }
    const user = userEvent.setup();
    renderApp(<Host />, { user: createMockUser() });
    await user.click(await screen.findByTestId("repo-tracking-expander"));
    await user.click(
      await screen.findByTestId(`mark-resolved-${MERGE_RUN_ID}`),
    );
    await screen.findByTestId(`mark-resolved-confirm-${MERGE_RUN_ID}`);
    await user.click(screen.getByTestId("lose-ownership"));
    await waitFor(() =>
      expect(
        screen.queryByTestId(`mark-resolved-confirm-${MERGE_RUN_ID}`),
      ).toBeNull(),
    );
    expect(screen.queryByTestId(`mark-resolved-${MERGE_RUN_ID}`)).toBeNull();
    expect(completePosted).toBe(false);
  });

  it("Mark resolved confirm flow calls the complete endpoint", async () => {
    let completePosted = false;
    server.use(
      syncRunsHandler({ syncPausedReason: "diverged" }),
      mergeRunsHandler([
        {
          id: MERGE_RUN_ID,
          workspaceId: WORKSPACE_ID,
          sourceKind: "remote_tracking",
          sourceBranchName: "main",
          status: "waiting_for_human",
          phase: "workspace_rebase_preflight",
          blockingReason: null,
          repoId: REPO_ID,
          sourceHeads: { workspace_tip: "aaaa", mirror_tip: "bbbb" },
          targetHeads: { baseline: "cccc" },
          startedAt: "2026-05-28T01:00:00Z",
        },
      ]),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs/${MERGE_RUN_ID}/complete`,
        () => {
          completePosted = true;
          return HttpResponse.json({ data: { completed: true } });
        },
      ),
    );

    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const markBtn = await screen.findByTestId(`mark-resolved-${MERGE_RUN_ID}`);
    await user.click(markBtn);
    const confirmRegion = await screen.findByTestId(
      `mark-resolved-confirm-${MERGE_RUN_ID}`,
    );
    expect(confirmRegion).toHaveTextContent(
      /Confirm after you or the resolution agent/i,
    );
    const confirmBtn = screen.getByRole("button", {
      name: /Confirm resolved/i,
    });
    await user.click(confirmBtn);

    await waitFor(() => {
      expect(completePosted).toBe(true);
    });
    // The confirm control disappears on success; keyboard focus returns to the
    // card's expander rather than falling to <body>.
    await waitFor(() =>
      expect(
        screen.queryByTestId(`mark-resolved-confirm-${MERGE_RUN_ID}`),
      ).toBeNull(),
    );
    await waitFor(() =>
      expect(screen.getByTestId("repo-tracking-expander")).toHaveFocus(),
    );
  });

  it("keeps the retry button from a reader (g5.workspace.ui.reader-publication)", async () => {
    server.use(
      syncRunsHandler({ latestRunStatus: "auth_required" }),
      mergeRunsHandler([]),
    );
    const user = userEvent.setup();
    renderApp(
      <WorkspaceLiveGateProvider
        value={{
          kind: "reader",
          canWrite: false,
          live: false,
          canStart: false,
          reviewCopy: false,
          reviewIdentityKnown: true,
          owner: false,
          workspaceNoun: "the workspace",
        }}
      >
        <BranchTrackingInline
          workspaceId={WORKSPACE_ID}
          repoId={REPO_ID}
          repoLabel="acme/demo"
        />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    // The status and history still show; the mutation does not.
    expect(await screen.findByTestId("repo-tracking-detail")).toBeVisible();
    expect(screen.queryByTestId("popover-retry-button")).toBeNull();
  });

  it("renders the retry button only for retryable last statuses", async () => {
    server.use(
      syncRunsHandler({ latestRunStatus: "auth_required" }),
      mergeRunsHandler([]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    expect(
      await screen.findByTestId("popover-retry-button"),
    ).toBeInTheDocument();
  });

  it("queued resolution agent opens its session and prevents duplicate launch", async () => {
    server.use(
      syncRunsHandler({ syncPausedReason: "diverged" }),
      mergeRunsHandler([
        {
          id: MERGE_RUN_ID,
          workspaceId: WORKSPACE_ID,
          sourceKind: "remote_tracking",
          sourceBranchName: "main",
          status: "waiting_for_human",
          phase: "workspace_rebase_preflight",
          resolutionTaskId: "resolution-task",
          resolutionSessionId: "resolution-session",
          resolutionTaskStatus: "queued",
          blockingReason: null,
          repoId: REPO_ID,
          sourceHeads: { workspace_tip: "aaaa", mirror_tip: "bbbb" },
          targetHeads: { baseline: "cccc" },
          startedAt: "2026-05-28T01:00:00Z",
        },
      ]),
    );

    const user = userEvent.setup();
    // Viewed while a file is open: the link must still target the chat at the
    // workspace root, not the current file route.
    renderApp(
      <Routes>
        <Route
          path="/:org/:workspace/*"
          element={
            <BranchTrackingInline
              workspaceId={WORKSPACE_ID}
              repoId={REPO_ID}
              repoLabel="acme/demo"
            />
          }
        />
      </Routes>,
      { user: createMockUser(), route: "/g5labs/test1/file/third.md" },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    await screen.findByTestId(`merge-run-card-${MERGE_RUN_ID}`);

    expect(screen.queryByTestId(`start-merge-${MERGE_RUN_ID}`)).toBeNull();
    expect(screen.queryByTestId(`mark-resolved-${MERGE_RUN_ID}`)).toBeNull();
    expect(
      screen.getByTestId(`merge-run-in-progress-${MERGE_RUN_ID}`),
    ).toHaveTextContent(/Merge in progress/i);
    expect(
      screen.getByRole("link", { name: "Open resolution session" }),
    ).toHaveAttribute("href", "/g5labs/test1?session=resolution-session");
  });

  it("hides the retry button for non-retryable last statuses", async () => {
    server.use(
      syncRunsHandler({ latestRunStatus: "diverged" }),
      mergeRunsHandler([]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    await screen.findByTestId("branch-tracking-popover");
    expect(screen.queryByTestId("popover-retry-button")).toBeNull();
  });

  it("renders reset-below-baseline as an approved-publication state (issue 719 item 4)", async () => {
    let publishCalls = 0;
    let genericPushCalls = 0;
    server.use(
      syncRunsHandler({
        syncPausedReason: "watermark_violation",
        latestRunStatus: "fetch_failed",
      }),
      mergeRunsHandler([fakeMergeRun()]),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/remote_tracking/approved_rebase_publish`,
        async ({ request }) => {
          publishCalls += 1;
          expect(await request.json()).toEqual({
            expectedMergeRunIds: [MERGE_RUN_ID],
          });
          return HttpResponse.json({
            data: {
              enqueued: 1,
              coordinated: {
                operation: "approved_rebase_publish",
                status: "queued",
                repos: [],
              },
            },
          });
        },
      ),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/push_to_remote`, () => {
        genericPushCalls += 1;
        return HttpResponse.json({ data: { enqueued: 1 } });
      }),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const statusLine = await screen.findByTestId("popover-status-line");
    expect(statusLine).toHaveAttribute("data-state", "review");
    // Same wording as the pill; the pause reason explains it below.
    expect(statusLine).toHaveTextContent(/^Review required/);
    expect(screen.getByTestId("popover-pause-reason")).toHaveTextContent(
      /provider history was rewritten outside G5/i,
    );

    const card = await screen.findByTestId(`merge-run-card-${MERGE_RUN_ID}`);
    // A provider-rewrite checkpoint keeps its own approval; it is never
    // adopted (g5.workspace.remote.adopt-provider-history).
    expect(screen.queryByTestId("provider-history-adoption")).toBeNull();

    // Neutral approved-push presentation, not the danger merge card.
    expect(screen.getByText(/^Rebase publication approval$/i)).toBeVisible();
    expect(card).toHaveAttribute("data-tone", "approval");
    expect(card.className).toContain("var(--g5-warn)");
    expect(card.className).not.toContain("var(--g5-blocked)");
    expect(card).toHaveTextContent(/Rebased publication needs approval/i);
    expect(card).toHaveTextContent(/normal workspace finalizer/i);
    expect(card).toHaveTextContent(/fast-forward update/i);
    expect(card).not.toHaveTextContent(/force|lease-protected/i);

    // Raw SHAs demoted behind a details disclosure, still available.
    expect(
      screen.getByTestId(`merge-run-details-${MERGE_RUN_ID}`),
    ).toBeInTheDocument();

    // Primary action reads as the user goal and still uses the approved lane.
    const pushButton = screen.getByTestId(
      `publish-approved-rebase-${MERGE_RUN_ID}`,
    );
    expect(pushButton).toHaveTextContent(/Approve rebased publication/i);
    expect(screen.queryByTestId("popover-retry-button")).toBeNull();
    expect(screen.queryByTestId("push-to-remote-button")).toBeNull();
    expect(screen.queryByTestId("popover-publish")).toBeNull();
    await user.click(pushButton);
    await waitFor(() => expect(publishCalls).toBe(1));
    expect(genericPushCalls).toBe(0);
    // Once the approval settles, the detail is still open.
    await waitFor(() =>
      expect(
        screen.getByTestId(`publish-approved-rebase-${MERGE_RUN_ID}`),
      ).toHaveTextContent(/Approve rebased publication/i),
    );
    expect(screen.getByTestId("branch-tracking-popover")).toBeVisible();
  });

  it("shows and approves the exact complete set when two repos need reset review", async () => {
    const secondWbrId = "repo-2";
    const secondRunId = "merge-run-2";
    let requestBody: unknown;
    server.use(
      http.get(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/sync_runs`,
        () =>
          HttpResponse.json({
            data: {
              branch: {
                workspaceId: WORKSPACE_ID,
                repos: [
                  {
                    repoId: REPO_ID,
                    repoName: "code",
                    repoBranchName: "main",
                    trackingMode: "remote_tracking",
                    headCommit: "aaaaaaaa",
                    remoteBranch: "main",
                    baselineCommit: "11111111",
                    syncPausedReason: "watermark_violation",
                  },
                  {
                    repoId: "repo-2",
                    repoName: "docs",
                    repoBranchName: "main",
                    trackingMode: "remote_tracking",
                    headCommit: "bbbbbbbb",
                    remoteBranch: "main",
                    baselineCommit: "22222222",
                    syncPausedReason: "watermark_violation",
                  },
                ],
              },
              runs: { data: [], total: 0, page: 1, perPage: 20 },
            },
          }),
      ),
      mergeRunsHandler([
        fakeMergeRun(),
        fakeMergeRun({
          id: secondRunId,
          repoId: secondWbrId,
          sourceHeads: {
            workspace_tip: "bbbbbbbb",
            mirror_tip: "22222222",
          },
          targetHeads: { baseline: "22222222" },
        }),
      ]),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/remote_tracking/approved_rebase_publish`,
        async ({ request }) => {
          requestBody = await request.json();
          return HttpResponse.json({
            data: {
              enqueued: 2,
              coordinated: {
                operation: "approved_rebase_publish",
                status: "queued",
                repos: [],
              },
            },
          });
        },
      ),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    expect(
      await screen.findByTestId(`merge-run-card-${MERGE_RUN_ID}`),
    ).toBeVisible();
    expect(screen.getByTestId(`merge-run-card-${secondRunId}`)).toBeVisible();
    expect(
      screen.getByTestId("approval-checkpoint-set-summary"),
    ).toHaveTextContent(/approves all 2 displayed checkpoints/i);
    const actions = screen.getAllByTestId(/^publish-approved-rebase-/);
    expect(actions).toHaveLength(1);
    expect(actions[0]).toHaveTextContent(/Approve 2 rebased publications/i);
    await user.click(actions[0]);
    await waitFor(() =>
      expect(requestBody).toEqual({
        expectedMergeRunIds: [MERGE_RUN_ID, secondRunId],
      }),
    );
  });

  it("renders the refreshed checkpoint after the first approval", async () => {
    let activeMergeRuns: Array<Record<string, unknown>> = [fakeMergeRun()];
    let mergeRunReads = 0;
    const approvalBodies: unknown[] = [];
    server.use(
      syncRunsHandler({
        syncPausedReason: "watermark_violation",
        latestRunStatus: "fetch_failed",
      }),
      http.get(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs`,
        () => {
          mergeRunReads += 1;
          return HttpResponse.json({ data: { items: activeMergeRuns } });
        },
      ),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/remote_tracking/approved_rebase_publish`,
        async ({ request }) => {
          const body = await request.json();
          approvalBodies.push(body);
          if (approvalBodies.length > 1) {
            return HttpResponse.json({
              data: {
                enqueued: 2,
                coordinated: {
                  operation: "approved_rebase_publish",
                  status: "queued",
                  message: "Approved publication queued.",
                  repos: [],
                },
              },
            });
          }
          activeMergeRuns = [
            fakeMergeRun({
              id: "successor-checkpoint",
              sourceHeads: {
                workspace_tip: "aaaaaaaaaaaaaaaa",
                mirror_tip: "cccccccccccccccc",
              },
              targetHeads: {
                baseline: "bbbbbbbbbbbbbbbb",
                refresh_reason: "provider_descendant_advanced",
                previous_remote_tip: "bbbbbbbbbbbbbbbb",
                supersedes_merge_run_id: MERGE_RUN_ID,
              },
            }),
          ];
          return HttpResponse.json({
            data: {
              enqueued: 0,
              coordinated: {
                operation: "approved_rebase_publish",
                status: "approval_checkpoint_refreshed",
                message: "Review the new checkpoint and approve again.",
                repos: [],
              },
            },
          });
        },
      ),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    await user.click(
      await screen.findByTestId(`publish-approved-rebase-${MERGE_RUN_ID}`),
    );

    const refreshed = await screen.findByTestId(
      "approval-checkpoint-refreshed-successor-checkpoint",
    );
    expect(mergeRunReads).toBeGreaterThan(1);
    expect(screen.getByTestId("branch-tracking-popover")).toBeVisible();
    expect(screen.queryByTestId(`merge-run-card-${MERGE_RUN_ID}`)).toBeNull();
    expect(refreshed).toHaveAttribute("role", "status");
    expect(refreshed).toHaveAttribute("aria-live", "polite");
    expect(refreshed).toHaveTextContent(/provider branch moved forward/i);
    expect(refreshed).toHaveTextContent(/preserved.*for audit/i);
    expect(refreshed).toHaveTextContent(/approve again/i);
    expect(
      screen.getByTestId("publish-approved-rebase-successor-checkpoint"),
    ).toHaveTextContent(/Approve rebased publication/i);
    expect(approvalBodies).toEqual([{ expectedMergeRunIds: [MERGE_RUN_ID] }]);

    await user.click(
      screen.getByTestId("publish-approved-rebase-successor-checkpoint"),
    );
    await waitFor(() =>
      expect(approvalBodies).toEqual([
        { expectedMergeRunIds: [MERGE_RUN_ID] },
        { expectedMergeRunIds: ["successor-checkpoint"] },
      ]),
    );
  });

  it("explains a provider-descendant checkpoint after a later reload", async () => {
    server.use(
      syncRunsHandler({
        syncPausedReason: "watermark_violation",
        latestRunStatus: "fetch_failed",
      }),
      mergeRunsHandler([
        fakeMergeRun({
          id: "successor-checkpoint",
          sourceHeads: {
            workspace_tip: "aaaaaaaaaaaaaaaa",
            mirror_tip: "cccccccccccccccc",
          },
          targetHeads: {
            baseline: "bbbbbbbbbbbbbbbb",
            refresh_reason: "provider_descendant_advanced",
            previous_remote_tip: "bbbbbbbbbbbbbbbb",
            supersedes_merge_run_id: MERGE_RUN_ID,
          },
        }),
      ]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const refreshed = await screen.findByTestId(
      "approval-checkpoint-refreshed-successor-checkpoint",
    );
    expect(refreshed).toHaveTextContent(/provider branch moved forward/i);
    expect(refreshed).toHaveTextContent(/preserved.*for audit/i);
    expect(refreshed).toHaveTextContent(/approve again/i);
    expect(
      screen.getByTestId("publish-approved-rebase-successor-checkpoint"),
    ).toHaveTextContent(/Approve rebased publication/i);
  });

  it("keeps watermark violations blocked when the matching run is not reset-below-baseline", async () => {
    server.use(
      syncRunsHandler({
        syncPausedReason: "watermark_violation",
        latestRunStatus: "fetch_failed",
      }),
      mergeRunsHandler([fakeMergeRun({ phase: "workspace_rebase_preflight" })]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const statusLine = await screen.findByTestId("popover-status-line");
    expect(statusLine).toHaveAttribute("data-state", "blocked");
    expect(
      screen.queryByTestId(`publish-approved-rebase-${MERGE_RUN_ID}`),
    ).toBeNull();
    expect(
      await screen.findByTestId("popover-retry-button"),
    ).toBeInTheDocument();
  });

  it("keeps watermark violations blocked when the reset run belongs to another repo", async () => {
    server.use(
      syncRunsHandler({
        syncPausedReason: "watermark_violation",
      }),
      mergeRunsHandler([fakeMergeRun({ repoId: "wbr-other" })]),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const statusLine = await screen.findByTestId("popover-status-line");
    expect(statusLine).toHaveAttribute("data-state", "blocked");
    expect(
      screen.queryByTestId(`publish-approved-rebase-${MERGE_RUN_ID}`),
    ).toBeNull();
  });

  it("shows a precise blocked reason when approved publish is rejected", async () => {
    server.use(
      syncRunsHandler({
        syncPausedReason: "watermark_violation",
        latestRunStatus: "diverged",
      }),
      mergeRunsHandler([fakeMergeRun()]),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/remote_tracking/approved_rebase_publish`,
        () =>
          HttpResponse.json(
            {
              error: {
                code: "VALIDATION_ERROR",
                message:
                  'Validation error: {"code":"APPROVED_REBASE_REMOTE_HISTORY_CHANGED"}',
                details: {
                  code: "APPROVED_REBASE_REMOTE_HISTORY_CHANGED",
                  operation: "approved_rebase_publish",
                  message:
                    "The provider branch no longer descends from the approved checkpoint; rebase against the current provider history before publishing.",
                  modal: {
                    severity: "error",
                    title: "Coordinated repo sync failed",
                    message:
                      "The provider branch no longer descends from the approved checkpoint; rebase against the current provider history before publishing.",
                    repos: [],
                  },
                },
              },
            },
            { status: 422 },
          ),
      ),
    );
    const user = userEvent.setup();
    renderApp(
      <BranchTrackingInline
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        repoLabel="acme/demo"
      />,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("repo-tracking-expander"));
    const pushButton = await screen.findByTestId(
      `publish-approved-rebase-${MERGE_RUN_ID}`,
    );
    await user.click(pushButton);

    const blocked = await screen.findByTestId(
      `publish-blocked-${MERGE_RUN_ID}`,
    );
    expect(blocked).toHaveTextContent(/Approval blocked:/i);
    expect(blocked).toHaveTextContent(/no longer descends/i);
    expect(blocked).not.toHaveTextContent(/Validation error:/i);
    expect(blocked).toHaveAttribute("role", "alert");
  });
});

describe("BranchTrackingPopover adopting the provider history (g5.workspace.remote.adopt-provider-history)", () => {
  const PREVIEW_ROUTE = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/provider_history_preview`;
  const ACCEPT_ROUTE = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/accept_provider_history`;
  const ownerGate = (over: Record<string, unknown> = {}) => ({
    kind: "live" as const,
    canWrite: true,
    live: true,
    canStart: false,
    reviewCopy: false,
    reviewIdentityKnown: true,
    owner: true,
    orgAdmin: false,
    workspaceNoun: "the workspace",
    ...over,
  });
  let accepted: unknown;

  beforeEach(() => {
    accepted = null;
    server.use(
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () =>
        HttpResponse.json({
          data: {
            ...createMockWorkspace({ id: WORKSPACE_ID }),
            orgId: "org-1",
            pushMode: "manual",
          },
        }),
      ),
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/config`, () =>
        HttpResponse.json({
          data: { effectiveSettings: {}, settings: {}, secretKeys: [] },
        }),
      ),
      http.post(LAUNCH_PREVIEW_ROUTE, async ({ request }) => {
        const { preferences } = (await request.json()) as {
          preferences: AgentLaunchPreferencesV1;
        };
        return HttpResponse.json({
          data: { resolved: resolvedLaunch(preferences) },
        });
      }),
      syncRunsHandler({
        syncPausedReason: "diverged",
        latestRunStatus: "diverged",
      }),
      http.post(PREVIEW_ROUTE, () =>
        HttpResponse.json({
          data: {
            workspaceId: WORKSPACE_ID,
            repoId: REPO_ID,
            remoteBranch: "main",
            head: "aaaaaaaaaaaaaaaa",
            providerTip: "bbbbbbbbbbbbbbbb",
            discardedCommits: 2,
            initialOnly: false,
          },
        }),
      ),
      http.post(ACCEPT_ROUTE, async ({ request }) => {
        accepted = await request.json();
        return HttpResponse.json({
          data: {
            workspaceId: WORKSPACE_ID,
            repoId: REPO_ID,
            head: "bbbbbbbbbbbbbbbb",
          },
        });
      }),
    );
  });

  async function openDetail(
    gate: ReturnType<typeof ownerGate>,
    run = fakeMergeRun({ phase: "workspace_rebase_preflight" }),
  ) {
    server.use(mergeRunsHandler([run]));
    const user = userEvent.setup();
    renderApp(
      <WorkspaceLiveGateProvider value={gate}>
        <BranchTrackingInline
          workspaceId={WORKSPACE_ID}
          repoId={REPO_ID}
          repoLabel="acme/demo"
        />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );
    await user.click(await screen.findByTestId("repo-tracking-expander"));
    await screen.findByTestId(`merge-run-card-${MERGE_RUN_ID}`);
    return user;
  }

  it("offers the owner adopting beside Resolve with agent and adopts the reviewed history", async () => {
    const user = await openDetail(ownerGate());

    const adopt = screen.getByTestId(`adopt-provider-history-${MERGE_RUN_ID}`);
    expect(
      screen.getByTestId(`start-merge-${MERGE_RUN_ID}`),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("accept-provider-history-button"));
    const review = await screen.findByTestId("accept-provider-history-preview");
    expect(adopt).toContainElement(review);
    expect(review).toHaveTextContent("bbbbbbb");
    expect(review).toHaveTextContent("2 local commits not on the provider");
    expect(review).toHaveTextContent("Nothing is pushed to the provider.");
    expect(accepted).toBeNull();

    await user.click(screen.getByTestId("accept-provider-history-confirm"));
    await waitFor(() =>
      expect(accepted).toEqual({
        expectedProviderTip: "bbbbbbbbbbbbbbbb",
        expectedHead: "aaaaaaaaaaaaaaaa",
      }),
    );
    // The run completes in the background; the detail says so meanwhile.
    expect(
      await screen.findByTestId("accept-provider-history-adopted"),
    ).toHaveTextContent("Tracking resumes once G5 confirms the reset.");
  });

  it("keeps adopting disabled with the VM reason while the workspace is stopped", async () => {
    const user = await openDetail(
      ownerGate({ kind: "stopped", live: false, canStart: true }),
    );

    await user.click(screen.getByTestId("accept-provider-history-button"));
    const confirm = await screen.findByTestId(
      "accept-provider-history-confirm",
    );
    expect(confirm).toBeDisabled();
    expect(confirm).toHaveAttribute(
      "title",
      "Start the workspace to adopt the provider history.",
    );
    await user.click(
      screen.getByTestId("accept-provider-history-confirm-hint-trigger"),
    );
    expect(
      screen.getByTestId("accept-provider-history-confirm-hint-start"),
    ).toBeInTheDocument();
    expect(accepted).toBeNull();
  });

  it("keeps adopting from a reader, with the status text", async () => {
    await openDetail(
      ownerGate({ kind: "reader", canWrite: false, live: false, owner: false }),
    );

    expect(
      screen.getByTestId("accept-provider-history-readonly"),
    ).toHaveTextContent("Only the owner can adopt the provider history.");
    expect(screen.queryByTestId("accept-provider-history-button")).toBeNull();
  });

  it("offers no adopting once a resolution agent owns the run", async () => {
    await openDetail(
      ownerGate(),
      fakeMergeRun({
        phase: "workspace_rebase_preflight",
        resolutionTaskId: "task-1",
        resolutionSessionId: "session-1",
        resolutionTaskStatus: "completed",
      }),
    );

    expect(screen.queryByTestId("provider-history-adoption")).toBeNull();
  });
});
