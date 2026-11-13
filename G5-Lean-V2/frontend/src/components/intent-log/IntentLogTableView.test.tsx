// =============================================================================
// IntentLogTableView component tests.
// Test view: intent_log.test_view.md TC-ILTBL-002 / TC-ILTBL-003 / TC-ILVIEW.
// Proves the table renders rows, exposes only read-only actions, and that only
// `traceable` rows enable the trace action.
// =============================================================================

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import type { ReactElement } from "react";

import type { IntentLogEntry } from "../../hooks/useIntentLog";
import { server } from "../../test/msw/server";
import { IntentLogTableView } from "./IntentLogTableView";

function entry(overrides: Partial<IntentLogEntry> = {}): IntentLogEntry {
  return {
    id: "id-1",
    occurredAt: "2026-06-01T12:00:00Z",
    occurredAtSource: "record",
    actor: "Ziniu",
    actorSource: "record",
    sourceKind: "intent_record",
    intentKind: "ontology",
    authorType: "human",
    artifactType: "ontology",
    artifactPath: ".semlift/specs/x.semlift.md",
    blockId: "I1",
    contentPreview: "Initialize the feature module.",
    content: "Initialize the feature module.\nMore detail here.",
    isHuman: true,
    traceKey: {
      fileType: "ontology",
      fileName: ".semlift/specs/x.semlift.md",
      id: "I1",
    },
    traceStatus: "traceable",
    rawSource: { type: "ontology", id: "I1", repoId: "repo-1" },
    ...overrides,
  };
}

const FORBIDDEN = [
  /edit/i,
  /delete/i,
  /approve/i,
  /resolve/i,
  /register/i,
  /repair/i,
  /ground/i,
];

function renderWithQuery(ui: ReactElement) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  return render(
    <QueryClientProvider client={client}>{ui}</QueryClientProvider>,
  );
}

describe("IntentLogTableView", () => {
  it("renders rows with kind and trace-status chips", () => {
    render(
      <IntentLogTableView
        rows={[entry()]}
        group="none"
        workspaceId="branch-1"
        onOpen={() => {}}
        onTrace={() => {}}
      />,
    );
    expect(screen.getByText("Initialize the feature module.")).toBeTruthy();
    expect(screen.getByText("ontology")).toBeTruthy();
    expect(screen.getByText("traceable")).toBeTruthy();
  });

  it("exposes only read-only actions (no edit/delete/approve/register/ground)", () => {
    render(
      <IntentLogTableView
        rows={[entry()]}
        group="none"
        workspaceId="branch-1"
        onOpen={() => {}}
        onTrace={() => {}}
      />,
    );
    for (const button of screen.getAllByRole("button")) {
      const label = `${button.getAttribute("title") ?? ""} ${button.textContent ?? ""}`;
      for (const forbidden of FORBIDDEN) {
        expect(forbidden.test(label)).toBe(false);
      }
    }
  });

  it("enables trace only for traceable rows", () => {
    const onTrace = vi.fn();
    render(
      <IntentLogTableView
        rows={[
          entry({ id: "ok", traceStatus: "traceable" }),
          entry({ id: "no", traceStatus: "unlinked", blockId: "I2" }),
        ]}
        group="none"
        workspaceId="branch-1"
        onOpen={() => {}}
        onTrace={onTrace}
      />,
    );
    const traceButtons = screen.getAllByTitle(/trace/i);
    // First row is traceable → invoking fires the callback.
    fireEvent.click(traceButtons[0]);
    expect(onTrace).toHaveBeenCalledTimes(1);
    // Second row is unlinked → its trace control does not fire.
    fireEvent.click(traceButtons[1]);
    expect(onTrace).toHaveBeenCalledTimes(1);
  });

  it("enables open only for navigable artifact rows", () => {
    const onOpen = vi.fn();
    render(
      <IntentLogTableView
        rows={[
          entry({
            id: "ontology",
            traceKey: {
              fileType: "ontology",
              fileName: ".semlift/specs/x.semlift.md",
              id: "I1",
            },
          }),
          entry({
            id: "chat",
            intentKind: "chat",
            artifactType: "chat_session",
            artifactPath: null,
            blockId: "C1",
            traceKey: { fileType: "chat", fileName: "session-1", id: "C1" },
            rawSource: { type: "chat", id: "C1", sessionId: "session-1" },
          }),
        ]}
        group="none"
        workspaceId="branch-1"
        onOpen={onOpen}
        onTrace={() => {}}
      />,
    );
    fireEvent.click(screen.getAllByTitle("Open artifact")[0]);
    expect(onOpen).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTitle("No file to open"));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("expands a row to show provenance detail", () => {
    render(
      <IntentLogTableView
        rows={[entry({ traceKey: null, traceStatus: "unlinked" })]}
        group="none"
        workspaceId="branch-1"
        onOpen={() => {}}
        onTrace={() => {}}
      />,
    );
    expect(screen.queryByText("Stable id")).toBeNull();
    fireEvent.click(screen.getByLabelText("Expand row"));
    expect(screen.getByText("Stable id")).toBeTruthy();
  });

  it("renders expanded-row lineage from existing trace endpoints", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json({
          data: {
            totalCount: 1,
            results: [
              {
                intentRecordId: "parent-1",
                workspaceId: "branch-parents",
                repoId: "repo-docs",
                fileType: "view",
                fileName: "views/pm_views/source.pm_view.md",
                intentRecordNumber: "I2",
                codeBlockName: null,
                content: "[I2. Source]",
                isHuman: true,
                priority: 1,
                certainty: 90,
              },
            ],
          },
        }),
      ),
      http.get("*/api/v1/grounding/trace-children", () =>
        HttpResponse.json({
          data: {
            totalCount: 1,
            results: [
              {
                intentRecordId: "child-1",
                workspaceId: "branch-children",
                repoId: "repo-code",
                fileType: "code",
                fileName: "client/src/components/intent-log/IntentLogView.tsx",
                intentRecordNumber: null,
                codeBlockName: "IntentLogView",
                content: null,
                isHuman: false,
                priority: 2,
                certainty: 80,
              },
            ],
          },
        }),
      ),
    );

    renderWithQuery(
      <IntentLogTableView
        rows={[entry()]}
        group="none"
        workspaceId="branch-1"
        onOpen={() => {}}
        onTrace={() => {}}
      />,
    );

    fireEvent.click(screen.getByLabelText("Expand row"));
    expect(screen.getByText(/intent graph/i)).toBeTruthy();

    await waitFor(() => {
      expect(screen.getByText(/1 ancestor/i)).toBeTruthy();
      expect(screen.getByText(/1 descendant/i)).toBeTruthy();
    });
  });

  it("shows an empty state when there are no rows", () => {
    render(
      <IntentLogTableView
        rows={[]}
        group="none"
        workspaceId="branch-1"
        onOpen={() => {}}
        onTrace={() => {}}
      />,
    );
    expect(screen.getByText(/no intent records match/i)).toBeTruthy();
  });

  it("makes the What column the primary reading column (TC-ILTBL-007)", () => {
    render(
      <IntentLogTableView
        rows={[entry()]}
        group="none"
        workspaceId="branch-1"
        onOpen={() => {}}
        onTrace={() => {}}
      />,
    );
    const span = screen.getByText("Initialize the feature module.");
    expect(span.className).toContain("line-clamp-3");
    const cell = span.closest("td");
    expect(cell?.className).toContain("w-[60%]");
    expect(cell?.className).toContain("max-w-[84rem]");
  });

  it("pins the expanded detail to the visible viewport (TC-ILTBL-010)", () => {
    const { container } = render(
      <IntentLogTableView
        rows={[entry({ traceKey: null, traceStatus: "unlinked" })]}
        group="none"
        workspaceId="branch-1"
        onOpen={() => {}}
        onTrace={() => {}}
      />,
    );
    fireEvent.click(screen.getByLabelText("Expand row"));
    const sticky = container.querySelector('[style*="sticky"]');
    expect(sticky).not.toBeNull();
  });
});
