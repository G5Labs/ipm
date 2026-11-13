import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderApp } from "../test/render";
import { server } from "../test/msw/server";
import {
  createMockChatSession,
  createMockUser,
  MOCK_INSTANCE_INFO,
} from "../test/fixtures";
import type { ChatSessionDTO, ProviderAccountDTO } from "../lib/types";
import { WorkspacePage } from "./WorkspacePage";

vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div />,
}));

// A later turn runs on the chat's account (routes/chat D33). Before the fix
// the page sent no account, so the server fell back to the sender's default
// assignment and, with none and no raw key, refused every follow-up with
// `missing_llm_credential` (QA session 31fc8e20, 2026-09-28).

const WORKSPACE_ID = "task-graph-rework";

const ACCOUNT: ProviderAccountDTO = {
  id: "acct-claude-1st",
  orgId: "org-g5labs",
  name: "claude-1st",
  kind: "claude_subscription",
  family: "claude",
  scope: "user",
  workspaceId: null,
  userId: "user-1",
  baseUrl: null,
  hasSecret: true,
  secretHint: "sk-ant-oat01",
  createdBy: "user-1",
  createdAt: "2026-09-28T00:00:00Z",
  updatedAt: "2026-09-28T00:00:00Z",
  usage: null,
};

/** Serve a workspace whose only chat is `session` once it exists. */
function serveChat(options: {
  session: ChatSessionDTO;
  existing: boolean;
  sent: Record<string, unknown>[];
  starts?: Record<string, unknown>[];
  selfHosted?: boolean;
}) {
  let exists = options.existing;
  server.use(
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: {
          ...MOCK_INSTANCE_INFO,
          selfHosted: options.selfHosted ?? false,
          features: [...MOCK_INSTANCE_INFO.features, "provider_accounts"],
        },
      }),
    ),
    http.get("/api/v1/orgs/:orgId/provider-accounts", () =>
      HttpResponse.json({ data: [ACCOUNT] }),
    ),
    http.get("/api/v1/workspaces/:id/provider-quota", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: exists ? [options.session] : [] }),
    ),
    http.get("/api/v1/sessions/:id", () =>
      HttpResponse.json({ data: options.session }),
    ),
    http.get("/api/v1/sessions/:id/task", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/sessions/:id/messages", () =>
      HttpResponse.json({ data: [] }),
    ),
    // The composer previews a new-chat launch before a session is active.
    http.post(
      "/api/v1/workspaces/:id/config/agent-launch/resolve",
      async ({ request }) => {
        const body = (await request.json()) as {
          preferences?: { account?: { value?: { accountId?: string } } };
        };
        const accountId = body.preferences?.account?.value?.accountId;
        return HttpResponse.json({
          data: {
            resolved: {
              schemaVersion: 1,
              resolutionId: `alr_${accountId ?? "runtime"}`,
              configRevision: "cfg-1",
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
              authMode: accountId ? "anthropic_oauth" : "anthropic_api_key",
              egressMode: "direct_provider",
              credentialReady: true,
              requiresInteractiveAuth: false,
              ...(accountId
                ? {
                    accountId,
                    accountKind: "claude_subscription",
                    accountScope: "user",
                  }
                : {}),
            },
          },
        });
      },
    ),
    http.post("/api/v1/workspaces/:id/sessions/start", async ({ request }) => {
      options.starts?.push((await request.json()) as Record<string, unknown>);
      exists = true;
      // The start response carries the account the start saved.
      return HttpResponse.json(
        {
          data: {
            session: options.session,
            message: {
              messageId: "first-input",
              inputId: "first-input",
              taskId: "first-task",
              delivery: "queued",
              status: "queued",
            },
            assistantMessageId: "first-input",
            taskId: "first-task",
          },
        },
        { status: 201 },
      );
    }),
    http.post("/api/v1/sessions/:id/messages", async ({ request }) => {
      options.sent.push((await request.json()) as Record<string, unknown>);
      return HttpResponse.json(
        { data: { messageId: "follow-up", status: "queued" } },
        { status: 201 },
      );
    }),
  );
}

describe("Workspace chat follow-up account", () => {
  beforeEach(() => {
    vi.mocked(window.matchMedia).mockImplementation((query) => ({
      matches: query.includes("min-width: 1024px"),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
  });

  it.each([false, true])(
    "subscribes the active chat to live events with selfHosted=%s",
    async (selfHosted) => {
      const session = createMockChatSession({
        id: "chat-live-subscription",
        workspaceId: WORKSPACE_ID,
        scopeRef: null,
        authMode: "anthropic_oauth",
        account: { id: ACCOUNT.id, kind: "claude_subscription", scope: "user" },
      });
      serveChat({ session, existing: true, sent: [], selfHosted });
      const { ws } = renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: `/org-g5labs/${WORKSPACE_ID}?session=${session.id}`,
          user: createMockUser(),
        },
      );

      await screen.findByTestId("chat-panel-send");
      await waitFor(() =>
        expect(ws.getSubscriptions()).toContain(`chat:${session.id}`),
      );
    },
  );

  it.each([false, true])(
    "does not subscribe to another workspace's session hint with selfHosted=%s",
    async (selfHosted) => {
      const session = createMockChatSession({
        id: "chat-in-another-workspace",
        workspaceId: "another-workspace",
        scopeRef: null,
      });
      // An authorized session read can succeed even though this workspace's
      // list does not contain it. A URL hint must not join its live channel.
      serveChat({ session, existing: false, sent: [], selfHosted });
      const { ws, queryClient } = renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
        </Routes>,
        {
          route: `/org-g5labs/${WORKSPACE_ID}?session=${session.id}`,
          user: createMockUser(),
        },
      );

      await screen.findByTestId("chat-panel-send");
      expect(
        queryClient.getQueryData(["chat-session", session.id]),
      ).toMatchObject({ workspaceId: "another-workspace" });
      expect(ws.getSubscriptions()).not.toContain(`chat:${session.id}`);
    },
  );

  it("leaves a later turn's account to the chat until the user picks one", async () => {
    const user = userEvent.setup();
    // Started on the account before the chat saved it: only its first
    // turn's frozen account names it, and nothing is assigned. The server
    // applies that account to a turn that names none.
    const session = createMockChatSession({
      id: "chat-on-account",
      workspaceId: WORKSPACE_ID,
      scopeRef: null,
      account: { id: ACCOUNT.id, kind: "claude_subscription", scope: "user" },
      nextAccount: null,
    });
    const sent: Record<string, unknown>[] = [];
    serveChat({ session, existing: true, sent });

    renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      {
        route: `/org-g5labs/${WORKSPACE_ID}?session=${session.id}`,
        user: createMockUser(),
      },
    );

    // The pill names the account the next turn will use.
    await waitFor(() =>
      expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
        "claude-1st",
      ),
    );
    await user.type(screen.getByTestId("chat-input"), "Keep going");
    await user.click(screen.getByTestId("chat-panel-send"));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].account).toBeUndefined();
  });

  /**
   * Open a chat that saved a named account while its detail request is
   * held. Until the detail arrives the page shows the session list entry,
   * which carries no account fields, so the Account field falls back to
   * Runtime variables meanwhile.
   */
  async function openChatWhileDetailLoads() {
    const session = createMockChatSession({
      id: "chat-detail-loading",
      workspaceId: WORKSPACE_ID,
      scopeRef: null,
      account: null,
      nextAccount: { kind: "account", accountId: ACCOUNT.id },
    });
    const listEntry: Record<string, unknown> = { ...session };
    delete listEntry.account;
    delete listEntry.nextAccount;
    const sent: Record<string, unknown>[] = [];
    serveChat({ session, existing: true, sent });
    let releaseDetail: () => void = () => {};
    const detailReleased = new Promise<void>((resolve) => {
      releaseDetail = resolve;
    });
    server.use(
      http.get("/api/v1/workspaces/:id/sessions", () =>
        HttpResponse.json({ data: [listEntry] }),
      ),
      http.get("/api/v1/sessions/:id", async () => {
        await detailReleased;
        return HttpResponse.json({ data: session });
      }),
    );

    const { ws } = renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      {
        route: `/org-g5labs/${WORKSPACE_ID}?session=${session.id}`,
        user: createMockUser(),
      },
    );
    // The composer is on the chat (its list entry) once the chat's send
    // button replaces the new-chat one.
    await screen.findByTestId("chat-panel-send");
    return { sent, releaseDetail, ws, session };
  }

  it("waits for the session's workspace before joining its live channel", async () => {
    const { releaseDetail, ws, session } = await openChatWhileDetailLoads();
    try {
      expect(ws.getSubscriptions()).not.toContain(`chat:${session.id}`);
    } finally {
      releaseDetail();
    }
    await waitFor(() =>
      expect(ws.getSubscriptions()).toContain(`chat:${session.id}`),
    );
  });

  it("sends no account while an opened chat's detail is still loading", async () => {
    const user = userEvent.setup();
    const { sent, releaseDetail } = await openChatWhileDetailLoads();

    await user.type(screen.getByTestId("chat-input"), "Keep going");
    await waitFor(() =>
      expect(screen.getByTestId("chat-panel-send")).toBeEnabled(),
    );
    await user.click(screen.getByTestId("chat-panel-send"));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].account).toBeUndefined();
    releaseDetail();
  });

  it("sends the shown Runtime variables once the user picks them while the detail loads", async () => {
    const user = userEvent.setup();
    const { sent, releaseDetail } = await openChatWhileDetailLoads();

    await user.click(screen.getByTestId("claude-settings-control"));
    await user.click(await screen.findByTestId("chat-account-select"));
    await user.click(
      await screen.findByRole("option", { name: "Runtime variables" }),
    );
    await user.keyboard("{Escape}");
    await user.type(screen.getByTestId("chat-input"), "Use the variables");
    await waitFor(() =>
      expect(screen.getByTestId("chat-panel-send")).toBeEnabled(),
    );
    await user.click(screen.getByTestId("chat-panel-send"));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].account).toEqual({ kind: "runtime_config" });
    releaseDetail();
  });

  it("leaves the turn sent right after a start to the chat's saved account", async () => {
    const user = userEvent.setup();
    const session = createMockChatSession({
      id: "chat-started-on-account",
      workspaceId: WORKSPACE_ID,
      scopeRef: null,
      account: null,
      nextAccount: { kind: "account", accountId: ACCOUNT.id },
    });
    const sent: Record<string, unknown>[] = [];
    const starts: Record<string, unknown>[] = [];
    serveChat({ session, existing: false, sent, starts });

    const { queryClient } = renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      { route: `/org-g5labs/${WORKSPACE_ID}`, user: createMockUser() },
    );
    // Production freshness: the started session is not refetched before
    // the next turn is sent.
    queryClient.setDefaultOptions({
      queries: { retry: false, staleTime: 30_000 },
      mutations: { retry: false },
    });

    await user.click(await screen.findByTestId("claude-settings-control"));
    await user.click(await screen.findByTestId("chat-account-select"));
    await user.click(await screen.findByRole("option", { name: "claude-1st" }));
    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
        "claude-1st",
      ),
    );
    await user.type(screen.getByTestId("chat-input"), "First turn");
    await waitFor(() =>
      expect(screen.getByTestId("chat-panel-no-session-send")).toBeEnabled(),
    );
    await user.click(screen.getByTestId("chat-panel-no-session-send"));
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({
      agentLaunch: {
        preferences: {
          account: {
            source: "explicit",
            value: { kind: "account", accountId: ACCOUNT.id },
          },
        },
      },
    });
    await waitFor(() =>
      expect(screen.getByTestId("chat-input")).toHaveValue(""),
    );

    await user.type(screen.getByTestId("chat-input"), "Keep going");
    await user.click(screen.getByTestId("chat-panel-send"));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].account).toBeUndefined();
  });

  it("keeps an Account picked while a later turn is sending", async () => {
    const user = userEvent.setup();
    let session = createMockChatSession({
      id: "chat-on-runtime-variables",
      workspaceId: WORKSPACE_ID,
      scopeRef: null,
      account: null,
      nextAccount: { kind: "runtime_config" },
    });
    const sent: Record<string, unknown>[] = [];
    serveChat({ session, existing: true, sent });
    let releaseFirstTurn: () => void = () => {};
    const firstTurnReleased = new Promise<void>((resolve) => {
      releaseFirstTurn = resolve;
    });
    let savedAccountServed = false;
    server.use(
      http.get("/api/v1/workspaces/:id/sessions", () =>
        HttpResponse.json({ data: [session] }),
      ),
      http.get("/api/v1/sessions/:id", () => {
        savedAccountServed ||= session.nextAccount?.kind === "account";
        return HttpResponse.json({ data: session });
      }),
      http.post("/api/v1/sessions/:id/messages", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        sent.push(body);
        if (sent.length === 1) await firstTurnReleased;
        // The server saves the account a turn names (routes/chat D33).
        session = {
          ...session,
          nextAccount: body.account as ChatSessionDTO["nextAccount"],
        };
        return HttpResponse.json(
          { data: { messageId: `follow-up-${sent.length}`, status: "queued" } },
          { status: 201 },
        );
      }),
    );

    renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      {
        route: `/org-g5labs/${WORKSPACE_ID}?session=${session.id}`,
        user: createMockUser(),
      },
    );

    const pickAccount = async (name: string) => {
      await user.click(await screen.findByTestId("claude-settings-control"));
      await user.click(await screen.findByTestId("chat-account-select"));
      await user.click(await screen.findByRole("option", { name }));
      await user.keyboard("{Escape}");
    };
    // The model control also exists in the provisional new-chat composer.
    // Wait for the existing chat before clicking controls that loading replaces.
    await screen.findByTestId("chat-panel-send");
    await pickAccount("claude-1st");
    await waitFor(() =>
      expect(screen.getByTestId("claude-harness-control")).toHaveTextContent(
        "claude-1st",
      ),
    );
    await user.type(screen.getByTestId("chat-input"), "Use claude-1st");
    await user.click(screen.getByTestId("chat-panel-send"));
    await waitFor(() => expect(sent).toHaveLength(1));

    // While that turn is sending, the user goes back to Runtime variables.
    await pickAccount("Runtime variables");
    await waitFor(() =>
      expect(
        screen.getByTestId("claude-harness-control"),
      ).not.toHaveTextContent("claude-1st"),
    );

    // The turn is accepted, and the session refetch that follows carries
    // the account it saved.
    releaseFirstTurn();
    await waitFor(() => expect(savedAccountServed).toBe(true));
    await user.type(screen.getByTestId("chat-input"), "Use the variables");
    await user.click(screen.getByTestId("chat-panel-send"));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1].account).toEqual({ kind: "runtime_config" });
  });
});
