/**
 * Vitest tests for BranchTrackingInline — the Workspace Details card's
 * remote-tracking status pill and in-flow detail expander.
 *
 * Ontology: g5.workspace.remote-ui.status, .detail, .unknown, .actions
 */

import React from "react";
import { beforeEach, describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { renderApp } from "../../test/render";
import { createMockUser, createMockWorkspace } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { BranchTrackingInline } from "./BranchTrackingInline";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";

const WORKSPACE_ID = "ws-abc";
const REPO_ID = "repo-1";
const SYNC_RUNS_ROUTE = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/sync_runs`;
const MERGE_RUNS_ROUTE = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs`;

function syncRunsBody(opts: {
  trackingMode?: string;
  syncPausedReason?: string;
  remoteTipAdvisorySha?: string;
  baselineCommit?: string;
  importHeldBy?: { pullRequestId: string; title: string };
}) {
  return {
    data: {
      branch: {
        workspaceId: WORKSPACE_ID,
        repos: [
          {
            repoId: REPO_ID,
            repoName: "demo-repo",
            repoBranchName: "main",
            trackingMode: opts.trackingMode ?? "remote_tracking",
            headCommit: "deadbeef",
            remoteBranch: "main",
            baselineCommit: opts.baselineCommit ?? "deadbeef",
            syncPausedReason: opts.syncPausedReason,
            remoteTipAdvisorySha: opts.remoteTipAdvisorySha,
            importHeldBy: opts.importHeldBy,
          },
        ],
      },
      runs: { data: [], total: 0, page: 1, perPage: 20 },
    },
  };
}

function syncRuns(opts: Parameters<typeof syncRunsBody>[0] = {}) {
  return http.get(SYNC_RUNS_ROUTE, () => HttpResponse.json(syncRunsBody(opts)));
}

function mergeRuns(items: Array<Record<string, unknown>> = []) {
  return http.get(MERGE_RUNS_ROUTE, () =>
    HttpResponse.json({ data: { items } }),
  );
}

function resetRun() {
  return {
    id: "merge-run-1",
    workspaceId: WORKSPACE_ID,
    sourceKind: "remote_tracking",
    sourceBranchName: "main",
    status: "waiting_for_human",
    phase: "reset_below_baseline",
    blockingReason: null,
    repoId: REPO_ID,
    sourceHeads: { workspace_tip: "aaaa", mirror_tip: "bbbb" },
    targetHeads: { baseline: "bbbb" },
    startedAt: "2026-05-28T01:00:00Z",
  };
}

/** Wait for the derived pill; the loading pill is a different node. */
async function settledPill(state: string) {
  await waitFor(() =>
    expect(screen.getByTestId("repo-status-pill")).toHaveAttribute(
      "data-state",
      state,
    ),
  );
  return screen.getByTestId("repo-status-pill");
}

function renderInline() {
  return renderApp(
    <BranchTrackingInline
      workspaceId={WORKSPACE_ID}
      repoId={REPO_ID}
      repoLabel="acme/demo"
    />,
    { user: createMockUser() },
  );
}

describe("BranchTrackingInline", () => {
  beforeEach(() => {
    server.use(
      mergeRuns(),
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () =>
        HttpResponse.json({
          data: {
            ...createMockWorkspace({ id: WORKSPACE_ID }),
            orgId: "org-1",
            pushMode: "manual",
          },
        }),
      ),
    );
  });

  it.each([
    ["healthy", {}, "Tracking healthy"],
    [
      "behind",
      { remoteTipAdvisorySha: "cafebabe" },
      "Remote moved — your branch is behind",
    ],
    [
      "ahead",
      { baselineCommit: "0000", remoteTipAdvisorySha: "0000" },
      "Ahead — local commits to push",
    ],
    [
      "blocked",
      { syncPausedReason: "diverged", remoteTipAdvisorySha: "cafebabe" },
      "Paused — merge required",
    ],
  ])("names the %s state in words", async (state, opts, label) => {
    server.use(syncRuns(opts));
    renderInline();

    const pill = await settledPill(state);
    expect(pill).toHaveTextContent(label);
    expect(pill.textContent ?? "").not.toMatch(/\d+\s*commits?/i);
  });

  it("exposes the pause reason on a blocked pill", async () => {
    server.use(syncRuns({ syncPausedReason: "auth_required" }));
    renderInline();

    const pill = await settledPill("blocked");
    expect(pill).toHaveAttribute("data-reason", "auth_required");
    expect(pill).toHaveTextContent("Authentication required");
  });

  it("presents a provider-rewrite checkpoint as review, not blocked", async () => {
    server.use(
      syncRuns({ syncPausedReason: "watermark_violation" }),
      mergeRuns([resetRun()]),
    );
    renderInline();

    const pill = await settledPill("review");
    expect(pill).toHaveTextContent(/Review required/);
  });

  it("names the half-merged workspace PR holding auto-pull", async () => {
    server.use(
      syncRuns({
        importHeldBy: { pullRequestId: "pr-1", title: "Cross-repo change" },
      }),
      mergeRuns(),
    );
    const user = userEvent.setup();
    renderInline();
    await user.click(
      await screen.findByRole("button", {
        name: "Details & actions for acme/demo",
      }),
    );
    const notice = await screen.findByTestId("popover-import-held");
    expect(notice).toHaveTextContent(/Cross-repo change/);
    expect(notice).toHaveTextContent(/Pull imports this repository alone/);
  });

  it("keeps the card head free of tracking mutations", async () => {
    server.use(
      syncRuns({ baselineCommit: "0000", remoteTipAdvisorySha: "0000" }),
    );
    renderInline();

    const inline = await screen.findByTestId("repo-tracking-inline");
    await screen.findByTestId("repo-tracking-expander");
    // Collapsed: the only control is the expander.
    expect(within(inline).getAllByRole("button")).toHaveLength(1);
    expect(screen.queryByTestId("push-to-remote-button")).toBeNull();
    expect(screen.queryByTestId("pull-from-remote-button")).toBeNull();
    expect(screen.queryByTestId("popover-retry-button")).toBeNull();
  });

  it("shows a loading pill and no expander until tracking state arrives", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.get(SYNC_RUNS_ROUTE, async () => {
        await gate;
        return HttpResponse.json(syncRunsBody({}));
      }),
    );
    renderInline();

    const pill = await screen.findByTestId("repo-status-pill");
    expect(pill).toHaveAttribute("data-state", "loading");
    expect(pill).toHaveTextContent("Loading tracking status…");
    expect(screen.queryByTestId("repo-tracking-expander")).toBeNull();

    release();
    await waitFor(() =>
      expect(screen.getByTestId("repo-status-pill")).toHaveAttribute(
        "data-state",
        "healthy",
      ),
    );
    expect(screen.getByTestId("repo-tracking-expander")).toBeInTheDocument();
  });

  it("shows an unavailable pill with a retry that recovers the normal pill", async () => {
    let calls = 0;
    server.use(
      http.get(SYNC_RUNS_ROUTE, () => {
        calls += 1;
        return calls === 1
          ? HttpResponse.json(
              { error: { message: "unavailable" } },
              { status: 500 },
            )
          : HttpResponse.json(syncRunsBody({}));
      }),
    );
    const user = userEvent.setup();
    renderInline();

    const retry = await screen.findByTestId("repo-tracking-retry");
    expect(screen.getByTestId("repo-status-pill")).toHaveAttribute(
      "data-state",
      "unavailable",
    );
    expect(screen.getByTestId("repo-status-pill")).toHaveTextContent(
      "Tracking status unavailable",
    );
    expect(screen.queryByTestId("repo-tracking-expander")).toBeNull();

    await user.click(retry);
    await waitFor(() =>
      expect(screen.getByTestId("repo-status-pill")).toHaveAttribute(
        "data-state",
        "healthy",
      ),
    );
    expect(screen.getByTestId("repo-tracking-expander")).toBeInTheDocument();
  });

  it("says tracking is pending when the feed has not reported the repo yet", async () => {
    server.use(syncRuns({ trackingMode: "tracking" }));
    renderInline();

    const pill = await settledPill("loading");
    await waitFor(() =>
      expect(screen.getByTestId("repo-status-pill")).toHaveTextContent(
        "Tracking status pending",
      ),
    );
    expect(pill).toBeInTheDocument();
    expect(screen.queryByTestId("repo-tracking-expander")).toBeNull();
  });

  it("waits for merge runs before deriving review, instead of guessing blocked", async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      syncRuns({ syncPausedReason: "watermark_violation" }),
      http.get(MERGE_RUNS_ROUTE, async () => {
        await gate;
        return HttpResponse.json({ data: { items: [resetRun()] } });
      }),
    );
    renderInline();

    // Sync runs have arrived, but merge runs have not: still loading.
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.getByTestId("repo-status-pill")).toHaveAttribute(
      "data-state",
      "loading",
    );
    expect(screen.queryByTestId("repo-tracking-expander")).toBeNull();

    release();
    await settledPill("review");
  });

  it("treats a merge-runs failure as unavailable and retries both feeds", async () => {
    let mergeCalls = 0;
    server.use(
      syncRuns({}),
      http.get(MERGE_RUNS_ROUTE, () => {
        mergeCalls += 1;
        return mergeCalls === 1
          ? HttpResponse.json({ error: { message: "down" } }, { status: 500 })
          : HttpResponse.json({ data: { items: [] } });
      }),
    );
    const user = userEvent.setup();
    renderInline();

    await user.click(await screen.findByTestId("repo-tracking-retry"));
    await settledPill("healthy");
    expect(mergeCalls).toBeGreaterThanOrEqual(2);
  });

  it.each([
    [
      "a provider rewrite",
      { syncPausedReason: "watermark_violation" },
      [resetRun()],
    ],
    ["a divergence", { syncPausedReason: "diverged" }, []],
  ])(
    "offers no VM-only reconciliation on Base after %s",
    async (_label, opts, runs) => {
      server.use(
        syncRuns(opts),
        mergeRuns(runs),
        http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () =>
          HttpResponse.json({
            data: {
              ...createMockWorkspace({ id: WORKSPACE_ID }),
              orgId: "org-1",
              isBase: true,
              pushMode: "manual",
            },
          }),
        ),
      );
      const user = userEvent.setup();
      renderInline();

      await user.click(
        await screen.findByRole("button", {
          name: "Details & actions for acme/demo",
        }),
      );
      expect(
        await screen.findByTestId("popover-base-history-paused"),
      ).toBeVisible();
      expect(
        screen.queryByTestId("popover-merge-required"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("popover-paused-divergence-resolution"),
      ).not.toBeInTheDocument();
    },
  );

  it("lets the Base owner review and accept the provider history", async () => {
    const ACCEPT_ROUTE = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/accept_provider_history`;
    const PREVIEW_ROUTE = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/provider_history_preview`;
    let accepted: unknown = null;
    server.use(
      syncRuns({ syncPausedReason: "diverged" }),
      mergeRuns([]),
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () =>
        HttpResponse.json({
          data: {
            ...createMockWorkspace({ id: WORKSPACE_ID }),
            orgId: "org-1",
            isBase: true,
            pushMode: "manual",
          },
        }),
      ),
      http.post(PREVIEW_ROUTE, () =>
        HttpResponse.json({
          data: {
            workspaceId: WORKSPACE_ID,
            repoId: REPO_ID,
            remoteBranch: "main",
            head: "deadbeef00000000000000000000000000000000",
            providerTip: "cafebabe00000000000000000000000000000000",
            discardedCommits: 2,
            syncPausedReason: "diverged",
          },
        }),
      ),
      http.post(ACCEPT_ROUTE, async ({ request }) => {
        accepted = await request.json();
        return HttpResponse.json({
          data: {
            workspaceId: WORKSPACE_ID,
            repoId: REPO_ID,
            head: "cafebabe00000000000000000000000000000000",
          },
        });
      }),
    );
    const user = userEvent.setup();
    renderInline();

    await user.click(
      await screen.findByRole("button", {
        name: "Details & actions for acme/demo",
      }),
    );
    await user.click(
      await screen.findByTestId("accept-provider-history-button"),
    );
    const review = await screen.findByTestId("accept-provider-history-preview");
    expect(review).toHaveTextContent("cafebab");
    expect(review).toHaveTextContent("2 Base commits");
    expect(review).toHaveTextContent("Nothing is pushed");
    expect(accepted).toBeNull();

    await user.click(screen.getByTestId("accept-provider-history-confirm"));
    await waitFor(() =>
      expect(accepted).toEqual({
        expectedProviderTip: "cafebabe00000000000000000000000000000000",
        expectedHead: "deadbeef00000000000000000000000000000000",
      }),
    );
  });

  describe("Base provider history for readers (g5.workspace.ui.reader-publication)", () => {
    const ACCEPT_ROUTE = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/accept_provider_history`;
    const PREVIEW_ROUTE = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/provider_history_preview`;
    const gate = (owner: boolean) => ({
      kind: owner ? ("base" as const) : ("reader" as const),
      canWrite: false,
      live: false,
      canStart: false,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner,
      workspaceNoun: "Base",
    });
    let accepted: unknown;
    let previewed: number;
    beforeEach(() => {
      accepted = null;
      previewed = 0;
      server.use(
        syncRuns({ syncPausedReason: "diverged" }),
        mergeRuns([]),
        http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () =>
          HttpResponse.json({
            data: {
              ...createMockWorkspace({ id: WORKSPACE_ID }),
              orgId: "org-1",
              isBase: true,
              pushMode: "manual",
            },
          }),
        ),
        http.post(PREVIEW_ROUTE, () => {
          previewed += 1;
          return HttpResponse.json({
            data: {
              workspaceId: WORKSPACE_ID,
              repoId: REPO_ID,
              remoteBranch: "main",
              head: "deadbeef00000000000000000000000000000000",
              providerTip: "cafebabe00000000000000000000000000000000",
              discardedCommits: 2,
              syncPausedReason: "diverged",
            },
          });
        }),
        http.post(ACCEPT_ROUTE, async ({ request }) => {
          accepted = await request.json();
          return HttpResponse.json({ data: {} });
        }),
      );
    });

    it("keeps the paused status readable and hides Accept provider history from a reader", async () => {
      const user = userEvent.setup();
      renderApp(
        <WorkspaceLiveGateProvider value={gate(false)}>
          <BranchTrackingInline
            workspaceId={WORKSPACE_ID}
            repoId={REPO_ID}
            repoLabel="acme/demo"
          />
        </WorkspaceLiveGateProvider>,
        { user: createMockUser() },
      );
      await user.click(
        await screen.findByRole("button", {
          name: "Details & actions for acme/demo",
        }),
      );
      const paused = await screen.findByTestId("popover-base-history-paused");
      expect(paused).toHaveTextContent("Base's history differs");
      expect(
        screen.getByTestId("accept-provider-history-readonly"),
      ).toHaveTextContent(
        "Only Base's owner or an organization administrator can adopt the provider history.",
      );
      expect(screen.queryByTestId("accept-provider-history-button")).toBeNull();
      expect(previewed).toBe(0);
    });

    it.each([
      ["an organization administrator", true, true],
      ["a plain member", false, false],
    ])(
      "offers Accept provider history on Base to %s: %s",
      async (_label, orgAdmin, offered) => {
        const user = userEvent.setup();
        renderApp(
          <WorkspaceLiveGateProvider
            value={{
              ...gate(false),
              kind: "base",
              owner: false,
              orgAdmin,
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
        await user.click(
          await screen.findByRole("button", {
            name: "Details & actions for acme/demo",
          }),
        );
        await screen.findByTestId("popover-base-history-paused");
        if (!offered) {
          expect(
            screen.getByTestId("accept-provider-history-readonly"),
          ).toBeVisible();
          expect(
            screen.queryByTestId("accept-provider-history-button"),
          ).toBeNull();
          return;
        }
        await user.click(screen.getByTestId("accept-provider-history-button"));
        await screen.findByTestId("accept-provider-history-preview");
        // Base has no VM to wait for: the administrator confirms directly.
        const confirm = screen.getByTestId("accept-provider-history-confirm");
        expect(confirm).toBeEnabled();
        await user.click(confirm);
        await waitFor(() =>
          expect(accepted).toEqual({
            expectedProviderTip: "cafebabe00000000000000000000000000000000",
            expectedHead: "deadbeef00000000000000000000000000000000",
          }),
        );
      },
    );

    it("drops a reviewed reset once ownership moves away and submits nothing", async () => {
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
      await user.click(
        await screen.findByRole("button", {
          name: "Details & actions for acme/demo",
        }),
      );
      await user.click(
        await screen.findByTestId("accept-provider-history-button"),
      );
      await screen.findByTestId("accept-provider-history-preview");
      expect(previewed).toBe(1);

      await user.click(screen.getByTestId("lose-ownership"));

      await waitFor(() =>
        expect(
          screen.queryByTestId("accept-provider-history-preview"),
        ).toBeNull(),
      );
      expect(
        screen.queryByTestId("accept-provider-history-confirm"),
      ).toBeNull();
      expect(
        screen.getByTestId("accept-provider-history-readonly"),
      ).toBeVisible();
      expect(accepted).toBeNull();
    });
  });

  it("names the expander for its repository and renders each action once", async () => {
    server.use(
      syncRuns({ baselineCommit: "0000", remoteTipAdvisorySha: "0000" }),
    );
    const user = userEvent.setup();
    renderInline();

    const expander = await screen.findByRole("button", {
      name: "Details & actions for acme/demo",
    });
    await user.click(expander);
    expect(expander).toHaveAccessibleName(
      "Hide details & actions for acme/demo",
    );
    expect(await screen.findAllByTestId("push-to-remote-button")).toHaveLength(
      1,
    );
    expect(screen.getAllByTestId("pull-from-remote-button")).toHaveLength(1);
  });
});
