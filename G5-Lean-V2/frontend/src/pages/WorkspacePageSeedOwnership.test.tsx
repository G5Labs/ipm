// =============================================================================
// WorkspacePage — a composer seed belongs to the workspace visit that made it
// (ui g5.workbench.chat-actions, issue #1190)
// =============================================================================
// Every chat action, Explain, Discuss and the review prompts seed the chat
// composer through one page-level request. Switching to another workspace
// mounts that workspace's own composer: it must see no seed, whatever the
// order of mount and cleanup effects, and a sent or abandoned seed must never
// return. The per-workspace draft keeps the person's edits across Dock and
// Pop out (g5.workspace.ui.chat-corner-button).
// =============================================================================

import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { GETTING_STARTED_AUTO_OPENED_STORAGE_KEY } from "../lib/userGuide";
import { findWorkbenchAction } from "../lib/journeySteps";
import { WORKBENCH_ACTIONS } from "../lib/workbenchDefaults";
import { workbenchActionMessage } from "../lib/workbenchMessage";
import { reviewWorkspaceChatPrompt } from "../lib/reviewPrompts";
import { createMockUser, mockWorkspaceDetails } from "../test/fixtures";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { seedUserSettings } from "../test/userSettings";
import { WorkspacePage } from "./WorkspacePage";

vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div data-testid="stub-workspace-terminal" />,
}));

/** Two cached, writable workspaces of the same owner. */
const A = "task-graph-rework";
const B = "auth-refactor";

type Phase = "be" | "se" | "ie";

const REVIEW_PHASE = (["be", "se", "ie"] as const).find((phase) =>
  findWorkbenchAction(WORKBENCH_ACTIONS[phase], "code-review"),
)!;

const GET_APPROVALS_PROMPT = workbenchActionMessage(
  findWorkbenchAction(WORKBENCH_ACTIONS.ie, "get-approvals")!,
  "ie",
  { kind: "phase" },
  "",
);

const RESOLVED_LAUNCH = {
  schemaVersion: 1,
  resolutionId: "seed-confirmed",
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

function mockSession(id: string, workspaceId: string, persona: Phase | null) {
  return {
    id,
    workspaceId,
    scopeType: "workspace",
    scopeRef: workspaceId,
    model: "configured-model",
    agentProvider: "claude_code",
    title: `Session ${id}`,
    summary: null,
    tags: [],
    messageCount: 2,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalCostUsd: 0,
    forkedFromId: null,
    linkedCommits: [],
    createdAt: "2026-11-09T00:00:00Z",
    updatedAt: "2026-11-09T00:00:00Z",
    canSend: true,
    persona,
  };
}

/**
 * Both workspaces, writable and owned, in `phase`, with no chats so each
 * lands on its fresh composer; recorders for every chat start and message
 * send. `hold` delays the start's answer, `fail` turns it into a 500.
 */
function installWorkspaces(options: { phase?: Phase } = {}) {
  const phase = options.phase ?? "ie";
  const starts: Array<Record<string, unknown> & { workspaceId: string }> = [];
  const sends: string[] = [];
  const sessions = new Map<string, ReturnType<typeof mockSession>>();
  const control: { hold: Promise<void> | null; fail: boolean } = {
    hold: null,
    fail: false,
  };
  let settled = 0;
  server.use(
    http.get("/api/v1/workspaces/:id", ({ params }) =>
      HttpResponse.json({
        data: {
          ...mockWorkspaceDetails[String(params.id)],
          isBase: false,
          ownerUserId: "user-alex",
          readOnly: false,
          phase,
        },
      }),
    ),
    http.get("/api/v1/workspaces/:id/provider-quota", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.get("/api/v1/workspaces/:id/pull-requests/targets", () =>
      HttpResponse.json({ data: [{ id: "parent-1", name: "parent" }] }),
    ),
    http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
      HttpResponse.json({ data: { resolved: RESOLVED_LAUNCH } }),
    ),
    http.get("/api/v1/workspaces/:id/repos/:repoId/preview-applications", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.post(
      "/api/v1/workspaces/:id/sessions/start",
      async ({ params, request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        starts.push({ ...body, workspaceId: String(params.id) });
        if (control.hold) await control.hold;
        settled += 1;
        if (control.fail) {
          return HttpResponse.json(
            {
              error: {
                code: "INTERNAL",
                message: "Could not start the chat",
                details: {},
              },
            },
            { status: 500 },
          );
        }
        const session = mockSession(
          `session-${starts.length}`,
          String(params.id),
          (body.persona as Phase | undefined) ?? null,
        );
        sessions.set(session.id, session);
        return HttpResponse.json(
          {
            data: {
              session,
              message: {
                messageId: `in-${starts.length}`,
                status: "queued",
                createdAt: "2026-11-09T00:00:00Z",
              },
              assistantMessageId: `in-${starts.length}`,
              taskId: `task-${starts.length}`,
            },
          },
          { status: 201 },
        );
      },
    ),
    http.post("/api/v1/sessions/:sessionId/messages", async ({ request }) => {
      const body = (await request.json()) as { content?: string };
      sends.push(String(body.content ?? ""));
      return HttpResponse.json(
        {
          error: { code: "FORBIDDEN", message: "unexpected send", details: {} },
        },
        { status: 403 },
      );
    }),
    http.get("/api/v1/sessions/:sessionId", ({ params }) =>
      HttpResponse.json({
        data:
          sessions.get(String(params.sessionId)) ??
          mockSession(String(params.sessionId), A, null),
      }),
    ),
    http.get("/api/v1/sessions/:sessionId/task", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/sessions/:sessionId/messages", () =>
      HttpResponse.json({ data: [] }),
    ),
  );
  return { starts, sends, control, settledCount: () => settled };
}

function SwitchablePage() {
  const navigate = useNavigate();
  const location = useLocation();
  return (
    <>
      <button onClick={() => navigate(`/org-g5labs/${A}`)}>Go to A</button>
      <button onClick={() => navigate(`/org-g5labs/${B}`)}>Go to B</button>
      <WorkspacePage />
      <div data-testid="route-probe">
        {location.pathname}
        {location.search}
      </div>
    </>
  );
}

function renderAt(workspaceId: string) {
  return renderApp(
    <Routes>
      <Route path="/:org/:workspace/*" element={<SwitchablePage />} />
    </Routes>,
    { route: `/org-g5labs/${workspaceId}`, user: createMockUser() },
  );
}

async function goTo(workspaceId: string) {
  await userEvent.click(
    screen.getByRole("button", {
      name: workspaceId === A ? "Go to A" : "Go to B",
    }),
  );
  await waitFor(() =>
    expect(screen.getByTestId("route-probe")).toHaveTextContent(
      `/org-g5labs/${workspaceId}`,
    ),
  );
  return screen.findByTestId("chat-input");
}

/** Click a ribbon chat action and wait for its prompt in the composer. */
async function seedFromRibbon(actionId: string, prompt: string) {
  const action = await screen.findByTestId(`ribbon-action-${actionId}`);
  await waitFor(() => expect(action).toBeEnabled());
  await userEvent.click(action);
  const input = await screen.findByTestId("chat-input");
  await waitFor(() => expect(input).toHaveValue(prompt));
  return input;
}

function freshSend() {
  return screen.getByTestId("chat-panel-no-session-send");
}

/** Let pending effects and queued renders settle before asserting absence. */
async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 50));
  });
}

function expectOrdinaryComposer(value: string) {
  expect(screen.getByTestId("chat-input")).toHaveValue(value);
  expect(freshSend()).not.toHaveAttribute("data-pulse");
  expect(screen.queryByTestId("composer-seed-review-loop")).toBeNull();
  expect(screen.queryByTestId("composer-seed-verification-notice")).toBeNull();
}

function desktop(matches: boolean) {
  vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
    matches: matches && query.includes("min-width: 1024px"),
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
}

beforeEach(() => {
  localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
  seedUserSettings({ showTabBar: true, sidebarDisplayMode: "expanded" });
  desktop(true);
});

describe("composer seeds stay with the workspace that made them (g5.workbench.chat-actions)", () => {
  it.each([
    ["an empty", ""],
    ["its own", "B's half-written thought"],
  ])(
    "a Get Approvals prompt sent in A never reaches B's composer, which keeps %s draft, and A → B → A revives nothing",
    async (_label, priorDraft) => {
      const { starts, sends } = installWorkspaces();
      renderAt(B);

      // B is visited first, so its route and data are cached.
      const bInput = await screen.findByTestId("chat-input");
      await waitFor(() => expect(bInput).toBeEnabled());
      if (priorDraft) {
        fireEvent.change(bInput, { target: { value: priorDraft } });
      }

      await goTo(A);
      await seedFromRibbon("get-approvals", GET_APPROVALS_PROMPT);
      expect(freshSend()).toHaveAttribute("data-pulse", "true");
      await waitFor(() => expect(freshSend()).toBeEnabled());
      await userEvent.click(freshSend());
      await waitFor(() => expect(starts).toHaveLength(1));
      expect(starts[0]).toMatchObject({
        workspaceId: A,
        persona: "ie",
        content: GET_APPROVALS_PROMPT,
      });
      await waitFor(() =>
        expect(screen.getByTestId("chat-input")).toHaveValue(""),
      );

      // Back in B without any chat action: B's composer is B's.
      await goTo(B);
      await settle();
      expectOrdinaryComposer(priorDraft);

      // And A's sent seed stays sent: A → B → A shows no prompt again.
      await goTo(A);
      await settle();
      expectOrdinaryComposer("");
      expect(starts).toHaveLength(1);
      expect(sends).toEqual([]);
    },
  );

  it.each([
    ["Get Approvals", "ie" as Phase, "get-approvals", GET_APPROVALS_PROMPT],
    ["Code review", REVIEW_PHASE, "code-review", reviewWorkspaceChatPrompt(1)],
  ])(
    "an unsent %s seed never follows a switch to B; A keeps its draft, and its later send is an ordinary one",
    async (_label, phase, actionId, prompt) => {
      const { starts } = installWorkspaces({ phase });
      renderAt(A);
      await seedFromRibbon(actionId, prompt);
      expect(freshSend()).toHaveAttribute("data-pulse", "true");

      // B has never been visited: a fresh composer, no seed, no header.
      await goTo(B);
      await settle();
      expectOrdinaryComposer("");
      expect(starts).toHaveLength(0);

      // A keeps the person's draft (g5.workspace.ui.chat-corner-button).
      const aInput = await goTo(A);
      await waitFor(() => expect(aInput).toHaveValue(prompt));

      // B again, after A was revisited with its draft: still nothing.
      await goTo(B);
      await settle();
      expectOrdinaryComposer("");

      // Leaving dropped the seed: the draft's send starts an ordinary chat,
      // under no action persona.
      await goTo(A);
      await waitFor(() => expect(freshSend()).toBeEnabled());
      await userEvent.click(freshSend());
      await waitFor(() => expect(starts).toHaveLength(1));
      expect(starts[0]).toMatchObject({ workspaceId: A, content: prompt });
      expect(starts[0]).not.toHaveProperty("persona");
    },
  );

  it("emptying the seeded draft abandons the seed: what is typed next starts an ordinary chat", async () => {
    const { starts } = installWorkspaces();
    renderAt(A);
    const input = await seedFromRibbon("get-approvals", GET_APPROVALS_PROMPT);

    fireEvent.change(input, { target: { value: "" } });
    expect(freshSend()).not.toHaveAttribute("data-pulse");
    fireEvent.change(input, { target: { value: "my own words" } });
    await waitFor(() => expect(freshSend()).toBeEnabled());
    await userEvent.click(freshSend());

    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({
      workspaceId: A,
      content: "my own words",
    });
    expect(starts[0]).not.toHaveProperty("persona");
  });

  it.each([false, true])(
    "a seeded send that settles after a switch to B (fails: %s) touches neither composer and starts nothing else",
    async (fails) => {
      const { starts, control, settledCount } = installWorkspaces();
      let release!: () => void;
      control.hold = new Promise<void>((resolve) => {
        release = resolve;
      });
      control.fail = fails;
      renderAt(A);
      await seedFromRibbon("get-approvals", GET_APPROVALS_PROMPT);
      await waitFor(() => expect(freshSend()).toBeEnabled());
      await userEvent.click(freshSend());
      await waitFor(() => expect(starts).toHaveLength(1));

      await goTo(B);
      await settle();
      expectOrdinaryComposer("");

      await act(async () => {
        release();
      });
      await waitFor(() => expect(settledCount()).toBe(1));
      await settle();
      // The late answer belongs to A's finished visit: B stays B's.
      expectOrdinaryComposer("");
      expect(starts).toHaveLength(1);

      // A's composer was cleared by its send; the late answer does not
      // bring the prompt back into a visit that has ended.
      await goTo(A);
      await settle();
      expectOrdinaryComposer("");
      expect(starts).toHaveLength(1);
    },
  );

  it("a failed seeded send keeps the edited draft and its persona for the retry", async () => {
    const { starts, control } = installWorkspaces();
    control.fail = true;
    renderAt(A);
    const input = await seedFromRibbon("get-approvals", GET_APPROVALS_PROMPT);
    const edited = `${GET_APPROVALS_PROMPT}\n\nAlso check the release notes.`;
    fireEvent.change(input, { target: { value: edited } });
    await waitFor(() => expect(freshSend()).toBeEnabled());
    await userEvent.click(freshSend());
    await waitFor(() => expect(starts).toHaveLength(1));

    // Refused: the exact edited draft is back, still pulsing.
    await waitFor(() =>
      expect(screen.getByTestId("chat-input")).toHaveValue(edited),
    );
    expect(freshSend()).toHaveAttribute("data-pulse", "true");

    control.fail = false;
    await waitFor(() => expect(freshSend()).toBeEnabled());
    await userEvent.click(freshSend());
    await waitFor(() => expect(starts).toHaveLength(2));
    expect(starts[1]).toMatchObject({
      workspaceId: A,
      persona: "ie",
      phase: "ie",
      content: edited,
    });
  });

  it("an edited seed survives Pop out and Dock without replaying the template", async () => {
    installWorkspaces();
    renderAt(A);
    const input = await seedFromRibbon("get-approvals", GET_APPROVALS_PROMPT);
    const edited = `${GET_APPROVALS_PROMPT}\n\nStart with the UI review.`;
    fireEvent.change(input, { target: { value: edited } });

    const rightPanel = screen.getByTestId("right-panel");
    await userEvent.click(within(rightPanel).getByTitle("Pop out"));
    const dock = await screen.findByTestId("floating-panel-dock");
    const floating = dock.parentElement!.parentElement as HTMLElement;
    expect(within(floating).getByTestId("chat-input")).toHaveValue(edited);
    expect(
      within(floating).getByTestId("chat-panel-no-session-send"),
    ).toHaveAttribute("data-pulse", "true");

    fireEvent.click(dock);
    await waitFor(() =>
      expect(screen.getByTestId("center-chat-panel")).toBeVisible(),
    );
    expect(
      within(screen.getByTestId("right-panel")).getByTestId("chat-input"),
    ).toHaveValue(edited);
  });

  it("on a phone each workspace's composer keeps its own draft across a switch", async () => {
    desktop(false);
    installWorkspaces();
    renderAt(A);
    const aInput = await screen.findByTestId("chat-input");
    await waitFor(() => expect(aInput).toBeEnabled());
    fireEvent.change(aInput, { target: { value: "A's phone draft" } });

    const bInput = await goTo(B);
    await settle();
    expect(bInput).toHaveValue("");

    const back = await goTo(A);
    expect(back).toHaveValue("A's phone draft");
  });
});
