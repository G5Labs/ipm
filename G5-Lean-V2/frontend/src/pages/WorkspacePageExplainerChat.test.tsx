import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../test/render";
import { server } from "../test/msw/server";
import {
  createMockChatMessage,
  createMockChatSession,
  createMockUser,
  mockWorkspaceDetails,
} from "../test/fixtures";
import { WorkspacePage } from "./WorkspacePage";

vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div />,
}));

function PageWithNavigation() {
  const location = useLocation();
  const navigate = useNavigate();
  return (
    <>
      <button onClick={() => navigate("/org-g5labs/auth-refactor")}>
        Switch workspace
      </button>
      <div data-testid="route-probe">
        {location.pathname}
        {location.search}
      </div>
      <WorkspacePage />
    </>
  );
}

describe("Create Explainer chat handoff", () => {
  it.each([false, true])(
    "opens the submitted chat only in its original workspace (navigate away: %s)",
    async (navigateAway) => {
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
      const user = userEvent.setup();
      const workspaceId = "task-graph-rework";
      const existing = createMockChatSession({
        id: "existing-chat",
        workspaceId,
        scopeRef: null,
      });
      const created = createMockChatSession({
        id: "explainer-chat",
        workspaceId,
        scopeRef: null,
        title: "Explainer conversation",
      });
      let content = "";
      let completed = false;
      let release!: () => void;
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      const starts: Record<string, unknown>[] = [];
      let jobCreates = 0;
      const sends: string[] = [];
      server.use(
        // The ribbon's Explainer action is a System Engineering action
        // (explainers are Project Docs; the rail has no Explainers section).
        http.get("/api/v1/workspaces/:id", ({ params }) =>
          HttpResponse.json({
            data: {
              ...mockWorkspaceDetails[String(params.id)],
              phase: "se",
            },
          }),
        ),
        http.get("/api/v1/workspaces/:id/provider-quota", () =>
          HttpResponse.json({ data: null }),
        ),
        http.get("/api/v1/workspaces/:id/sessions", ({ params }) =>
          HttpResponse.json({
            data:
              params.id === workspaceId
                ? completed
                  ? [created, existing]
                  : [existing]
                : [],
          }),
        ),
        http.get("/api/v1/sessions/:id", ({ params }) =>
          HttpResponse.json({
            data: params.id === created.id ? created : existing,
          }),
        ),
        http.get("/api/v1/sessions/:id/task", () =>
          HttpResponse.json({ data: null }),
        ),
        http.get("/api/v1/sessions/:id/messages", ({ params }) =>
          HttpResponse.json({
            data:
              params.id === created.id
                ? [
                    createMockChatMessage({
                      id: "explainer-input",
                      sessionId: created.id,
                      role: "user",
                      content,
                      status: "complete",
                    }),
                  ]
                : [],
          }),
        ),
        http.post("/api/v1/sessions/:id/messages", ({ params }) => {
          sends.push(String(params.id));
          return HttpResponse.json({
            data: { messageId: "follow-up", status: "queued" },
          });
        }),
        http.get("/api/v1/branches/:id/explainer-templates", () =>
          HttpResponse.json({ data: [] }),
        ),
        http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
          HttpResponse.json({
            data: {
              resolved: {
                schemaVersion: 1,
                resolutionId: "alr_explainer",
                configRevision: "cfg-1",
                catalogVersion: "agent-launch-catalog-v1",
                provider: { value: "claude_code", provenance: "configured" },
                providerRoute: "anthropic_direct",
                model: {
                  value: "anthropic/claude-opus-4.8",
                  provenance: "configured",
                },
                upstreamModel: "anthropic/claude-opus-4.8",
                permissionMode: {
                  value: "bypassPermissions",
                  provenance: "configured",
                },
                effort: { value: "high", provenance: "configured" },
                authMode: "anthropic_api_key",
                egressMode: "direct_provider",
                credentialReady: true,
                requiresInteractiveAuth: false,
              },
            },
          }),
        ),
        http.post("/api/v1/workspaces/:id/docs/explainers", () => {
          jobCreates += 1;
          return HttpResponse.json({
            data: { docId: "doc-old" },
          });
        }),
        http.post(
          "/api/v1/workspaces/:id/sessions/start",
          async ({ request, params }) => {
            expect(params.id).toBe(workspaceId);
            const body = (await request.json()) as Record<string, unknown>;
            starts.push(body);
            content = String(body.content);
            await gate;
            completed = true;
            return HttpResponse.json(
              {
                data: {
                  session: created,
                  message: { messageId: "explainer-input", status: "queued" },
                  assistantMessageId: "explainer-input",
                  taskId: "explainer-task",
                },
              },
              { status: 201 },
            );
          },
        ),
      );
      const { queryClient } = renderApp(
        <Routes>
          <Route path="/:org/:workspace/*" element={<PageWithNavigation />} />
        </Routes>,
        {
          route: `/org-g5labs/${workspaceId}?session=${existing.id}`,
          user: createMockUser(),
        },
      );
      const explainer = await screen.findByTestId("ribbon-action-explainer");
      await waitFor(() => expect(explainer).toBeEnabled());
      await user.click(explainer);
      await screen.findByRole("dialog", { name: "Create Explainer" });
      fireEvent.change(screen.getByTestId("create-explainer-topic"), {
        target: { value: "Explain the retry flow" },
      });
      await user.selectOptions(
        screen.getByTestId("create-explainer-template-select"),
        "__custom__",
      );
      fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
        target: { value: "Use a concrete example." },
      });
      fireEvent.change(screen.getByTestId("create-explainer-file-name"), {
        target: { value: "retry-flow" },
      });
      await waitFor(() =>
        expect(screen.getByTestId("create-explainer-submit")).toBeEnabled(),
      );
      await user.click(screen.getByTestId("create-explainer-submit"));
      await waitFor(() => expect(starts).toHaveLength(1));
      if (navigateAway) {
        await user.click(
          screen.getByRole("button", { name: "Switch workspace" }),
        );
        await waitFor(() =>
          expect(screen.getByTestId("route-probe")).toHaveTextContent(
            "/org-g5labs/auth-refactor",
          ),
        );
        await waitFor(() =>
          expect(
            queryClient.getQueryData(["workspace", "auth-refactor"]),
          ).toBeDefined(),
        );
      }
      await act(async () => {
        release();
      });
      await waitFor(() =>
        expect(
          queryClient.getQueryData(["chat-session", created.id]),
        ).toBeDefined(),
      );
      expect(jobCreates).toBe(0);
      expect(sends).toEqual([]);
      expect(starts[0]).toMatchObject({
        scopeType: "workspace",
        agentLaunch: { confirmedResolutionId: "alr_explainer" },
      });
      expect(content).toContain("Explain the retry flow");
      expect(content).toContain("Use a concrete example.");
      expect(content).toContain("retry-flow-<date>.html");
      expect(content).toContain(`docs/${workspaceId}/explainers/`);
      if (navigateAway) {
        expect(screen.getByTestId("route-probe")).not.toHaveTextContent(
          created.id,
        );
        expect(
          screen.queryByRole("dialog", { name: "Create Explainer" }),
        ).toBeNull();
      } else {
        await waitFor(() =>
          expect(screen.getByTestId("route-probe")).toHaveTextContent(
            `session=${created.id}`,
          ),
        );
        await waitFor(() =>
          expect(
            screen.queryByRole("dialog", { name: "Create Explainer" }),
          ).toBeNull(),
        );
        expect(screen.getByTestId("center-chat-panel")).not.toHaveAttribute(
          "hidden",
        );
        expect(screen.queryByTestId("explainer-output-generating")).toBeNull();
        expect(
          await screen.findByText(
            "Create an explainer: Explain the retry flow",
            { exact: false },
          ),
        ).toBeInTheDocument();
        await user.type(screen.getByTestId("chat-input"), "Make it shorter");
        await user.click(screen.getByTestId("chat-panel-send"));
        await waitFor(() => expect(sends).toEqual([created.id]));
        expect(starts).toHaveLength(1);
      }
    },
  );
});
