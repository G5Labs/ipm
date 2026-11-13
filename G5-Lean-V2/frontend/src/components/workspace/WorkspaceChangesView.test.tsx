import { fireEvent, screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";
import { createMockUser } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { WorkspaceChangesView } from "./WorkspaceChangesView";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";

const WORKSPACE_ID = "workspace-changes-test";

function report(overrides: Record<string, unknown> = {}) {
  return {
    workspace_id: WORKSPACE_ID,
    source: "live",
    attached: true,
    status: "succeeded",
    members: [
      {
        repo_id: "11111111-1111-1111-1111-111111111111",
        root_path: "code",
        shared_branch: "feature/demo",
        head: "other-branch",
        on_shared_branch: false,
        errors: ["BRANCH_ASSOCIATION_MISMATCH"],
        dirty: [{ status: "M", path: "src/main.rs" }],
        against_upstream: { ahead: 2, behind: 1 },
      },
    ],
    ...overrides,
  };
}

const REVIEW_CONTEXT = {
  forkId: "fork-1",
  orgId: "org-1",
  sourceWorkspaceId: "22222222-2222-2222-2222-222222222222",
  sourceWorkspaceName: "kaihui-test3",
  reviewWorkspaceId: WORKSPACE_ID,
  reviewWorkspaceName: "kaihui-test3-review-alex",
  reviewerUserId: "user-alex",
  reviewerDisplayName: "Alex",
  vmStatus: "running",
  vmStartedAt: null,
  vmLastActivityAt: null,
  lastFlushedAt: null,
  createdAt: "2026-09-23T14:00:00.000Z",
};

describe("WorkspaceChangesView", () => {
  it("shows an association mismatch and invokes the owner switch-back action", async () => {
    let switched = false;
    server.use(
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/changes`, () =>
        HttpResponse.json({ data: report() }),
      ),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/changes/switch-back`,
        () => {
          switched = true;
          return HttpResponse.json({
            data: report({
              members: [
                {
                  repo_id: "11111111-1111-1111-1111-111111111111",
                  root_path: "code",
                  switched: true,
                  errors: [],
                },
              ],
            }),
          });
        },
      ),
    );

    renderApp(<WorkspaceChangesView workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    expect(
      await screen.findByTestId("workspace-association-mismatch"),
    ).toHaveTextContent("no longer on the workspace branch");
    expect(screen.getByTestId("workspace-changes-push")).toBeDisabled();
    // The owner commits from here too; the report lists one dirty file.
    expect(screen.getByTestId("workspace-commit")).toHaveTextContent(
      "Commit 1 file",
    );
    fireEvent.click(screen.getByTestId("workspace-changes-switch-back"));
    await waitFor(() => expect(switched).toBe(true));
  });

  it("offers no native actions in a review copy: it is saved and published by Push review", async () => {
    server.use(
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/review`, () =>
        HttpResponse.json({
          data: {
            asSource: null,
            asReview: REVIEW_CONTEXT,
            openSuggestions: 0,
          },
        }),
      ),
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/changes`, () =>
        HttpResponse.json({ data: report() }),
      ),
    );

    renderApp(<WorkspaceChangesView workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    expect(
      await screen.findByTestId("workspace-association-mismatch"),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByTestId("workspace-commit")).not.toBeInTheDocument(),
    );
    expect(
      screen.queryByTestId("workspace-changes-push"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-changes-update"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("workspace-changes-review-copy"),
    ).toHaveTextContent("saved and published by Push review");
  });

  it("keeps Switch back off with the reason once the page's gate says the VM is stopped, even with a cached mismatch report", async () => {
    let switched = false;
    server.use(
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/changes`, () =>
        HttpResponse.json({ data: report() }),
      ),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/changes/switch-back`,
        () => {
          switched = true;
          return HttpResponse.json({ data: report() });
        },
      ),
    );

    renderApp(
      <WorkspaceLiveGateProvider
        value={{
          kind: "stopped",
          canWrite: true,
          live: false,
          canStart: true,
          reviewCopy: false,
          reviewIdentityKnown: true,
          owner: true,
          workspaceNoun: "the workspace",
        }}
      >
        <WorkspaceChangesView workspaceId={WORKSPACE_ID} />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );

    expect(
      await screen.findByTestId("workspace-association-mismatch"),
    ).toBeInTheDocument();
    const switchBack = screen.getByTestId("workspace-changes-switch-back");
    expect(switchBack).toBeDisabled();
    expect(switchBack).toHaveAttribute(
      "title",
      "Start the workspace to commit or push.",
    );
    // The reason and Start appear once the blocked Push is tried.
    expect(screen.queryByTestId("workspace-changes-start-hint")).toBeNull();
    fireEvent.click(screen.getByTestId("workspace-changes-start-hint-trigger"));
    expect(screen.getByTestId("workspace-changes-start-hint")).toBeVisible();
    fireEvent.click(switchBack);
    expect(switched).toBe(false);
  });

  it("keeps a known copy's Commit withheld when its own review-context read fails (g5.governance.review-workspace.exclusions)", async () => {
    server.use(
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/review`, () =>
        HttpResponse.json(
          { error: { code: "INTERNAL", message: "unavailable" } },
          { status: 500 },
        ),
      ),
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/changes`, () =>
        HttpResponse.json({ data: report() }),
      ),
    );

    renderApp(
      <WorkspaceLiveGateProvider
        value={{
          kind: "live",
          canWrite: true,
          live: true,
          canStart: false,
          reviewCopy: true,
          reviewIdentityKnown: true,
          owner: true,
          workspaceNoun: "your review workspace",
        }}
      >
        <WorkspaceChangesView workspaceId={WORKSPACE_ID} />
      </WorkspaceLiveGateProvider>,
      { user: createMockUser() },
    );

    expect(
      await screen.findByTestId("workspace-association-mismatch"),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByTestId("workspace-commit")).not.toBeInTheDocument(),
    );
    expect(
      screen.queryByTestId("workspace-changes-push"),
    ).not.toBeInTheDocument();
  });

  it("keeps native actions hidden for a durable reader view", async () => {
    server.use(
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/changes`, () =>
        HttpResponse.json({
          data: report({ source: "durable", attached: false, members: [] }),
        }),
      ),
    );

    renderApp(<WorkspaceChangesView workspaceId={WORKSPACE_ID} />, {
      user: createMockUser(),
    });

    expect(
      await screen.findByTestId("workspace-changes-source"),
    ).toHaveTextContent("Durable snapshot");
    expect(
      screen.queryByTestId("workspace-changes-push"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-changes-update"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("workspace-commit")).not.toBeInTheDocument();
  });
});
