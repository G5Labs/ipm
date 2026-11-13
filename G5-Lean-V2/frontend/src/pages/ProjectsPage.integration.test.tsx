import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";

import { ProjectsPage } from "./ProjectsPage";
import { renderApp } from "../test/render";
import {
  createMockUser,
  createMockWorkspace,
  mockInstanceInfo,
} from "../test/fixtures";
import { server } from "../test/msw/server";

const navigateMock = vi.fn();

function createJwtForOrg(org: string): string {
  const now = Math.floor(Date.now() / 1000);
  const header = btoa(JSON.stringify({ alg: "HS256", typ: "JWT" }));
  const payload = btoa(
    JSON.stringify({
      sub: "user-alex",
      org,
      scopes: ["workspaces:read", "workspaces:write"],
      iat: now,
      exp: now + 3600,
    }),
  );
  return `${header}.${payload}.test-signature`;
}

vi.mock("react-router-dom", async () => {
  const actual =
    await vi.importActual<typeof import("react-router-dom")>(
      "react-router-dom",
    );
  return {
    ...actual,
    useNavigate: () => navigateMock,
  };
});

describe("ProjectsPage", () => {
  beforeEach(() => {
    navigateMock.mockReset();
  });

  it("opens formal review inventory through source approvals without private controls", async () => {
    const user = createMockUser({
      activeOrgId: "org-g5labs",
      orgs: [
        {
          orgId: "org-g5labs",
          orgName: "G5 Labs",
          orgSlug: "g5labs",
          role: "admin",
        },
      ],
    });
    server.use(
      http.get("/api/v1/orgs/:orgId/workspaces", () =>
        HttpResponse.json({
          data: [
            createMockWorkspace({
              id: "agent-run",
              name: "Agent run",
              isReview: true,
              concurrentTasks: 1,
              agentReview: {
                reviewId: "review",
                sourceWorkspaceId: "source",
                sourceWorkspaceName: "Source workspace",
              },
            }),
          ],
        }),
      ),
    );
    renderApp(<ProjectsPage />, { route: "/projects", user });
    const name = await screen.findByText("Agent run");
    const row = within(name.closest("tr")!);
    expect(row.getByText("Agent review")).toBeVisible();
    for (const action of [
      "Abort workspace tasks",
      "Fork workspace",
      "More actions",
    ])
      expect(row.queryByRole("button", { name: action })).toBeNull();
    await userEvent.click(name);
    await waitFor(() =>
      expect(navigateMock).toHaveBeenCalledWith(
        "/g5labs/Source%20workspace?view=approvals",
      ),
    );
  });

  it("loads workspaces across every org membership", async () => {
    const user = createMockUser({
      orgs: [
        {
          orgId: "org-g5labs",
          orgName: "G5 Labs",
          orgSlug: "g5labs",
          role: "admin",
        },
        {
          orgId: "org-acme",
          orgName: "Acme Corp",
          orgSlug: "acme",
          role: "admin",
        },
      ],
    });

    server.use(
      http.get("/api/v1/orgs/:orgId/workspaces", ({ params }) => {
        const orgId = params.orgId as string;
        if (orgId === "org-g5labs") {
          return HttpResponse.json({
            data: [
              createMockWorkspace({
                id: "ws-g5",
                name: "G5 Workspace",
                lastUpdated: "2026-03-19T10:00:00Z",
              }),
            ],
          });
        }
        if (orgId === "org-acme") {
          return HttpResponse.json({
            data: [
              createMockWorkspace({
                id: "ws-acme",
                name: "Acme Workspace",
                lastUpdated: "2026-03-18T10:00:00Z",
              }),
            ],
          });
        }
        return HttpResponse.json({ data: [] });
      }),
    );

    renderApp(<ProjectsPage />, {
      route: "/projects",
      user,
    });

    await waitFor(() => {
      expect(screen.getByText("G5 Workspace")).toBeInTheDocument();
    });
    expect(screen.getByText("Acme Workspace")).toBeInTheDocument();
  });

  it("forks the organization base and navigates by the exact workspace name", async () => {
    const user = createMockUser({
      orgs: [
        {
          orgId: "org-g5labs",
          orgName: "G5 Labs",
          orgSlug: "g5labs",
          role: "admin",
        },
      ],
    });

    server.use(
      http.get("/api/v1/orgs/:orgId/workspaces", () =>
        HttpResponse.json({
          data: [
            createMockWorkspace({
              id: "ws-base",
              name: "Base",
              isBase: true,
            }),
          ],
        }),
      ),
      http.post("/api/v1/workspaces/ws-base/clone", async ({ request }) => {
        const input = (await request.json()) as {
          name: string;
          idempotencyKey: string;
        };
        expect(input.idempotencyKey).toBeTruthy();
        return HttpResponse.json(
          {
            data: {
              workspace: createMockWorkspace({
                id: "ws-created-uuid",
                name: input.name,
              }),
              clonedBranches: 1,
              clonedDrafts: 0,
              carriedSessions: 0,
            },
          },
          { status: 201 },
        );
      }),
    );

    const userActions = userEvent.setup();
    renderApp(<ProjectsPage />, {
      route: "/projects",
      user,
    });

    await userActions.click(
      await screen.findByRole("button", { name: "New Project" }),
    );
    await userActions.type(
      await screen.findByLabelText("Workspace name (also its branch name)"),
      "Created Workspace",
    );
    await userActions.click(screen.getByRole("button", { name: "Fork" }));

    await waitFor(() => {
      // The exact name is one encoded path segment, so names with "/" route.
      // A fork without a chat lands on the fresh center composer
      // (ui g5.workspace.ui.fork-landing).
      expect(navigateMock).toHaveBeenCalledWith(
        "/g5labs/Created%20Workspace?view=chat",
        { state: { forkLanding: { draft: null } } },
      );
    });
  });

  it("switches to the selected organization before opening New Project", async () => {
    const user = createMockUser({
      orgs: [
        {
          orgId: "org-g5labs",
          orgName: "G5 Labs",
          orgSlug: "g5labs",
          role: "admin",
        },
        {
          orgId: "org-acme",
          orgName: "Acme Corp",
          orgSlug: "acme",
          role: "admin",
        },
      ],
    });
    const refreshBodies: Array<{ orgId?: string }> = [];

    server.use(
      http.get("/api/v1/orgs/:orgId/workspaces", ({ params }) =>
        HttpResponse.json({
          data: [
            createMockWorkspace({
              id: `base-${String(params.orgId)}`,
              name: "Base",
              isBase: true,
            }),
          ],
        }),
      ),
      http.post("/api/v1/auth/refresh", async ({ request }) => {
        refreshBodies.push(
          ((await request.json()) ?? {}) as { orgId?: string },
        );
        return HttpResponse.json({
          data: { jwt: createJwtForOrg("org-acme") },
        });
      }),
    );

    const userActions = userEvent.setup();
    renderApp(<ProjectsPage />, {
      route: "/projects",
      user,
    });

    await userActions.selectOptions(
      await screen.findByRole("combobox", {
        name: "Select organization for new project",
      }),
      "1",
    );
    await userActions.click(
      screen.getByRole("button", { name: "New Project" }),
    );

    await screen.findByLabelText("Workspace name (also its branch name)");
    expect(refreshBodies).toContainEqual({ orgId: "org-acme" });
  });

  it("keeps New Project closed when the selected organization cannot be activated", async () => {
    const user = createMockUser({
      orgs: [
        {
          orgId: "org-g5labs",
          orgName: "G5 Labs",
          orgSlug: "g5labs",
          role: "admin",
        },
        {
          orgId: "org-acme",
          orgName: "Acme Corp",
          orgSlug: "acme",
          role: "admin",
        },
      ],
    });

    server.use(
      http.get("/api/v1/orgs/:orgId/workspaces", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json({
          data: { jwt: globalThis.localStorage.getItem("g5_auth_token") },
        }),
      ),
    );

    const userActions = userEvent.setup();
    renderApp(<ProjectsPage />, {
      route: "/projects",
      user,
    });
    await userActions.selectOptions(
      await screen.findByRole("combobox", {
        name: "Select organization for new project",
      }),
      "1",
    );
    await userActions.click(
      screen.getByRole("button", { name: "New Project" }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not switch to Acme Corp.",
    );
    expect(
      screen.queryByLabelText("Workspace name (also its branch name)"),
    ).not.toBeInTheDocument();
  });

  it("hides workspace creation entrypoints for self-hosted instances", async () => {
    const user = createMockUser({
      orgs: [
        {
          orgId: "org-g5labs",
          orgName: "G5 Labs",
          orgSlug: "g5labs",
          role: "admin",
        },
      ],
    });

    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            ...mockInstanceInfo,
            selfHosted: true,
          },
        }),
      ),
      http.get("/api/v1/orgs/:orgId/workspaces", () =>
        HttpResponse.json({
          data: [
            createMockWorkspace({
              id: "ws-g5",
              name: "G5 Workspace",
            }),
          ],
        }),
      ),
    );

    renderApp(<ProjectsPage />, {
      route: "/projects",
      user,
    });

    await waitFor(() => {
      expect(screen.getByText("G5 Workspace")).toBeInTheDocument();
    });

    expect(
      screen.queryByRole("button", { name: "New Project" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Fork workspace" }),
    ).not.toBeInTheDocument();
  });
});
