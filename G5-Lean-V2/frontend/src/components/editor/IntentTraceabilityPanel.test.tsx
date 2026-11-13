// =============================================================================
// IntentTraceabilityPanel — Unit Tests
// =============================================================================
// Covers: loading state, results rendering, empty state, dismiss callback.
// Uses MSW to mock the grounding API endpoints.
// =============================================================================

import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import React from "react";
import { server } from "../../test/msw/server";
import { IntentTraceabilityPanel } from "./IntentTraceabilityPanel";
import type { IntentTraceabilityPanelProps } from "./IntentTraceabilityPanel";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
    },
  });
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);
}

function renderPanel(overrides: Partial<IntentTraceabilityPanelProps> = {}) {
  const props: IntentTraceabilityPanelProps = {
    direction: "parents",
    workspaceId: "branch-001",
    repoId: "repo-001",
    fileType: "ontology",
    fileName: ".semlift/specs/g5/g5-api/routes.semlift.md",
    intentRecordNumber: "I1",
    onDismiss: vi.fn(),
    onNavigate: vi.fn(),
    ...overrides,
  };
  return {
    ...render(<IntentTraceabilityPanel {...props} />, {
      wrapper: createWrapper(),
    }),
    props,
  };
}

// ---------------------------------------------------------------------------
// Mock responses
// ---------------------------------------------------------------------------

const PARENTS_RESPONSE = {
  data: {
    totalCount: 2,
    results: [
      {
        intentRecordId: "uuid-p1",
        workspaceId: "branch-docs",
        repoId: "repo-docs",
        fileType: "view",
        fileName: "views/pm_views/auth.pm_view.md",
        intentRecordNumber: "I3",
        codeBlockName: null,
        // Server ships label-line-free content plus extraction metadata (D10).
        content: "Authentication flow using OAuth2 tokens",
        title: "Auth Flow",
        blockLabel: "I3",
        isHuman: true,
        priority: 1,
        certainty: 90,
        relationship: "ref",
        relevance: 80,
      },
      {
        intentRecordId: "uuid-p2",
        workspaceId: "branch-code",
        repoId: "repo-code",
        fileType: "ontology",
        fileName: ".semlift/specs/g5/auth.semlift.md",
        intentRecordNumber: "D2",
        codeBlockName: null,
        content: null,
        title: null,
        blockLabel: null,
        isHuman: false,
        priority: 2,
        certainty: 80,
        relationship: null,
        relevance: null,
      },
    ],
  },
};

const EMPTY_RESPONSE = {
  data: {
    totalCount: 0,
    results: [],
  },
};

const NON_NAVIGABLE_RESPONSE = {
  data: {
    totalCount: 1,
    results: [
      {
        intentRecordId: "chat-record",
        workspaceId: "branch-docs",
        repoId: "repo-docs",
        fileType: "chat",
        fileName: null,
        intentRecordNumber: null,
        codeBlockName: null,
        content: null,
        title: null,
        blockLabel: null,
        isHuman: true,
        priority: null,
        certainty: null,
        relationship: null,
        relevance: null,
      },
    ],
  },
};

const SAME_ID_DIFFERENT_OWNER_RESPONSE = {
  data: {
    totalCount: 2,
    results: [
      {
        intentRecordId: "shared-record-id",
        workspaceId: "branch-one",
        repoId: "repo-one",
        fileType: "ontology",
        fileName: ".semlift/specs/one.semlift.md",
        intentRecordNumber: "I1",
        codeBlockName: null,
        content: "First owner content",
        title: "First owner",
        blockLabel: "I1",
        isHuman: false,
        priority: null,
        certainty: null,
        relationship: null,
        relevance: null,
      },
      {
        intentRecordId: "shared-record-id",
        workspaceId: "branch-two",
        repoId: "repo-two",
        fileType: "ontology",
        fileName: ".semlift/specs/two.semlift.md",
        intentRecordNumber: "I1",
        codeBlockName: null,
        content: "Second owner content",
        title: "Second owner",
        blockLabel: "I1",
        isHuman: false,
        priority: null,
        certainty: null,
        relationship: null,
        relevance: null,
      },
    ],
  },
};

const EXPANDABLE_CONTENT_RESPONSE = {
  data: {
    totalCount: 1,
    results: [
      {
        intentRecordId: "uuid-expandable",
        workspaceId: "branch-code",
        repoId: "repo-code",
        fileType: "ontology",
        fileName: ".semlift/specs/g5/expandable.semlift.md",
        intentRecordNumber: "I8",
        codeBlockName: null,
        content:
          "[I8. Expandable intent]\nFirst content line\nSecond content line\nThird content line",
        isHuman: false,
        priority: null,
        certainty: null,
        relationship: "ref",
        relevance: 75,
      },
    ],
  },
};

const QUESTION_RESPONSE = {
  data: {
    totalCount: 1,
    results: [
      {
        intentRecordId: "uuid-question",
        workspaceId: "branch-code",
        repoId: "repo-code",
        fileType: "ontology",
        fileName: ".semlift/specs/g5/questions.semlift.md",
        intentRecordNumber: "Q2a",
        blockLabel: "Q2a",
        title: "Which policy applies?",
        codeBlockName: null,
        content: "Question body only",
        isHuman: true,
        priority: null,
        certainty: null,
        relationship: "ref",
        relevance: 80,
      },
    ],
  },
};

const RICH_PREVIEW_RESPONSE = {
  data: {
    totalCount: 1,
    results: [
      {
        intentRecordId: "uuid-rich-preview",
        workspaceId: "branch-code",
        repoId: "repo-code",
        fileType: "ontology",
        fileName: ".semlift/specs/g5/rich-preview.semlift.md",
        intentRecordNumber: "D8",
        blockLabel: "D8",
        title: "Rendered preview",
        codeBlockName: null,
        content:
          "Uses **bold intent** and *emphasis*.\n\n- First outcome\n- Second outcome\n\n| State | Meaning |\n| --- | --- |\n| ready | Can run |",
        isHuman: false,
        priority: null,
        certainty: null,
        relationship: "ref",
        relevance: 80,
      },
    ],
  },
};

const CODE_PREVIEW_RESPONSE = {
  data: {
    totalCount: 1,
    results: [
      {
        intentRecordId: "uuid-code-preview",
        workspaceId: "branch-code",
        repoId: "repo-code",
        fileType: "code",
        fileName: "client/src/example.ts",
        intentRecordNumber: null,
        blockLabel: null,
        title: null,
        codeBlockName: "exampleFunction",
        content: "# not a heading\nconst value = '**literal**';",
        isHuman: false,
        priority: null,
        certainty: null,
        relationship: "implements",
        relevance: 90,
      },
    ],
  },
};

const NAVIGATION_RESPONSE = {
  data: {
    totalCount: 2,
    results: [
      {
        intentRecordId: "uuid-view",
        workspaceId: "branch-docs",
        repoId: "repo-docs",
        fileType: "view",
        fileName: "views/security.pm_view.md",
        intentRecordNumber: "I4",
        codeBlockName: null,
        content: "Trace to view content",
        title: "Security View",
        blockLabel: "I4",
        isHuman: true,
        priority: null,
        certainty: null,
      },
      {
        intentRecordId: "uuid-doc",
        workspaceId: "branch-docs",
        repoId: "repo-docs",
        fileType: "external_doc",
        fileName: "docs/security_plan.md",
        intentRecordNumber: "D1",
        codeBlockName: null,
        // external_doc rows have whole-file content and no block title.
        content: "Trace to project doc content",
        title: null,
        blockLabel: null,
        isHuman: true,
        priority: null,
        certainty: null,
      },
    ],
  },
};

const CHAT_RESPONSE = {
  data: {
    totalCount: 1,
    results: [
      {
        intentRecordId: "uuid-chat",
        workspaceId: "branch-docs",
        repoId: "repo-docs",
        fileType: "chat",
        fileName: null,
        intentRecordNumber: "abcd1234",
        codeBlockName: null,
        content:
          "Chat context: Legacy combined context\n---------\nUser input: Legacy combined content",
        chatSessionId: "session-chat-1",
        chatContext: "Release review\nSecond line",
        userContent: "Please keep Unicode intact: café 東京 ✅",
        createdBy: "human@example.com",
        createdAt: "2026-08-03T12:34:56Z",
        sourceFilePath: null,
        title: null,
        blockLabel: null,
        isHuman: true,
        priority: 2,
        certainty: 94,
      },
    ],
  },
};

const CODE_NAVIGATION_RESPONSE = {
  data: {
    totalCount: 3,
    results: [
      {
        intentRecordId: "uuid-code-physical",
        workspaceId: "branch-code-owner",
        repoId: "repo-code-owner",
        fileType: "code",
        fileName: "g5-api/src/routes/grounding.rs",
        sourceFilePath: "generation/g5-api/src/routes/grounding.rs",
        intentRecordNumber: null,
        codeBlockName: "open_physical",
        content: "fn open_physical() {}",
        title: null,
        blockLabel: null,
        isHuman: false,
        priority: null,
        certainty: null,
      },
      {
        intentRecordId: "uuid-code-identical",
        workspaceId: "branch-code-owner",
        repoId: "repo-code-owner",
        fileType: "code",
        fileName: "src/lib.rs",
        sourceFilePath: "src/lib.rs",
        intentRecordNumber: null,
        codeBlockName: "open_same",
        content: "fn open_same() {}",
        title: null,
        blockLabel: null,
        isHuman: false,
        priority: null,
        certainty: null,
      },
      {
        intentRecordId: "uuid-code-missing",
        workspaceId: "branch-code-owner",
        repoId: "repo-code-owner",
        fileType: "code",
        fileName: "missing/source.rs",
        sourceFilePath: null,
        intentRecordNumber: null,
        codeBlockName: "missing_path",
        content: "fn missing_path() {}",
        title: null,
        blockLabel: null,
        isHuman: false,
        priority: null,
        certainty: null,
      },
    ],
  },
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("IntentTraceabilityPanel", () => {
  it("shows loading state initially", () => {
    // Never resolve the request → stays in loading
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () => {
        return new Promise(() => {}); // hang forever
      }),
    );
    renderPanel();
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("renders parent results after successful fetch", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(PARENTS_RESPONSE),
      ),
    );
    renderPanel({ direction: "parents" });

    await waitFor(() => {
      expect(screen.getByText("I3. Auth Flow")).toBeInTheDocument();
    });

    expect(screen.getByText("Source")).toBeInTheDocument();
    expect(screen.getByText("D2")).toBeInTheDocument();
    // TA4.2: no raw label-line prefix is ever rendered — titles come from
    // server extraction metadata, not content parsing.
    expect(screen.queryByText(/\[I\d/)).toBeNull();
    // Human-authored indicator for first result
    expect(screen.getByTitle("Human-authored")).toBeInTheDocument();
    // Content subtitle for first result (title line stripped, body remains)
    expect(
      screen.getByText("Authentication flow using OAuth2 tokens"),
    ).toBeInTheDocument();
    // Table column headers
    expect(screen.getByText("File")).toBeInTheDocument();
    expect(screen.getByText("Intent")).toBeInTheDocument();
    expect(screen.getByText("Content")).toBeInTheDocument();
    expect(screen.queryByText("Rel")).not.toBeInTheDocument();
    expect(screen.queryByText("ref")).not.toBeInTheDocument();
    expect(screen.queryByText("80%")).not.toBeInTheDocument();
    expect(screen.queryByText("NA")).not.toBeInTheDocument();
    // File type tags rendered
    expect(screen.getByText("view")).toBeInTheDocument();
    expect(screen.getByText("ontology")).toBeInTheDocument();
    // File column shows basename only (both results have basename "auth")
    expect(screen.getAllByText("auth")).toHaveLength(2);
    // Verify table rows rendered (2 data rows)
    const rows = screen.getAllByRole("row");
    // 1 header row + 2 data rows
    expect(rows.length).toBeGreaterThanOrEqual(3);
    // Intent label with title is a clickable link (button)
    const intentLink = screen.getByText("I3. Auth Flow");
    expect(intentLink.tagName).toBe("BUTTON");
  });

  it("uses responsive cards when embedded in a semantic record", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(PARENTS_RESPONSE),
      ),
    );
    renderPanel({ direction: "parents", variant: "record" });

    await waitFor(() => {
      expect(screen.getByText("I3. Auth Flow")).toBeInTheDocument();
    });
    expect(
      screen.getByTestId("traceability-record-results"),
    ).toBeInTheDocument();
    expect(screen.getAllByTestId("traceability-record-card")).toHaveLength(2);
    expect(screen.getByTestId("trace-no-content")).toHaveTextContent("—");
    expect(screen.queryByRole("columnheader")).not.toBeInTheDocument();
  });

  it("keeps root and expanded trace requests scoped to their repository", async () => {
    const user = userEvent.setup();
    const requestedRepoIds: string[] = [];
    server.use(
      http.get("*/api/v1/grounding/trace-parents", ({ request }) => {
        const repoId = new URL(request.url).searchParams.get("repoId") ?? "";
        requestedRepoIds.push(repoId);
        return HttpResponse.json(
          requestedRepoIds.length === 1
            ? EXPANDABLE_CONTENT_RESPONSE
            : EMPTY_RESPONSE,
        );
      }),
    );
    renderPanel({ direction: "parents", variant: "record" });

    await user.click(await screen.findByTestId("trace-expand"));
    await waitFor(() => {
      expect(requestedRepoIds).toEqual(["repo-001", "repo-code"]);
    });
  });

  it("shows a disabled navigation explanation for non-navigable records", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(NON_NAVIGABLE_RESPONSE),
      ),
    );
    renderPanel({ direction: "parents", variant: "record" });

    const navigate = await screen.findByTestId("trace-navigate");
    expect(navigate).toBeDisabled();
    expect(navigate).toHaveAttribute(
      "title",
      "External intent — no file to open",
    );
    expect(screen.getByTestId("trace-no-content")).toHaveTextContent("—");
  });

  it("fails closed for code records without source paths in record cards", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(CODE_NAVIGATION_RESPONSE),
      ),
    );
    renderPanel({ direction: "parents", variant: "record" });

    const missingLabel = await screen.findByText("missing_path");
    const missingCard = missingLabel.closest<HTMLElement>(
      '[data-testid="traceability-record-card"]',
    );
    expect(missingCard).not.toBeNull();
    expect(
      within(missingCard!).getByTitle(
        "Code source unavailable — no file to open",
      ),
    ).toBeDisabled();
    expect(
      within(missingCard!).queryByTestId("intent-traceability-open-intent"),
    ).toBeNull();
  });

  it("keeps hide state separate for equal record ids from different owners", async () => {
    const user = userEvent.setup();
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(SAME_ID_DIFFERENT_OWNER_RESPONSE),
      ),
    );
    renderPanel({ direction: "parents", variant: "record" });

    expect(await screen.findByText("I1. First owner")).toBeInTheDocument();
    expect(screen.getByText("I1. Second owner")).toBeInTheDocument();
    await user.click(screen.getAllByTestId("trace-hide")[0]);

    expect(screen.queryByText("I1. First owner")).not.toBeInTheDocument();
    expect(screen.getByText("I1. Second owner")).toBeInTheDocument();
    expect(screen.getAllByTestId("traceability-record-card")).toHaveLength(1);
  });

  it("extracts question titles and removes their raw label from the preview", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-children", () =>
        HttpResponse.json(QUESTION_RESPONSE),
      ),
    );
    renderPanel({ direction: "children", variant: "record" });

    expect(
      await screen.findByText("Q2a. Which policy applies?"),
    ).toBeInTheDocument();
    expect(screen.getByText("Question body only")).toBeInTheDocument();
    expect(
      screen.queryByText("[Q2a. Which policy applies?]"),
    ).not.toBeInTheDocument();
  });

  it.each(["parents", "children"] as const)(
    "clamps %s content to two lines and toggles the complete preview on click",
    async (direction) => {
      const user = userEvent.setup();
      server.use(
        http.get(`*/api/v1/grounding/trace-${direction}`, () =>
          HttpResponse.json(EXPANDABLE_CONTENT_RESPONSE),
        ),
      );
      renderPanel({ direction, variant: "record" });

      const card = (
        await screen.findAllByTestId("traceability-record-card")
      )[0];
      const toggle = within(card).getByTestId("trace-content-toggle");
      const preview = within(card).getByTestId("trace-content-preview");
      expect(preview).toHaveAttribute("aria-expanded", "false");
      expect(preview.style.maxHeight).toBe("40px");
      expect(preview).toHaveStyle({ overflow: "hidden" });
      expect(screen.queryByText("ref")).not.toBeInTheDocument();
      expect(screen.queryByText("75%")).not.toBeInTheDocument();

      await user.click(preview);
      expect(preview).toHaveAttribute("aria-expanded", "true");
      expect(toggle).toHaveAttribute("aria-expanded", "true");
      expect(preview.style.maxHeight).toBe("");
      expect(preview.style.overflow).toBe("");

      await user.click(preview);
      expect(preview).toHaveAttribute("aria-expanded", "false");
      expect(toggle).toHaveAttribute("aria-expanded", "false");
      expect(preview.style.maxHeight).not.toBe("");
      expect(preview).toHaveStyle({ overflow: "hidden" });
    },
  );

  it.each(["parents", "children"] as const)(
    "renders %s ontology previews as Rich Markdown instead of source syntax",
    async (direction) => {
      server.use(
        http.get(`*/api/v1/grounding/trace-${direction}`, () =>
          HttpResponse.json(RICH_PREVIEW_RESPONSE),
        ),
      );
      renderPanel({ direction, variant: "record" });

      const preview = await screen.findByTestId("trace-content-preview");
      expect(within(preview).getByText("bold intent").tagName).toBe("STRONG");
      expect(within(preview).getByText("emphasis").tagName).toBe("EM");
      expect(preview.querySelectorAll("li")).toHaveLength(2);
      expect(preview.querySelector("table")).toBeInTheDocument();
      expect(preview).not.toHaveTextContent("**bold intent**");
    },
  );

  it("keeps code trace content in a code preview instead of parsing source as Markdown", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-children", () =>
        HttpResponse.json(CODE_PREVIEW_RESPONSE),
      ),
    );
    renderPanel({ direction: "children", variant: "record" });

    const preview = await screen.findByTestId("trace-content-preview");
    expect(within(preview).getByTestId("trace-code-preview")).toHaveTextContent(
      "# not a heading",
    );
    expect(preview.style.maxHeight).toBe("52px");
    expect(preview.querySelector("h1")).not.toBeInTheDocument();
    expect(preview).toHaveTextContent("**literal**");
  });

  it("renders empty state when no results", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-children", () =>
        HttpResponse.json(EMPTY_RESPONSE),
      ),
    );
    renderPanel({ direction: "children" });

    await waitFor(() => {
      expect(screen.getByText("No driven intents found")).toBeInTheDocument();
    });
    expect(screen.getByText("Drives")).toBeInTheDocument();
  });

  it("calls onDismiss when dismiss button is clicked", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(PARENTS_RESPONSE),
      ),
    );
    const { props } = renderPanel();

    await waitFor(() => {
      expect(screen.getByText("I3. Auth Flow")).toBeInTheDocument();
    });

    await userEvent.click(screen.getByTestId("traceability-dismiss"));
    expect(props.onDismiss).toHaveBeenCalledOnce();
  });

  it("navigates view and project-doc trace rows as doc tabs with source metadata", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(NAVIGATION_RESPONSE),
      ),
    );
    const { props } = renderPanel({
      fileType: "view",
      fileName: "views/source.pm_view.md",
      intentRecordNumber: "I1",
    });

    await waitFor(() => {
      expect(screen.getByText("I4. Security View")).toBeInTheDocument();
    });
    expect(screen.getByText("Trace to view content")).toBeInTheDocument();

    await userEvent.click(screen.getByText("I4. Security View"));
    expect(props.onNavigate).toHaveBeenCalledWith({
      type: "doc",
      id: "views/security.pm_view.md",
      workspaceId: "branch-docs",
      repoId: "repo-docs",
      intentRecordNumber: "I4",
      sourceFileName: "views/source.pm_view.md",
      sourceIntentRecordNumber: "I1",
      sourceFileType: "doc",
    });

    // external_doc rows carry no extraction title, so the label alone renders.
    await userEvent.click(screen.getByText("D1"));
    expect(props.onNavigate).toHaveBeenCalledWith({
      type: "doc",
      id: "docs/security_plan.md",
      workspaceId: "branch-docs",
      repoId: "repo-docs",
      intentRecordNumber: "D1",
      sourceFileName: "views/source.pm_view.md",
      sourceIntentRecordNumber: "I1",
      sourceFileType: "doc",
    });
  });

  it("opens only API-resolved physical code paths while preserving owner and origin metadata", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(CODE_NAVIGATION_RESPONSE),
      ),
    );
    const { props } = renderPanel({
      fileName: ".semlift/specs/source.semlift.md",
      intentRecordNumber: "D4",
    });

    await userEvent.click(await screen.findByText("open_physical"));
    expect(props.onNavigate).toHaveBeenLastCalledWith({
      type: "file",
      id: "generation/g5-api/src/routes/grounding.rs",
      workspaceId: "branch-code-owner",
      repoId: "repo-code-owner",
      intentRecordNumber: "open_physical",
      sourceFileName: ".semlift/specs/source.semlift.md",
      sourceIntentRecordNumber: "D4",
      sourceFileType: "spec",
    });

    await userEvent.click(screen.getByText("open_same"));
    expect(props.onNavigate).toHaveBeenLastCalledWith({
      type: "file",
      id: "src/lib.rs",
      workspaceId: "branch-code-owner",
      repoId: "repo-code-owner",
      intentRecordNumber: "open_same",
      sourceFileName: ".semlift/specs/source.semlift.md",
      sourceIntentRecordNumber: "D4",
      sourceFileType: "spec",
    });

    const missingRow = screen.getByText("missing_path").closest("tr");
    expect(missingRow).not.toBeNull();
    expect(within(missingRow!).getByTestId("trace-navigate")).toBeDisabled();
    expect(
      within(missingRow!).getByTitle(
        "Code source unavailable — no file to open",
      ),
    ).toBeDisabled();
    expect(props.onNavigate).toHaveBeenCalledTimes(2);
  });

  it("shows error state when API fails", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json({ error: "Not found" }, { status: 404 }),
      ),
    );
    renderPanel();

    await waitFor(() => {
      expect(screen.getByText("Failed to load intents")).toBeInTheDocument();
    });
  });

  it.each(["table", "record"] as const)(
    "opens structured chat details from the %s presentation without navigation or another request",
    async (variant) => {
      let requestCount = 0;
      server.use(
        http.get("*/api/v1/grounding/trace-parents", () => {
          requestCount += 1;
          return HttpResponse.json(CHAT_RESPONSE);
        }),
      );
      const { props } = renderPanel({ variant });

      const trigger = await screen.findByRole("button", {
        name: "Show chat details for session-chat-1",
      });
      await userEvent.click(trigger);

      const dialog = screen.getByRole("dialog", {
        name: "Chat intent details",
      });
      expect(dialog).toHaveStyle({ position: "fixed" });
      expect(within(dialog).getByText("session-chat-1")).toBeInTheDocument();
      expect(within(dialog).getByText(/Release review/)).toHaveTextContent(
        "Release review Second line",
      );
      expect(
        within(dialog).getByText("Please keep Unicode intact: café 東京 ✅"),
      ).toBeInTheDocument();
      expect(within(dialog).getByText("human@example.com")).toBeInTheDocument();
      expect(
        within(dialog).getByText("2026-08-03T12:34:56Z"),
      ).toBeInTheDocument();
      expect(within(dialog).getByText("Human-authored")).toBeInTheDocument();
      expect(within(dialog).getByText("Priority 2")).toBeInTheDocument();
      expect(within(dialog).getByText("Certainty 94%")).toBeInTheDocument();
      expect(dialog).not.toHaveTextContent("Legacy combined context");
      expect(props.onNavigate).not.toHaveBeenCalled();
      expect(requestCount).toBe(1);
      expect(screen.getByTestId("trace-navigate")).toBeDisabled();
    },
  );

  it("supports keyboard activation and renders legacy content once only when structured text is absent", async () => {
    const legacy = structuredClone(CHAT_RESPONSE);
    const row = legacy.data.results[0];
    delete (row as Partial<typeof row>).chatContext;
    delete (row as Partial<typeof row>).userContent;
    row.content = "Legacy content exactly once";
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(legacy),
      ),
    );
    renderPanel();

    const trigger = await screen.findByRole("button", {
      name: "Show chat details for session-chat-1",
    });
    trigger.focus();
    await userEvent.keyboard("{Enter}");

    const dialog = screen.getByRole("dialog", { name: "Chat intent details" });
    expect(
      within(dialog).getAllByText("Legacy content exactly once"),
    ).toHaveLength(1);
  });

  it("clamps and repositions the portaled chat popup on viewport changes", async () => {
    let rect = {
      top: 40,
      bottom: 60,
      left: 980,
      right: 1040,
      width: 60,
      height: 20,
      x: 980,
      y: 40,
      toJSON: () => ({}),
    } as DOMRect;
    const rectSpy = vi
      .spyOn(HTMLElement.prototype, "getBoundingClientRect")
      .mockImplementation(() => rect);
    const originalWidth = window.innerWidth;
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1024,
    });
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(CHAT_RESPONSE),
      ),
    );
    renderPanel();
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Show chat details for session-chat-1",
      }),
    );

    const dialog = screen.getByRole("dialog", { name: "Chat intent details" });
    expect(dialog.parentElement).toBe(document.body);
    expect(dialog).toHaveStyle({ left: "600px", top: "64px" });

    rect = { ...rect, left: 24, bottom: 100, top: 80, x: 24, y: 80 };
    fireEvent.scroll(window);
    await waitFor(() =>
      expect(dialog).toHaveStyle({ left: "24px", top: "104px" }),
    );

    rect = { ...rect, left: -20, bottom: 120, top: 100, x: -20, y: 100 };
    fireEvent.resize(window);
    await waitFor(() =>
      expect(dialog).toHaveStyle({ left: "8px", top: "124px" }),
    );

    rectSpy.mockRestore();
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: originalWidth,
    });
  });

  it("dismisses chat details by close, outside pointer, Escape, and anchor removal", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(CHAT_RESPONSE),
      ),
    );
    renderPanel();
    const trigger = await screen.findByRole("button", {
      name: "Show chat details for session-chat-1",
    });

    await userEvent.click(trigger);
    await userEvent.click(
      screen.getByRole("button", { name: "Close chat details" }),
    );
    expect(
      screen.queryByRole("dialog", { name: "Chat intent details" }),
    ).toBeNull();

    await userEvent.click(trigger);
    fireEvent.pointerDown(document.body);
    expect(
      screen.queryByRole("dialog", { name: "Chat intent details" }),
    ).toBeNull();

    await userEvent.click(trigger);
    await userEvent.keyboard("{Escape}");
    expect(
      screen.queryByRole("dialog", { name: "Chat intent details" }),
    ).toBeNull();

    await userEvent.click(trigger);
    await userEvent.click(screen.getByTestId("trace-hide"));
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "Chat intent details" }),
      ).toBeNull(),
    );
  });

  it("keeps popup state independent across two trace panels", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json(CHAT_RESPONSE),
      ),
    );
    const wrapper = createWrapper();
    const props: IntentTraceabilityPanelProps = {
      direction: "parents",
      workspaceId: "branch-001",
      repoId: "repo-001",
      fileType: "ontology",
      fileName: ".semlift/specs/example.semlift.md",
      intentRecordNumber: "I1",
      onDismiss: vi.fn(),
      onNavigate: vi.fn(),
    };
    render(
      <>
        <IntentTraceabilityPanel {...props} />
        <IntentTraceabilityPanel {...props} intentRecordNumber="I2" />
      </>,
      { wrapper },
    );

    await waitFor(() =>
      expect(
        screen.getAllByRole("button", {
          name: "Show chat details for session-chat-1",
        }),
      ).toHaveLength(2),
    );
    const triggers = screen.getAllByRole("button", {
      name: "Show chat details for session-chat-1",
    });
    await userEvent.click(triggers[0]);
    expect(
      await screen.findAllByRole("dialog", { name: "Chat intent details" }),
    ).toHaveLength(1);
    await userEvent.click(triggers[1]);
    await waitFor(() =>
      expect(
        screen.getAllByRole("dialog", { name: "Chat intent details" }),
      ).toHaveLength(2),
    );
  });
});
