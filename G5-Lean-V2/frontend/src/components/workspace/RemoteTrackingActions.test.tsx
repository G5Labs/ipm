/**
 * Vitest unit tests for RemoteTrackingActions — the per-workspace
 * remote-tracking publish controls (Push to Remote + auto/manual toggle).
 *
 * Scenarios:
 * 1. Manual mode (default): toggle off, hint explains explicit publish.
 * 2. Auto mode: toggle on, hint explains automatic publish.
 * 3. Clicking "Push to Remote" POSTs to /push_to_remote.
 * 4. Toggling on PUTs `{ pushMode: 'auto' }` to /push_mode.
 * 5. Clicking Push acknowledges the async enqueue with a "queued/publishing"
 *    notice (C30).
 *
 * SOT:
 * - designs/REMOTE_REPO_TRACKING.md §6.2; arch D49/D66/D67; PM D20
 * - Chat C27 (auto/manual toggle + PushToRemote), C28 (per-workspace, manual default),
 *   C30 (push acknowledgement)
 */

import { describe, it, expect } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { renderApp } from "../../test/render";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { RemoteTrackingActions } from "./RemoteTrackingActions";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";

const WORKSPACE_ID = "ws-push";

function workspaceDetail(pushMode: "manual" | "auto", isBase = false) {
  return {
    id: WORKSPACE_ID,
    orgId: "org-1",
    name: "demo",
    isBase,
    description: null,
    clonedFromId: null,
    docsRepoId: null,
    createdBy: "user-1",
    createdAt: "2026-05-28T00:00:00Z",
    updatedAt: "2026-05-28T00:00:00Z",
    pushMode,
    branches: [],
  };
}

function mockWorkspace(pushMode: "manual" | "auto", isBase = false) {
  return http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () =>
    HttpResponse.json({
      data: workspaceDetail(pushMode, isBase),
    }),
  );
}

describe("RemoteTrackingActions", () => {
  it("keeps Fetch and Push for the owner of a read-only workspace and hides them from a reader (g5.workspace.ui.reader-publication)", async () => {
    server.use(mockWorkspace("manual"));
    const gate = (kind: "read_only" | "reader") => ({
      kind,
      canWrite: false,
      live: false,
      canStart: false,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner: kind === "read_only",
      workspaceNoun: "the workspace",
    });

    const owner = renderApp(
      <WorkspaceLiveGateProvider value={gate("read_only")}>
        <RemoteTrackingActions workspaceId={WORKSPACE_ID} />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );
    expect(
      await screen.findByTestId("pull-from-remote-button"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("push-to-remote-button")).toBeInTheDocument();
    owner.unmount();

    renderApp(
      <WorkspaceLiveGateProvider value={gate("reader")}>
        <RemoteTrackingActions workspaceId={WORKSPACE_ID} />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );
    await waitFor(() =>
      expect(screen.queryByTestId("remote-tracking-actions")).toBeNull(),
    );
  });

  it("renders both Pull and Push on non-Base and reflects manual mode (toggle off)", async () => {
    server.use(mockWorkspace("manual"));

    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    // Non-Base exposes both directions symmetrically (#1198).
    expect(
      await screen.findByTestId("pull-from-remote-button"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("push-to-remote-button")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("auto-push-toggle")).not.toBeChecked(),
    );
    expect(screen.getByTestId("push-mode-hint")).toHaveTextContent(/Manual/i);
  });

  it("reflects auto mode (toggle on)", async () => {
    server.use(mockWorkspace("auto"));

    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    await waitFor(() =>
      expect(screen.getByTestId("auto-push-toggle")).toBeChecked(),
    );
    expect(screen.getByTestId("push-mode-hint")).toHaveTextContent(/Auto/i);
  });

  it("clicking Push to Remote POSTs to /push_to_remote", async () => {
    let pushed = false;
    server.use(
      mockWorkspace("manual"),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/push_to_remote`, () => {
        pushed = true;
        return HttpResponse.json({ data: { enqueued: 1 } });
      }),
    );

    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    fireEvent.click(await screen.findByTestId("push-to-remote-button"));
    await waitFor(() => expect(pushed).toBe(true));
  });

  it("fetching remote refs queues sync without promising file changes", async () => {
    let pulled = false;
    server.use(
      mockWorkspace("manual"),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/sync`, () => {
        pulled = true;
        return HttpResponse.json({
          data: { branchesUpdated: 1, branchesSkipped: 1, details: [] },
        });
      }),
    );

    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    const fetchButton = await screen.findByTestId("pull-from-remote-button");
    expect(fetchButton).toHaveTextContent("Fetch from Remote");
    fireEvent.click(fetchButton);
    await waitFor(() => expect(pulled).toBe(true));
    expect(
      await screen.findByText(/workspace files stay unchanged/i),
    ).toBeInTheDocument();
    expect(screen.getByTestId("pull-complete-notice")).toHaveTextContent(
      "G5 will accept fast-forward updates",
    );
    expect(screen.getByTestId("pull-complete-notice")).toHaveTextContent(
      "use git pull in the workspace",
    );
  });

  it("toggling on PUTs { pushMode: 'auto' } to /push_mode", async () => {
    let body: unknown = null;
    server.use(
      mockWorkspace("manual"),
      http.put(
        `/api/v1/workspaces/${WORKSPACE_ID}/push_mode`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({ data: { pushMode: "auto" } });
        },
      ),
    );

    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    const toggle = await screen.findByTestId("auto-push-toggle");
    await waitFor(() => expect(toggle).not.toBeChecked());
    fireEvent.click(toggle);

    await waitFor(() => expect(body).toEqual({ pushMode: "auto" }));
  });

  it("acknowledges the async push with a queued/publishing notice (C30)", async () => {
    server.use(
      mockWorkspace("manual"),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/push_to_remote`, () =>
        HttpResponse.json({ data: { enqueued: 2 } }),
      ),
    );

    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    // No notice until the user pushes.
    expect(screen.queryByTestId("push-queued-notice")).not.toBeInTheDocument();

    fireEvent.click(await screen.findByTestId("push-to-remote-button"));

    // The async enqueue is acknowledged: count + "publishing" copy that points
    // the user at the sync feed (the worker pushes moments later).
    const notice = await screen.findByTestId("push-queued-notice");
    expect(notice).toHaveTextContent(/Queued 2 branches/i);
    expect(notice).toHaveTextContent(/publishing to the remote/i);
  });

  it.each(["noop", "fetched"] as const)(
    "reports per-repo %s outcomes after the worker writes sync runs",
    async (codeStatus) => {
      server.use(
        mockWorkspace("manual"),
        http.post(`/api/v1/workspaces/${WORKSPACE_ID}/push_to_remote`, () =>
          HttpResponse.json({
            data: {
              enqueued: 2,
              coordinated: {
                operation: "push",
                status: "queued",
                repos: [
                  {
                    workspaceId: WORKSPACE_ID,
                    repoId: "repo-code",

                    repoName: "code",
                    branchName: "main",
                    operation: "push",
                    status: "queued",
                    newCommit: "code-tip",
                  },
                  {
                    workspaceId: WORKSPACE_ID,
                    repoId: "repo-docs",

                    repoName: "docs",
                    branchName: "main",
                    operation: "push",
                    status: "queued",
                    newCommit: "docs-tip",
                  },
                ],
              },
            },
          }),
        ),
        http.get(
          `/api/v1/workspaces/${WORKSPACE_ID}/repos/:repoId/sync_runs`,
          () =>
            HttpResponse.json({
              data: {
                branch: {
                  workspaceId: WORKSPACE_ID,
                  repos: [],
                },
                runs: {
                  data: [
                    {
                      id: "run-code",
                      repoId: "repo-code",
                      actingUserId: "user-1",
                      kind: "integration",
                      triggerKind: "push",
                      status: codeStatus,
                      newWorkspaceTip: "code-tip",
                      startedAt: new Date().toISOString(),
                    },
                    {
                      id: "run-docs",
                      repoId: "repo-docs",
                      actingUserId: "user-1",
                      kind: "integration",
                      triggerKind: "push",
                      status: "pushed",
                      newWorkspaceTip: "docs-tip",
                      startedAt: new Date().toISOString(),
                    },
                  ],
                  total: 2,
                  page: 1,
                  perPage: 50,
                },
              },
            }),
        ),
      );

      renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
        user: createMockUser(),
      });

      fireEvent.click(await screen.findByTestId("push-to-remote-button"));

      const queuedNotice = await screen.findByTestId("push-queued-notice");
      expect(queuedNotice).toHaveTextContent(
        /Queued push for repo code and repo docs/i,
      );

      await waitFor(
        () =>
          expect(screen.getByTestId("push-queued-notice")).toHaveTextContent(
            /Push succeeded for repo docs/i,
          ),
        { timeout: 2500 },
      );
      expect(screen.getByTestId("push-queued-notice")).toHaveTextContent(
        codeStatus === "noop"
          ? /No changes to push in repo code/i
          : /Remote changes available for repo code.*Reconcile them/i,
      );
    },
  );

  it("tells the user when there was nothing to publish (C30)", async () => {
    server.use(
      mockWorkspace("manual"),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/push_to_remote`, () =>
        HttpResponse.json({ data: { enqueued: 0 } }),
      ),
    );

    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    fireEvent.click(await screen.findByTestId("push-to-remote-button"));

    const notice = await screen.findByTestId("push-queued-notice");
    expect(notice).toHaveTextContent(/Nothing to publish/i);
  });

  it("names a half-merged workspace PR that the pull imports past", async () => {
    server.use(
      mockWorkspace("manual"),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/sync`, () =>
        HttpResponse.json({
          data: {
            branchesUpdated: 0,
            branchesSkipped: 0,
            details: [
              {
                workspaceId: WORKSPACE_ID,
                repoId: "repo-code",
                repoName: "repo-code",
                branchName: "feature",
                status: "queued",
                oldCommit: "abc",
                newCommit: null,
                heldBy: { pullRequestId: "pr-1", title: "Cross-repo change" },
              },
            ],
          },
        }),
      ),
    );
    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });
    fireEvent.click(await screen.findByTestId("pull-from-remote-button"));
    const notice = await screen.findByTestId("pull-complete-notice");
    expect(notice).toHaveTextContent(/Cross-repo change/);
    expect(notice).toHaveTextContent(/imports that repository alone/);
  });

  it("renders Pull but never Push or auto-push on Base", async () => {
    let pulled = false;
    server.use(
      mockWorkspace("manual", true),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/sync`, () => {
        pulled = true;
        return HttpResponse.json({
          data: { branchesUpdated: 0, branchesSkipped: 0, details: [] },
        });
      }),
    );

    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    // g5.workspace.remote.base-parity: pull-side parity with no Base-only
    // auto-pull control, and Base never publishes to its provider.
    expect(await screen.findByTestId("pull-from-remote-button")).toBeVisible();
    expect(
      screen.queryByTestId("push-to-remote-button"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("auto-push-toggle")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("base-auto-pull-toggle"),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("pull-from-remote-button"));
    await waitFor(() => expect(pulled).toBe(true));
    const notice = await screen.findByTestId("pull-complete-notice");
    expect(notice).toHaveTextContent(/fast-forward updates into Base/i);
    expect(notice).not.toHaveTextContent(/git pull/i);
  });

  it("offers Base's Pull to an organization administrator who does not own it, and nothing more (g5.workspace.remote.base-admin-pull)", async () => {
    const gate = (kind: "base" | "reader", orgAdmin: boolean) => ({
      kind,
      canWrite: false,
      live: false,
      canStart: false,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner: false,
      orgAdmin,
      workspaceNoun: "the workspace",
    });
    server.use(mockWorkspace("manual", true));
    const admin = renderApp(
      <WorkspaceLiveGateProvider value={gate("base", true)}>
        <RemoteTrackingActions workspaceId={WORKSPACE_ID} />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );
    expect(await screen.findByTestId("pull-from-remote-button")).toBeVisible();
    expect(screen.queryByTestId("push-to-remote-button")).toBeNull();
    expect(screen.queryByTestId("auto-push-toggle")).toBeNull();
    admin.unmount();

    const member = renderApp(
      <WorkspaceLiveGateProvider value={gate("base", false)}>
        <RemoteTrackingActions workspaceId={WORKSPACE_ID} />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );
    await waitFor(() =>
      expect(screen.queryByTestId("remote-tracking-actions")).toBeNull(),
    );
    member.unmount();

    // On a working workspace the administrator is only a reader.
    server.use(mockWorkspace("manual"));
    renderApp(
      <WorkspaceLiveGateProvider value={gate("reader", true)}>
        <RemoteTrackingActions workspaceId={WORKSPACE_ID} />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );
    await waitFor(() =>
      expect(screen.queryByTestId("remote-tracking-actions")).toBeNull(),
    );
  });

  it("shows a modal when a pull fails validation", async () => {
    server.use(
      mockWorkspace("manual", true),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/sync`, () =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "Validation error",
              requestId: "req-pull",
              details: {
                modal: {
                  title: "Coordinated repo sync failed",
                  message: "Code repo must be made remote before pull.",
                  repos: [
                    {
                      workspaceId: WORKSPACE_ID,
                      repoId: "repo-code",
                      repoName: "code",
                      branchName: "main",
                      operation: "pull",
                      status: "blocked",
                      error: "repo is not remote-tracked",
                    },
                  ],
                },
              },
            },
          },
          { status: 422 },
        ),
      ),
    );

    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    fireEvent.click(await screen.findByTestId("pull-from-remote-button"));

    expect(await screen.findByTestId("coordinated-sync-modal")).toBeVisible();
    expect(
      screen.getByText("Code repo must be made remote before pull."),
    ).toBeVisible();
    expect(screen.getByText("repo is not remote-tracked")).toBeVisible();
  });

  it("shows a modal when coordinated push preflight fails", async () => {
    server.use(
      mockWorkspace("manual"),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/push_to_remote`, () =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "Validation error",
              requestId: "req-push",
              details: {
                modal: {
                  title: "Coordinated repo sync failed",
                  message: "Both expected repos must be remote-tracked.",
                  repos: [
                    {
                      workspaceId: WORKSPACE_ID,
                      repoId: "repo-docs",
                      repoName: "docs",
                      branchName: "main",
                      operation: "push",
                      status: "blocked",
                      error: "repo is not remote-tracked",
                    },
                  ],
                },
              },
            },
          },
          { status: 422 },
        ),
      ),
    );

    renderApp(<RemoteTrackingActions workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    fireEvent.click(await screen.findByTestId("push-to-remote-button"));

    expect(await screen.findByTestId("coordinated-sync-modal")).toBeVisible();
    expect(
      screen.getByText("Both expected repos must be remote-tracked."),
    ).toBeVisible();
    expect(screen.getByText("repo is not remote-tracked")).toBeVisible();
  });
});
