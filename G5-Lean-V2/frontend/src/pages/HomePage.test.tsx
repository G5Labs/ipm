import { selectHomeWorkspaces } from "../components/home/HomeWorkspacesTile";
import { serveOrgExternalIssues } from "../test/externalIssueFixtures";
import {
  QueryClient,
  QueryClientProvider,
  focusManager,
} from "@tanstack/react-query";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { WorkspaceTabsProvider } from "../hooks/useWorkspaceTabs";
import { HttpResponse, http } from "msw";
import {
  MemoryRouter,
  Route,
  Routes,
  useLocation,
  useParams,
} from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AsyncState, CurrentUserDTO, InstanceInfoDTO } from "../lib/types";
import { DEFAULT_SETTINGS } from "../lib/defaults";
import {
  HOME_WORKSPACES_IDLE_REFRESH_MS,
  HOME_WORKSPACES_WORKING_REFRESH_MS,
  homeWorkspacesRefetchInterval,
} from "../hooks/useHome";
import {
  recentWorkspaceOpensKey,
  type RecentWorkspaceOpen,
} from "../lib/recentWorkspaces";
import {
  makeIssueDomain,
  makeIssueSummary,
} from "../test/externalIssueFixtures";
import { ISSUE_SEARCH_DEBOUNCE_MS } from "../lib/issueSearchQuery";
import { VERIFICATION_ACTION_ENABLED_SETTING } from "../lib/verificationActionFlags";
import { server } from "../test/msw/server";
import { createMockWorkspace } from "../test/fixtures";
import { MockWebSocketProvider, MockWsController } from "../test/msw/ws-mock";
import { HomePage } from "./HomePage";

const mockUseAuth = vi.fn();
vi.mock("../hooks/useAuth", () => ({ useAuth: () => mockUseAuth() }));

const mockUseInstanceInfo = vi.fn();
vi.mock("../hooks/useSettings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../hooks/useSettings")>()),
  useInstanceInfo: () => mockUseInstanceInfo(),
}));

const USER_ID = "user-1";

function authedUser(
  activeOrgId: string | null = "org1",
  includeSecondOrg = false,
): CurrentUserDTO {
  return {
    id: USER_ID,
    displayName: "Ada Lovelace",
    email: "ada@example.com",
    avatarUrl: null,
    oauthProvider: "github",
    activeOrgId,
    orgs: [
      {
        orgId: "org1",
        orgName: "Org One",
        orgSlug: "org-one",
        role: "member",
        orgKind: "tenant",
      },
      ...(includeSecondOrg
        ? [
            {
              orgId: "org2",
              orgName: "Org Two",
              orgSlug: "org-two",
              role: "member",
              orgKind: "tenant" as const,
            },
          ]
        : []),
    ],
    scmConnections: [],
    createdAt: "2026-01-01T00:00:00Z",
    // Finished the onboarding tour: the tiles open on the person's own
    // (g5.ui.home-workspaces); the All-before-completion test overrides it.
    onboardingStep: 7,
    onboardingSkipped: false,
  };
}

function instanceInfoWithFeatures(
  features: string[],
  overrides: Partial<InstanceInfoDTO> = {},
): AsyncState<InstanceInfoDTO> {
  return {
    status: "ok",
    data: {
      features,
      ...overrides,
    } as InstanceInfoDTO,
  };
}

function workspaceFixture(
  id: string,
  name: string,
  createdBy: string,
  lastUpdated: string,
) {
  return {
    id,
    name,
    description: null,
    isBase: false,
    ownerUserId: createdBy,
    createdBy,
    clonedFromId: null,
    branches: [],
    tasksCompleted: 0,
    totalTasks: 0,
    runningTime: null,
    cost: 0,
    concurrentTasks: 0,
    lastUpdated,
    createdAt: lastUpdated,
  };
}

function taskFixture(id: string, name: string) {
  return {
    id,
    name,
    status: "succeeded",
    taskType: "agent",
    workspaceId: "ws-1",
    sessionId: null,
    costUsd: 0,
    createdAt: "2026-08-24T14:00:00Z",
    updatedAt: "2026-08-24T15:00:00Z",
  };
}

function serveOrgWorkspaces(
  orgId: string,
  workspaces: unknown[],
  requests?: string[],
) {
  server.use(
    http.get(`/api/v1/orgs/${orgId}/workspaces`, () => {
      requests?.push(orgId);
      return HttpResponse.json({ data: workspaces });
    }),
  );
}

function seedRecentOpens(opens: RecentWorkspaceOpen[]) {
  localStorage.setItem(recentWorkspaceOpensKey(USER_ID), JSON.stringify(opens));
}

const WorkspaceRouteEcho = () => {
  const { org, workspace } = useParams();
  const { search } = useLocation();
  return (
    <div data-testid="workspace-route">
      {org}/{workspace}
      {search}
    </div>
  );
};

const HomeLocationEcho = () => {
  const { search } = useLocation();
  return <div data-testid="home-location">{search}</div>;
};

const IssueRouteEcho = () => {
  const { org, repositoryId, number } = useParams();
  return (
    <div data-testid="issue-route">
      {org}/{repositoryId}/{number}
    </div>
  );
};

function homeTree(
  route: string,
  queryClient: QueryClient,
  // The tab strip (and the global controls it carries) exists only under
  // the tabs provider, as in production; most cases do without it.
  withTabs = false,
) {
  // Production mounts every route under the WebSocket provider; Home's
  // personal chat subscribes to its workspace's channels.
  const wsController = new MockWsController();
  wsController.queryClient = queryClient;
  const routes = (
    <Routes>
      <Route
        path="/:org/home"
        element={
          <>
            <HomePage />
            <HomeLocationEcho />
          </>
        }
      />
      <Route path="/:org/base" element={<div data-testid="org-base" />} />
      <Route path="/login" element={<div data-testid="login" />} />
      <Route
        path="/:org/issues/github/:repositoryId/:number"
        element={<IssueRouteEcho />}
      />
      <Route path="/:org/:workspace" element={<WorkspaceRouteEcho />} />
    </Routes>
  );
  return (
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={[route]}>
        <MockWebSocketProvider controller={wsController}>
          {withTabs ? (
            <WorkspaceTabsProvider>{routes}</WorkspaceTabsProvider>
          ) : (
            routes
          )}
        </MockWebSocketProvider>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

function renderHome(
  route = "/org-one/home",
  queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  }),
  withTabs = false,
) {
  const result = render(homeTree(route, queryClient, withTabs));
  return {
    ...result,
    queryClient,
    rerenderHome: () => result.rerender(homeTree(route, queryClient, withTabs)),
  };
}

describe("HomePage — global controls", () => {
  it("opens the person's Settings page from the strip's settings button (g5.ui.settings-page)", async () => {
    renderHome("/org-one/home", undefined, true);
    const button = await screen.findByTestId("settings-button");
    await userEvent.click(button);
    // The canonical `/:org/settings` route redirects to Base's settings tab
    // in the app; the test tree echoes the route it was sent to.
    expect(await screen.findByTestId("workspace-route")).toHaveTextContent(
      "org-one/settings",
    );
  });
});

/** Starting work from an issue resolves the chat's agent launch on Base. */
function serveBaseAgentLaunch() {
  server.use(
    http.get("/api/v1/workspaces/base/config", () =>
      HttpResponse.json({
        data: {
          scope: "workspace",
          settings: {},
          secretKeys: [],
          effectiveSettings: {},
        },
      }),
    ),
    http.post("/api/v1/workspaces/base/config/agent-launch/resolve", () =>
      HttpResponse.json({
        data: {
          resolved: {
            schemaVersion: 1,
            resolutionId: "alr_home",
            configRevision: "cfg-test",
            catalogVersion: "agent-launch-catalog-v1",
            provider: { value: "claude_code", provenance: "explicit" },
            providerRoute: "anthropic_direct",
            model: {
              value: "anthropic/claude-opus-4.8",
              provenance: "explicit",
            },
            upstreamModel: "anthropic/claude-opus-4.8",
            permissionMode: {
              value: "bypassPermissions",
              provenance: "explicit",
            },
            effort: { value: "high", provenance: "explicit" },
            authMode: "anthropic_api_key",
            egressMode: "direct_provider",
            credentialReady: true,
            requiresInteractiveAuth: false,
          },
        },
      }),
    ),
  );
}

beforeEach(() => {
  localStorage.clear();
  mockUseAuth.mockReturnValue({
    user: authedUser(),
    switchOrg: vi.fn().mockResolvedValue(undefined),
    logout: vi.fn(),
    hasScope: () => false,
  });
  mockUseInstanceInfo.mockReturnValue(instanceInfoWithFeatures([]));
  server.use(
    http.get("/api/v1/orgs/:orgId/home/attention", () =>
      HttpResponse.json({ data: { items: [], nextCursor: null } }),
    ),
    http.get("/api/v1/orgs/:orgId/workspaces/open-question-metrics", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.get("/api/v1/workspaces/:workspaceId/approvals", () =>
      HttpResponse.json({
        data: { requiredInto: [], attachmentHistory: [], outgoing: null },
      }),
    ),
    http.get("/api/v1/workspaces/:workspaceId/pull-requests", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.get("/api/v1/workspaces/:workspaceId/tasks", () =>
      HttpResponse.json({ data: { tasks: [], cursor: null, hasMore: false } }),
    ),
  );
});

describe("Needs Your Attention (routing-and-auth D22)", () => {
  it("filters on the server with independent pagination and contextual empty copy", async () => {
    const user = userEvent.setup();
    const requests: Array<{ kind: string | null; cursor: string | null }> = [];
    serveOrgWorkspaces("org1", []);
    server.use(
      http.get("/api/v1/orgs/org1/home/attention", ({ request }) => {
        const params = new URL(request.url).searchParams;
        const kind = params.get("kind");
        const cursor = params.get("cursor");
        requests.push({ kind, cursor });
        if (kind === "approval") {
          return HttpResponse.json({
            data: {
              items: [
                {
                  id: cursor ? "approval:ws-a:def-2" : "approval:ws-a:def-1",
                  kind: "approval",
                  workspaceId: "ws-a",
                  workspaceName: cursor
                    ? "Second Approval Workspace"
                    : "First Approval Workspace",
                  sourceId: cursor ? "def-2" : "def-1",
                  actionableAt: cursor
                    ? "2026-08-24T15:02:00Z"
                    : "2026-08-24T15:01:00Z",
                },
              ],
              nextCursor: cursor ? null : "approval-page-2",
            },
          });
        }
        if (kind === "open_question") {
          return HttpResponse.json({
            data: { items: [], nextCursor: null },
          });
        }
        return HttpResponse.json({
          data: {
            items: [
              {
                id: "open_question:ws-q:doc-1",
                kind: "open_question",
                workspaceId: "ws-q",
                workspaceName: "All Kinds Workspace",
                sourceId: "doc-1",
                actionableAt: "2026-08-24T15:00:00Z",
              },
            ],
            nextCursor: null,
          },
        });
      }),
    );

    renderHome();

    const filters = await screen.findByRole("group", {
      name: "Filter attention items",
    });
    const all = within(filters).getByRole("button", { name: "All" });
    expect(all).toHaveAttribute("aria-pressed", "true");
    expect(requests[0]).toEqual({ kind: null, cursor: null });
    expect(await screen.findByText("All Kinds Workspace")).toBeVisible();

    const approval = within(filters).getByRole("button", {
      name: "Approval",
    });
    await user.click(approval);
    expect(approval).toHaveAttribute("aria-pressed", "true");
    expect(approval.querySelector("svg")).not.toBeNull();
    // An approval row names its workspace twice: the link and the caption.
    expect(
      await screen.findByRole("link", { name: "First Approval Workspace" }),
    ).toBeVisible();
    expect(requests.at(-1)).toEqual({ kind: "approval", cursor: null });

    await user.click(
      screen.getByRole("button", { name: "Show more attention items" }),
    );
    expect(
      await screen.findByRole("link", { name: "Second Approval Workspace" }),
    ).toBeVisible();
    expect(requests.at(-1)).toEqual({
      kind: "approval",
      cursor: "approval-page-2",
    });

    await user.click(
      within(filters).getByRole("button", { name: "Open question" }),
    );
    expect(
      await screen.findByText("No open questions need your attention."),
    ).toBeVisible();
    expect(requests.at(-1)).toEqual({ kind: "open_question", cursor: null });

    const headingRow = screen.getByTestId("attention-heading-row");
    expect(headingRow).toHaveClass("flex-wrap");
    expect(filters).toHaveClass("flex-wrap");
    expect(filters.parentElement).toHaveClass(
      "w-full",
      "sm:w-auto",
      "flex-wrap",
    );
  });

  it("renders both singular kinds in two text rows with approved copy and action links", async () => {
    serveOrgWorkspaces("org1", []);
    server.use(
      http.get("/api/v1/orgs/org1/home/attention", () =>
        HttpResponse.json({
          data: {
            items: [
              {
                id: "approval:ws-a:def-1",
                kind: "approval",
                workspaceId: "ws-a",
                workspaceName: "Approval Workspace",
                sourceId: "def-1",
                displayName: "Security sign-off",
                actionableAt: "2026-08-24T13:00:00Z",
              },
              {
                id: "open_question:ws-q:doc-1",
                kind: "open_question",
                workspaceId: "ws-q",
                workspaceName: "Question Workspace",
                sourceId: "doc-1",
                displayName: "Design notes",
                openQuestionCount: 2,
                actionableAt: "2026-08-24T14:00:00Z",
              },
            ],
            nextCursor: null,
          },
        }),
      ),
    );

    renderHome();

    const section = screen.getByTestId("home-needs-attention");
    const approvalBadge = await within(section).findByTestId(
      "attention-kind-badge-approval",
    );
    const questionBadge = within(section).getByTestId(
      "attention-kind-badge-open_question",
    );
    expect(approvalBadge).toHaveClass(
      "border-[var(--g5-warn)]/40",
      "bg-[color-mix(in_srgb,var(--g5-warn)_10%,transparent)]",
      "text-[var(--g5-warn)]",
    );
    expect(questionBadge).toHaveClass(
      "border-[var(--g5-info)]/40",
      "bg-[color-mix(in_srgb,var(--g5-info)_10%,transparent)]",
      "text-[var(--g5-info)]",
    );
    expect(
      within(section).queryByText("Approval awaiting your decision"),
    ).not.toBeInTheDocument();
    const approvalWorkspace = within(section).getByText("Approval Workspace");
    expect(approvalWorkspace).toBeVisible();
    expect(approvalWorkspace).toHaveAttribute("title", "Approval Workspace");
    expect(approvalWorkspace.parentElement).toHaveTextContent(
      /^Approval Workspace$/,
    );
    expect(
      within(section).getByText("2 open questions need an answer"),
    ).toBeVisible();
    const questionPrimaryLine =
      within(section).getByText("Design notes").parentElement;
    expect(questionPrimaryLine).toContainElement(questionBadge);
    expect(questionPrimaryLine).not.toHaveTextContent("Question Workspace");
    expect(questionPrimaryLine?.parentElement?.children).toHaveLength(2);
    expect(
      within(section).getByText("2 open questions need an answer")
        .parentElement,
    ).toHaveTextContent(
      "2 open questions need an answer·Workspace:Question Workspace",
    );
    // Each name links where its action does (g5.ui.home-attention).
    expect(
      within(section)
        .getAllByTestId("home-attention-name")
        .map((name) => name.getAttribute("href")),
    ).toEqual([
      "/org-one/Approval%20Workspace?view=approvals",
      "/org-one/Question%20Workspace/doc/doc-1",
    ]);
    const actions = within(section).getAllByRole("link", {
      name: /^(Review|Answer)$/,
    });
    expect(actions[0]).toHaveAttribute(
      "href",
      "/org-one/Approval%20Workspace?view=approvals",
    );
    expect(actions[1]).toHaveAttribute(
      "href",
      "/org-one/Question%20Workspace/doc/doc-1",
    );
    for (const action of actions) {
      expect(action).toHaveClass(
        "rounded",
        "border",
        "px-2.5",
        "py-1",
        "text-xs",
        "font-medium",
      );
    }
    const attentionRows = within(section).getAllByRole("listitem");
    expect(attentionRows).toHaveLength(2);
    ["2026-08-24T13:00:00Z", "2026-08-24T14:00:00Z"].forEach(
      (dateTime, index) => {
        const row = attentionRows[index];
        const owner = within(row).getByTestId("home-attention-owner");
        const time = within(row).getByLabelText(/^Attention updated /);
        const action = within(row).getByRole("link", {
          name: /^(Review|Answer)$/,
        });
        expect(time).toHaveAttribute("datetime", dateTime);
        // Owner avatar sits right of the name/reason block, before the time.
        expect(row.children[1]).toBe(owner);
        expect(row.children[2]).toBe(time);
        expect(row.children[3]).toBe(action);
        expect(time).not.toHaveAttribute("tabindex");
      },
    );
    expect(screen.queryByText(/Ready to ship/i)).not.toBeInTheDocument();
  });

  it("shows each row's owner avatar from the roster, falling back when unresolved", async () => {
    serveOrgWorkspaces("org1", []);
    server.use(
      http.get("/api/v1/orgs/org1/members", () =>
        HttpResponse.json({
          data: [
            {
              userId: USER_ID,
              email: "ada@example.com",
              displayName: "Ada Lovelace",
              avatarUrl: null,
              role: "member",
              joinedAt: "2026-01-01T00:00:00Z",
            },
            {
              userId: "user-2",
              email: "grace@example.com",
              displayName: "Grace Hopper",
              avatarUrl: null,
              role: "admin",
              joinedAt: "2026-01-01T00:00:00Z",
            },
          ],
        }),
      ),
      http.get("/api/v1/orgs/org1/home/attention", () =>
        HttpResponse.json({
          data: {
            items: [
              {
                id: "approval:ws-a:def-1",
                kind: "approval",
                workspaceId: "ws-a",
                workspaceName: "Approval Workspace",
                // The owning workspace's owner, not the caller.
                ownerUserId: "user-2",
                sourceId: "def-1",
                displayName: "Security sign-off",
                actionableAt: "2026-08-24T13:00:00Z",
              },
              {
                id: "open_question:ws-q:doc-1",
                kind: "open_question",
                workspaceId: "ws-q",
                workspaceName: "Question Workspace",
                ownerUserId: USER_ID,
                sourceId: "doc-1",
                displayName: "Design notes",
                openQuestionCount: 1,
                actionableAt: "2026-08-24T14:00:00Z",
              },
              {
                id: "open_question:ws-z:doc-2",
                kind: "open_question",
                workspaceId: "ws-z",
                workspaceName: "Orphan Workspace",
                // No owner the roster can resolve: the deterministic fallback.
                sourceId: "doc-2",
                displayName: "Loose notes",
                openQuestionCount: 1,
                actionableAt: "2026-08-24T15:00:00Z",
              },
            ],
            nextCursor: null,
          },
        }),
      ),
    );

    renderHome();

    const section = screen.getByTestId("home-needs-attention");
    const rows = await within(section).findAllByRole("listitem");
    expect(rows).toHaveLength(3);

    // The owner avatar matches the Workspaces panel: size 18, the owner's
    // name as tooltip and an sr-only "Owner: …" label.
    const graceOwner = within(rows[0]).getByTestId("home-attention-owner");
    expect(graceOwner).toHaveAttribute("title", "Grace Hopper");
    expect(graceOwner).toHaveTextContent("Owner: Grace Hopper");
    const graceAvatar = within(graceOwner).getByTitle("Grace Hopper");
    expect(graceAvatar).toHaveStyle({ width: "18px", height: "18px" });

    const adaOwner = within(rows[1]).getByTestId("home-attention-owner");
    expect(adaOwner).toHaveAttribute("title", "Ada Lovelace");
    expect(adaOwner).toHaveTextContent("Owner: Ada Lovelace");

    // An unresolved owner falls back to "Unassigned", as the Workspaces panel
    // does, rather than breaking the row.
    const orphanOwner = within(rows[2]).getByTestId("home-attention-owner");
    expect(orphanOwner).toHaveAttribute("title", "Unassigned");
    expect(orphanOwner).toHaveTextContent("Owner: Unassigned");
  });

  it("fetches the next cursor page and retries a page failure in place", async () => {
    const user = userEvent.setup();
    let secondPageAttempts = 0;
    serveOrgWorkspaces("org1", []);
    server.use(
      http.get("/api/v1/orgs/org1/home/attention", ({ request }) => {
        const cursor = new URL(request.url).searchParams.get("cursor");
        if (!cursor) {
          return HttpResponse.json({
            data: {
              items: [
                {
                  id: "approval:ws-a:def-1",
                  kind: "approval",
                  workspaceId: "ws-a",
                  workspaceName: "First Workspace",
                  sourceId: "def-1",
                  actionableAt: "2026-08-24T15:00:00Z",
                },
              ],
              nextCursor: "page-2",
            },
          });
        }
        secondPageAttempts += 1;
        if (secondPageAttempts === 1) {
          return HttpResponse.json({ error: "temporary" }, { status: 503 });
        }
        return HttpResponse.json({
          data: {
            items: [
              {
                id: "open_question:ws-b:doc-2",
                kind: "open_question",
                workspaceId: "ws-b",
                workspaceName: "Second Workspace",
                sourceId: "doc-2",
                openQuestionCount: 1,
                actionableAt: "2026-08-24T15:01:00Z",
              },
            ],
            nextCursor: null,
          },
        });
      }),
    );

    renderHome();
    await user.click(
      await screen.findByRole("button", { name: "Show more attention items" }),
    );
    expect(
      await screen.findByText("More attention items could not be loaded."),
    ).toBeVisible();
    expect(screen.getByRole("link", { name: "First Workspace" })).toBeVisible();
    await user.click(
      within(screen.getByTestId("home-needs-attention")).getByRole("button", {
        name: "Retry",
      }),
    );
    expect(await screen.findByText("Second Workspace")).toBeVisible();
  });
});

describe("organization Home shell (routing-and-auth D22)", () => {
  it("renders the organization Home heading and authenticated greeting", async () => {
    serveOrgWorkspaces("org1", []);
    renderHome();

    expect(
      await screen.findByRole("heading", { name: "Org One Home" }),
    ).toBeVisible();
    expect(screen.getByText("Hi, Ada Lovelace")).toBeVisible();
  });

  it("falls back to authenticated email when the display name is blank", async () => {
    mockUseAuth.mockReturnValue({
      user: { ...authedUser(), displayName: "   " },
      switchOrg: vi.fn().mockResolvedValue(undefined),
      logout: vi.fn(),
      hasScope: () => false,
    });
    serveOrgWorkspaces("org1", []);
    renderHome();

    expect(await screen.findByText("Hi, ada@example.com")).toBeVisible();
  });

  it.each(["ok", "loading", "error"] as const)(
    "switches the JWT organization before reading Home when instance metadata is %s",
    async (status) => {
      if (status !== "ok") {
        mockUseInstanceInfo.mockReturnValue(
          status === "error"
            ? { status, error: new Error("Instance metadata unavailable") }
            : { status },
        );
      }
      const requests: string[] = [];
      const switchOrg = vi.fn().mockResolvedValue(undefined);
      let user = authedUser("org1", true);
      mockUseAuth.mockImplementation(() => ({
        user,
        switchOrg,
        logout: vi.fn(),
        hasScope: () => false,
      }));
      serveOrgWorkspaces("org1", [], requests);
      serveOrgWorkspaces("org2", [], requests);

      const { rerenderHome } = renderHome("/org-two/home");

      await waitFor(() => expect(switchOrg).toHaveBeenCalledWith("org2"));
      expect(requests).toEqual([]);
      expect(screen.queryByTestId("home-workspaces")).not.toBeInTheDocument();
      user = authedUser("org2", true);
      rerenderHome();
      await waitFor(() => expect(requests).toEqual(["org2"]));
      expect(screen.getByTestId("home-workspaces")).toBeVisible();
    },
  );

  it("fails closed for an unauthorized route organization without requesting data", async () => {
    const requests: string[] = [];
    server.use(
      http.get("/api/v1/orgs/:orgId/workspaces", ({ params }) => {
        requests.push(String(params.orgId));
        return HttpResponse.json({ data: [] });
      }),
    );
    renderHome("/not-a-membership/home");

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "You do not have access to this organization.",
    );
    expect(requests).toEqual([]);
  });

  it.each([false, true])(
    "holds external-issue configuration until route organization adoption (switch fails: %s)",
    async (switchFails) => {
      mockUseInstanceInfo.mockReturnValue(
        instanceInfoWithFeatures(["external_issues"]),
      );
      let user = authedUser("org1", true);
      const switchOrg = switchFails
        ? vi.fn().mockRejectedValue(new Error("denied"))
        : vi.fn().mockResolvedValue(undefined);
      mockUseAuth.mockImplementation(() => ({
        user,
        switchOrg,
        logout: vi.fn(),
        hasScope: () => false,
      }));
      const configRequests: Array<{
        orgId: string;
        activeOrgId: string | null;
      }> = [];
      server.use(
        http.get("/api/v1/orgs/:orgId/config", ({ params }) => {
          configRequests.push({
            orgId: String(params.orgId),
            activeOrgId: user.activeOrgId,
          });
          return HttpResponse.json({
            data: {
              scope: "org",
              settings: { external_issues_enabled: true },
              secretKeys: [],
              effectiveSettings: { external_issues_enabled: true },
              effectiveSecretKeys: [],
            },
          });
        }),
        http.get("/api/v1/orgs/org2/issue-domains", () =>
          HttpResponse.json({ data: { domains: [] } }),
        ),
      );
      const workspaceRequests: string[] = [];
      serveOrgWorkspaces("org2", [], workspaceRequests);
      const { queryClient, rerenderHome } = renderHome("/org-two/home");

      await waitFor(() => expect(switchOrg).toHaveBeenCalledWith("org2"));
      await waitFor(() => expect(queryClient.isFetching()).toBe(0));
      expect(configRequests).toEqual([]);
      expect(workspaceRequests).toEqual([]);
      if (switchFails) {
        expect(await screen.findByRole("alert")).toHaveTextContent(
          "Could not switch to Org Two.",
        );
        return;
      }
      user = authedUser("org2", true);
      rerenderHome();
      expect(await screen.findByTestId("home-issues-tile")).toBeVisible();
      expect(configRequests).toEqual([{ orgId: "org2", activeOrgId: "org2" }]);
      expect(workspaceRequests).toEqual(["org2"]);
    },
  );

  it("fails closed when route organization synchronization fails", async () => {
    const requests: string[] = [];
    mockUseAuth.mockReturnValue({
      user: authedUser("org1", true),
      switchOrg: vi.fn().mockRejectedValue(new Error("denied")),
      logout: vi.fn(),
      hasScope: () => false,
    });
    serveOrgWorkspaces("org2", [], requests);
    renderHome("/org-two/home");

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not switch to Org Two.",
    );
    expect(requests).toEqual([]);
  });

  it("keeps organization Home available without instance features", () => {
    mockUseInstanceInfo.mockReturnValue(instanceInfoWithFeatures([]));
    renderHome();

    expect(screen.queryByTestId("org-base")).toBeNull();
    expect(screen.getByTestId("home-columns")).toBeInTheDocument();
  });

  it.each(["loading", "error"] as const)(
    "loads active organization Home while instance metadata is %s",
    async (status) => {
      mockUseInstanceInfo.mockReturnValue(
        status === "error"
          ? { status, error: new Error("Instance metadata unavailable") }
          : { status },
      );
      const requests: string[] = [];
      serveOrgWorkspaces(
        "org1",
        [
          workspaceFixture(
            "owned",
            "My Workspace",
            USER_ID,
            "2026-08-24T00:00:00Z",
          ),
        ],
        requests,
      );
      const attentionRequests: string[] = [];
      server.use(
        http.get("/api/v1/orgs/org1/home/attention", () => {
          attentionRequests.push("org1");
          return HttpResponse.json({ data: { items: [], nextCursor: null } });
        }),
      );
      renderHome();

      expect(await screen.findByTestId("home-workspace-owned")).toBeVisible();
      await waitFor(() => expect(attentionRequests).toEqual(["org1"]));
      expect(requests).toEqual(["org1"]);
      expect(screen.queryByTestId("org-base")).not.toBeInTheDocument();
      expect(
        screen.queryByRole("status", { name: "Loading page" }),
      ).not.toBeInTheDocument();
      // Creation waits for the instance capabilities.
      expect(
        screen.queryByTestId("home-new-workspace"),
      ).not.toBeInTheDocument();
    },
  );

  it("keeps the attention column available when the Workspaces tile fails", async () => {
    server.use(
      http.get("/api/v1/orgs/org1/workspaces", () =>
        HttpResponse.json({ error: "unavailable" }, { status: 503 }),
      ),
    );
    renderHome();

    expect(
      await screen.findByRole("alert", { name: "Workspaces error" }),
    ).toBeVisible();
    expect(screen.getByTestId("home-needs-attention")).toBeVisible();
  });
});

describe("alignment Workspaces rows (g5.ui.home-workspaces)", () => {
  function installAlignmentHome(features: string[] = []) {
    mockUseInstanceInfo.mockReturnValue(instanceInfoWithFeatures(features));
    server.use(
      http.get("/api/v1/orgs/org1/members", () =>
        HttpResponse.json({ data: [] }),
      ),
    );
  }

  function approvalsWithPhases(
    entries: Array<{
      phase: "be" | "se" | "ie";
      state: "approved" | "pending" | "rejected";
    }>,
  ) {
    return {
      requiredInto: [],
      attachmentHistory: [],
      outgoing: {
        targetWorkspaceId: "target",
        targetWorkspaceName: "Base",
        mergeAdmission: { admitted: false, reason: null, requirements: [] },
        requirements: entries.map((entry, index) => ({
          definition: { id: `def-${index}`, name: `Sign-off ${index}` },
          phase: entry.phase,
          state: entry.state,
          callerCanDecide: false,
          decisionLog: [],
          // One human approver mirroring the entry state, so the classic IE
          // roster (approvalRosterForPhase) has a member to count.
          approvers: [
            {
              userId: "approver-1",
              displayName: "Grace Hopper",
              state: entry.state,
              comment: null,
              decidedAt:
                entry.state === "approved" ? "2026-08-24T00:00:00Z" : null,
            },
          ],
        })),
      },
    };
  }

  it("summarizes workspace-wide approvals once, with no phase gates and honest failed reads", async () => {
    installAlignmentHome();
    serveOrgWorkspaces("org1", [
      workspaceFixture("ws-mixed", "Mixed", USER_ID, "2026-08-24T00:00:00Z"),
      workspaceFixture("ws-done", "Done", USER_ID, "2026-08-23T00:00:00Z"),
      {
        ...workspaceFixture("ws-base", "Base", USER_ID, "2026-08-22T00:00:00Z"),
        isBase: true,
      },
      workspaceFixture("ws-err", "Broken", USER_ID, "2026-08-21T00:00:00Z"),
      workspaceFixture("ws-ie", "Ship only", USER_ID, "2026-08-20T00:00:00Z"),
    ]);
    let errorReads = 0;
    server.use(
      http.get("/api/v1/workspaces/ws-err/approvals", () => {
        errorReads += 1;
        return errorReads === 1
          ? HttpResponse.json({ error: "boom" }, { status: 500 })
          : HttpResponse.json({
              data: approvalsWithPhases([{ phase: "se", state: "pending" }]),
            });
      }),
      http.get("/api/v1/workspaces/ws-mixed/approvals", () =>
        HttpResponse.json({
          data: approvalsWithPhases([
            { phase: "be", state: "approved" },
            { phase: "ie", state: "approved" },
            { phase: "ie", state: "rejected" },
          ]),
        }),
      ),
      http.get("/api/v1/workspaces/ws-done/approvals", () =>
        HttpResponse.json({
          data: approvalsWithPhases([
            { phase: "be", state: "approved" },
            { phase: "se", state: "approved" },
            { phase: "ie", state: "approved" },
          ]),
        }),
      ),
      http.get("/api/v1/workspaces/ws-base/approvals", () =>
        HttpResponse.json({
          data: { requiredInto: [], attachmentHistory: [], outgoing: null },
        }),
      ),
      http.get("/api/v1/workspaces/ws-ie/approvals", () =>
        HttpResponse.json({
          data: approvalsWithPhases([{ phase: "ie", state: "approved" }]),
        }),
      ),
    );
    renderHome();

    const mixed = await screen.findByTestId("home-workspace-ws-mixed");
    const gates = await within(mixed).findByTestId("home-workspace-approvals");
    const gateState = (row: HTMLElement) =>
      within(row)
        .getByTestId("home-workspace-approvals")
        .getAttribute("data-state");
    expect(gateState(mixed)).toBe("rej");
    // Requirement counts, stacked, from each requirement's server state.
    expect(gates).toHaveAttribute("title", "2 approved, 1 rejected, 0 pending");
    expect(gates).toHaveAccessibleName(
      "Mixed approvals: 2 approved, 1 rejected, 0 pending",
    );
    expect(Array.from(gates.children).map((line) => line.textContent)).toEqual([
      "2 approved",
      "1 rejected",
      "0 pending",
    ]);
    expect(within(mixed).queryAllByTestId(/^home-approval-gate-/)).toHaveLength(
      0,
    );
    // No phase buttons anywhere on Home.
    expect(screen.queryByTestId("home-phase-stepper")).toBeNull();
    expect(
      within(mixed).queryByRole("button", { name: /Engineering/ }),
    ).toBeNull();

    const done = screen.getByTestId("home-workspace-ws-done");
    await within(done).findByTestId("home-workspace-approvals");
    expect(gateState(done)).toBe("ok");
    expect(
      within(done).getByTestId("home-workspace-approvals"),
    ).toHaveAccessibleName("Done approvals: 3 approved, 0 rejected, 0 pending");

    // A single requirement appears once regardless of its recommendation.
    const shipOnly = screen.getByTestId("home-workspace-ws-ie");
    await within(shipOnly).findByTestId("home-workspace-approvals");
    expect(gateState(shipOnly)).toBe("ok");
    expect(
      within(shipOnly).getByTestId("home-workspace-approvals"),
    ).toHaveAccessibleName(
      "Ship only approvals: 1 approved, 0 rejected, 0 pending",
    );

    const base = screen.getByTestId("home-workspace-ws-base");
    await within(base).findByTestId("home-workspace-approvals-none");
    expect(within(base).queryByTestId("home-workspace-approvals")).toBeNull();

    // A failed read is not "nothing required": it says so and retries.
    const broken = screen.getByTestId("home-workspace-ws-err");
    const unavailable = await within(broken).findByTestId(
      "home-workspace-approvals-error",
    );
    expect(unavailable).toHaveTextContent("Unavailable");
    expect(
      within(broken).queryByTestId("home-workspace-approvals-none"),
    ).toBeNull();
    await userEvent.click(
      within(broken).getByRole("button", {
        name: "Retry loading Broken approvals",
      }),
    );
    await within(broken).findByTestId("home-workspace-approvals");
    expect(gateState(broken)).toBe("wait");
    expect(
      within(broken).getByTestId("home-workspace-approvals"),
    ).toHaveAccessibleName(
      "Broken approvals: 0 approved, 0 rejected, 1 pending",
    );
    expect(errorReads).toBe(2);
  });

  it("discloses recent tasks only on a working row, marks it with the presence dot and announces a change once", async () => {
    const user = userEvent.setup();
    installAlignmentHome();
    let busyTasks = 2;
    let theirTasks = 0;
    let workspaceReads = 0;
    server.use(
      http.get("/api/v1/orgs/org1/workspaces", () => {
        workspaceReads += 1;
        return HttpResponse.json({
          data: [
            {
              ...workspaceFixture(
                "ws-busy",
                "Busy",
                USER_ID,
                "2026-08-24T00:00:00Z",
              ),
              concurrentTasks: busyTasks,
            },
            workspaceFixture(
              "ws-idle",
              "Quiet",
              USER_ID,
              "2026-08-23T00:00:00Z",
            ),
            // Someone else's workspace: absent under Mine, never announced.
            {
              ...workspaceFixture(
                "ws-theirs",
                "Theirs",
                "user-2",
                "2026-08-22T00:00:00Z",
              ),
              concurrentTasks: theirTasks,
            },
          ],
        });
      }),
    );
    const taskRequests: string[] = [];
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/tasks", ({ params }) => {
        taskRequests.push(String(params.workspaceId));
        return HttpResponse.json({
          data: {
            tasks: [taskFixture("task-1", "Index rebuild")],
            cursor: null,
            hasMore: false,
          },
        });
      }),
    );
    renderHome();

    const busy = await screen.findByTestId("home-workspace-ws-busy");
    expect(busy).toHaveAttribute("data-working", "true");
    const idle = screen.getByTestId("home-workspace-ws-idle");
    expect(idle).not.toHaveAttribute("data-working");
    expect(within(idle).queryByTestId("home-workspace-disclosure")).toBeNull();
    expect(within(idle).queryByTestId("home-workspace-presence")).toBeNull();
    expect(within(idle).queryByTestId("home-workspace-subline")).toBeNull();

    const disclosure = within(busy).getByTestId("home-workspace-disclosure");
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    expect(disclosure).toHaveAccessibleName(
      "Working, 2 running tasks. Expand Busy",
    );
    expect(within(busy).getByTestId("home-workspace-presence")).toHaveClass(
      "home-presence-dot",
    );
    expect(
      within(busy).getByTestId("home-workspace-subline"),
    ).toHaveTextContent("2 running tasks");
    // Nothing reads the tasks until the row is disclosed.
    expect(taskRequests).toEqual([]);
    await user.click(disclosure);
    expect(
      await within(busy).findByTestId("home-recent-task"),
    ).toHaveTextContent("Index rebuild");
    expect(taskRequests).toEqual(["ws-busy"]);
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    expect(disclosure).toHaveAccessibleName(
      "Working, 2 running tasks. Collapse Busy",
    );
    expect(screen.getByTestId("home-workspaces-live")).toHaveTextContent("");

    // The row stops working and a hidden row starts; the list re-reads when
    // the window regains focus (the same path the poll takes), the control
    // leaves and only the listed row's change is announced.
    busyTasks = 0;
    theirTasks = 3;
    await act(async () => {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
    });
    await waitFor(() =>
      expect(screen.getByTestId("home-workspace-ws-busy")).not.toHaveAttribute(
        "data-working",
      ),
    );
    expect(
      within(screen.getByTestId("home-workspace-ws-busy")).queryByTestId(
        "home-workspace-disclosure",
      ),
    ).toBeNull();
    expect(screen.getByTestId("home-workspaces-live")).toHaveTextContent(
      "Busy finished working",
    );
    expect(screen.getByTestId("home-workspaces-live")).not.toHaveTextContent(
      "Theirs",
    );
    // An unchanged re-read leaves the live region untouched: no mutation,
    // so nothing is re-announced.
    const readsBefore = workspaceReads;
    const mutations: MutationRecord[] = [];
    const observer = new MutationObserver((records) =>
      mutations.push(...records),
    );
    observer.observe(screen.getByTestId("home-workspaces-live"), {
      childList: true,
      characterData: true,
      subtree: true,
    });
    await act(async () => {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
    });
    await waitFor(() => expect(workspaceReads).toBeGreaterThan(readsBefore));
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    observer.disconnect();
    expect(mutations).toEqual([]);
    focusManager.setFocused(undefined);
  });

  it("pages five rows by default, reads the person's page size and passes it to the attention read (g5.ui.home-paging)", async () => {
    const user = userEvent.setup();
    installAlignmentHome();
    const attentionLimits: Array<string | null> = [];
    server.use(
      http.get("/api/v1/orgs/org1/home/attention", ({ request }) => {
        attentionLimits.push(new URL(request.url).searchParams.get("limit"));
        return HttpResponse.json({ data: { items: [], nextCursor: null } });
      }),
    );
    serveOrgWorkspaces(
      "org1",
      Array.from({ length: 7 }, (_, index) =>
        workspaceFixture(
          `ws-${index}`,
          `Space ${index}`,
          USER_ID,
          `2026-08-${String(10 + index).padStart(2, "0")}T00:00:00Z`,
        ),
      ),
    );
    const first = renderHome();
    const tile = await screen.findByTestId("home-workspaces");
    const names = () =>
      within(tile)
        .queryAllByTestId("home-workspace-open")
        .map((link) => link.textContent);
    await waitFor(() =>
      expect(names()).toEqual([
        "Space 6",
        "Space 5",
        "Space 4",
        "Space 3",
        "Space 2",
      ]),
    );
    expect(within(tile).getByTestId("home-workspaces-count")).toHaveTextContent(
      "7",
    );
    await user.click(
      within(tile).getByRole("button", { name: "Show 2 more workspaces" }),
    );
    expect(names()).toHaveLength(7);
    expect(within(tile).queryByTestId("home-workspaces-show-more")).toBeNull();
    await waitFor(() => expect(attentionLimits).toEqual(["5"]));

    // The setting changes while Home stays mounted (another tab, or the
    // settings page): paging starts over at the new size, both ways.
    const setRows = (homeRowsPerTile: number) =>
      act(() => {
        window.dispatchEvent(
          new CustomEvent("g5-user-settings-changed", {
            detail: {
              ...DEFAULT_SETTINGS,
              homeRowsPerTile,
            },
          }),
        );
      });
    await setRows(10);
    await waitFor(() => expect(names()).toHaveLength(7));
    expect(within(tile).queryByTestId("home-workspaces-show-more")).toBeNull();
    await waitFor(() => expect(attentionLimits).toContain("10"));
    await setRows(5);
    await waitFor(() => expect(names()).toHaveLength(5));
    expect(
      within(tile).getByRole("button", { name: "Show 2 more workspaces" }),
    ).toBeVisible();
    first.unmount();
  });

  it("polls the list quickly while a workspace works and slowly otherwise", () => {
    expect(homeWorkspacesRefetchInterval(undefined)).toBe(
      HOME_WORKSPACES_IDLE_REFRESH_MS,
    );
    expect(homeWorkspacesRefetchInterval([{ concurrentTasks: 0 }])).toBe(
      HOME_WORKSPACES_IDLE_REFRESH_MS,
    );
    expect(
      homeWorkspacesRefetchInterval([
        { concurrentTasks: 0 },
        { concurrentTasks: 1 },
      ]),
    ).toBe(HOME_WORKSPACES_WORKING_REFRESH_MS);
  });

  it("re-sorts Recently opened when another tab records an open", async () => {
    installAlignmentHome();
    serveOrgWorkspaces("org1", [
      workspaceFixture("older", "Older", USER_ID, "2026-08-20T00:00:00Z"),
      workspaceFixture("newer", "Newer", USER_ID, "2026-08-24T00:00:00Z"),
    ]);
    renderHome();
    const tile = await screen.findByTestId("home-workspaces");
    const names = () =>
      within(tile)
        .queryAllByTestId("home-workspace-open")
        .map((link) => link.textContent);
    await waitFor(() => expect(names()).toEqual(["Newer", "Older"]));

    const key = recentWorkspaceOpensKey(USER_ID);
    const value = JSON.stringify([
      { id: "older", openedAt: "2026-08-25T00:00:00Z" },
    ]);
    localStorage.setItem(key, value);
    act(() => {
      window.dispatchEvent(
        new StorageEvent("storage", { key, newValue: value }),
      );
    });
    await waitFor(() => expect(names()).toEqual(["Older", "Newer"]));
  });
});

describe("alignment Home tiles (workbench Phase 6)", () => {
  const HOUR = 3_600_000;

  function installAlignmentTiles(
    extraFeatures: string[] = ["external_issues"],
    githubLogin: string | null = "octocat",
  ) {
    mockUseInstanceInfo.mockReturnValue(
      instanceInfoWithFeatures(extraFeatures, { selfHosted: false }),
    );
    serveOrgExternalIssues(true);
    server.use(
      http.get("/api/v1/orgs/org1/members", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/github-app/user-connection", () =>
        HttpResponse.json({
          data: { connected: githubLogin !== null, githubLogin },
        }),
      ),
    );
  }

  function serveDomains(count = 1) {
    server.use(
      http.get("/api/v1/orgs/org1/issue-domains", () =>
        HttpResponse.json({
          data: {
            domains: Array.from({ length: count }, (_, index) =>
              makeIssueDomain({
                id: `domain-${index + 1}`,
                displayKey: `acme/repo${index + 1}`,
              }),
            ),
          },
        }),
      ),
    );
  }

  function issue(
    number: number,
    overrides: Partial<ReturnType<typeof makeIssueSummary>> = {},
  ) {
    return makeIssueSummary({
      locator: { repositoryId: "11", number },
      title: `Issue ${number}`,
      updatedAt: new Date(Date.UTC(2026, 8, 1, number)).toISOString(),
      ...overrides,
    });
  }

  interface SearchRequest {
    q: string | null;
    assignee: string | null;
  }

  function serveSearch(
    results: ReturnType<typeof makeIssueSummary>[],
    requests?: SearchRequest[],
    options: {
      truncated?: boolean;
      domainCount?: number;
      domainErrors?: Array<{
        domainId: string;
        displayKey: string;
        issueError: string;
        message: string;
      }>;
    } = {},
  ) {
    server.use(
      http.get("/api/v1/orgs/org1/issues/search", ({ request }) => {
        const url = new URL(request.url);
        requests?.push({
          q: url.searchParams.get("q"),
          assignee: url.searchParams.get("assignee"),
        });
        return HttpResponse.json({
          data: {
            results,
            domainErrors: options.domainErrors ?? [],
            domainCount: options.domainCount ?? 1,
            truncated: options.truncated ?? false,
          },
        });
      }),
    );
  }

  function computeRow(
    workspaceId: string,
    workspaceName: string,
    ownerUserId: string,
    overrides: Record<string, unknown> = {},
  ) {
    return {
      workspaceId,
      workspaceName,
      status: "running",
      startedAt: new Date(
        Date.now() - 2 * HOUR - 14 * 60_000 - 5_000,
      ).toISOString(),
      lastActivityAt: null,
      ownerUserId,
      ownerDisplayName: ownerUserId === USER_ID ? "Ada Lovelace" : "Grace",
      hostId: "host-1",
      slotCpus: 4,
      slotMemMib: 16384,
      instanceType: "n2-standard-16",
      ...overrides,
    };
  }

  function serveCompute(rows: unknown[] | (() => unknown[]), hits?: number[]) {
    server.use(
      http.get("/api/v1/orgs/:orgId/compute/running", () => {
        hits?.push(1);
        return HttpResponse.json({
          data: typeof rows === "function" ? rows() : rows,
        });
      }),
    );
  }

  const rowKeysOf = (rows: HTMLElement) =>
    [...rows.children].map((row) => row.getAttribute("data-testid"));

  it("renders the four tiles — Workspaces, attention, issues, compute — and no full search (g5.ui.home-workspaces)", async () => {
    installAlignmentTiles();
    serveOrgWorkspaces("org1", []);
    serveDomains();
    serveSearch([]);
    serveCompute([]);
    renderHome();

    const columns = screen.getByTestId("home-columns");
    await within(columns).findByTestId("home-compute-tile");
    await within(columns).findByTestId("home-issues-tile");
    expect(
      [...columns.children].map((child) => child.getAttribute("data-testid")),
    ).toEqual([
      "home-workspaces",
      "home-needs-attention",
      "home-issues-tile",
      "home-compute-tile",
    ]);
    expect(screen.queryByTestId("home-workspace-directory")).toBeNull();
    expect(screen.queryByTestId("home-recent-work")).toBeNull();
    expect(screen.queryByTestId("home-issue-search")).toBeNull();
    expect(screen.queryByTestId("organization-home-topbar")).toBeNull();
    expect(
      within(screen.getByTestId("home-workspaces")).getByRole("heading", {
        name: "Workspaces",
      }),
    ).toBeVisible();
    expect(
      within(screen.getByTestId("home-issues-tile")).getByRole("heading", {
        name: "Issues and tasks",
      }),
    ).toBeVisible();
    expect(
      within(screen.getByTestId("home-compute-tile")).getByRole("heading", {
        name: "Running compute",
      }),
    ).toBeVisible();
    // No View all: the tile pages its own slice.
    expect(screen.queryByTestId("home-issues-view-all")).toBeNull();
  });

  it("the Workspaces tile takes the directory's and Recent Workspaces' job: Mine by default, All omits others' review copies, search, sort and filter in the URL, the name opens, Delete for the owner only, Fork, the owner from the roster", async () => {
    const user = userEvent.setup();
    installAlignmentTiles();
    mockUseAuth.mockReturnValue({
      ...mockUseAuth(),
      hasScope: (scope: string) => scope === "workspaces:write",
    });
    serveOrgWorkspaces("org1", [
      {
        ...workspaceFixture("base", "Base", "user-2", "2026-08-24T00:00:00Z"),
        isBase: true,
      },
      {
        ...workspaceFixture("alpha", "alpha", USER_ID, "2026-08-25T00:00:00Z"),
        clonedFromId: "base",
      },
      {
        ...workspaceFixture(
          "alpha-fix",
          "alpha-fix",
          USER_ID,
          "2026-08-26T00:00:00Z",
        ),
        ownerUserId: "user-2",
        clonedFromId: "alpha",
      },
      {
        ...workspaceFixture("beta", "beta", "user-2", "2026-08-23T00:00:00Z"),
        ownerUserId: USER_ID,
        clonedFromId: "base",
        readOnly: true,
        createdAt: "2026-08-27T00:00:00Z",
      },
      // Another reviewer's copy is listed nowhere (the switcher's rule).
      {
        ...workspaceFixture(
          "review-copy",
          "alpha-review",
          "user-2",
          "2026-08-27T00:00:00Z",
        ),
        clonedFromId: "alpha",
        isReview: true,
      },
    ]);
    // beta was opened here after alpha's later update: it leads "Recently opened".
    seedRecentOpens([{ id: "beta", openedAt: "2026-08-28T00:00:00Z" }]);
    server.use(
      http.get("/api/v1/orgs/org1/members", () =>
        HttpResponse.json({
          data: [
            {
              userId: USER_ID,
              email: "ada@example.com",
              displayName: "Ada Lovelace",
              avatarUrl: null,
              role: "member",
              joinedAt: "2026-01-01T00:00:00Z",
            },
            {
              userId: "user-2",
              email: "grace@example.com",
              displayName: "Grace Hopper",
              avatarUrl: null,
              role: "admin",
              joinedAt: "2026-01-01T00:00:00Z",
            },
          ],
        }),
      ),
      // The fork dialog reads the source's advisories and approvals.
      http.get("/api/v1/workspaces/:workspaceId/fork_advisories", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/workspaces/:workspaceId/approvals", () =>
        HttpResponse.json({
          data: { requiredInto: [], attachmentHistory: [], outgoing: null },
        }),
      ),
    );
    serveDomains();
    serveSearch([]);
    serveCompute([]);
    renderHome();

    const tile = await screen.findByTestId("home-workspaces");
    expect(screen.queryByTestId("organization-home-topbar")).toBeNull();
    const names = () =>
      within(tile)
        .queryAllByTestId("home-workspace-open")
        .map((link) => link.textContent);
    await waitFor(() => expect(names()).toEqual(["beta", "alpha"]));
    expect(
      within(tile).getByTestId("home-workspaces-filter-mine"),
    ).toHaveAttribute("aria-pressed", "true");
    expect(within(tile).getByTestId("home-workspaces-count")).toHaveTextContent(
      "2",
    );
    expect(screen.getByTestId("home-new-workspace")).toBeVisible();
    expect(screen.getByTestId("home-location")).toHaveTextContent("");

    const betaRow = screen.getByTestId("home-workspace-beta");
    expect(
      within(betaRow).getByTestId("workspace-readonly-badge"),
    ).toBeInTheDocument();
    expect(
      within(betaRow).getByTestId("home-workspace-subline"),
    ).toHaveTextContent("forked from Base");
    expect(
      within(betaRow).getByTestId("home-workspace-owner"),
    ).toHaveTextContent("Ada Lovelace");
    expect(
      within(betaRow)
        .getAllByRole("button")
        .map((button) => button.getAttribute("aria-label")),
    ).toEqual(["Remove beta", "Fork beta"]);
    expect(
      within(betaRow).queryByTestId("home-workspace-disclosure"),
    ).toBeNull();
    await within(betaRow).findByTestId("home-workspace-approvals-none");

    // Sorts reorder the list and land in the URL; the default is omitted.
    const sort = within(tile).getByTestId("home-workspaces-sort");
    await user.selectOptions(sort, "updated");
    expect(names()).toEqual(["alpha", "beta"]);
    expect(screen.getByTestId("home-location")).toHaveTextContent(
      "?sort=updated",
    );
    await user.selectOptions(sort, "created");
    expect(names()).toEqual(["beta", "alpha"]);
    await user.selectOptions(sort, "name");
    expect(names()).toEqual(["alpha", "beta"]);
    await user.selectOptions(sort, "opened");
    expect(names()).toEqual(["beta", "alpha"]);
    expect(screen.getByTestId("home-location")).toHaveTextContent("");

    // All lists everyone's but another reviewer's copy. The other owner's
    // row offers Fork alone; its owner comes from the roster.
    await user.click(within(tile).getByTestId("home-workspaces-filter-all"));
    expect(names()).toEqual(["beta", "alpha-fix", "alpha", "Base"]);
    expect(screen.getByTestId("home-location")).toHaveTextContent(
      "?workspaces=all",
    );
    expect(screen.queryByText("alpha-review")).toBeNull();
    const fixRow = screen.getByTestId("home-workspace-alpha-fix");
    expect(
      within(fixRow).getByTestId("home-workspace-owner"),
    ).toHaveTextContent("Grace Hopper");
    expect(
      within(fixRow)
        .getAllByRole("button")
        .map((button) => button.getAttribute("aria-label")),
    ).toEqual(["Fork alpha-fix"]);

    // Search narrows by name within the filter.
    await user.type(screen.getByTestId("home-workspace-search"), "fix");
    expect(names()).toEqual(["alpha-fix"]);
    await user.click(within(tile).getByTestId("home-workspaces-filter-mine"));
    expect(screen.getByTestId("home-workspaces-empty")).toHaveTextContent(
      'No workspace you own matches "fix".',
    );
    expect(screen.getByTestId("home-location")).toHaveTextContent("");
    await user.click(screen.getByTestId("home-workspace-search-clear"));
    expect(names()).toEqual(["beta", "alpha"]);

    // Fork from a row runs the ordinary fork dialog.
    await user.click(
      within(screen.getByTestId("home-workspace-beta")).getByRole("button", {
        name: "Fork beta",
      }),
    );
    expect(screen.getByRole("heading", { name: "Fork beta" })).toBeVisible();
    await user.keyboard("{Escape}");

    // The name is the Open link.
    const alphaLink = within(
      screen.getByTestId("home-workspace-alpha"),
    ).getByTestId("home-workspace-open");
    expect(alphaLink).toHaveAttribute("href", "/org-one/alpha");
    await user.click(alphaLink);
    expect(await screen.findByTestId("workspace-route")).toHaveTextContent(
      "org-one/alpha",
    );
  });

  it("reads the filter and sort from the URL on arrival", async () => {
    installAlignmentTiles();
    serveOrgWorkspaces("org1", [
      workspaceFixture("mine", "mine", USER_ID, "2026-08-25T00:00:00Z"),
      {
        ...workspaceFixture(
          "theirs",
          "theirs",
          "user-2",
          "2026-08-24T00:00:00Z",
        ),
      },
    ]);
    serveDomains();
    serveSearch([]);
    serveCompute([]);
    renderHome("/org-one/home?workspaces=all&sort=name");

    const tile = await screen.findByTestId("home-workspaces");
    await waitFor(() =>
      expect(
        within(tile)
          .getAllByTestId("home-workspace-open")
          .map((link) => link.textContent),
      ).toEqual(["mine", "theirs"]),
    );
    expect(
      within(tile).getByTestId("home-workspaces-filter-all"),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      (within(tile).getByTestId("home-workspaces-sort") as HTMLSelectElement)
        .value,
    ).toBe("name");
  });

  // personal_workspace g5.personal.home: the caller's personal panel sits
  // above the tiles, and the personal workspace is no Workspaces row.
  it("shows the personal panel and leaves the personal workspace out of the Workspaces tile", async () => {
    installAlignmentTiles([]);
    serveOrgWorkspaces("org1", [
      createMockWorkspace({
        id: "ws-feature",
        name: "feature",
        ownerUserId: USER_ID,
      }),
      createMockWorkspace({
        id: "personal-alex",
        name: "users/ada",
        kind: "personal",
        ownerUserId: USER_ID,
      }),
    ]);
    serveCompute([]);
    renderHome();

    const panel = await screen.findByTestId("home-personal-panel");
    expect(within(panel).getByText("Personal workspace")).toBeInTheDocument();
    expect(within(panel).getByTestId("home-personal-open")).toHaveAttribute(
      "href",
      "/org-one/me",
    );
    const tile = await screen.findByTestId("home-workspaces");
    expect(await within(tile).findByText("feature")).toBeInTheDocument();
    expect(within(tile).queryByText("users/ada")).toBeNull();
    // The personal Chat sits beside Home (g5.personal.home).
    expect(await screen.findByTestId("home-personal-chat")).toBeInTheDocument();
  });

  it("offers viewers no personal panel", async () => {
    mockUseAuth.mockReturnValue({
      user: {
        ...authedUser(),
        orgs: [
          {
            orgId: "org1",
            orgName: "Org One",
            orgSlug: "org-one",
            role: "viewer",
            orgKind: "tenant",
          },
        ],
      },
      switchOrg: vi.fn().mockResolvedValue(undefined),
      logout: vi.fn(),
      hasScope: () => false,
    });
    installAlignmentTiles([]);
    serveOrgWorkspaces("org1", []);
    serveCompute([]);
    renderHome();
    await screen.findByTestId("home-compute-tile");
    expect(screen.queryByTestId("home-personal-panel")).toBeNull();
    expect(screen.queryByTestId("home-personal-chat")).toBeNull();
    expect(screen.queryByTestId("home-personal-chat-collapsed")).toBeNull();
  });

  it("hides the issues tile while external issues are off and shows the connect hint without a domain", async () => {
    installAlignmentTiles([]);
    serveOrgWorkspaces("org1", []);
    serveCompute([]);
    const { unmount } = renderHome();
    await screen.findByTestId("home-compute-tile");
    expect(screen.queryByTestId("home-issues-tile")).not.toBeInTheDocument();
    unmount();

    installAlignmentTiles();
    serveDomains(0);
    const requests: SearchRequest[] = [];
    serveSearch([], requests, { domainCount: 0 });
    renderHome();
    const tile = await screen.findByTestId("home-issues-tile");
    expect(
      await within(tile).findByTestId("home-issues-no-domains"),
    ).toHaveTextContent("No issue tracker is configured");
    expect(within(tile).queryByTestId("home-issues-refresh")).toBeNull();
    expect(requests).toEqual([]);
  });

  it("opens every tile on All until the person finishes the onboarding tour (g5.ui.home-workspaces)", async () => {
    mockUseAuth.mockReturnValue({
      user: { ...authedUser(), onboardingStep: 2 },
      switchOrg: vi.fn().mockResolvedValue(undefined),
      logout: vi.fn(),
      hasScope: () => false,
    });
    installAlignmentTiles();
    serveOrgWorkspaces("org1", []);
    serveDomains();
    serveSearch([issue(1, { labels: [], assignees: [] })], [], {});
    serveCompute([]);
    renderHome();

    const issues = await screen.findByTestId("home-issues-tile");
    expect(
      within(issues).getByTestId("home-issues-filter-any"),
    ).toHaveAttribute("aria-pressed", "true");
    const workspaces = await screen.findByTestId("home-workspaces");
    expect(
      within(workspaces).getByTestId("home-workspaces-filter-all"),
    ).toHaveAttribute("aria-pressed", "true");
    // Mine is the non-default now, so choosing it is written to the URL.
    await userEvent.click(
      within(workspaces).getByTestId("home-workspaces-filter-mine"),
    );
    expect(
      within(workspaces).getByTestId("home-workspaces-filter-mine"),
    ).toHaveAttribute("aria-pressed", "true");
    const compute = await screen.findByTestId("home-compute-tile");
    expect(
      within(compute).getByTestId("home-compute-filter-all"),
    ).toHaveAttribute("aria-pressed", "true");
  });

  it("loads one open slice, pages it five at a time and narrows by assignee and label on the client", async () => {
    const user = userEvent.setup();
    installAlignmentTiles();
    serveOrgWorkspaces("org1", []);
    serveDomains();
    const requests: SearchRequest[] = [];
    const results = [
      ...Array.from({ length: 9 }, (_, index) =>
        issue(index + 1, {
          labels: index % 2 === 0 ? [{ name: "bug", color: "d73a4a" }] : [],
          assignees: [],
        }),
      ),
      issue(10, {
        labels: [
          { name: "bug", color: "d73a4a" },
          { name: "ui", color: null },
        ],
        assignees: [{ login: "OctoCat", avatarUrl: null }],
      }),
      issue(11, {
        labels: [],
        assignees: [{ login: "hubot", avatarUrl: null }],
      }),
    ];
    serveSearch(results, requests, { truncated: true });
    serveCompute([]);
    renderHome();

    const tile = await screen.findByTestId("home-issues-tile");
    // Assigned to me is the default (the caller's own issues first): the one
    // issue assigned to the connected login, matched regardless of case.
    expect(within(tile).getByTestId("home-issues-filter-me")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await waitFor(() =>
      expect(rowKeysOf(within(tile).getByTestId("home-issues-rows"))).toEqual([
        "home-issue-900010",
      ]),
    );
    await user.click(within(tile).getByTestId("home-issues-filter-any"));
    const rows = within(tile).getByTestId("home-issues-rows");
    // Newest update first, a page of five; the footer pages the loaded slice.
    expect(rowKeysOf(rows)).toEqual(
      [11, 10, 9, 8, 7].map((number) => `home-issue-9000${number}`),
    );
    expect(within(tile).getByTestId("home-issues-count")).toHaveTextContent(
      "11",
    );
    expect(within(tile).queryByTestId("home-issues-view-all")).toBeNull();
    await user.click(
      within(tile).getByRole("button", { name: "Show 5 more issues" }),
    );
    expect(rowKeysOf(rows)).toHaveLength(10);
    await user.click(
      within(tile).getByRole("button", { name: "Show 1 more issue" }),
    );
    expect(rowKeysOf(rows)).toHaveLength(11);
    expect(within(tile).queryByTestId("home-issues-show-more")).toBeNull();
    // One request for the whole open slice, under no server assignee filter.
    expect(requests).toEqual([{ q: "", assignee: null }]);

    // Label chips come from the loaded slice, most frequent first.
    const chips = within(tile).getByTestId("home-issues-label-chips");
    expect(
      within(chips)
        .getAllByRole("button", { pressed: false })
        .map((chip) => chip.textContent),
    ).toEqual(["bug", "ui"]);
    await user.click(within(chips).getByTestId("home-issues-label-ui"));
    expect(rowKeysOf(rows)).toEqual(["home-issue-900010"]);
    await user.click(within(chips).getByTestId("home-issues-label-bug"));
    expect(rowKeysOf(rows)).toEqual(["home-issue-900010"]);

    // Assigned to me narrows by the caller's connected login — no request;
    // the selected labels survive the switch and stay clearable even when
    // the narrowed slice has no "ui" label of its own.
    await user.click(within(tile).getByTestId("home-issues-filter-me"));
    expect(rowKeysOf(within(tile).getByTestId("home-issues-rows"))).toEqual([
      "home-issue-900010",
    ]);
    await user.click(within(tile).getByTestId("home-issues-filter-none"));
    expect(within(tile).getByTestId("home-issues-empty")).toHaveTextContent(
      "No open issues carry every selected label.",
    );
    const chipsAfter = within(tile).getByTestId("home-issues-label-chips");
    expect(
      within(chipsAfter).getByTestId("home-issues-label-ui"),
    ).toHaveAttribute("aria-pressed", "true");
    await user.click(within(chipsAfter).getByTestId("home-issues-label-clear"));
    // Filter changes start the paging over at the configured size.
    const unassigned = within(tile).getByTestId("home-issues-rows");
    expect(rowKeysOf(unassigned)).toHaveLength(5);
    expect(
      within(tile).getByRole("button", { name: "Show 4 more issues" }),
    ).toBeVisible();
    expect(
      within(unassigned.children[0] as HTMLElement).getByTestId(
        "home-issue-assignee",
      ),
    ).toHaveTextContent("unassigned");
    expect(within(tile).getByTestId("home-issues-count")).toHaveTextContent(
      "9",
    );
    expect(requests).toHaveLength(1);

    // Refresh is the only way to spend another search.
    await user.click(within(tile).getByTestId("home-issues-refresh"));
    await waitFor(() => expect(requests).toHaveLength(2));
    expect(requests[1]).toEqual({ q: "", assignee: null });
  });

  it("searches typed terms after a pause or on Enter and clears back to the loaded slice", async () => {
    const user = userEvent.setup();
    installAlignmentTiles();
    serveOrgWorkspaces("org1", []);
    serveDomains();
    const requests: SearchRequest[] = [];
    server.use(
      http.get("/api/v1/orgs/org1/issues/search", ({ request }) => {
        const q = new URL(request.url).searchParams.get("q");
        requests.push({ q, assignee: null });
        return HttpResponse.json({
          data: {
            results:
              q === ""
                ? [issue(1), issue(2)]
                : q === "login"
                  ? [issue(3, { title: "Login fails" })]
                  : [],
            domainErrors: [],
            domainCount: 1,
            truncated: false,
          },
        });
      }),
    );
    serveCompute([]);
    renderHome();

    const tile = await screen.findByTestId("home-issues-tile");
    await user.click(within(tile).getByTestId("home-issues-filter-any"));
    await within(tile).findByTestId("home-issues-rows");
    const field = within(tile).getByTestId("home-issues-search");
    expect(field).toHaveAttribute("placeholder", "Search");

    // Enter searches at once.
    await user.type(field, "login{Enter}");
    await waitFor(() =>
      expect(rowKeysOf(within(tile).getByTestId("home-issues-rows"))).toEqual([
        `home-issue-${issue(3).externalIssueId}`,
      ]),
    );
    expect(requests.map((request) => request.q)).toEqual(["", "login"]);
    expect(within(tile).getByTestId("home-issues-count")).toHaveTextContent(
      "1",
    );

    // Clearing restores the termless slice at once, from the cache.
    await user.click(within(tile).getByTestId("home-issues-search-clear"));
    expect(
      rowKeysOf(within(tile).getByTestId("home-issues-rows")),
    ).toHaveLength(2);
    expect(requests).toHaveLength(2);

    // A pause after an unfinished qualifier is not a search; after finished
    // text it is, and no match says so with the terms.
    await user.type(field, "author:");
    await new Promise((resolve) =>
      setTimeout(resolve, ISSUE_SEARCH_DEBOUNCE_MS + 300),
    );
    expect(requests).toHaveLength(2);
    await user.type(field, "octo");
    await waitFor(
      () =>
        expect(requests.map((request) => request.q)).toContain("author:octo"),
      { timeout: ISSUE_SEARCH_DEBOUNCE_MS + 1000 },
    );
    expect(
      await within(tile).findByTestId("home-issues-empty"),
    ).toHaveTextContent("No open issues match “author:octo”.");
  });

  it("asks to connect GitHub for the default Assigned to me without a connected account, without searching again", async () => {
    const user = userEvent.setup();
    installAlignmentTiles(["external_issues"], null);
    serveOrgWorkspaces("org1", []);
    serveDomains();
    const requests: SearchRequest[] = [];
    serveSearch([issue(1)], requests);
    serveCompute([]);
    renderHome();

    const tile = await screen.findByTestId("home-issues-tile");
    // Assigned to me is the default, so the hint shows at once; All lists
    // the loaded slice without another search.
    const hint = await within(tile).findByTestId(
      "home-issues-connection-required",
    );
    expect(within(hint).getByTestId("home-issues-connect")).toHaveAttribute(
      "href",
      expect.stringContaining("/account/github?returnTo="),
    );
    expect(requests).toHaveLength(1);
    await user.click(within(tile).getByTestId("home-issues-filter-any"));
    await within(tile).findByTestId("home-issues-rows");
    expect(requests).toHaveLength(1);
  });

  it("offers Open for an owned issue, Start for an unowned one, and links an existing workspace with the panel's conflict fallbacks", async () => {
    const user = userEvent.setup();
    installAlignmentTiles();
    serveOrgWorkspaces("org1", [
      workspaceFixture("ws-1", "Alpha", USER_ID, "2026-08-24T15:00:00Z"),
      workspaceFixture("ws-2", "Beta", USER_ID, "2026-08-23T15:00:00Z"),
      workspaceFixture("ws-3", "Theirs", "user-2", "2026-08-25T15:00:00Z"),
    ]);
    serveDomains();
    serveSearch([
      issue(1, {
        primaryWorkspace: { visible: true, workspaceId: "ws-1", name: "Alpha" },
      }),
      issue(2, { primaryWorkspace: { visible: false } }),
      issue(3),
      issue(4),
    ]);
    serveCompute([]);
    const linkBodies: Array<{ workspaceId: string; body: unknown }> = [];
    const conflicts: Array<Record<string, unknown>> = [
      { code: "workspace_has_primary" },
      {
        code: "issue_has_primary",
        ownerWorkspace: { visible: true, workspaceId: "ws-9", name: "Gamma" },
      },
    ];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/issue-links",
        async ({ request, params }) => {
          const body = (await request.json()) as { role: string };
          linkBodies.push({ workspaceId: String(params.workspaceId), body });
          const conflict = body.role === "primary" ? conflicts.shift() : null;
          if (conflict) {
            return HttpResponse.json(
              {
                error: {
                  code: "CONFLICT",
                  message: "primary exists",
                  details: conflict,
                },
              },
              { status: 409 },
            );
          }
          return HttpResponse.json({ data: { id: "link-1" } }, { status: 201 });
        },
      ),
    );
    renderHome();

    const tile = await screen.findByTestId("home-issues-tile");
    await user.click(within(tile).getByTestId("home-issues-filter-any"));
    const owned = await within(tile).findByTestId("home-issue-90001");
    expect(within(owned).getByTestId("home-issue-open")).toHaveAttribute(
      "href",
      "/org-one/Alpha",
    );
    expect(
      within(owned).getByTestId("home-issue-workspace-chip"),
    ).toHaveTextContent("Alpha");
    expect(
      within(owned).queryByTestId("home-issue-link-existing"),
    ).not.toBeInTheDocument();
    const hidden = within(tile).getByTestId("home-issue-90002");
    expect(
      within(hidden).getByTestId("home-issue-workspace-hidden"),
    ).toBeVisible();
    expect(
      within(hidden).queryByTestId("home-issue-start-workspace"),
    ).not.toBeInTheDocument();

    // workspace_has_primary → the shared notice offers "related".
    const unowned = within(tile).getByTestId("home-issue-90003");
    await user.click(within(unowned).getByTestId("home-issue-link-existing"));
    const picker = within(unowned).getByTestId("home-issue-link-picker");
    expect(
      within(picker)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Alpha", "Beta"]);
    await user.selectOptions(
      within(picker).getByTestId("home-issue-link-target"),
      "ws-2",
    );
    await user.click(within(picker).getByTestId("home-issue-link-submit"));
    expect(
      await within(picker).findByTestId(
        "home-issue-conflict-workspace_has_primary",
      ),
    ).toBeVisible();
    await user.click(within(picker).getByTestId("home-issue-link-as-related"));
    await waitFor(() =>
      expect(
        within(unowned).queryByTestId("home-issue-link-picker"),
      ).not.toBeInTheDocument(),
    );

    // issue_has_primary → the owner is named and "related" is still offered.
    const other = within(tile).getByTestId("home-issue-90004");
    await user.click(within(other).getByTestId("home-issue-link-existing"));
    const picker2 = within(other).getByTestId("home-issue-link-picker");
    await user.click(within(picker2).getByTestId("home-issue-link-submit"));
    const conflict = await within(picker2).findByTestId(
      "home-issue-conflict-issue_has_primary",
    );
    expect(
      within(conflict).getByTestId("home-issue-conflict-owner"),
    ).toHaveAttribute("href", "/org-one/Gamma?view=issues");
    await user.click(within(picker2).getByTestId("home-issue-link-as-related"));
    await waitFor(() =>
      expect(
        within(other).queryByTestId("home-issue-link-picker"),
      ).not.toBeInTheDocument(),
    );
    expect(linkBodies).toEqual([
      { workspaceId: "ws-2", body: { ref: "acme/api#3", role: "primary" } },
      { workspaceId: "ws-2", body: { ref: "acme/api#3", role: "related" } },
      { workspaceId: "ws-1", body: { ref: "acme/api#4", role: "primary" } },
      { workspaceId: "ws-1", body: { ref: "acme/api#4", role: "related" } },
    ]);

    await user.click(within(other).getByTestId("home-issue-start-workspace"));
    expect(await screen.findByRole("dialog")).toBeVisible();
  });

  it("leaves an administrator's Base out of the Link picker (g5.issue.base-links)", async () => {
    const user = userEvent.setup();
    installAlignmentTiles();
    serveOrgWorkspaces("org1", [
      {
        ...workspaceFixture("ws-base", "Base", USER_ID, "2026-08-26T15:00:00Z"),
        isBase: true,
      },
      workspaceFixture("ws-1", "Alpha", USER_ID, "2026-08-24T15:00:00Z"),
    ]);
    serveDomains();
    serveSearch([issue(3)]);
    serveCompute([]);
    renderHome();

    const tile = await screen.findByTestId("home-issues-tile");
    await user.click(within(tile).getByTestId("home-issues-filter-any"));
    const row = await within(tile).findByTestId("home-issue-90003");
    await user.click(within(row).getByTestId("home-issue-link-existing"));
    const picker = within(row).getByTestId("home-issue-link-picker");
    expect(
      within(picker)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Alpha"]);
  });

  function serveCreateTarget(
    state: string,
    overrides: Record<string, unknown> = {},
  ) {
    server.use(
      http.get("/api/v1/orgs/org1/issues/create-target", () =>
        HttpResponse.json({
          data: {
            state,
            destination:
              state === "no_target"
                ? null
                : {
                    domainId: "domain-1",
                    trackerId: "tracker-1",
                    repositoryId: "11",
                    displayKey: "acme/repo1",
                    contentTrusted: true,
                  },
            githubLogin: state === "connection_required" ? null : "octocat",
            assignableUsers: [],
            error: null,
            ...overrides,
          },
        }),
      ),
    );
  }

  function serveCreatedIssue() {
    const requests: Array<Record<string, unknown>> = [];
    server.use(
      http.post("/api/v1/orgs/org1/issues", async ({ request }) => {
        const input = (await request.json()) as Record<string, unknown>;
        requests.push(input);
        return HttpResponse.json(
          {
            data: {
              state: "created",
              sent: true,
              issue: {
                externalIssueId: "900042",
                repositoryId: "11",
                number: 42,
                displayKey: "acme/repo1#42",
                title: String(input.title),
                url: "https://github.com/acme/repo1/issues/42",
              },
              idempotencyKey: input.idempotencyKey,
              destination: {
                domainId: "domain-1",
                trackerId: "tracker-1",
                repositoryId: "11",
                displayKey: "acme/repo1",
                contentTrusted: true,
              },
              githubLogin: "octocat",
              title: String(input.title),
              payload: "<!-- g5-issue abc -->",
              assignees: [],
              message: null,
              parent: null,
            },
          },
          { status: 201 },
        );
      }),
    );
    return requests;
  }

  it.each([
    ["no_target", "No repository is set up for new issues"],
    [
      "comments_disabled",
      "An organization admin has not enabled G5 comments and issue creation for acme/repo1",
    ],
    [
      "public_destination",
      "acme/repo1 is public, and G5 does not create issues in public repositories",
    ],
    [
      "visibility_unknown",
      "GitHub did not report whether acme/repo1 is private, so G5 cannot create issues there",
    ],
    ["connection_required", "Connect GitHub to create issues"],
  ])("shows Create disabled with the %s reason", async (state, reason) => {
    installAlignmentTiles();
    serveOrgWorkspaces("org1", []);
    serveDomains();
    serveSearch([]);
    serveCompute([]);
    serveCreateTarget(state);
    renderHome();

    const create = await within(
      await screen.findByTestId("home-issues-tile"),
    ).findByTestId("home-issues-create");
    await waitFor(() => expect(create).toHaveAttribute("title", reason));
    expect(create).toBeDisabled();
    expect(create).toBeVisible();
  });

  it("Create starts an empty dialog whatever the filters and opens the new issue's page", async () => {
    const user = userEvent.setup();
    installAlignmentTiles();
    serveOrgWorkspaces("org1", []);
    serveDomains();
    serveSearch([issue(1)]);
    serveCompute([]);
    serveCreateTarget("ready");
    const requests = serveCreatedIssue();
    renderHome();

    const tile = await screen.findByTestId("home-issues-tile");
    await user.click(within(tile).getByTestId("home-issues-filter-me"));
    const create = within(tile).getByTestId("home-issues-create");
    await waitFor(() => expect(create).toBeEnabled());
    await user.click(create);

    const dialog = await screen.findByTestId("create-issue-dialog");
    expect(within(dialog).getByTestId("create-issue-title")).toHaveValue("");
    expect(
      within(dialog).getByTestId("create-issue-assign-me"),
    ).not.toBeChecked();
    expect(
      within(dialog).getByTestId("create-issue-destination"),
    ).toHaveTextContent("Creating in acme/repo1 as @octocat");
    await user.type(
      within(dialog).getByTestId("create-issue-title"),
      "Ship the tile",
    );
    await user.click(within(dialog).getByTestId("create-issue-submit"));

    expect(await screen.findByTestId("issue-route")).toHaveTextContent(
      "org-one/11/42",
    );
    expect(requests).toEqual([
      expect.objectContaining({
        title: "Ship the tile",
        assignees: [],
        domainId: "domain-1",
      }),
    ]);
  });

  it("Create & Fork opens the fork dialog with the new issue preselected as primary", async () => {
    const user = userEvent.setup();
    installAlignmentTiles();
    serveOrgWorkspaces("org1", [
      {
        ...workspaceFixture("base", "Base", USER_ID, "2026-08-24T00:00:00Z"),
        isBase: true,
      },
    ]);
    serveDomains();
    serveSearch([]);
    serveCompute([]);
    serveCreateTarget("ready");
    serveCreatedIssue();
    serveBaseAgentLaunch();
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/fork_advisories", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/orgs/org1/issues/resolve", () =>
        HttpResponse.json({
          data: makeIssueSummary({
            externalIssueId: "900042",
            displayKey: "acme/repo1#42",
            title: "Ship the tile",
            locator: { repositoryId: "11", number: 42 },
          }),
        }),
      ),
    );
    renderHome();

    const create = await within(
      await screen.findByTestId("home-issues-tile"),
    ).findByTestId("home-issues-create");
    await waitFor(() => expect(create).toBeEnabled());
    await user.click(create);
    await user.type(
      await screen.findByTestId("create-issue-title"),
      "Ship the tile",
    );
    await user.click(screen.getByTestId("create-issue-submit-fork"));

    expect(
      await screen.findByRole("heading", { name: "Fork Base" }),
    ).toBeVisible();
    expect(screen.queryByTestId("create-issue-dialog")).not.toBeInTheDocument();
    expect(await screen.findByTestId("clone-issue-ref")).toHaveValue(
      "acme/repo1#42",
    );
    expect(screen.getByTestId("clone-issue-role")).toHaveValue("primary");
    await waitFor(() =>
      expect(screen.getByTestId("clone-issue-lookup")).toHaveTextContent(
        "acme/repo1#42 · Ship the tile",
      ),
    );
  });

  it("tells unavailable, partial and empty slices apart", async () => {
    const user = userEvent.setup();
    installAlignmentTiles();
    serveOrgWorkspaces("org1", []);
    serveDomains(2);
    serveCompute([]);
    const failures = [
      {
        domainId: "domain-1",
        displayKey: "acme/repo1",
        issueError: "github_rate_limited",
        message: "rate limited",
      },
      {
        domainId: "domain-2",
        displayKey: "acme/repo2",
        issueError: "github_unavailable",
        message: "upstream down",
      },
    ];
    const requests: SearchRequest[] = [];
    let phase: "unavailable" | "partial" | "empty" = "unavailable";
    server.use(
      http.get("/api/v1/orgs/org1/issues/search", () => {
        requests.push({ q: "", assignee: null });
        return HttpResponse.json({
          data: {
            results: phase === "partial" ? [issue(1)] : [],
            domainErrors:
              phase === "unavailable"
                ? failures
                : phase === "partial"
                  ? [failures[1]]
                  : [],
            domainCount: 2,
            truncated: false,
          },
        });
      }),
    );
    renderHome();

    const tile = await screen.findByTestId("home-issues-tile");
    await user.click(within(tile).getByTestId("home-issues-filter-any"));
    const unavailable = await within(tile).findByTestId(
      "home-issues-unavailable",
    );
    expect(
      within(unavailable).getByTestId("home-issue-domain-error-domain-1"),
    ).toHaveTextContent("acme/repo1");
    expect(
      within(unavailable).getByTestId("home-issue-domain-error-domain-2"),
    ).toHaveTextContent("acme/repo2");
    expect(within(tile).queryByTestId("home-issues-empty")).toBeNull();
    expect(within(tile).queryByTestId("home-issues-view-all")).toBeNull();

    phase = "partial";
    await user.click(
      within(unavailable).getByRole("button", { name: /retry/i }),
    );
    const partial = await within(tile).findByTestId(
      "home-issues-domain-errors",
    );
    expect(
      within(partial).getByTestId("home-issue-domain-error-domain-2"),
    ).toBeVisible();
    expect(within(tile).getByTestId("home-issues-rows").children).toHaveLength(
      1,
    );

    phase = "empty";
    await user.click(within(tile).getByTestId("home-issues-refresh"));
    expect(
      await within(tile).findByTestId("home-issues-empty"),
    ).toHaveTextContent("No open issues.");
    expect(within(tile).queryByTestId("home-issues-domain-errors")).toBeNull();
    expect(requests).toHaveLength(3);
  });

  it("labels agent review resources, links source approvals, and offers no private controls", async () => {
    installAlignmentTiles([]);
    const agentReview = {
      reviewId: "run",
      sourceWorkspaceId: "source",
      sourceWorkspaceName: "Source",
    };
    serveOrgWorkspaces("org1", [
      {
        ...workspaceFixture(
          "review",
          "agent-review",
          USER_ID,
          "2026-08-27T00:00:00Z",
        ),
        isReview: true,
        agentReview,
      },
    ]);
    serveCompute(() => [
      computeRow("review", "agent-review", USER_ID, { agentReview }),
    ]);
    renderHome();
    const workspace = await screen.findByTestId("home-workspace-review");
    const compute = await screen.findByTestId("home-compute-review");
    for (const row of [workspace, compute])
      expect(within(row).getByText("Agent review")).toBeVisible();
    expect(
      within(workspace).getByTestId("home-workspace-open"),
    ).toHaveAttribute("href", "/org-one/Source?view=approvals");
    expect(within(compute).getByTestId("home-compute-open")).toHaveAttribute(
      "href",
      "/org-one/Source?view=approvals",
    );
    expect(within(compute).queryByTestId("home-compute-shutdown")).toBeNull();
    expect(within(workspace).queryByTestId("home-workspace-fork")).toBeNull();
    expect(within(workspace).queryByTestId("home-workspace-remove")).toBeNull();
    expect(within(workspace).queryByTestId("home-workspace-rename")).toBeNull();
    expect(
      selectHomeWorkspaces(
        [
          {
            ...workspaceFixture(
              "other",
              "other-agent",
              "user-2",
              "2026-08-27T00:00:00Z",
            ),
            isReview: true,
            agentReview,
          },
        ],
        {
          callerUserId: USER_ID,
          filter: "all",
          sort: "updated",
          query: "",
          openedAt: new Map(),
        },
      ),
    ).toHaveLength(1);
  });

  it("lists running compute with size and uptime, Mine first, and stops only the caller's own rows without claiming a flush", async () => {
    const user = userEvent.setup();
    installAlignmentTiles([]);
    serveOrgWorkspaces("org1", []);
    let rows = [
      computeRow("ws-1", "Alpha", USER_ID),
      computeRow("ws-3", "Theirs", "user-2", {
        status: "starting",
        startedAt: null,
        slotCpus: null,
        slotMemMib: null,
        instanceType: null,
      }),
    ];
    serveCompute(() => rows);
    const stops: string[] = [];
    server.use(
      http.post("/api/v1/workspaces/:workspaceId/stop", ({ params }) => {
        stops.push(String(params.workspaceId));
        // The inventory still reports the row as running for this poll: the
        // control must stay disabled regardless.
        return HttpResponse.json({
          data: {
            operationId: "op-1",
            status: "stopping",
            flushId: null,
            unflushedLoss: false,
          },
        });
      }),
    );
    renderHome();

    const tile = await screen.findByTestId("home-compute-tile");
    // Mine is the default: the caller's row shows and the other person's
    // waits behind All; the count is the whole inventory.
    const mine = await within(tile).findByTestId("home-compute-ws-1");
    expect(
      within(tile).getByTestId("home-compute-filter-mine"),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      within(tile).queryByTestId("home-compute-ws-3"),
    ).not.toBeInTheDocument();
    expect(within(tile).getByTestId("home-compute-count")).toHaveTextContent(
      "2",
    );
    expect(within(mine).getByTestId("home-compute-details")).toHaveTextContent(
      "4 vCPU · 16 GiB · up 2h 14m",
    );
    // The name is the link to the workspace; there is no Open control.
    expect(within(mine).getByTestId("home-compute-open")).toHaveAttribute(
      "href",
      "/org-one/Alpha",
    );
    expect(within(mine).getByTestId("home-compute-open")).toHaveTextContent(
      "Alpha",
    );
    expect(within(mine).queryByRole("link", { name: "Open" })).toBeNull();
    await user.click(within(tile).getByTestId("home-compute-filter-all"));
    const theirs = within(tile).getByTestId("home-compute-ws-3");
    expect(
      within(theirs).getByTestId("home-compute-details"),
    ).toHaveTextContent("starting");
    expect(
      within(theirs).queryByTestId("home-compute-shutdown"),
    ).not.toBeInTheDocument();

    await user.click(within(tile).getByTestId("home-compute-filter-mine"));
    expect(
      within(tile).queryByTestId("home-compute-ws-3"),
    ).not.toBeInTheDocument();
    expect(within(tile).getByTestId("home-compute-ws-1")).toBeVisible();

    await user.click(within(mine).getByTestId("home-compute-shutdown"));
    await waitFor(() => expect(stops).toEqual(["ws-1"]));
    // A pending stop is announced as such — no flush claim — and the row
    // stays "shutting down…" with its control disabled after the mutation
    // settles, even while the inventory still says running.
    expect(await screen.findByText("Shutting down Alpha…")).toBeVisible();
    expect(screen.queryByText(/latest flush was recorded/)).toBeNull();
    await waitFor(() => expect(stops).toHaveLength(1));
    expect(within(tile).getByTestId("home-compute-ws-1")).toHaveAttribute(
      "data-status",
      "stopping",
    );
    expect(within(tile).getByTestId("home-compute-shutdown")).toBeDisabled();
    expect(within(tile).getByTestId("home-compute-shutdown")).toHaveTextContent(
      "Stopping…",
    );
    rows = rows.filter((row) => row.workspaceId !== "ws-1");
    await user.click(within(tile).getByTestId("home-compute-filter-all"));
    await waitFor(
      () =>
        expect(
          within(tile).queryByTestId("home-compute-ws-1"),
        ).not.toBeInTheDocument(),
      { timeout: 8_000 },
    );
    expect(stops).toEqual(["ws-1"]);
  }, 15_000);

  it.each([
    {
      name: "a recorded flush",
      outcome: { status: "stopped", flushId: "flush-1", unflushedLoss: false },
      message: "The latest flush was recorded.",
    },
    {
      name: "no flush evidence",
      outcome: { status: "stopped", flushId: null, unflushedLoss: true },
      message: "The VM is stopped.",
    },
  ])(
    "announces an already-stopped outcome with $name honestly",
    async ({ outcome, message }) => {
      const user = userEvent.setup();
      installAlignmentTiles([]);
      serveOrgWorkspaces("org1", []);
      serveCompute([computeRow("ws-1", "Alpha", USER_ID)]);
      server.use(
        http.post("/api/v1/workspaces/ws-1/stop", () =>
          HttpResponse.json({ data: { operationId: "op-1", ...outcome } }),
        ),
      );
      renderHome();
      const tile = await screen.findByTestId("home-compute-tile");
      const mine = await within(tile).findByTestId("home-compute-ws-1");
      await user.click(within(mine).getByTestId("home-compute-shutdown"));
      expect(await screen.findByText("Workspace stopped")).toBeVisible();
      expect(screen.getByText(message)).toBeVisible();
    },
  );

  it("pages running compute by the person's size (g5.ui.home-paging)", async () => {
    const user = userEvent.setup();
    installAlignmentTiles([]);
    serveOrgWorkspaces("org1", []);
    serveCompute(
      Array.from({ length: 7 }, (_, index) =>
        computeRow(`ws-${index}`, `Space ${index}`, USER_ID),
      ),
    );
    renderHome();

    const tile = await screen.findByTestId("home-compute-tile");
    await waitFor(() =>
      expect(within(tile).getAllByTestId(/^home-compute-ws-/)).toHaveLength(5),
    );
    expect(within(tile).getByTestId("home-compute-count")).toHaveTextContent(
      "7",
    );
    await user.click(
      within(tile).getByRole("button", { name: "Show 2 more workspaces" }),
    );
    expect(within(tile).getAllByTestId(/^home-compute-ws-/)).toHaveLength(7);
    expect(within(tile).queryByTestId("home-compute-show-more")).toBeNull();
    // A filter change starts the paging over.
    await user.click(within(tile).getByTestId("home-compute-filter-all"));
    expect(within(tile).getAllByTestId(/^home-compute-ws-/)).toHaveLength(5);
  });

  it("reports a refused stop, an unavailable inventory and an empty one", async () => {
    const user = userEvent.setup();
    installAlignmentTiles([]);
    serveOrgWorkspaces("org1", []);
    let failInventory = true;
    server.use(
      http.get("/api/v1/orgs/:orgId/compute/running", () =>
        failInventory
          ? HttpResponse.json(
              { error: { code: "INTERNAL", message: "boom" } },
              { status: 500 },
            )
          : HttpResponse.json({
              data: [computeRow("ws-1", "Alpha", USER_ID)],
            }),
      ),
      http.post("/api/v1/workspaces/ws-1/stop", () =>
        HttpResponse.json(
          { error: { code: "CONFLICT", message: "stop is already pending" } },
          { status: 409 },
        ),
      ),
    );
    renderHome();

    const tile = await screen.findByTestId("home-compute-tile");
    expect(
      await within(tile).findByRole("alert", { name: "Running compute error" }),
    ).toBeVisible();
    failInventory = false;
    await user.click(within(tile).getByTestId("home-compute-retry"));
    const mine = await within(tile).findByTestId("home-compute-ws-1");
    await user.click(within(mine).getByTestId("home-compute-shutdown"));
    expect(await screen.findByText("Could not stop workspace")).toBeVisible();
    // A refused stop leaves the row in place and the control enabled again.
    await waitFor(() =>
      expect(within(tile).getByTestId("home-compute-shutdown")).toBeEnabled(),
    );
    expect(within(tile).getByTestId("home-compute-ws-1")).toHaveAttribute(
      "data-status",
      "running",
    );
  });

  function selfMember(shutdownPending: boolean) {
    return {
      userId: USER_ID,
      email: "ada@example.com",
      displayName: "Ada Lovelace",
      avatarUrl: null,
      role: "member",
      joinedAt: "2026-01-01T00:00:00Z",
      deactivatedAt: null,
      activityRevision: 1,
      shutdownPending,
    };
  }

  it("Stop my workspaces stops the caller's own after confirming, then shows progress and only progress while pending (g5.lifecycle.stop.by-user)", async () => {
    const user = userEvent.setup();
    installAlignmentTiles([]);
    serveOrgWorkspaces("org1", []);
    const computeHits: number[] = [];
    serveCompute([], computeHits);
    let pending = false;
    const stopped: string[] = [];
    server.use(
      http.get("/api/v1/orgs/org1/members", () =>
        HttpResponse.json({ data: [selfMember(pending)] }),
      ),
      http.post(
        "/api/v1/orgs/:orgId/members/:userId/stop-workspaces",
        ({ params }) => {
          stopped.push(`${String(params.orgId)}/${String(params.userId)}`);
          pending = true;
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
              workspaceId: "ws-1",
              name: "alpha",
              state: "stopping",
              blocker: "stop_transport_retry",
            },
            { workspaceId: "ws-2", name: "beta", state: "stopped" },
          ],
        }),
      ),
    );
    renderHome();

    const control = await screen.findByTestId("home-stop-my-workspaces");
    expect(control).toHaveTextContent("Stop my workspaces");
    // It sits in the Running compute tile's header, where the caller's VMs
    // are listed, not beside New workspace.
    expect(screen.getByTestId("home-compute-tile")).toContainElement(control);
    // Cancel sends nothing.
    await user.click(control);
    expect(screen.getByText(/nothing restarts automatically/)).toBeVisible();
    await user.click(screen.getByTestId("home-stop-my-workspaces-cancel"));
    expect(stopped).toEqual([]);

    await waitFor(() => expect(computeHits.length).toBeGreaterThan(0));
    const computeReadsBefore = computeHits.length;
    await user.click(control);
    await user.click(screen.getByTestId("home-stop-my-workspaces-confirm"));
    expect(
      await screen.findByText(/alpha: Stopping; awaiting confirmation/),
    ).toBeVisible();
    expect(screen.getByText("Blocked: stop_transport_retry")).toBeVisible();
    expect(screen.getByText(/beta: Stopped/)).toBeVisible();
    expect(stopped).toEqual([`org1/${USER_ID}`]);
    // Running compute refreshes rather than waiting for its slow poll.
    await waitFor(() =>
      expect(computeHits.length).toBeGreaterThan(computeReadsBefore),
    );

    await user.click(screen.getByRole("button", { name: "Close" }));
    // The roster now reports the hold: the control opens progress directly.
    await waitFor(() =>
      expect(screen.getByTestId("home-stop-my-workspaces")).toHaveTextContent(
        "Stopping my workspaces…",
      ),
    );
    await user.click(screen.getByTestId("home-stop-my-workspaces"));
    expect(
      await screen.findByText(/alpha: Stopping; awaiting confirmation/),
    ).toBeVisible();
    expect(screen.queryByTestId("home-stop-my-workspaces-confirm")).toBeNull();
    expect(stopped).toHaveLength(1);
  });

  it("reports a refused Stop my workspaces and offers viewers none", async () => {
    const user = userEvent.setup();
    installAlignmentTiles([]);
    serveOrgWorkspaces("org1", []);
    serveCompute([]);
    server.use(
      http.get("/api/v1/orgs/org1/members", () =>
        HttpResponse.json({ data: [selfMember(false)] }),
      ),
      http.post("/api/v1/orgs/:orgId/members/:userId/stop-workspaces", () =>
        HttpResponse.json(
          { error: { code: "FORBIDDEN", message: "membership is inactive" } },
          { status: 403 },
        ),
      ),
    );
    const { unmount } = renderHome();
    await user.click(await screen.findByTestId("home-stop-my-workspaces"));
    await user.click(screen.getByTestId("home-stop-my-workspaces-confirm"));
    expect(
      await screen.findByText("Could not stop your workspaces"),
    ).toBeVisible();
    expect(screen.queryByLabelText("Workspace shutdown progress")).toBeNull();
    unmount();

    mockUseAuth.mockReturnValue({
      user: {
        ...authedUser(),
        orgs: [
          {
            orgId: "org1",
            orgName: "Org One",
            orgSlug: "org-one",
            role: "viewer",
          },
        ],
      },
      switchOrg: vi.fn().mockResolvedValue(undefined),
      logout: vi.fn(),
      hasScope: () => false,
    });
    renderHome();
    await screen.findByTestId("home-new-workspace");
    expect(screen.queryByTestId("home-stop-my-workspaces")).toBeNull();
  });
});

describe("Needs Your Attention order (g5.ui.home-attention-order)", () => {
  function installLayout() {
    mockUseInstanceInfo.mockReturnValue(instanceInfoWithFeatures([]));
    server.use(
      http.get("/api/v1/orgs/org1/members", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/orgs/org1/compute/running", () =>
        HttpResponse.json({ data: [] }),
      ),
    );
  }

  // Seven outstanding items, one day apart, alternating kinds. The fake
  // server orders the whole list as the real one does, and pages it by an
  // index cursor that belongs to one order.
  const ITEMS = Array.from({ length: 7 }, (_, index) => ({
    id: `item-${index}`,
    kind: index % 2 === 0 ? "approval" : "open_question",
    workspaceId: `ws-${index}`,
    workspaceName: `Item ${index}`,
    sourceId: `src-${index}`,
    actionableAt: `2026-08-${String(10 + index)}T00:00:00Z`,
  }));
  type AttentionRequest = {
    order: string | null;
    kind: string | null;
    cursor: string | null;
  };

  function serveOrderedAttention(failNext?: { current: boolean }) {
    const requests: AttentionRequest[] = [];
    server.use(
      http.get("/api/v1/orgs/org1/home/attention", ({ request }) => {
        const params = new URL(request.url).searchParams;
        const order = params.get("order");
        const kind = params.get("kind");
        const cursor = params.get("cursor");
        requests.push({ order, kind, cursor });
        if (failNext?.current) {
          failNext.current = false;
          return HttpResponse.json({ error: "temporary" }, { status: 503 });
        }
        const limit = Number(params.get("limit"));
        const listed = ITEMS.filter((item) => !kind || item.kind === kind);
        if (order === "newest") listed.reverse();
        const [cursorOrder, cursorIndex] = cursor?.split(":") ?? [order, "0"];
        if (cursorOrder !== order) {
          return HttpResponse.json({ error: "cursor" }, { status: 422 });
        }
        const start = Number(cursorIndex);
        const items = listed.slice(start, start + limit);
        const next = start + items.length;
        return HttpResponse.json({
          data: {
            items,
            nextCursor: next < listed.length ? `${order}:${next}` : null,
          },
        });
      }),
    );
    return requests;
  }

  const attentionRows = () =>
    within(screen.getByTestId("home-needs-attention"))
      .queryAllByRole("listitem")
      .map((row) => within(row).getByTestId("home-attention-name").textContent);
  const orderControl = () =>
    within(screen.getByTestId("home-needs-attention")).getByRole("combobox", {
      name: "Sort attention items",
    }) as HTMLSelectElement;

  it("opens on Newest for everyone with the Workspaces sort's pill, offering only Newest and Oldest", async () => {
    installLayout();
    serveOrgWorkspaces("org1", []);
    const requests = serveOrderedAttention();
    renderHome();

    await waitFor(() =>
      expect(attentionRows()).toEqual([
        "Item 6",
        "Item 5",
        "Item 4",
        "Item 3",
        "Item 2",
      ]),
    );
    expect(requests).toEqual([{ order: "newest", kind: null, cursor: null }]);
    const control = orderControl();
    expect(control.value).toBe("newest");
    expect(
      within(control)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Newest", "Oldest"]);
    expect(
      within(control).getByRole("option", { selected: true }),
    ).toHaveTextContent("Newest");
    // R4: the same control as the Workspaces sort, look included.
    const workspacesSort = await screen.findByRole("combobox", {
      name: "Sort workspaces",
    });
    expect(control.tagName).toBe(workspacesSort.tagName);
    expect(control.className).toBe(workspacesSort.className);
    expect(screen.getByTestId("home-location")).toHaveTextContent(/^$/);
  });

  it("orders the whole list by Oldest through Show more and the kind filter, and keeps it in the URL", async () => {
    const user = userEvent.setup();
    installLayout();
    serveOrgWorkspaces("org1", []);
    const requests = serveOrderedAttention();
    renderHome();
    await waitFor(() => expect(attentionRows()[0]).toBe("Item 6"));

    await user.selectOptions(orderControl(), "oldest");
    await waitFor(() =>
      expect(attentionRows()).toEqual([
        "Item 0",
        "Item 1",
        "Item 2",
        "Item 3",
        "Item 4",
      ]),
    );
    expect(requests.at(-1)).toEqual({
      order: "oldest",
      kind: null,
      cursor: null,
    });
    expect(screen.getByTestId("home-location")).toHaveTextContent(
      "?attentionSort=oldest",
    );

    await user.click(
      screen.getByRole("button", { name: "Show more attention items" }),
    );
    await waitFor(() => expect(attentionRows()).toHaveLength(7));
    expect(attentionRows().slice(5)).toEqual(["Item 5", "Item 6"]);
    expect(requests.at(-1)).toEqual({
      order: "oldest",
      kind: null,
      cursor: "oldest:5",
    });

    await user.click(
      within(
        screen.getByRole("group", { name: "Filter attention items" }),
      ).getByRole("button", { name: "Approval" }),
    );
    await waitFor(() =>
      expect(attentionRows()).toEqual(["Item 0", "Item 2", "Item 4", "Item 6"]),
    );
    expect(requests.at(-1)).toEqual({
      order: "oldest",
      kind: "approval",
      cursor: null,
    });
    expect(orderControl().value).toBe("oldest");
  });

  it("returns to the first page on every switch, including back to an order whose pages were revealed", async () => {
    const user = userEvent.setup();
    installLayout();
    serveOrgWorkspaces("org1", []);
    const requests = serveOrderedAttention();
    renderHome();
    await waitFor(() => expect(attentionRows()).toHaveLength(5));
    await user.click(
      screen.getByRole("button", { name: "Show more attention items" }),
    );
    await waitFor(() => expect(attentionRows()).toHaveLength(7));

    await user.selectOptions(orderControl(), "oldest");
    await waitFor(() => expect(attentionRows()[0]).toBe("Item 0"));
    expect(attentionRows()).toHaveLength(5);

    requests.length = 0;
    await user.selectOptions(orderControl(), "newest");
    await waitFor(() => expect(attentionRows()[0]).toBe("Item 6"));
    expect(attentionRows()).toHaveLength(5);
    expect(
      screen.getByRole("button", { name: "Show more attention items" }),
    ).toBeVisible();
    expect(requests).toEqual([{ order: "newest", kind: null, cursor: null }]);
    expect(screen.getByTestId("home-location")).toHaveTextContent(/^$/);
  });

  it("arrives on the order named in the URL beside the Workspaces sort, and changes one without the other", async () => {
    const user = userEvent.setup();
    installLayout();
    serveOrgWorkspaces("org1", []);
    const requests = serveOrderedAttention();
    renderHome("/org-one/home?attentionSort=oldest&sort=name");

    await waitFor(() => expect(attentionRows()[0]).toBe("Item 0"));
    expect(requests[0]).toEqual({ order: "oldest", kind: null, cursor: null });
    expect(orderControl().value).toBe("oldest");
    const workspacesSort = (await screen.findByRole("combobox", {
      name: "Sort workspaces",
    })) as HTMLSelectElement;
    expect(workspacesSort.value).toBe("name");

    await user.selectOptions(orderControl(), "newest");
    await waitFor(() => expect(attentionRows()[0]).toBe("Item 6"));
    expect(screen.getByTestId("home-location")).toHaveTextContent(
      /^\?sort=name$/,
    );
    expect(workspacesSort.value).toBe("name");
  });

  it("keeps the chosen order when its read fails, and Retry loads that order", async () => {
    const user = userEvent.setup();
    installLayout();
    serveOrgWorkspaces("org1", []);
    const failNext = { current: false };
    const requests = serveOrderedAttention(failNext);
    renderHome();
    await waitFor(() => expect(attentionRows()[0]).toBe("Item 6"));

    failNext.current = true;
    await user.selectOptions(orderControl(), "oldest");
    expect(
      await screen.findByText("Attention items are temporarily unavailable."),
    ).toBeVisible();
    expect(attentionRows()).toEqual([]);
    expect(screen.queryByText("Nothing needs your attention.")).toBeNull();
    expect(orderControl().value).toBe("oldest");

    await user.click(screen.getByTestId("home-attention-retry"));
    await waitFor(() => expect(attentionRows()[0]).toBe("Item 0"));
    expect(requests.at(-1)).toEqual({
      order: "oldest",
      kind: null,
      cursor: null,
    });
  });
});

// The operator organization's Home is the platform console
// (g5.organization.platform-operator.home); every other organization keeps
// the development tiles.
describe("platform operator Home", () => {
  function operatorUser(role = "member"): CurrentUserDTO {
    const base = authedUser("org-operator");
    return {
      ...base,
      orgs: [
        ...base.orgs,
        {
          orgId: "org-operator",
          orgName: "Platform operator",
          orgSlug: "operator",
          role,
          orgKind: "operator",
        },
      ],
    };
  }

  it("renders the platform console, not the development tiles, for the operator kind", async () => {
    const developmentReads: string[] = [];
    server.use(
      http.get("/api/v1/orgs/org-operator/workspaces", () => {
        developmentReads.push("workspaces");
        return HttpResponse.json({ data: [] });
      }),
      http.get("/api/v1/orgs/org-operator/home/attention", () => {
        developmentReads.push("attention");
        return HttpResponse.json({ data: { items: [], nextCursor: null } });
      }),
    );
    mockUseAuth.mockReturnValue({
      user: operatorUser(),
      switchOrg: vi.fn().mockResolvedValue(undefined),
      logout: vi.fn(),
      hasScope: () => false,
    });

    renderHome("/operator/home");

    expect(
      await screen.findByRole("heading", { name: "Platform operator" }),
    ).toBeVisible();
    expect(await screen.findAllByTestId("platform-org-row")).toHaveLength(2);
    expect(screen.queryByTestId("home-columns")).toBeNull();
    expect(screen.queryByTestId("home-needs-attention")).toBeNull();
    expect(developmentReads).toEqual([]);
  });

  it("keeps the development Home for a tenant organization", async () => {
    serveOrgWorkspaces("org1", []);
    mockUseAuth.mockReturnValue({
      user: { ...operatorUser(), activeOrgId: "org1" },
      switchOrg: vi.fn().mockResolvedValue(undefined),
      logout: vi.fn(),
      hasScope: () => false,
    });

    renderHome("/org-one/home");

    expect(await screen.findByTestId("home-columns")).toBeInTheDocument();
    expect(screen.queryByTestId("platform-home")).toBeNull();
  });
});

// Base's re-verification line (ui g5.ui.home-reverification); its states
// and dialog are covered in components/home/HomeReverification.test.tsx.
describe("HomePage — Base re-verification", () => {
  function serveVerificationSwitch(value: unknown) {
    server.use(
      http.get("/api/v1/orgs/org1/config", () =>
        HttpResponse.json({
          data: {
            scope: "org",
            settings:
              value === undefined
                ? {}
                : { [VERIFICATION_ACTION_ENABLED_SETTING]: value },
          },
        }),
      ),
    );
  }

  it("shows nothing and reads no status unless the switch is exactly on", async () => {
    for (const value of [undefined, false, "true"]) {
      const statusReads: string[] = [];
      serveOrgWorkspaces("org1", []);
      serveVerificationSwitch(value);
      server.use(
        http.get("/api/v1/orgs/org1/home/base-reverification", () => {
          statusReads.push("status");
          return HttpResponse.json({ data: { enabled: false } });
        }),
      );
      const { unmount, queryClient } = renderHome("/org-one/home");
      expect(await screen.findByTestId("home-columns")).toBeInTheDocument();
      await waitFor(() =>
        expect(
          queryClient.getQueryState(["scoped-config", "org", "org1"])?.status,
        ).toBe("success"),
      );
      expect(screen.queryByTestId("home-reverification")).toBeNull();
      expect(statusReads).toEqual([]);
      unmount();
    }
  });

  it("puts the status line above the tiles while the switch is on", async () => {
    serveOrgWorkspaces("org1", []);
    serveVerificationSwitch(true);
    renderHome("/org-one/home");

    const line = await screen.findByTestId("home-reverification-status");
    // The shared fixture's demo status: stale since its completed run.
    expect(line).toHaveTextContent(
      /^Base has not been re-verified for 4 merged PRs \(\+3,410 −1,022 lines\)$/,
    );
    const columns = screen.getByTestId("home-columns");
    expect(
      line.compareDocumentPosition(columns) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(screen.getByTestId("home-reverification-open")).toBeEnabled();
  });
});
