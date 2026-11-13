// =============================================================================
// PersonalPanel — Home's personal workspace panel (g5.personal.home)
// =============================================================================
// With the VM stopped the panel still lists the caller's personal
// conversations, and its composer offers
// Start and send (g5.personal.start-and-send) through the same live gate the
// workbench builds. A Start in progress shows inline and closes the composer
// with the workbench's reason; the panel handles the shared Start event for
// its own workspace.
// =============================================================================

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { act } from "react";
import { beforeEach, describe, expect, it } from "vitest";

import { MOCK_PERSONAL_WORKSPACE_DETAIL } from "../../data/mock";
import {
  createMockChatSession,
  createMockUser,
  createMockWorkspaceVm,
} from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { requestWorkspaceStart } from "../workspace/reviewWorkspace";
import { PersonalPanel } from "./PersonalPanel";

const WORKSPACE_ID = MOCK_PERSONAL_WORKSPACE_DETAIL.id;
const REPO_ID = MOCK_PERSONAL_WORKSPACE_DETAIL.branches[0].repoId;

const RESOLVED_LAUNCH = {
  schemaVersion: 1,
  resolutionId: "home-confirmed",
  configRevision: "test",
  catalogVersion: "test",
  provider: { value: "claude_code", provenance: "configured" },
  providerRoute: "anthropic_direct",
  model: { value: "configured-model", provenance: "configured" },
  upstreamModel: "configured-model",
  permissionMode: { value: "auto", provenance: "configured" },
  effort: { value: "high", provenance: "configured" },
  authMode: "anthropic_api_key",
  egressMode: "direct_provider",
  credentialReady: true,
  requiresInteractiveAuth: false,
};

const SESSION = createMockChatSession({
  id: "personal-notes",
  workspaceId: WORKSPACE_ID,
  scopeType: "workspace",
  scopeRef: WORKSPACE_ID,
  title: "Reading notes",
});

interface Recorded {
  starts: Record<string, unknown>[];
  sends: Record<string, unknown>[];
  resumes: number;
}

function servePersonal(
  vmStatus: "stopped" | "starting" | "queued_for_capacity" | "running",
): Recorded {
  const recorded: Recorded = { starts: [], sends: [], resumes: 0 };
  server.use(
    http.get(`/api/v1/workspaces/${WORKSPACE_ID}/vm`, () =>
      HttpResponse.json({
        data: createMockWorkspaceVm({
          orgId: "org-g5labs",
          workspaceId: WORKSPACE_ID,
          ownerUserId: "user-alex",
          status: vmStatus,
        }),
      }),
    ),
    http.get(`/api/v1/workspaces/${WORKSPACE_ID}/sessions`, () =>
      HttpResponse.json({ data: [SESSION] }),
    ),
    http.get(`/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/files`, () =>
      HttpResponse.json({
        data: {
          entries: [
            {
              name: "notes",
              path: "notes",
              type: "directory",
              size: null,
              children: [
                {
                  name: "ideas.md",
                  path: "notes/ideas.md",
                  type: "file",
                  size: 12,
                },
              ],
            },
            { name: "README.md", path: "README.md", type: "file", size: 5 },
          ],
          truncated: false,
          source: "committed",
          repoOid: "f00d",
        },
      }),
    ),
    http.post(
      `/api/v1/workspaces/${WORKSPACE_ID}/config/agent-launch/resolve`,
      () => HttpResponse.json({ data: { resolved: RESOLVED_LAUNCH } }),
    ),
    http.post(
      `/api/v1/workspaces/${WORKSPACE_ID}/sessions/start`,
      async ({ request }) => {
        recorded.starts.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json(
          {
            data: {
              session: { ...SESSION, id: "personal-new", title: null },
              taskId: "task-1",
              assistantMessageId: "m-1",
              message: { messageId: "m-1", inputId: "i-1", status: "queued" },
              startCompute: { intentId: "intent-1", status: "starting" },
            },
          },
          { status: 201 },
        );
      },
    ),
    http.post(
      `/api/v1/sessions/${SESSION.id}/messages`,
      async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        recorded.sends.push(body);
        // Every send returns its input; only Start and send records a Start.
        return HttpResponse.json({
          data: {
            messageId: "m-2",
            status: "queued",
            inputId: "i-2",
            ...(body.startCompute
              ? { startCompute: { intentId: "intent-2", status: "starting" } }
              : {}),
          },
        });
      },
    ),
    http.post(`/api/v1/workspaces/${WORKSPACE_ID}/resume`, () => {
      recorded.resumes += 1;
      return HttpResponse.json({
        data: {
          outcome: "starting",
          vm: createMockWorkspaceVm({ status: "starting" }),
        },
      });
    }),
  );
  return recorded;
}

function renderPanel() {
  return renderApp(
    <PersonalPanel
      orgId="org-g5labs"
      orgSlug="g5labs"
      userId="user-alex"
      orgAdmin={false}
    />,
    { route: "/g5labs/home", user: createMockUser() },
  );
}

beforeEach(() => localStorage.clear());

describe("PersonalPanel (g5.personal.home)", () => {
  it("shows conversations and the admin notice while the VM is stopped, leaving files to the workbench", async () => {
    servePersonal("stopped");
    renderPanel();

    const panel = await screen.findByTestId("home-personal-panel");
    expect(await within(panel).findByText("Reading notes")).toBeInTheDocument();
    expect(
      within(panel).getByTestId("home-personal-admin-notice"),
    ).toHaveTextContent(
      "Organization administrators can see your personal work.",
    );
    expect(within(panel).getByTestId("home-personal-open")).toHaveAttribute(
      "href",
      "/g5labs/me",
    );
    expect(
      await within(panel).findByTestId("home-personal-vm-status"),
    ).toHaveTextContent("Stopped");
    // Files stay in the workbench (g5.personal.home).
    expect(within(panel).queryByText("My files")).toBeNull();
    expect(within(panel).queryByText("README.md")).toBeNull();
  });

  it("is for narrow screens only beside Home's Chat side panel", async () => {
    servePersonal("stopped");
    renderApp(
      <PersonalPanel
        orgId="org-g5labs"
        orgSlug="g5labs"
        userId="user-alex"
        orgAdmin={false}
        chatInSidePanel
      />,
      { route: "/g5labs/home", user: createMockUser() },
    );
    expect(await screen.findByTestId("home-personal-panel")).toHaveClass(
      "lg:hidden",
    );
  });

  it("offers Start and send while stopped and starts a new conversation with startCompute", async () => {
    const recorded = servePersonal("stopped");
    renderPanel();
    const user = userEvent.setup();

    const composer = await screen.findByTestId("home-personal-composer");
    await waitFor(() => expect(composer).toBeEnabled());
    expect(screen.getByTestId("start-and-send-note")).toHaveTextContent(
      "Your personal workspace is stopped.",
    );
    await user.type(composer, "draft my weekly update");
    const button = screen.getByTestId("home-personal-start-and-send");
    expect(button).toHaveTextContent("Start and send");
    await user.click(button);

    await waitFor(() => expect(recorded.starts).toHaveLength(1));
    expect(recorded.starts[0]).toMatchObject({
      content: "draft my weekly update",
      scopeType: "workspace",
      scopeRef: WORKSPACE_ID,
      startCompute: true,
      agentLaunch: { confirmedResolutionId: "home-confirmed" },
    });
    expect(await screen.findByTestId("home-personal-sent")).toHaveTextContent(
      "Queued; it is sent once your personal workspace is running.",
    );
  });

  it("continues a selected conversation with startCompute on its messages route", async () => {
    const recorded = servePersonal("stopped");
    renderPanel();
    const user = userEvent.setup();

    await user.click(await screen.findByText("Reading notes"));
    const composer = screen.getByTestId("home-personal-composer");
    await waitFor(() => expect(composer).toBeEnabled());
    await user.type(composer, "add today's page{Enter}");

    await waitFor(() => expect(recorded.sends).toHaveLength(1));
    expect(recorded.sends[0]).toMatchObject({
      content: "add today's page",
      startCompute: true,
    });
    expect(recorded.starts).toHaveLength(0);
  });

  it("sends without startCompute while the VM runs", async () => {
    const recorded = servePersonal("running");
    renderPanel();
    const user = userEvent.setup();

    await user.click(await screen.findByText("Reading notes"));
    const composer = screen.getByTestId("home-personal-composer");
    await waitFor(() => expect(composer).toBeEnabled());
    expect(screen.queryByTestId("home-personal-start-and-send")).toBeNull();
    await user.type(composer, "hello");
    await user.click(screen.getByTestId("home-personal-send"));

    await waitFor(() => expect(recorded.sends).toHaveLength(1));
    expect(recorded.sends[0]).not.toHaveProperty("startCompute");
    expect(await screen.findByTestId("home-personal-sent")).toHaveTextContent(
      /^Sent\./,
    );
  });

  it("shows Start progress inline and closes the composer with the gate's reason", async () => {
    servePersonal("starting");
    renderPanel();

    const progress = await screen.findByTestId("start-progress");
    expect(progress).toHaveTextContent("Starting your personal workspace…");
    const composer = screen.getByTestId("home-personal-composer");
    expect(composer).toBeDisabled();
    expect(composer).toHaveAttribute(
      "placeholder",
      "Your personal workspace is starting; you can send messages once it is running.",
    );
  });

  it("handles the shared Start event for its own workspace", async () => {
    const recorded = servePersonal("stopped");
    renderPanel();
    await screen.findByTestId("home-personal-start");

    act(() => requestWorkspaceStart());
    await waitFor(() => expect(recorded.resumes).toBe(1));
  });

  it("tells a viewer refused by /me that viewers have none", async () => {
    server.use(
      http.get("/api/v1/orgs/:orgId/me", () =>
        HttpResponse.json(
          { error: { code: "FORBIDDEN", message: "no" } },
          { status: 403 },
        ),
      ),
    );
    renderPanel();
    expect(
      await screen.findByText(
        "Viewers have no personal workspace in this organization.",
      ),
    ).toBeInTheDocument();
  });
});
