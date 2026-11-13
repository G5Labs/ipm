// =============================================================================
// PersonalChatSidePanel — Home's personal chat (#624, g5.personal.home)
// =============================================================================
// The side panel mounts the workbench's ChatPanel for the personal workspace:
// an existing conversation opens with its messages, a stopped VM offers Start
// and send with `startCompute: true` (g5.personal.start-and-send), the header
// carries the VM status and admin notice, and the panel collapses to a rail
// that is remembered per browser.
// =============================================================================

import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";

import { MOCK_PERSONAL_WORKSPACE_DETAIL } from "../../data/mock";
import {
  createMockChatMessage,
  createMockChatSession,
  createMockUser,
  createMockWorkspaceVm,
} from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import {
  PERSONAL_CHAT_OPEN_KEY,
  PersonalChatSidePanel,
} from "./PersonalChatSidePanel";
import { PersonalHomeProvider } from "./PersonalHomeContext";

const WORKSPACE_ID = MOCK_PERSONAL_WORKSPACE_DETAIL.id;

const RESOLVED_LAUNCH = {
  schemaVersion: 1,
  resolutionId: "home-side-confirmed",
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

function serve(vmStatus: "stopped" | "running") {
  const starts: Record<string, unknown>[] = [];
  server.use(
    http.get(`/api/v1/workspaces/${WORKSPACE_ID}/vm`, () =>
      HttpResponse.json({
        data: createMockWorkspaceVm({
          orgId: "org-g5labs",
          workspaceId: WORKSPACE_ID,
          ownerUserId: "user-alex",
          status: vmStatus,
          vmId: vmStatus === "running" ? "vm-personal" : null,
        }),
      }),
    ),
    http.get(`/api/v1/workspaces/${WORKSPACE_ID}/sessions`, () =>
      HttpResponse.json({ data: [SESSION] }),
    ),
    http.get(`/api/v1/sessions/${SESSION.id}`, () =>
      HttpResponse.json({ data: SESSION }),
    ),
    http.get(`/api/v1/sessions/${SESSION.id}/messages`, () =>
      HttpResponse.json({
        data: [
          createMockChatMessage({
            id: "m-user",
            sessionId: SESSION.id,
            role: "user",
            content: "what did I read last week?",
            status: "complete",
          }),
          createMockChatMessage({
            id: "m-reply",
            sessionId: SESSION.id,
            role: "assistant",
            content: "You read three papers on CRDTs.",
            status: "complete",
          }),
        ],
      }),
    ),
    http.get(`/api/v1/sessions/${SESSION.id}/task`, () =>
      HttpResponse.json({ data: null }),
    ),
    http.post(
      `/api/v1/workspaces/${WORKSPACE_ID}/config/agent-launch/resolve`,
      () => HttpResponse.json({ data: { resolved: RESOLVED_LAUNCH } }),
    ),
    http.get(`/api/v1/workspaces/${WORKSPACE_ID}/provider-quota`, () =>
      HttpResponse.json({ data: null }),
    ),
    http.post(
      `/api/v1/workspaces/${WORKSPACE_ID}/sessions/start`,
      async ({ request }) => {
        starts.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json(
          {
            data: {
              session: { ...SESSION, id: "personal-new", title: null },
              taskId: "task-1",
              assistantMessageId: "m-1",
              message: {
                messageId: "m-1",
                inputId: "i-1",
                status: "queued",
                startCompute: { intentId: "intent-1", status: "starting" },
              },
            },
          },
          { status: 201 },
        );
      },
    ),
  );
  return starts;
}

function renderSidePanel() {
  return renderApp(
    <PersonalHomeProvider
      orgId="org-g5labs"
      orgSlug="g5labs"
      userId="user-alex"
      orgAdmin={false}
    >
      <PersonalChatSidePanel />
    </PersonalHomeProvider>,
    { route: "/g5labs/home", user: createMockUser() },
  );
}

beforeEach(() => localStorage.clear());

describe("PersonalChatSidePanel (g5.personal.home)", () => {
  it("opens a personal conversation with its replies, as in the workspace", async () => {
    serve("running");
    renderSidePanel();
    const user = userEvent.setup();

    expect(await screen.findByTestId("home-personal-chat")).toBeInTheDocument();
    // The header carries status and the admin notice (g5.personal.access).
    await waitFor(() =>
      expect(
        screen.getByTestId("home-personal-chat-vm-status"),
      ).toHaveTextContent("Running"),
    );
    expect(
      screen.getByTestId("home-personal-chat-admin-notice"),
    ).toHaveTextContent(
      "Organization administrators can see your personal work.",
    );
    const history = await screen.findByTestId(
      /chat-(center|history|active)-(history|header)/,
    );
    await user.click(history);
    await user.click(await screen.findByTestId("chat-session-select"));
    expect(
      await screen.findByText("You read three papers on CRDTs."),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("home-personal-chat-open-workspace"),
    ).toHaveAttribute("href", "/g5labs/me");
  });

  it("keeps an existing Home chat follow-up in the queue through submission", async () => {
    serve("running");
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.get(`/api/v1/sessions/${SESSION.id}/inputs`, () =>
        HttpResponse.json({ data: [] }),
      ),
      http.post(
        `/api/v1/sessions/${SESSION.id}/messages`,
        async () => {
          const inputId = "server-input";
          await pending;
          return HttpResponse.json({
            data: {
              inputId,
              messageId: inputId,
              status: "queued",
              delivery: "queued",
            },
          });
        },
      ),
    );
    renderSidePanel();
    const user = userEvent.setup();
    await user.click(
      await screen.findByTestId(
        /chat-(center|history|active)-(history|header)/,
      ),
    );
    await user.click(await screen.findByTestId("chat-session-select"));
    await screen.findByText("You read three papers on CRDTs.");
    await user.type(screen.getByTestId("chat-input"), "Home follow up{Enter}");
    expect(await screen.findByTestId("chat-queued-inputs")).toHaveTextContent(
      "Home follow up",
    );
    expect(screen.getAllByText("Home follow up")).toHaveLength(1);
    await act(async () => {
      release();
    });
    await screen.findByTestId("chat-queued-input-server-input");
    expect(screen.getByTestId("chat-queued-inputs")).toHaveTextContent(
      "Home follow up",
    );
    expect(screen.getAllByText("Home follow up")).toHaveLength(1);
  });

  it("starts the stopped VM and the conversation together with Start and send", async () => {
    const starts = serve("stopped");
    renderSidePanel();
    const user = userEvent.setup();

    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toBeEnabled());
    await user.type(input, "summarize my notes");
    const send = screen.getByTestId("chat-panel-no-session-send");
    expect(send).toHaveTextContent("Start and send");
    await waitFor(() => expect(send).toBeEnabled());
    await user.click(send);

    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({
      content: "summarize my notes",
      scopeType: "workspace",
      startCompute: true,
    });
  });

  it("collapses to a rail and remembers it", async () => {
    serve("running");
    const { unmount } = renderSidePanel();
    const user = userEvent.setup();

    await user.click(await screen.findByTestId("home-personal-chat-close"));
    expect(screen.queryByTestId("home-personal-chat")).toBeNull();
    expect(localStorage.getItem(PERSONAL_CHAT_OPEN_KEY)).toBe("false");
    unmount();

    renderSidePanel();
    await user.click(await screen.findByTestId("home-personal-chat-open"));
    expect(await screen.findByTestId("home-personal-chat")).toBeInTheDocument();
    expect(localStorage.getItem(PERSONAL_CHAT_OPEN_KEY)).toBe("true");
  });
});
