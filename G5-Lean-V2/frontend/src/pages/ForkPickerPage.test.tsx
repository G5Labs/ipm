import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it } from "vitest";

import { WorkspaceTabsProvider } from "../hooks/useWorkspaceTabs";
import {
  createMockUser,
  createMockWorkspace,
  mockInstanceInfo,
} from "../test/fixtures";
import { server } from "../test/msw/server";
import { createTestJwt, renderApp } from "../test/render";
import { ForkPickerPage } from "./ForkPickerPage";

const USER = createMockUser();

const RouteEcho = () => {
  const { pathname, search } = useLocation();
  return (
    <div data-testid="landed">
      {pathname}
      {search}
    </div>
  );
};

function renderPicker(route = "/g5labs/fork") {
  return renderApp(
    <Routes>
      <Route path="/:org/fork" element={<ForkPickerPage />} />
      <Route path="/:org/:workspace" element={<RouteEcho />} />
    </Routes>,
    { route, user: USER },
  );
}

function serveOrgWorkspaces(orgId: string, workspaces: unknown[]) {
  server.use(
    http.get(`/api/v1/orgs/${orgId}/workspaces`, () =>
      HttpResponse.json({ data: workspaces }),
    ),
  );
}

/** A refresh answer whose JWT names `orgId`, so `switchOrg` accepts it. */
function serveRefreshFor(orgId: string) {
  server.use(
    http.post("/api/v1/auth/refresh", () =>
      HttpResponse.json({ data: { jwt: createTestJwt(USER, orgId) } }),
    ),
  );
}

const g5Workspaces = [
  // Sorts before "base" by name: only Base-first ordering puts it later.
  createMockWorkspace({ id: "g5-aardvark", name: "aardvark" }),
  createMockWorkspace({ id: "g5-base", name: "base", isBase: true }),
  createMockWorkspace({
    id: "g5-child",
    name: "alpha",
    clonedFromId: "g5-base",
  }),
  createMockWorkspace({
    id: "g5-grandchild",
    name: "alpha-fix",
    clonedFromId: "g5-child",
  }),
  createMockWorkspace({
    id: "g5-orphan",
    name: "zeta",
    clonedFromId: "gone",
  }),
  // Another reviewer's copy is never forked, so the picker omits it.
  createMockWorkspace({
    id: "g5-review",
    name: "alpha-review",
    clonedFromId: "g5-child",
    isReview: true,
  }),
];

const acmeBase = () =>
  createMockWorkspace({ id: "acme-base", name: "base", isBase: true });

// The organization fork picker (workbench g5.workbench.tabs; mock story B4),
// scoped to the organization in its route.
describe("ForkPickerPage", () => {
  beforeEach(() => {
    localStorage.clear();
    serveOrgWorkspaces("org-g5labs", g5Workspaces);
    serveOrgWorkspaces("org-acme", [acmeBase()]);
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/fork_advisories", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/workspaces/:workspaceId/approvals", () =>
        HttpResponse.json({
          data: { requiredInto: [], attachmentHistory: [], outgoing: null },
        }),
      ),
    );
  });

  it("lists the route organization with its role and its workspaces as a lineage tree, and no other", async () => {
    renderPicker();
    const orgs = await screen.findAllByTestId("fork-picker-org");
    expect(orgs.map((org) => org.getAttribute("data-org-slug"))).toEqual([
      "g5labs",
    ]);
    expect(within(orgs[0]).getByText("G5 Labs")).toBeInTheDocument();
    expect(within(orgs[0]).getByText("Admin")).toBeInTheDocument();
    expect(screen.queryByText("Acme Corp")).toBeNull();

    const rows = await within(orgs[0]).findAllByTestId("fork-picker-row");
    // Base first, children indented under their parent, other roots by name.
    expect(rows.map((row) => row.getAttribute("data-workspace-name"))).toEqual([
      "base",
      "alpha",
      "alpha-fix",
      "aardvark",
      "zeta",
    ]);
    expect(rows[0]).toHaveAttribute("data-base", "true");
    expect(rows[1]).toHaveStyle({ marginLeft: "22px" });
    expect(rows[2]).toHaveStyle({ marginLeft: "44px" });
    expect(rows[3]).not.toHaveStyle({ marginLeft: "22px" });
    expect(rows[4]).not.toHaveStyle({ marginLeft: "22px" });
    expect(
      within(rows[0]).getByRole("button", { name: "Fork base" }),
    ).toBeEnabled();
  });

  it("narrows the tree to the matches and their ancestors, and renders no title bar (g5.workbench.shell-chrome)", async () => {
    renderPicker();
    const [org] = await screen.findAllByTestId("fork-picker-org");
    await within(org).findAllByTestId("fork-picker-row");
    // Without the tabs provider the strip is absent, so the page keeps its
    // own header here; the alignment page suite covers the strip.
    const search = screen.getByTestId("fork-picker-search");
    await userEvent.type(search, "fix");
    const rows = within(org).getAllByTestId("fork-picker-row");
    expect(rows.map((row) => row.getAttribute("data-workspace-name"))).toEqual([
      "base",
      "alpha",
      "alpha-fix",
    ]);
    await userEvent.clear(search);
    await userEvent.type(search, "nothing-here");
    expect(within(org).getByTestId("fork-picker-org-empty")).toHaveTextContent(
      'No workspace matches "nothing-here".',
    );
    await userEvent.click(screen.getByTestId("fork-picker-search-clear"));
    expect(within(org).getAllByTestId("fork-picker-row")).toHaveLength(5);
    expect(screen.queryByText("alpha-review")).toBeNull();
    expect(screen.queryByTestId("shell-controls")).toBeNull();
  });

  it("keeps the Account control in the strip, opening account settings (g5.ui.workbench-shell)", async () => {
    renderApp(
      <WorkspaceTabsProvider>
        <Routes>
          <Route path="/:org/fork" element={<ForkPickerPage />} />
        </Routes>
      </WorkspaceTabsProvider>,
      { route: "/g5labs/fork", user: USER },
    );
    const shell = await screen.findByTestId("shell-controls");
    await screen.findAllByTestId("fork-picker-org");
    expect(document.querySelector('[data-surface="titlebar"]')).toBeNull();
    await userEvent.click(within(shell).getByTestId("org-management-button"));
    expect(screen.getByRole("dialog", { name: "Account" })).toBeVisible();
  });

  it("opens the person's Settings from the strip's settings button (g5.ui.settings-page)", async () => {
    const SettingsRouteEcho = () => {
      const location = useLocation();
      return <div data-testid="settings-route">{location.pathname}</div>;
    };
    renderApp(
      <WorkspaceTabsProvider>
        <Routes>
          <Route path="/:org/fork" element={<ForkPickerPage />} />
          <Route path="/:org/settings" element={<SettingsRouteEcho />} />
        </Routes>
      </WorkspaceTabsProvider>,
      { route: "/g5labs/fork", user: USER },
    );
    const shell = await screen.findByTestId("shell-controls");
    await screen.findAllByTestId("fork-picker-org");
    await userEvent.click(within(shell).getByTestId("settings-button"));
    expect(await screen.findByTestId("settings-route")).toHaveTextContent(
      "/g5labs/settings",
    );
  });

  it("reports the organization's load failure and retries", async () => {
    let fail = true;
    server.use(
      http.get("/api/v1/orgs/org-acme/workspaces", () =>
        fail
          ? HttpResponse.json(
              { error: { code: "INTERNAL", message: "boom" } },
              { status: 500 },
            )
          : HttpResponse.json({ data: [acmeBase()] }),
      ),
    );
    const user = userEvent.setup();
    renderPicker("/acme/fork");
    const [org] = await screen.findAllByTestId("fork-picker-org");
    const error = await within(org).findByTestId("fork-picker-org-error");
    expect(error).toHaveTextContent("Could not load");
    fail = false;
    await user.click(within(org).getByTestId("fork-picker-org-retry"));
    expect(await within(org).findByTestId("fork-picker-row")).toHaveAttribute(
      "data-workspace-name",
      "base",
    );
  });

  it("lists nothing for an organization the person is not a member of", async () => {
    renderPicker("/nope/fork");
    expect(
      await screen.findByTestId("fork-picker-not-member"),
    ).toHaveTextContent("You are not a member of this organization.");
    expect(screen.queryByTestId("fork-picker-org")).toBeNull();
  });

  it("switches to the organization, forks through the ordinary dialog and lands on the new workspace", async () => {
    serveRefreshFor("org-acme");
    const cloneBodies: unknown[] = [];
    server.use(
      http.post("/api/v1/workspaces/acme-base/clone", async ({ request }) => {
        const body = (await request.json()) as { name: string };
        cloneBodies.push(body);
        return HttpResponse.json(
          {
            data: {
              workspace: createMockWorkspace({
                id: "acme-new",
                name: body.name,
                clonedFromId: "acme-base",
              }),
              clonedBranches: 1,
              clonedDrafts: 0,
              carriedSessions: 0,
              startedChat: { sessionId: "chat-9" },
            },
          },
          { status: 201 },
        );
      }),
    );
    const user = userEvent.setup();
    renderPicker("/acme/fork");
    const [org] = await screen.findAllByTestId("fork-picker-org");
    const baseRow = await within(org).findByTestId("fork-picker-row");
    await user.click(
      within(baseRow).getByRole("button", { name: "Fork base" }),
    );

    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByRole("heading", { name: /^Fork base$/ }),
    ).toBeVisible();
    await user.type(
      within(dialog).getByLabelText("Workspace name (also its branch name)"),
      "acme-feature",
    );
    await user.click(within(dialog).getByTestId("clone-submit-button"));

    expect(await screen.findByTestId("landed")).toHaveTextContent(
      "/acme/acme-feature?session=chat-9",
    );
    expect(cloneBodies).toHaveLength(1);
    expect(cloneBodies[0]).toMatchObject({ name: "acme-feature" });
  });

  // New workspace replaces the former Create Organization button: Home's
  // create dialog (a fork of the organization Base) after the org switch.
  it("offers New workspace, which switches to the organization, forks Base and lands on the new workspace", async () => {
    serveRefreshFor("org-acme");
    const cloneBodies: unknown[] = [];
    server.use(
      http.post("/api/v1/workspaces/acme-base/clone", async ({ request }) => {
        const body = (await request.json()) as { name: string };
        cloneBodies.push(body);
        return HttpResponse.json(
          {
            data: {
              workspace: createMockWorkspace({
                id: "acme-created",
                name: body.name,
                clonedFromId: "acme-base",
              }),
              clonedBranches: 1,
              clonedDrafts: 0,
              carriedSessions: 0,
              startedChat: { sessionId: "chat-10" },
            },
          },
          { status: 201 },
        );
      }),
    );
    const user = userEvent.setup();
    renderPicker("/acme/fork");
    await screen.findAllByTestId("fork-picker-org");
    expect(screen.queryByText("Create Organization")).toBeNull();
    await user.click(screen.getByTestId("fork-picker-new-workspace"));
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByRole("heading", { name: /^Fork base$/ }),
    ).toBeVisible();
    await user.type(
      within(dialog).getByLabelText("Workspace name (also its branch name)"),
      "acme-fresh",
    );
    await user.click(within(dialog).getByTestId("clone-submit-button"));
    expect(await screen.findByTestId("landed")).toHaveTextContent(
      "/acme/acme-fresh?session=chat-10",
    );
    expect(cloneBodies).toEqual([
      expect.objectContaining({ name: "acme-fresh" }),
    ]);
  });

  it("opens a workspace as the new tab from its name, without a fork (g5.workbench.tabs)", async () => {
    const user = userEvent.setup();
    renderPicker();
    const orgs = await screen.findAllByTestId("fork-picker-org");
    const alphaRow = (
      await within(orgs[0]).findAllByTestId("fork-picker-row")
    ).find((row) => row.dataset.workspaceName === "alpha");
    expect(alphaRow).toBeDefined();
    const open = within(alphaRow!).getByTestId("fork-picker-open");
    expect(open).toHaveAttribute("href", "/g5labs/alpha");
    await user.click(open);
    expect(await screen.findByTestId("landed")).toHaveTextContent(
      "/g5labs/alpha",
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("reports a failed organization switch and opens no dialog", async () => {
    server.use(
      http.post("/api/v1/auth/refresh", () =>
        HttpResponse.json(
          { error: { code: "UNAUTHORIZED", message: "no" } },
          { status: 401 },
        ),
      ),
    );
    const user = userEvent.setup();
    renderPicker("/acme/fork");
    const [org] = await screen.findAllByTestId("fork-picker-org");
    const baseRow = await within(org).findByTestId("fork-picker-row");
    await user.click(
      within(baseRow).getByRole("button", { name: "Fork base" }),
    );
    expect(
      await screen.findByTestId("fork-picker-switch-error"),
    ).toHaveTextContent("Could not switch to Acme Corp.");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
