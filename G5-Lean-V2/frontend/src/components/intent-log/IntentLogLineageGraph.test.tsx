// =============================================================================
// IntentLogLineageGraph component tests.
// Test view: intent_log.test_view.md TC-ILOG-203.
// Proves the expanded-row graph uses existing trace endpoints, preserves
// ownership in navigation, and does not fabricate lineage for unlinked rows.
// =============================================================================

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import type { ReactElement } from "react";
import { describe, expect, it, vi } from "vitest";

import type { IntentLogEntry } from "../../hooks/useIntentLog";
import { server } from "../../test/msw/server";
import { IntentLogLineageGraph } from "./IntentLogLineageGraph";

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
    rawSource: { type: "ontology", id: "I1", repoId: "repo-owning" },
    ...overrides,
  };
}

function renderWithQuery(ui: ReactElement) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  return render(
    <QueryClientProvider client={client}>{ui}</QueryClientProvider>,
  );
}

function emptyTraceResponse() {
  return HttpResponse.json({
    data: {
      totalCount: 0,
      results: [],
    },
  });
}

function parentChildHandlers() {
  return [
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
              content: "",
              title: "Source",
              blockLabel: "I2",
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
  ];
}

describe("IntentLogLineageGraph", () => {
  it("renders endpoint state from empty existing trace responses", async () => {
    let traceGraphCalls = 0;
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () => emptyTraceResponse()),
      http.get("*/api/v1/grounding/trace-children", () => emptyTraceResponse()),
      http.all("*/api/v1/grounding/trace-graph", () => {
        traceGraphCalls += 1;
        return HttpResponse.json({ data: {} });
      }),
    );

    renderWithQuery(
      <IntentLogLineageGraph entry={entry()} workspaceId="branch-1" />,
    );

    await waitFor(() => {
      expect(screen.getByText(/chain endpoint/i)).toBeTruthy();
    });
    expect(traceGraphCalls).toBe(0);
  });

  it("does not fetch or render lineage for unlinked rows", async () => {
    let parentCalls = 0;
    let childCalls = 0;
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () => {
        parentCalls += 1;
        return emptyTraceResponse();
      }),
      http.get("*/api/v1/grounding/trace-children", () => {
        childCalls += 1;
        return emptyTraceResponse();
      }),
    );

    const { container } = renderWithQuery(
      <IntentLogLineageGraph
        entry={entry({ traceStatus: "unlinked" })}
        workspaceId="branch-1"
      />,
    );

    await waitFor(() => {
      expect(parentCalls).toBe(0);
      expect(childCalls).toBe(0);
    });
    expect(container.textContent).toBe("");
  });

  it("passes the row owning repo id to trace requests", async () => {
    const requests: URLSearchParams[] = [];
    server.use(
      http.get("*/api/v1/grounding/trace-parents", ({ request }) => {
        requests.push(new URL(request.url).searchParams);
        return emptyTraceResponse();
      }),
      http.get("*/api/v1/grounding/trace-children", ({ request }) => {
        requests.push(new URL(request.url).searchParams);
        return emptyTraceResponse();
      }),
    );

    renderWithQuery(
      <IntentLogLineageGraph entry={entry()} workspaceId="branch-1" />,
    );

    await waitFor(() => {
      expect(requests).toHaveLength(2);
    });
    expect(
      requests.every((params) => params.get("repoId") === "repo-owning"),
    ).toBe(true);
    expect(
      requests.every((params) => params.get("workspaceId") === "branch-1"),
    ).toBe(true);
  });

  it("inspects a node then navigates via Go to I/D block, preserving ownership", async () => {
    const onNavigate = vi.fn();
    server.use(...parentChildHandlers());

    renderWithQuery(
      <IntentLogLineageGraph
        entry={entry()}
        workspaceId="branch-1"
        onNavigate={onNavigate}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText(/1 ancestor/i)).toBeTruthy();
      expect(screen.getByText(/1 descendant/i)).toBeTruthy();
    });

    // Clicking a node opens the inline inspector — it does NOT navigate directly.
    fireEvent.click(
      screen.getByRole("button", { name: /inspect intent block i2 source/i }),
    );
    expect(onNavigate).not.toHaveBeenCalled();
    expect(screen.getByTestId("block-inspector")).toBeTruthy();

    // Navigation happens only via the explicit "Go to I/D block" action.
    fireEvent.click(screen.getByRole("button", { name: /go to i\/d block/i }));
    expect(onNavigate).toHaveBeenCalledWith({
      fileType: "view",
      fileName: "views/pm_views/source.pm_view.md",
      recordId: "I2",
      workspaceId: "branch-parents",
      repoId: "repo-docs",
    });

    // Child node (ownership = code repo).
    fireEvent.click(
      screen.getByRole("button", {
        name: /inspect intent block fn IntentLogView/i,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /go to i\/d block/i }));
    expect(onNavigate).toHaveBeenCalledWith({
      fileType: "code",
      fileName: "client/src/components/intent-log/IntentLogView.tsx",
      recordId: "IntentLogView",
      workspaceId: "branch-children",
      repoId: "repo-code",
    });

    // Center node (the current intent, owning repo).
    fireEvent.click(
      screen.getByRole("button", { name: /inspect intent block i1/i }),
    );
    fireEvent.click(screen.getByRole("button", { name: /go to i\/d block/i }));
    expect(onNavigate).toHaveBeenCalledWith({
      fileType: "ontology",
      fileName: ".semlift/specs/x.semlift.md",
      recordId: "I1",
      workspaceId: "branch-1",
      repoId: "repo-owning",
    });
  });

  it("inspector shows id, title, file path and content (TC-ILLG-003/004)", async () => {
    server.use(...parentChildHandlers());
    renderWithQuery(
      <IntentLogLineageGraph entry={entry()} workspaceId="branch-1" />,
    );
    await waitFor(() => {
      expect(screen.getByText(/1 ancestor/i)).toBeTruthy();
    });

    fireEvent.click(
      screen.getByRole("button", { name: /inspect intent block i1/i }),
    );
    const inspector = screen.getByTestId("block-inspector");
    // id + title + file path + content; "current intent" badge on the center.
    expect(inspector.textContent).toContain("Initialize the feature module");
    expect(inspector.textContent).toContain(".semlift/specs/x.semlift.md");
    expect(inspector.textContent).toContain("current intent");
    // No fabricated diff section when no diff is provided (PM [D26]).
    expect(screen.queryByTestId("block-inspector-diff")).toBeNull();

    // Outside-click dismisses the inspector.
    fireEvent.click(screen.getByTestId("block-inspector-backdrop"));
    expect(screen.queryByTestId("block-inspector")).toBeNull();
  });

  it("renders pan/zoom controls in the focus graph (TC-ILLG-006)", async () => {
    server.use(...parentChildHandlers());
    renderWithQuery(
      <IntentLogLineageGraph entry={entry()} workspaceId="branch-1" />,
    );
    await waitFor(() => {
      expect(screen.getByText(/1 ancestor/i)).toBeTruthy();
    });

    expect(screen.getByRole("button", { name: /zoom in/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /zoom out/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /reset view/i })).toBeTruthy();
    expect(screen.getByText(/drag to pan/i)).toBeTruthy();
  });

  it("greys 'Go to I/D block' for chat nodes (TC-ILLG-007)", async () => {
    const onNavigate = vi.fn();
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json({
          data: {
            totalCount: 1,
            results: [
              {
                intentRecordId: "chat-1",
                workspaceId: "branch-chat",
                repoId: "repo-docs",
                fileType: "chat",
                fileName: null,
                intentRecordNumber: "C7",
                codeBlockName: null,
                content:
                  "Chat context: Reviewing audit behavior\n---------\nUser input: Please add the audit log",
                isHuman: true,
                priority: 1,
                certainty: 90,
              },
            ],
          },
        }),
      ),
      http.get("*/api/v1/grounding/trace-children", () => emptyTraceResponse()),
    );

    renderWithQuery(
      <IntentLogLineageGraph
        entry={entry()}
        workspaceId="branch-1"
        onNavigate={onNavigate}
      />,
    );
    await waitFor(() => {
      expect(screen.getByText(/1 ancestor/i)).toBeTruthy();
    });
    expect(screen.getByText("Please add the audit log")).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", { name: /inspect intent block c7/i }),
    );
    const goto = screen.getByRole("button", {
      name: /go to i\/d block/i,
    }) as HTMLButtonElement;
    expect(goto.disabled).toBe(true);
    expect(screen.getByText(/no i\/d block to open/i)).toBeTruthy();

    fireEvent.click(goto);
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("uses server blockLabel when title and content are unavailable", async () => {
    server.use(
      http.get("*/api/v1/grounding/trace-parents", () =>
        HttpResponse.json({
          data: {
            totalCount: 1,
            results: [
              {
                intentRecordId: "parent-label-only",
                workspaceId: "branch-parent",
                repoId: "repo-docs",
                fileType: "view",
                fileName: "views/label-only.pm_view.md",
                intentRecordNumber: "D9",
                codeBlockName: null,
                content: null,
                title: null,
                blockLabel: "D9",
                isHuman: true,
                priority: null,
                certainty: null,
              },
            ],
          },
        }),
      ),
      http.get("*/api/v1/grounding/trace-children", () => emptyTraceResponse()),
    );
    renderWithQuery(
      <IntentLogLineageGraph entry={entry()} workspaceId="branch-1" />,
    );

    await waitFor(() => {
      expect(
        screen.getByRole("button", {
          name: "Inspect intent block D9 D9",
        }),
      ).toBeTruthy();
    });
  });
});
