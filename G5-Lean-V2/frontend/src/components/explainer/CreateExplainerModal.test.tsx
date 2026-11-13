// =============================================================================
// CreateExplainerModal Tests — focus-node picker, template combobox, format,
// submit payload shape, 422 surface, and pending-disable behavior.
// =============================================================================
// Harness cloned from components/views/CreateProjectDocModal.test.tsx:
// renderApp provider tree + MSW request interception. Controlled inputs use
// fireEvent.change and interactions gate on waitFor/findBy (experience note
// react_test_state_update_race).
// =============================================================================

import React from "react";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { createMockChatSession } from "../../test/fixtures";
import type { SyncAgentConfig } from "../../lib/types";
import type { ExplainerTemplate } from "../../hooks/useExplainers";

import { CreateExplainerModal } from "./CreateExplainerModal";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";

const TEMPLATES: ExplainerTemplate[] = [
  {
    name: "Big picture",
    slug: "big-picture",
    prompt: "Explain the big picture of the topic.",
    source: "builtin",
  },
  {
    name: "Balanced",
    slug: "balanced",
    prompt: "Give a balanced walkthrough.",
    source: "builtin",
  },
  {
    name: "My saved",
    slug: "my-saved",
    prompt: "My saved custom prompt.",
    source: "saved",
  },
];

const RESOLVED_AGENT_LAUNCH = {
  schemaVersion: 1,
  resolutionId: "alr_test",
  configRevision: "cfg-test",
  catalogVersion: "agent-launch-catalog-v1",
  provider: { value: "claude_code", provenance: "configured" },
  providerRoute: "anthropic_direct",
  model: { value: "anthropic/claude-opus-5.5", provenance: "configured" },
  upstreamModel: "anthropic/claude-opus-5.5",
  permissionMode: { value: "bypassPermissions", provenance: "configured" },
  effort: { value: "high", provenance: "configured" },
  authMode: "anthropic_api_key",
  egressMode: "direct_provider",
  credentialReady: true,
  requiresInteractiveAuth: false,
} as const;

const CONFIGURED_AGENT_LAUNCH_SELECTION = {
  preferences: {
    schemaVersion: 1,
    provider: { source: "explicit", value: "claude_code" },
    model: {
      source: "explicit",
      value: "anthropic/claude-opus-5.5",
    },
    permissionMode: {
      source: "explicit",
      value: "bypassPermissions",
    },
    effort: { source: "explicit", value: "high" },
  },
  confirmedResolutionId: "alr_test",
} as const;

const DEFAULT_AGENT_CONFIG: SyncAgentConfig = {
  agentProvider: "claude_code",
  permissionMode: "bypassPermissions",
  permissionLabel: "Bypass",
  model: undefined,
  modelLabel: "Opus 5.5",
};

function useDefaultHandlers() {
  server.use(
    http.get(
      "/api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates",
      () => HttpResponse.json({ data: TEMPLATES }),
    ),
    http.post(
      "/api/v1/workspaces/:workspaceId/config/agent-launch/resolve",
      () => HttpResponse.json({ data: { resolved: RESOLVED_AGENT_LAUNCH } }),
    ),
  );
}

function startedChat(id: string) {
  return {
    session: createMockChatSession({ id, workspaceId: "ws-1", scopeRef: null }),
    message: { messageId: "input-1", status: "queued" },
    assistantMessageId: "input-1",
    taskId: "task-1",
  };
}

function renderModal(
  props: Partial<React.ComponentProps<typeof CreateExplainerModal>> = {},
) {
  const onClose = vi.fn();
  const onCreated = vi.fn();
  const result = renderApp(
    <CreateExplainerModal
      open
      workspaceId="ws-1"
      branchId="branch-docs"
      agentConfig={DEFAULT_AGENT_CONFIG}
      onClose={onClose}
      onCreated={onCreated}
      {...props}
    />,
    { route: "/org-g5labs/test-workspace" },
  );
  return { ...result, onClose, onCreated };
}

async function fillTopic(value: string) {
  fireEvent.change(screen.getByTestId("create-explainer-topic"), {
    target: { value },
  });
  await waitFor(() =>
    expect(screen.getByTestId("create-explainer-topic")).toHaveValue(value),
  );
}

describe("CreateExplainerModal", () => {
  beforeEach(() => {
    localStorage.clear();
    useDefaultHandlers();
  });

  it("starts an ordinary chat with the assembled first message without creating an explainer job", async () => {
    const user = userEvent.setup();
    const starts: Record<string, unknown>[] = [];
    const createJob = vi.fn(() =>
      HttpResponse.json({ data: { docId: "doc-old" } }),
    );
    server.use(
      http.post("/api/v1/workspaces/:workspaceId/docs/explainers", createJob),
      http.post(
        "/api/v1/workspaces/:workspaceId/sessions/start",
        async ({ request }) => {
          starts.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            {
              data: {
                session: createMockChatSession({
                  id: "explainer-chat",
                  workspaceId: "ws-1",
                }),
                message: { messageId: "input-1", status: "queued" },
                assistantMessageId: "input-1",
                taskId: "task-1",
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    const { onCreated } = renderModal();
    await fillTopic("Explain sign-in");
    await user.selectOptions(
      await screen.findByTestId("create-explainer-template-select"),
      "__custom__",
    );
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Show a concrete example." },
    });
    await user.click(screen.getByTestId("create-explainer-submit"));
    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
    expect(createJob.mock.calls).toHaveLength(0);
    expect(starts).toHaveLength(1);
    expect(starts[0]).toMatchObject({
      scopeType: "workspace",
      agentLaunch: CONFIGURED_AGENT_LAUNCH_SELECTION,
    });
    expect(starts[0].content).toContain("Explain sign-in");
    expect(starts[0].content).toContain("Show a concrete example.");
    expect(starts[0].content).toContain("docs/ws-1/explainers/");
  });

  it("offers Start and generate once the workspace stops, and greys Generate with the reason and Start while it starts (g5.chat.start-and-send)", async () => {
    const user = userEvent.setup();
    const starts: Record<string, unknown>[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/sessions/start",
        async ({ request }) => {
          starts.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json({ data: {} }, { status: 500 });
        },
      ),
    );
    type State = "live" | "stopped" | "starting";
    const gate = (state: State) => ({
      kind: state,
      canWrite: true,
      live: state === "live",
      canStart: state === "stopped",
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner: true,
      workspaceNoun: "the workspace",
    });
    const onCreated = vi.fn();
    function Host() {
      const [state, setState] = React.useState<State>("live");
      return (
        <WorkspaceLiveGateProvider value={gate(state)}>
          <button
            type="button"
            data-testid="stop-workspace"
            onClick={() => setState("stopped")}
          >
            stop
          </button>
          <button
            type="button"
            data-testid="starting-workspace"
            onClick={() => setState("starting")}
          >
            starting
          </button>
          <CreateExplainerModal
            open
            workspaceId="ws-1"
            branchId="branch-docs"
            agentConfig={DEFAULT_AGENT_CONFIG}
            onClose={vi.fn()}
            onCreated={onCreated}
          />
        </WorkspaceLiveGateProvider>
      );
    }
    renderApp(<Host />, { route: "/org-g5labs/test-workspace" });
    await fillTopic("Explain sign-in");
    await user.selectOptions(
      await screen.findByTestId("create-explainer-template-select"),
      "__custom__",
    );
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Show a concrete example." },
    });
    const submit = screen.getByTestId("create-explainer-submit");
    await waitFor(() => expect(submit).toBeEnabled());
    expect(submit).toHaveTextContent("Generate");
    expect(screen.queryByTestId("create-explainer-start-hint")).toBeNull();

    // Starting: the dialog starts the chat itself, so the control waits with
    // the reason and keeps the draft; nothing is queued.
    await user.click(screen.getByTestId("starting-workspace"));
    await waitFor(() => expect(submit).toBeDisabled());
    expect(submit).toHaveAttribute(
      "title",
      "The workspace is starting; you can generate an explainer once it is running.",
    );
    fireEvent.click(screen.getByTestId("create-explainer-start-hint-trigger"));
    expect(screen.getByTestId("create-explainer-start-hint")).toHaveTextContent(
      "The workspace is starting; you can generate an explainer once it is running.",
    );
    expect(
      screen.queryByTestId("create-explainer-start-hint-start"),
    ).toBeNull();
    expect(screen.getByTestId("create-explainer-topic")).toHaveValue(
      "Explain sign-in",
    );
    fireEvent.click(submit);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(starts).toHaveLength(0);

    // Stopped: the same button starts the workspace and queues the prompt
    // bound to that Start, in one request.
    await user.click(screen.getByTestId("stop-workspace"));
    await waitFor(() => expect(submit).toBeEnabled());
    expect(submit).toHaveTextContent("Start and generate");
    expect(
      screen.queryByTestId("create-explainer-start-hint-trigger"),
    ).toBeNull();
    expect(screen.getByTestId("create-explainer-prompt")).toHaveValue(
      "Show a concrete example.",
    );
    fireEvent.click(submit);
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({ startCompute: true });
    expect(onCreated).not.toHaveBeenCalled();
  });

  it("keeps actions outside the scrollable modal body", () => {
    renderModal();

    expect(
      screen.getByTestId("create-explainer-modal-body"),
    ).not.toContainElement(
      screen.getByTestId("create-explainer-modal-actions"),
    );
  });

  it("fills the prompt from a selected template, flips to Custom (unsaved) on edit, and re-selects on verbatim re-entry", async () => {
    const user = userEvent.setup();
    renderModal();

    await screen.findByRole("option", { name: "Big picture" });
    const select = screen.getByTestId("create-explainer-template-select");

    await user.selectOptions(select, "big-picture");
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-prompt")).toHaveValue(
        "Explain the big picture of the topic.",
      ),
    );
    expect(select).toHaveValue("big-picture");

    // Editing the prompt away from the template flips to Custom (unsaved).
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Explain the big picture of the topic. Plus more." },
    });
    await waitFor(() => expect(select).toHaveValue("__custom__"));
    expect(
      screen.getByRole("option", { name: "Custom (write my own)" }),
    ).toBeInTheDocument();

    // Re-entering the template text verbatim re-selects the template.
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Explain the big picture of the topic." },
    });
    await waitFor(() => expect(select).toHaveValue("big-picture"));
  });

  // Spec D5, user decision 2026-08-28 (chat intent ffa3881b): the grouped
  // select is the ONLY template surface — the duplicated quick-pick chip row
  // is gone, and Custom stays reachable as a select option.
  it("offers templates only through the grouped select (no quick-pick chips)", async () => {
    const user = userEvent.setup();
    renderModal();

    const prompt = screen.getByTestId("create-explainer-prompt");
    expect(prompt).toBeDisabled();

    await screen.findByRole("option", { name: "Big picture" });
    expect(
      screen.queryByTestId("create-explainer-template-chip"),
    ).not.toBeInTheDocument();

    const select = screen.getByTestId("create-explainer-template-select");
    await user.selectOptions(select, "big-picture");
    await waitFor(() =>
      expect(prompt).toHaveValue("Explain the big picture of the topic."),
    );
    expect(prompt).toBeEnabled();

    // Selecting the Custom option restores the (never typed, so empty)
    // custom brief.
    await user.selectOptions(select, "__custom__");
    await waitFor(() => expect(prompt).toHaveValue(""));
    expect(select).toHaveValue("__custom__");
  });

  // Spec D3/D12, user decision 2026-08-28: the subject field is labeled
  // "Instruction", and every length limit fails loudly in realtime — live
  // counter, over-limit error styling, and a blocked Generate.
  it("labels the subject Instruction and blocks Generate loudly when the instruction exceeds its limit", async () => {
    const user = userEvent.setup();
    const { onCreated } = renderModal();

    expect(screen.getByText(/^Instruction/)).toBeInTheDocument();
    expect(screen.queryByText(/^Topic\b/)).not.toBeInTheDocument();

    // Under 80% of the limit no counter renders.
    await fillTopic("Short instruction");
    expect(
      screen.queryByTestId("create-explainer-topic-length"),
    ).not.toBeInTheDocument();

    // Near the limit the quiet counter appears.
    await fillTopic("a".repeat(9_500));
    expect(
      screen.getByTestId("create-explainer-topic-length"),
    ).toHaveTextContent("9,500 / 10,000");

    // Over the limit the counter flips to the loud over-limit message.
    await fillTopic("a".repeat(10_050));
    expect(
      screen.getByTestId("create-explainer-topic-length"),
    ).toHaveTextContent(
      "Instruction is 50 characters over the 10,000-character limit",
    );

    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Prompt." },
    });
    await user.click(screen.getByTestId("create-explainer-submit"));
    await waitFor(() =>
      expect(
        screen.getByText(
          "Instruction exceeds the 10,000-character limit — shorten it to continue",
        ),
      ).toBeInTheDocument(),
    );
    expect(onCreated).not.toHaveBeenCalled();

    // Shortening back under the limit clears the loud state in realtime.
    await fillTopic("a".repeat(9_999));
    expect(
      screen.getByTestId("create-explainer-topic-length"),
    ).toHaveTextContent("9,999 / 10,000");
  });

  // Spec D12, PR #2299 review finding: limits count Unicode code points
  // (matching the server's chars().count()), never UTF-16 units — emoji
  // must not double-count.
  it("counts Unicode code points, not UTF-16 units, for the instruction limit", async () => {
    renderModal();

    // 5,000 emoji are 10,000 UTF-16 units but only 5,000 characters —
    // under the 80% threshold, so no counter renders (UTF-16 counting
    // would show the counter at the limit).
    await fillTopic("😀".repeat(5_000));
    expect(
      screen.queryByTestId("create-explainer-topic-length"),
    ).not.toBeInTheDocument();

    // 10,050 emoji are 50 characters over the limit.
    await fillTopic("😀".repeat(10_050));
    expect(
      screen.getByTestId("create-explainer-topic-length"),
    ).toHaveTextContent(
      "Instruction is 50 characters over the 10,000-character limit",
    );
  });

  // PR #2299 review finding: within-limit keystrokes must not clear the
  // fileName error while the alphanumeric-slug rule still fails.
  it("keeps the fileName letter-or-number error visible while more symbols are typed", async () => {
    const user = userEvent.setup();
    renderModal();

    await fillTopic("Auth deep dive");
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Prompt." },
    });
    fireEvent.change(screen.getByTestId("create-explainer-file-name"), {
      target: { value: "???" },
    });
    await user.click(screen.getByTestId("create-explainer-submit"));
    await waitFor(() =>
      expect(
        screen.getByText(
          "File name must contain at least one letter or number",
        ),
      ).toBeInTheDocument(),
    );

    // More symbols: still invalid — the error must stay.
    fireEvent.change(screen.getByTestId("create-explainer-file-name"), {
      target: { value: "????" },
    });
    expect(
      screen.getByText("File name must contain at least one letter or number"),
    ).toBeInTheDocument();

    // A letter makes the value slugifiable; the error clears reactively.
    fireEvent.change(screen.getByTestId("create-explainer-file-name"), {
      target: { value: "????a" },
    });
    await waitFor(() =>
      expect(
        screen.queryByText(
          "File name must contain at least one letter or number",
        ),
      ).not.toBeInTheDocument(),
    );
  });

  it("fails loudly on an over-limit file name and blocks Generate", async () => {
    const user = userEvent.setup();
    const { onCreated } = renderModal();

    await fillTopic("Auth deep dive");
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Prompt." },
    });
    fireEvent.change(screen.getByTestId("create-explainer-file-name"), {
      target: { value: "n".repeat(130) },
    });
    expect(
      screen.getByTestId("create-explainer-file-name-length"),
    ).toHaveTextContent(
      "File name is 10 characters over the 120-character limit",
    );

    await user.click(screen.getByTestId("create-explainer-submit"));
    await waitFor(() =>
      expect(
        screen.getByText(
          "File name exceeds the 120-character limit — shorten it to continue",
        ),
      ).toBeInTheDocument(),
    );
    expect(onCreated).not.toHaveBeenCalled();
  });

  // Spec D8/D11: the agent disclosure row starts EXPANDED (agent-launch I9
  // keeps launch values visible at open); collapsing hides but never
  // unmounts the controls, so launch-preview resolution still completes.
  it("opens the agent row expanded and keeps controls mounted while collapsed", async () => {
    const user = userEvent.setup();
    renderModal();

    expect(screen.getByTestId("create-explainer-agent-select")).toBeVisible();
    expect(
      screen.getByTestId("create-explainer-agent-summary"),
    ).toHaveTextContent("Claude Code");

    await user.click(screen.getByTestId("create-explainer-agent-toggle"));
    await waitFor(() =>
      expect(
        screen.getByTestId("create-explainer-agent-select"),
      ).not.toBeVisible(),
    );

    // Resolution completes even while collapsed, so Generate enables.
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-submit")).toBeEnabled(),
    );

    await user.click(screen.getByTestId("create-explainer-agent-toggle"));
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-agent-select")).toBeVisible(),
    );
  });

  // Spec D5, user report 2026-08-10: choosing a template must be reversible —
  // explicitly selecting Custom (unsaved) restores the user's own prompt.
  it("restores the typed custom prompt when Custom is explicitly selected", async () => {
    const user = userEvent.setup();
    renderModal();

    await screen.findByRole("option", { name: "Big picture" });
    const select = screen.getByTestId("create-explainer-template-select");

    // The template choice is the required first step (spec D5): the prompt
    // unlocks only after choosing Custom (or a template).
    expect(screen.getByTestId("create-explainer-prompt")).toBeDisabled();
    await user.selectOptions(select, "__custom__");

    // The user types their own brief…
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "My own custom brief." },
    });
    expect(select).toHaveValue("__custom__");

    // …tries a built-in template…
    await user.selectOptions(select, "big-picture");
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-prompt")).toHaveValue(
        "Explain the big picture of the topic.",
      ),
    );

    // …and switching back to Custom restores the typed brief.
    await user.selectOptions(select, "__custom__");
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-prompt")).toHaveValue(
        "My own custom brief.",
      ),
    );
    expect(select).toHaveValue("__custom__");
  });

  it("keeps the earlier custom brief when typed text later matches a template verbatim", async () => {
    const user = userEvent.setup();
    renderModal();

    await screen.findByRole("option", { name: "Big picture" });
    const select = screen.getByTestId("create-explainer-template-select");

    await user.selectOptions(select, "__custom__");
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "My genuinely custom brief." },
    });

    // Typing a template's exact text re-selects that template — it must
    // NOT overwrite the remembered custom brief (spec D5).
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Explain the big picture of the topic." },
    });
    await waitFor(() => expect(select).toHaveValue("big-picture"));

    await user.selectOptions(select, "__custom__");
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-prompt")).toHaveValue(
        "My genuinely custom brief.",
      ),
    );
    expect(select).toHaveValue("__custom__");
  });

  it("restores an empty prompt on Custom when the user never typed one", async () => {
    const user = userEvent.setup();
    renderModal();

    await screen.findByRole("option", { name: "Big picture" });
    const select = screen.getByTestId("create-explainer-template-select");

    await user.selectOptions(select, "big-picture");
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-prompt")).toHaveValue(
        "Explain the big picture of the topic.",
      ),
    );

    await user.selectOptions(select, "__custom__");
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-prompt")).toHaveValue(""),
    );
    expect(select).toHaveValue("__custom__");
  });

  // Spec D6, user decision 2026-08-10: saving only makes sense for a custom
  // prompt — the affordance hides while a template is verbatim-selected.
  it("shows Save as template only while the selection is Custom", async () => {
    const user = userEvent.setup();
    renderModal();

    await screen.findByRole("option", { name: "Big picture" });
    const select = screen.getByTestId("create-explainer-template-select");

    // Before the required template choice, the save affordance is hidden.
    expect(
      screen.queryByTestId("create-explainer-save-template-toggle"),
    ).toBeNull();
    await user.selectOptions(select, "__custom__");

    // Custom state: affordance available; open the save panel.
    expect(
      screen.getByTestId("create-explainer-save-template-toggle"),
    ).toBeInTheDocument();
    await user.click(
      screen.getByTestId("create-explainer-save-template-toggle"),
    );
    expect(
      screen.getByTestId("create-explainer-template-name"),
    ).toBeInTheDocument();

    // Selecting a template hides both the toggle and the open panel.
    await user.selectOptions(select, "big-picture");
    await waitFor(() =>
      expect(
        screen.queryByTestId("create-explainer-save-template-toggle"),
      ).toBeNull(),
    );
    expect(screen.queryByTestId("create-explainer-template-name")).toBeNull();

    // Editing back to a custom prompt brings the affordance back.
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Edited into a custom brief." },
    });
    await waitFor(() =>
      expect(
        screen.getByTestId("create-explainer-save-template-toggle"),
      ).toBeInTheDocument(),
    );
  });

  // Spec D12, PR #2299 review finding: the DERIVED slug's 64-character
  // server rule is validated in realtime — a 65-character name is within the
  // 80-character name limit but its slug is over the slug rule, so the loud
  // slug error shows while typing and Save is blocked with no request.
  it("blocks Save loudly when a 65-character template name exceeds the 64-character slug limit", async () => {
    const user = userEvent.setup();
    const saveCalls: unknown[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates",
        async ({ request }) => {
          saveCalls.push(await request.json());
          return HttpResponse.json(
            {
              data: {
                headCommit: "c1",
                template: {
                  name: "x",
                  slug: "x",
                  prompt: "p",
                  source: "saved",
                },
              },
            },
            { status: 200 },
          );
        },
      ),
    );
    renderModal();

    await screen.findByRole("option", { name: "Big picture" });
    await user.selectOptions(
      screen.getByTestId("create-explainer-template-select"),
      "__custom__",
    );
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "My custom brief." },
    });
    await user.click(
      screen.getByTestId("create-explainer-save-template-toggle"),
    );

    // 65 ASCII chars → a 65-character slug: name counter stays within its
    // 80 limit, but the realtime slug error fires.
    fireEvent.change(screen.getByTestId("create-explainer-template-name"), {
      target: { value: "a".repeat(65) },
    });
    expect(
      screen.getByTestId("create-explainer-template-slug-length"),
    ).toHaveTextContent(
      "Template name saves as a 65-character slug — over the 64-character slug limit; shorten the name to continue",
    );

    await user.click(screen.getByTestId("create-explainer-save-template"));
    expect(
      screen.getAllByText(/over the 64-character slug limit/).length,
    ).toBeGreaterThanOrEqual(1);
    expect(saveCalls).toHaveLength(0);

    // Shortening the name clears the loud slug state in realtime, and Save
    // goes through.
    fireEvent.change(screen.getByTestId("create-explainer-template-name"), {
      target: { value: "a".repeat(64) },
    });
    expect(
      screen.queryByTestId("create-explainer-template-slug-length"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("create-explainer-save-template"));
    await waitFor(() => expect(saveCalls).toHaveLength(1));
  });

  it("sends the selected style and HTML format in the first chat message", async () => {
    const user = userEvent.setup();
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/sessions/start",
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            { data: startedChat("session-1") },
            { status: 201 },
          );
        },
      ),
    );
    const { onClose, onCreated } = renderModal();

    await fillTopic("Auth deep dive");

    await screen.findByRole("option", { name: "Big picture" });
    await user.selectOptions(
      screen.getByTestId("create-explainer-template-select"),
      "big-picture",
    );
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-prompt")).not.toHaveValue(""),
    );

    await user.click(screen.getByTestId("create-explainer-submit"));

    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
    expect(onCreated).toHaveBeenCalledWith({ sessionId: "session-1" });
    expect(onClose).toHaveBeenCalled();
    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({
      scopeType: "workspace",
      agentLaunch: CONFIGURED_AGENT_LAUNCH_SELECTION,
    });
    expect(bodies[0].content).toContain("Auth deep dive");
    expect(bodies[0].content).toContain(
      "Explain the big picture of the topic.",
    );
    expect(bodies[0].content).toContain("self-contained HTML page (.html)");
    // A used template is recorded most-recent-first for the Recent group.
    expect(
      JSON.parse(
        localStorage.getItem("g5:explainerTemplates:recent:ws-1") ?? "[]",
      ),
    ).toEqual(["big-picture"]);
  });

  it("uses the selected Codex launch configuration for the new chat", async () => {
    const user = userEvent.setup();
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/config/agent-launch/resolve",
        async ({ request }) => {
          const { preferences } = (await request.json()) as {
            preferences: { provider: { value?: string } };
          };
          const codex = preferences.provider.value === "codex";
          return HttpResponse.json({
            data: {
              resolved: codex
                ? {
                    ...RESOLVED_AGENT_LAUNCH,
                    resolutionId: "alr_codex",
                    provider: { value: "codex", provenance: "explicit" },
                    providerRoute: "openai_direct",
                    model: { value: "gpt-5.5", provenance: "explicit" },
                    upstreamModel: "gpt-5.5",
                    permissionMode: {
                      value: "never/danger-full-access",
                      provenance: "explicit",
                    },
                    authMode: "openai_api_key",
                  }
                : RESOLVED_AGENT_LAUNCH,
            },
          });
        },
      ),
      http.post(
        "/api/v1/workspaces/:workspaceId/sessions/start",
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            { data: startedChat("codex-chat") },
            { status: 201 },
          );
        },
      ),
    );
    const { onCreated } = renderModal({
      agentConfigOptions: [
        DEFAULT_AGENT_CONFIG,
        {
          agentProvider: "codex",
          model: "gpt-5.5",
          modelLabel: "GPT-5.5",
          permissionMode: "never/danger-full-access",
          effort: "high",
        },
      ],
    });
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-agent-select")).toBeEnabled(),
    );
    await user.click(screen.getByTestId("create-explainer-agent-select"));
    await user.click(await screen.findByRole("option", { name: "Codex" }));
    await fillTopic("Explain the workspace");
    await user.selectOptions(
      screen.getByTestId("create-explainer-template-select"),
      "__custom__",
    );
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Be brief." },
    });
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-submit")).toBeEnabled(),
    );
    await user.click(screen.getByTestId("create-explainer-submit"));
    await waitFor(() =>
      expect(onCreated).toHaveBeenCalledWith({ sessionId: "codex-chat" }),
    );
    expect(bodies[0].agentLaunch).toEqual({
      preferences: {
        schemaVersion: 1,
        provider: { source: "explicit", value: "codex" },
        model: { source: "explicit", value: "gpt-5.5" },
        permissionMode: {
          source: "explicit",
          value: "never/danger-full-access",
        },
        effort: { source: "explicit", value: "high" },
      },
      confirmedResolutionId: "alr_codex",
    });
  });

  it("sends whole-workspace scope, custom instructions, and the slides format", async () => {
    const user = userEvent.setup();
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/sessions/start",
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            { data: startedChat("session-2") },
            { status: 201 },
          );
        },
      ),
    );
    const { onCreated } = renderModal();

    await fillTopic("Whole workspace tour");
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Explain everything briefly." },
    });

    await user.click(screen.getByTestId("create-explainer-format-slides"));
    await waitFor(() =>
      // Assert the semantic selected state, not a styling class — the
      // control is a segmented radiogroup and its visual treatment is free
      // to change.
      expect(
        screen.getByTestId("create-explainer-format-slides"),
      ).toHaveAttribute("aria-checked", "true"),
    );

    await user.click(screen.getByTestId("create-explainer-submit"));

    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({
      scopeType: "workspace",
      agentLaunch: CONFIGURED_AGENT_LAUNCH_SELECTION,
    });
    expect(bodies[0].content).toContain("Whole workspace tour");
    expect(bodies[0].content).toContain("Explain everything briefly.");
    expect(bodies[0].content).toContain("Use the workspace files as context");
    expect(bodies[0].content).toContain(
      "self-contained HTML slide deck (.html)",
    );
  });

  it("surfaces a 422 validation error inline and keeps the modal open", async () => {
    const user = userEvent.setup();
    server.use(
      http.post("/api/v1/workspaces/:workspaceId/sessions/start", () =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "nodeIds contains an unknown node id node-gone",
            },
          },
          { status: 422 },
        ),
      ),
    );
    const { onClose, onCreated } = renderModal();

    await fillTopic("Broken");
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Prompt." },
    });

    await user.click(screen.getByTestId("create-explainer-submit"));

    await waitFor(() =>
      expect(
        screen.getByTestId("create-explainer-submit-error"),
      ).toHaveTextContent("nodeIds contains an unknown node id node-gone"),
    );
    expect(onCreated).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("disables Generate while the create request is pending", async () => {
    let release: (() => void) | undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    let requests = 0;
    server.use(
      http.post("/api/v1/workspaces/:workspaceId/sessions/start", async () => {
        requests += 1;
        await gate;
        return HttpResponse.json(
          { data: startedChat("session-3") },
          { status: 201 },
        );
      }),
    );
    const { onCreated } = renderModal();

    await fillTopic("Pending");
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Prompt." },
    });

    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-submit")).toBeEnabled(),
    );
    act(() => {
      fireEvent.click(screen.getByTestId("create-explainer-submit"));
      fireEvent.click(screen.getByTestId("create-explainer-submit"));
    });

    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-submit")).toBeDisabled(),
    );
    expect(screen.getByTestId("create-explainer-submit")).toHaveTextContent(
      "Generating…",
    );
    expect(screen.getByTestId("create-explainer-close")).toBeDisabled();
    expect(screen.getByTestId("create-explainer-cancel")).toBeDisabled();

    release?.();
    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
    expect(requests).toBe(1);
  });

  it("preserves the form on a rejected chat start and can retry", async () => {
    const user = userEvent.setup();
    const bodies: unknown[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/sessions/start",
        async ({ request }) => {
          bodies.push(await request.json());
          return bodies.length === 1
            ? HttpResponse.json(
                {
                  error: {
                    code: "CONFLICT",
                    message: "Workspace is not ready",
                  },
                },
                { status: 409 },
              )
            : HttpResponse.json(
                { data: startedChat("retried-session") },
                { status: 201 },
              );
        },
      ),
    );
    const { onCreated, onClose } = renderModal();
    await fillTopic("Retry this explainer");
    await user.selectOptions(
      screen.getByTestId("create-explainer-template-select"),
      "__custom__",
    );
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Keep my instructions." },
    });
    await user.click(screen.getByTestId("create-explainer-submit"));
    expect(
      await screen.findByTestId("create-explainer-submit-error"),
    ).toHaveTextContent("Workspace is not ready");
    expect(onCreated).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId("create-explainer-topic")).toHaveValue(
      "Retry this explainer",
    );
    expect(screen.getByTestId("create-explainer-prompt")).toHaveValue(
      "Keep my instructions.",
    );
    await user.click(screen.getByTestId("create-explainer-submit"));
    await waitFor(() =>
      expect(onCreated).toHaveBeenCalledWith({ sessionId: "retried-session" }),
    );
    expect(bodies).toHaveLength(2);
    expect(bodies[1]).toEqual(bodies[0]);
  });

  it("does not navigate when a pending launch finishes after unmount", async () => {
    const user = userEvent.setup();
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.post("/api/v1/workspaces/:workspaceId/sessions/start", async () => {
        await gate;
        return HttpResponse.json(
          { data: startedChat("background-session") },
          { status: 201 },
        );
      }),
    );
    const { onCreated, onClose, unmount, queryClient } = renderModal();
    await fillTopic("Navigate while starting");
    await user.selectOptions(
      screen.getByTestId("create-explainer-template-select"),
      "__custom__",
    );
    fireEvent.change(screen.getByTestId("create-explainer-prompt"), {
      target: { value: "Explain." },
    });
    await user.click(screen.getByTestId("create-explainer-submit"));
    expect(screen.getByTestId("create-explainer-submit")).toBeDisabled();
    unmount();
    release();
    await waitFor(() =>
      expect(
        queryClient.getQueryData(["chat-session", "background-session"]),
      ).toBeDefined(),
    );
    expect(onCreated).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("includes the chosen filename separately from the instruction, matching the preview", async () => {
    const user = userEvent.setup();
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/sessions/start",
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            { data: startedChat("session-1") },
            { status: 201 },
          );
        },
      ),
    );
    renderModal();

    await fillTopic("How workspace merge works end to end");
    fireEvent.change(screen.getByTestId("create-explainer-file-name"), {
      target: { value: "Workspace Merge" },
    });

    // The preview matches the filename instruction sent to the agent.
    await screen.findByText(/explainers\/workspace-merge-<date>\.html/);

    await screen.findByRole("option", { name: "Big picture" });
    await user.selectOptions(
      screen.getByTestId("create-explainer-template-select"),
      "big-picture",
    );
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-prompt")).not.toHaveValue(""),
    );
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-submit")).toBeEnabled(),
    );
    // Edit only the filename after all other selections have settled.
    fireEvent.change(screen.getByTestId("create-explainer-file-name"), {
      target: { value: "Workspace Final" },
    });
    await user.click(screen.getByTestId("create-explainer-submit"));

    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0].content).toContain("How workspace merge works end to end");
    expect(bodies[0].content).toContain(
      "Use the filename workspace-final-<date>.html",
    );
  });

  it("asks the agent to choose a short filename when the filename is blank", async () => {
    const user = userEvent.setup();
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/sessions/start",
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            { data: startedChat("session-1") },
            { status: 201 },
          );
        },
      ),
    );
    renderModal();

    await fillTopic("Auth deep dive");
    // File name left empty — the saved-as hint keeps its placeholder and
    // never mirrors the topic (typing the topic must not visibly rewrite
    // the file name); the agent chooses a short name from the instruction.
    await screen.findByText(/explainers\/<name>-<date>\.html/);
    expect(
      screen.queryByText(/explainers\/auth-deep-dive-<date>\.html/),
    ).toBeNull();

    await screen.findByRole("option", { name: "Big picture" });
    await user.selectOptions(
      screen.getByTestId("create-explainer-template-select"),
      "big-picture",
    );
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-prompt")).not.toHaveValue(""),
    );
    await user.click(screen.getByTestId("create-explainer-submit"));

    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0].content).toContain("Auth deep dive");
    expect(bodies[0].content).toContain(
      "Choose a short descriptive kebab-case filename from the instruction",
    );
  });

  it("rejects a file name with no alphanumerics without issuing a request", async () => {
    const user = userEvent.setup();
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/sessions/start",
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            { data: startedChat("session-1") },
            { status: 201 },
          );
        },
      ),
    );
    renderModal();

    await fillTopic("Auth deep dive");
    fireEvent.change(screen.getByTestId("create-explainer-file-name"), {
      target: { value: "???" },
    });

    await screen.findByRole("option", { name: "Big picture" });
    await user.selectOptions(
      screen.getByTestId("create-explainer-template-select"),
      "big-picture",
    );
    await waitFor(() =>
      expect(screen.getByTestId("create-explainer-prompt")).not.toHaveValue(""),
    );
    await user.click(screen.getByTestId("create-explainer-submit"));

    await screen.findByText(
      "File name must contain at least one letter or number",
    );
    expect(bodies).toHaveLength(0);
  });

  // Spec D7 (user decision 2026-08-10): format is a two-step choice — the
  // document kind makes the HTML/Markdown split explicit, and the HTML
  // layout follow-up (Single page/Slides) only exists while HTML is chosen.
  it("chooses format as document kind plus an HTML-only layout follow-up", async () => {
    renderModal();

    const html = screen.getByTestId("create-explainer-format-html");
    const markdown = screen.getByTestId("create-explainer-format-markdown");
    const page = screen.getByTestId("create-explainer-format-page");
    const slides = screen.getByTestId("create-explainer-format-slides");

    // Defaults: HTML kind with the Single page layout.
    expect(html).toHaveAttribute("aria-checked", "true");
    expect(markdown).toHaveAttribute("aria-checked", "false");
    expect(page).toHaveAttribute("aria-checked", "true");
    expect(slides).toHaveAttribute("aria-checked", "false");

    fireEvent.click(slides);
    await waitFor(() => expect(slides).toHaveAttribute("aria-checked", "true"));
    expect(page).toHaveAttribute("aria-checked", "false");

    // Choosing Markdown hides the HTML layout follow-up entirely.
    fireEvent.click(markdown);
    await waitFor(() =>
      expect(markdown).toHaveAttribute("aria-checked", "true"),
    );
    expect(html).toHaveAttribute("aria-checked", "false");
    expect(screen.queryByTestId("create-explainer-format-page")).toBeNull();
    expect(screen.queryByTestId("create-explainer-format-slides")).toBeNull();

    // Toggling back to HTML restores the remembered layout (Slides).
    fireEvent.click(html);
    await waitFor(() => expect(html).toHaveAttribute("aria-checked", "true"));
    expect(
      screen.getByTestId("create-explainer-format-slides"),
    ).toHaveAttribute("aria-checked", "true");
    expect(screen.getByTestId("create-explainer-format-page")).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  // Markdown remains an explicit instruction and .md output filename.
  it("sends Markdown authoring instructions and the .md extension", async () => {
    const user = userEvent.setup();
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/sessions/start",
        async ({ request }) => {
          bodies.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            { data: startedChat("session-3") },
            { status: 201 },
          );
        },
      ),
    );
    const { onCreated } = renderModal();

    await fillTopic("Markdown tour");
    // Template-first flow: the prompt stays disabled until a template (or
    // Custom) is chosen, so select Custom and wait for the enable before
    // typing — fireEvent.change on the disabled input would create a state a
    // user cannot reach.
    await user.selectOptions(
      screen.getByTestId("create-explainer-template-select"),
      "__custom__",
    );
    const promptInput = screen.getByTestId("create-explainer-prompt");
    await waitFor(() => expect(promptInput).toBeEnabled());
    fireEvent.change(promptInput, {
      target: { value: "Explain it as plain markdown." },
    });

    await user.click(screen.getByTestId("create-explainer-format-markdown"));
    await waitFor(() =>
      expect(
        screen.getByTestId("create-explainer-format-markdown"),
      ).toHaveAttribute("aria-checked", "true"),
    );

    await user.click(screen.getByTestId("create-explainer-submit"));

    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({
      scopeType: "workspace",
      agentLaunch: CONFIGURED_AGENT_LAUNCH_SELECTION,
    });
    expect(bodies[0].content).toContain("Markdown tour");
    expect(bodies[0].content).toContain("Explain it as plain markdown.");
    expect(bodies[0].content).toContain(
      "self-contained Markdown document (.md)",
    );
    expect(bodies[0].content).toContain("-<date>.md");
  });
});
