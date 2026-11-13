import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { createMockUser, mockOrgMembers } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { OrgMembersPageContent } from "./OrgMembersPageContent";
import { transferCandidates, findOwner } from "../../lib/workspaceOwner";

function renderMembers() {
  return renderApp(
    <Routes>
      <Route
        path="/:org/settings/members"
        element={<OrgMembersPageContent />}
      />
    </Routes>,
    {
      route: "/g5labs/settings/members",
      user: createMockUser(),
    },
  );
}

describe("organization member activity", () => {
  it("deactivates and explicitly reactivates with the observed revision, showing shutdown separately", async () => {
    const user = userEvent.setup();
    const members = structuredClone(mockOrgMembers["org-g5labs"]);
    const target = members.find(
      (m) => m.role === "admin" && m.userId !== "user-alex",
    )!;
    const requests: unknown[] = [];
    server.use(
      http.get("/api/v1/orgs/:orgId/members", () =>
        HttpResponse.json({ data: members }),
      ),
      http.post(
        "/api/v1/orgs/:orgId/members/:userId/deactivate",
        async ({ request }) => {
          requests.push(await request.json());
          target.deactivatedAt = "2026-10-17T00:00:00Z";
          target.activityRevision++;
          target.shutdownPending = true;
          return HttpResponse.json({ data: target });
        },
      ),
      http.post(
        "/api/v1/orgs/:orgId/members/:userId/reactivate",
        async ({ request }) => {
          requests.push(await request.json());
          target.deactivatedAt = null;
          target.activityRevision++;
          return HttpResponse.json({ data: target });
        },
      ),
      http.get("/api/v1/orgs/:orgId/members/:userId/shutdown", () =>
        HttpResponse.json({
          data: [
            {
              workspaceId: "retained-workspace",
              name: "My workspace",
              state: "stopping",
            },
          ],
        }),
      ),
    );
    renderMembers();
    expect(
      screen.queryByRole("button", { name: "Deactivate Alex Chen" }),
    ).not.toBeInTheDocument();
    await user.click(
      await screen.findByRole("button", {
        name: `Deactivate ${target.displayName}`,
      }),
    );
    expect(
      screen.getByText(/Other organizations are unaffected/),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("org-members-confirm-accept"));
    await screen.findByRole("button", {
      name: `Reactivate ${target.displayName}`,
    });
    const row = screen.getByText(target.email).closest("tr")!;
    expect(within(row).getByText("Inactive")).toBeInTheDocument();
    expect(within(row).getByRole("combobox")).toHaveValue("admin");
    expect(
      within(row).getByRole("button", { name: `Remove ${target.displayName}` }),
    ).toBeDisabled();
    await user.click(
      within(row).getByRole("button", { name: "Shutdown pending" }),
    );
    await screen.findByText("My workspace: Stopping; awaiting confirmation");
    await user.click(screen.getByRole("button", { name: "Close" }));
    await user.click(
      screen.getByRole("button", { name: `Reactivate ${target.displayName}` }),
    );
    expect(
      screen.getByText(/Workspaces stay stopped until deliberately started/),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("org-members-confirm-accept"));
    await screen.findByRole("button", {
      name: `Deactivate ${target.displayName}`,
    });
    expect(
      within(screen.getByText(target.email).closest("tr")!).getByText("Active"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Shutdown pending" }),
    ).toBeInTheDocument();
    expect(requests).toEqual([
      { activityRevision: 0 },
      { activityRevision: 1 },
    ]);
  });

  it("keeps an Admin active and shows a last-administrator refusal after a concurrent change", async () => {
    const user = userEvent.setup();
    const members = structuredClone(mockOrgMembers["org-g5labs"]);
    // The current user's organization was created with an Admin, no Owner.
    members[0].role = "admin";
    const target = members[1];
    const message =
      "A tenant organization must retain at least one active Admin";
    server.use(
      http.get("/api/v1/orgs/:orgId/members", () =>
        HttpResponse.json({ data: members }),
      ),
      http.post("/api/v1/orgs/:orgId/members/:userId/deactivate", () =>
        HttpResponse.json(
          { error: { code: "CONFLICT", message } },
          { status: 409 },
        ),
      ),
    );
    renderMembers();
    await user.click(
      await screen.findByRole("button", {
        name: `Deactivate ${target.displayName}`,
      }),
    );
    await user.click(screen.getByTestId("org-members-confirm-accept"));
    await screen.findByText(message);
    const row = screen.getByText(target.email).closest("tr")!;
    expect(within(row).getByText("Active")).toBeInTheDocument();
    expect(
      within(row).queryByRole("button", { name: "Shutdown pending" }),
    ).not.toBeInTheDocument();
  });

  it("refreshes after a stale mutation instead of overwriting another admin's decision", async () => {
    const user = userEvent.setup();
    const members = structuredClone(mockOrgMembers["org-g5labs"]);
    const target = members.find(
      (m) => m.role === "admin" && m.userId !== "user-alex",
    )!;
    server.use(
      http.get("/api/v1/orgs/:orgId/members", () =>
        HttpResponse.json({ data: members }),
      ),
      http.post("/api/v1/orgs/:orgId/members/:userId/deactivate", () => {
        target.deactivatedAt = "2026-10-17T00:00:00Z";
        target.activityRevision = 3;
        return HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "Membership changed; refresh before trying again",
            },
          },
          { status: 409 },
        );
      }),
    );
    renderMembers();
    await user.click(
      await screen.findByRole("button", {
        name: `Deactivate ${target.displayName}`,
      }),
    );
    await user.click(screen.getByTestId("org-members-confirm-accept"));
    await screen.findByText("Membership changed; refresh before trying again");
    await screen.findByRole("button", {
      name: `Reactivate ${target.displayName}`,
    });
  });

  it("stops all of a member's workspaces and shows per-workspace blockers", async () => {
    const user = userEvent.setup();
    const members = structuredClone(mockOrgMembers["org-g5labs"]);
    const target = members.find(
      (m) => m.role === "admin" && m.userId !== "user-alex",
    )!;
    const stopped: string[] = [];
    server.use(
      http.get("/api/v1/orgs/:orgId/members", () =>
        HttpResponse.json({ data: members }),
      ),
      http.post(
        "/api/v1/orgs/:orgId/members/:userId/stop-workspaces",
        ({ params }) => {
          stopped.push(String(params.userId));
          target.shutdownPending = true;
          return HttpResponse.json(
            { data: { holdId: "hold-1", joined: false, workspaces: [] } },
            { status: 201 },
          );
        },
      ),
      http.get("/api/v1/orgs/:orgId/members/:userId/shutdown", () =>
        HttpResponse.json({
          data: [
            {
              workspaceId: "w-1",
              name: "child-1",
              state: "stopping",
              blocker: "stop_transport_retry",
            },
          ],
        }),
      ),
    );
    renderMembers();
    // The current user can stop their own workspaces as well as an admin can
    // stop another member's.
    expect(
      await screen.findByRole("button", {
        name: "Stop all workspaces of Alex Chen",
      }),
    ).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", {
        name: `Stop all workspaces of ${target.displayName}`,
      }),
    );
    expect(
      screen.getByText(/nothing restarts automatically/),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("org-members-confirm-accept"));
    await screen.findByText(/child-1: Stopping; awaiting confirmation/);
    expect(
      screen.getByText("Blocked: stop_transport_retry"),
    ).toBeInTheDocument();
    expect(stopped).toEqual([target.userId]);
  });

  it("retains inactive owners for attribution and excludes inactive or viewer transfer recipients", () => {
    const members = structuredClone(mockOrgMembers["org-g5labs"]);
    members[0].deactivatedAt = "2026-10-17T00:00:00Z";
    members[1].role = "viewer";
    expect(findOwner(members, members[0].userId)).toBe(members[0]);
    expect(transferCandidates(members, null)).toEqual([members[2]]);
  });
});
