/**
 * Vitest unit tests for BranchAutoPullControl — the per-repository-child
 * "Auto-pull" toggle + "Publish to Remote" control (Option 1/2, G5 #1198).
 *
 * Scenarios:
 * 1. Remote-tracking child, auto-pull off: toggle present + unchecked.
 * 2. Remote-tracking child, auto-pull on: toggle checked.
 * 3. Local-only/pinned child: Publish button present, no auto-pull toggle.
 * 4. Toggling PUTs `{ enabled: true }` to the per-child auto_pull endpoint.
 * 5. Base workspace: the same Auto-pull toggle for a remote-tracking child;
 *    no Publish for a local-only child (Base binds its default branch).
 *
 * SOT:
 * - docs/.../auto_pull_publication_options_spec.md §5.5, §6.2
 * - workspace_service D42/D43; routes/workspaces I30
 */

import { describe, it, expect, vi } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { renderApp } from "../../test/render";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { BranchAutoPullControl } from "./BranchAutoPullControl";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";

const WORKSPACE_ID = "ws-autopull";
const REPO_ID = "repo-1";

function branch(
  trackingMode: "remote_tracking" | "pinned" | "tracking",
  autoPullEnabled: boolean,
) {
  return {
    workspaceId: "workspace-1",

    repoId: REPO_ID,
    repoName: "code",
    branchName: "feature",

    trackingMode,
    headCommit: "abc123",
    divergence: 0,
    behindBase: 0,
    hasDraft: false,
    dirtyFileCount: 0,
    autoPullEnabled,
  };
}

function workspaceDetail(
  branches: ReturnType<typeof branch>[],
  isBase = false,
) {
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
    pushMode: "manual",
    branches,
  };
}

function mockWorkspace(
  branches: ReturnType<typeof branch>[],
  isBase = false,
  onRequest?: () => void,
) {
  return http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () => {
    onRequest?.();
    return HttpResponse.json({ data: workspaceDetail(branches, isBase) });
  });
}

function render() {
  renderApp(
    <BranchAutoPullControl workspaceId={WORKSPACE_ID} repoId={REPO_ID} />,
    { user: createMockUser() },
  );
}

describe("BranchAutoPullControl", () => {
  it("shows an unchecked Auto-pull toggle for a remote-tracking child with auto-pull off", async () => {
    server.use(mockWorkspace([branch("remote_tracking", false)]));
    render();
    await waitFor(() =>
      expect(screen.getByTestId("branch-auto-pull-toggle")).not.toBeChecked(),
    );
    expect(
      screen.queryByTestId("publish-branch-button"),
    ).not.toBeInTheDocument();
  });

  it("shows a checked Auto-pull toggle for a remote-tracking child with auto-pull on", async () => {
    server.use(mockWorkspace([branch("remote_tracking", true)]));
    render();
    await waitFor(() =>
      expect(screen.getByTestId("branch-auto-pull-toggle")).toBeChecked(),
    );
  });

  it("shows a Publish button (no toggle) for a local-only/pinned child", async () => {
    server.use(mockWorkspace([branch("pinned", false)]));
    render();
    expect(
      await screen.findByTestId("publish-branch-button"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("branch-auto-pull-toggle"),
    ).not.toBeInTheDocument();
  });

  it("renders nothing for unsupported tracking modes", async () => {
    server.use(mockWorkspace([branch("tracking", false)]));
    render();

    await waitFor(() =>
      expect(
        screen.queryByTestId("branch-auto-pull-control"),
      ).not.toBeInTheDocument(),
    );
    expect(
      screen.queryByTestId("publish-branch-button"),
    ).not.toBeInTheDocument();
  });

  it("PUTs { enabled: true } to the per-child auto_pull endpoint when toggled on", async () => {
    const putBodies: Array<{ enabled: boolean }> = [];
    server.use(
      mockWorkspace([branch("remote_tracking", false)]),
      http.put(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/auto_pull`,
        async ({ request }) => {
          const body = (await request.json()) as { enabled: boolean };
          putBodies.push(body);
          return HttpResponse.json({
            data: {
              repoId: REPO_ID,
              autoPullEnabled: body.enabled,
            },
          });
        },
      ),
    );
    render();
    const toggle = await screen.findByTestId("branch-auto-pull-toggle");
    fireEvent.click(toggle);
    await waitFor(() => expect(putBodies).toContainEqual({ enabled: true }));
  });

  it("POSTs to the publish endpoint and fires onActionCompleted after publishing", async () => {
    const onActionCompleted = vi.fn();
    let published = false;
    server.use(
      mockWorkspace([branch("pinned", false)]),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/publish`,
        () => {
          published = true;
          return HttpResponse.json({
            data: {
              repoId: REPO_ID,
              trackingMode: "remote_tracking",
            },
          });
        },
      ),
    );
    renderApp(
      <BranchAutoPullControl
        workspaceId={WORKSPACE_ID}
        repoId={REPO_ID}
        onActionCompleted={onActionCompleted}
      />,
      { user: createMockUser() },
    );

    fireEvent.click(await screen.findByTestId("publish-branch-button"));

    await waitFor(() => expect(published).toBe(true));
    await waitFor(() => expect(onActionCompleted).toHaveBeenCalledTimes(1));
  });

  it("shows the ordinary Auto-pull toggle for a remote-tracking Base child", async () => {
    server.use(mockWorkspace([branch("remote_tracking", true)], true));
    render();
    await waitFor(() =>
      expect(screen.getByTestId("branch-auto-pull-toggle")).toBeChecked(),
    );
  });

  it("lets an organization administrator who does not own Base set its auto-pull (g5.workspace.remote.base-admin-pull)", async () => {
    let body: unknown = null;
    server.use(
      mockWorkspace([branch("remote_tracking", true)], true),
      http.put(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/auto_pull`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({
            data: {
              workspaceId: WORKSPACE_ID,
              repoId: REPO_ID,
              autoPullEnabled: false,
            },
          });
        },
      ),
    );
    const gate = (orgAdmin: boolean) => ({
      kind: "base" as const,
      canWrite: false,
      live: false,
      canStart: false,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner: false,
      orgAdmin,
      workspaceNoun: "the workspace",
    });
    const admin = renderApp(
      <WorkspaceLiveGateProvider value={gate(true)}>
        <BranchAutoPullControl workspaceId={WORKSPACE_ID} repoId={REPO_ID} />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );
    const toggle = await screen.findByTestId("branch-auto-pull-toggle");
    fireEvent.click(toggle);
    await waitFor(() => expect(body).toEqual({ enabled: false }));
    admin.unmount();

    let workspaceLoaded = false;
    server.use(
      mockWorkspace([branch("remote_tracking", true)], true, () => {
        workspaceLoaded = true;
      }),
    );
    renderApp(
      <WorkspaceLiveGateProvider value={gate(false)}>
        <BranchAutoPullControl workspaceId={WORKSPACE_ID} repoId={REPO_ID} />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );
    await waitFor(() => expect(workspaceLoaded).toBe(true));
    expect(screen.queryByTestId("branch-auto-pull-control")).toBeNull();
  });

  it("offers no Publish for a local-only Base child", async () => {
    let workspaceLoaded = false;
    server.use(
      mockWorkspace([branch("pinned", false)], true, () => {
        workspaceLoaded = true;
      }),
    );
    render();
    await waitFor(() => expect(workspaceLoaded).toBe(true));
    expect(
      screen.queryByTestId("branch-auto-pull-control"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("publish-branch-button"),
    ).not.toBeInTheDocument();
  });
});
