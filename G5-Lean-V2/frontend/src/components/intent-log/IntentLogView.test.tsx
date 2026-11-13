// =============================================================================
// IntentLogView container tests (React Query + react-router + MSW).
// Test view: intent_log.test_view.md TC-ILVIEW-001/002.
// =============================================================================

import { describe, it, expect, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, useLocation, useNavigate } from "react-router-dom";

import { server } from "../../test/msw/server";
import { IntentLogView } from "./IntentLogView";

function LocationProbe({ onChange }: { onChange?: (search: string) => void }) {
  const location = useLocation();
  React.useEffect(() => {
    onChange?.(location.search);
  }, [location.search, onChange]);
  return null;
}

function NavigateProbe({ to }: { to: string }) {
  const navigate = useNavigate();
  return (
    <button type="button" onClick={() => navigate(to)}>
      navigate-test
    </button>
  );
}

function renderView(
  onNavigate = vi.fn(),
  initialEntry = "/o/w",
  onLocationChange?: (search: string) => void,
) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const rendered = render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[initialEntry]}>
        <LocationProbe onChange={onLocationChange} />
        <IntentLogView
          workspaceId="branch-1"
          repoId="repo-1"
          onNavigate={onNavigate}
        />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return { ...rendered, onNavigate };
}

function rowsResponse(
  diagnostics: Array<{ sourcePath: string; reason: string }> = [],
) {
  return {
    data: {
      rows: [
        {
          id: "id-1",
          occurredAt: "2026-06-01T12:00:00Z",
          occurredAtSource: "record",
          actor: "Ziniu",
          actorSource: "record",
          sourceKind: "intent_record",
          intentKind: "chat",
          authorType: "human",
          artifactType: "chat_session",
          artifactPath: null,
          blockId: "C1",
          contentPreview: "Please add the intent log",
          content: "Please add the intent log",
          isHuman: true,
          traceKey: null,
          traceStatus: "unlinked",
          rawSource: { type: "chat", id: "C1", sessionId: "sess-1" },
        },
      ],
      meta: {
        facets: { repo: [{ value: "g5", count: 1 }] },
        cursor: null,
        hasMore: false,
        diagnostics,
      },
    },
  };
}

describe("IntentLogView", () => {
  it("renders rows returned by the API (TC-ILVIEW-001)", async () => {
    server.use(
      http.get("*/api/v1/intent-log", () => HttpResponse.json(rowsResponse())),
    );
    renderView();
    await waitFor(() =>
      expect(screen.getByText("Please add the intent log")).toBeTruthy(),
    );
    expect(screen.getByText("All intent logs")).toBeTruthy();
    expect(
      screen
        .getByRole("button", { name: /^human$/i })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    expect(
      screen.getByRole("button", { name: /^supervisor agent$/i }),
    ).toBeTruthy();
    expect(screen.queryByText(/^Source$/i, { selector: "span" })).toBeNull();
    expect(screen.queryByText(/^Artifact$/i, { selector: "span" })).toBeNull();
    expect(screen.queryByText(/supervisor decision/i)).toBeNull();
    // No diagnostics → no audit-incomplete banner.
    expect(screen.queryByText(/audit incomplete/i)).toBeNull();
  });

  it("shows the audit-incomplete banner when diagnostics are present (TC-ILVIEW-002)", async () => {
    server.use(
      http.get("*/api/v1/intent-log", () =>
        HttpResponse.json(
          rowsResponse([
            {
              sourcePath: "intent_record/ontology/bad.jsonl",
              reason: "malformed JSONL",
            },
          ]),
        ),
      ),
    );
    renderView();
    await waitFor(() =>
      expect(screen.getByText(/audit incomplete/i)).toBeTruthy(),
    );
    expect(
      screen.getByText(/intent_record\/ontology\/bad\.jsonl/),
    ).toBeTruthy();
  });

  it("shows an empty state when the API returns no rows", async () => {
    server.use(
      http.get("*/api/v1/intent-log", () =>
        HttpResponse.json({
          data: {
            rows: [],
            meta: { facets: {}, cursor: null, hasMore: false, diagnostics: [] },
          },
        }),
      ),
    );
    renderView();
    await waitFor(() =>
      expect(screen.getByText(/no intent records match/i)).toBeTruthy(),
    );
  });

  it("renders the empty state (not an indefinite loading spinner) when no workspace branch is resolved (TC-ILVIEW-005)", async () => {
    const apiCalls: string[] = [];
    server.use(
      http.get("*/api/v1/intent-log", ({ request }) => {
        apiCalls.push(request.url);
        return HttpResponse.json(rowsResponse());
      }),
    );
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={["/o/w"]}>
          <IntentLogView workspaceId="" repoId="" onNavigate={vi.fn()} />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    // An empty/unborn workspace resolves no branch, so `useIntentLog` is
    // disabled. The viewer must surface the empty state immediately rather than
    // a loading skeleton that can never resolve, and must not hit the API.
    await waitFor(() =>
      expect(screen.getByText(/no intent records match/i)).toBeTruthy(),
    );
    expect(screen.queryByText(/loading intent records/i)).toBeNull();
    expect(apiCalls).toHaveLength(0);
  });

  it("opens navigable rows with branch and repo ownership", async () => {
    server.use(
      http.get("*/api/v1/intent-log", () =>
        HttpResponse.json({
          data: {
            rows: [
              {
                id: "id-ontology",
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
                content: "Initialize the feature module.",
                isHuman: true,
                traceKey: {
                  fileType: "ontology",
                  fileName: ".semlift/specs/x.semlift.md",
                  id: "I1",
                },
                traceStatus: "traceable",
                rawSource: { type: "ontology", id: "I1", repoId: "repo-1" },
              },
            ],
            meta: { facets: {}, cursor: null, hasMore: false, diagnostics: [] },
          },
        }),
      ),
    );
    const { onNavigate } = renderView();
    await waitFor(() =>
      expect(screen.getByText("Initialize the feature module.")).toBeTruthy(),
    );
    fireEvent.click(screen.getByTitle("Open artifact"));
    expect(onNavigate).toHaveBeenCalledWith({
      type: "spec",
      id: ".semlift/specs/x.semlift.md",
      intentRecordNumber: "I1",
      workspaceId: "branch-1",
      repoId: "repo-1",
    });
  });

  it("reflects user-facing filters into API queries", async () => {
    const capturedUrls: string[] = [];
    server.use(
      http.get("*/api/v1/intent-log", ({ request }) => {
        capturedUrls.push(request.url);
        return HttpResponse.json(rowsResponse());
      }),
    );
    renderView();
    await waitFor(() =>
      expect(screen.getByText("Please add the intent log")).toBeTruthy(),
    );

    fireEvent.click(screen.getByText("traceable"));
    const authorTypeFilters = screen.getByText("Author type").parentElement;
    expect(authorTypeFilters).not.toBeNull();
    fireEvent.click(within(authorTypeFilters!).getByText("agent"));
    fireEvent.click(within(authorTypeFilters!).getByText("system"));
    fireEvent.click(within(authorTypeFilters!).getByText("unknown"));
    fireEvent.change(screen.getByLabelText("Time"), {
      target: { value: "custom" },
    });
    fireEvent.change(screen.getByLabelText("From"), {
      target: { value: "2026-05-01" },
    });
    fireEvent.change(screen.getByLabelText("To"), {
      target: { value: "2026-06-01" },
    });
    fireEvent.change(screen.getByLabelText("Actor"), {
      target: { value: "Ziniu" },
    });
    fireEvent.change(screen.getByLabelText("Repo"), {
      target: { value: "g5" },
    });

    await waitFor(() => {
      const params = capturedUrls.map((url) => new URL(url).searchParams);
      expect(params.some((p) => p.get("traceStatus") === "traceable")).toBe(
        true,
      );
      expect(
        params.some(
          (p) => p.get("authorType") === "human,agent,system,unknown",
        ),
      ).toBe(true);
      expect(params.some((p) => p.get("timeRange") === "custom")).toBe(true);
      expect(params.some((p) => p.get("from") === "2026-05-01")).toBe(true);
      expect(params.some((p) => p.get("to") === "2026-06-01")).toBe(true);
      expect(params.some((p) => p.get("actor") === "Ziniu")).toBe(true);
      expect(params.some((p) => p.get("repo") === "g5")).toBe(true);
      expect(params.every((p) => p.get("sourceKind") === null)).toBe(true);
      expect(params.every((p) => p.get("artifactType") === null)).toBe(true);
    });
  });

  it("preserves unrelated workspace query params when reflecting filters", async () => {
    const locationSearches: string[] = [];
    server.use(
      http.get("*/api/v1/intent-log", () => HttpResponse.json(rowsResponse())),
    );
    renderView(
      vi.fn(),
      "/o/w?branch=branch-keep&session=session-keep&tab=changes&view=settings",
      (search) => locationSearches.push(search),
    );
    await waitFor(() =>
      expect(screen.getByText("Please add the intent log")).toBeTruthy(),
    );

    fireEvent.click(screen.getByText("traceable"));

    await waitFor(() => {
      const latest = new URLSearchParams(
        locationSearches[locationSearches.length - 1] ?? "",
      );
      expect(latest.get("traceStatus")).toBe("traceable");
      expect(latest.get("authorType")).toBe("human");
      expect(latest.get("branch")).toBe("branch-keep");
      expect(latest.get("session")).toBe("session-keep");
      expect(latest.get("tab")).toBe("changes");
      expect(latest.get("view")).toBe("settings");
    });
  });

  it("hydrates cursor/filter state from the URL and reflects the default author type", async () => {
    const capturedParams: URLSearchParams[] = [];
    const locationSearches: string[] = [];
    server.use(
      http.get("*/api/v1/intent-log", ({ request }) => {
        capturedParams.push(new URL(request.url).searchParams);
        return HttpResponse.json(rowsResponse());
      }),
    );

    renderView(
      vi.fn(),
      "/o/w?q=provenance&cursor=page-2&branch=branch-keep",
      (search) => locationSearches.push(search),
    );

    await waitFor(() =>
      expect(screen.getByText("Please add the intent log")).toBeTruthy(),
    );
    expect(
      capturedParams.some(
        (params) =>
          params.get("q") === "provenance" &&
          params.get("cursor") === "page-2" &&
          params.get("authorType") === "human",
      ),
    ).toBe(true);

    await waitFor(() => {
      const latest = new URLSearchParams(
        locationSearches[locationSearches.length - 1] ?? "",
      );
      expect(latest.get("cursor")).toBe("page-2");
      expect(latest.get("authorType")).toBe("human");
      expect(latest.get("branch")).toBe("branch-keep");
    });
  });

  it("updates the URL cursor when loading more while preserving accumulated rows", async () => {
    const capturedParams: URLSearchParams[] = [];
    const locationSearches: string[] = [];
    server.use(
      http.get("*/api/v1/intent-log", ({ request }) => {
        const params = new URL(request.url).searchParams;
        capturedParams.push(params);
        const cursor = params.get("cursor");
        return HttpResponse.json({
          data: {
            rows: [
              {
                ...rowsResponse().data.rows[0],
                id: cursor ? "id-2" : "id-1",
                contentPreview: cursor
                  ? "Second page intent"
                  : "First page intent",
                content: cursor ? "Second page intent" : "First page intent",
              },
            ],
            meta: {
              facets: {},
              cursor: cursor ? null : "page-2",
              hasMore: !cursor,
              diagnostics: [],
            },
          },
        });
      }),
    );

    renderView(vi.fn(), "/o/w?branch=branch-keep", (search) =>
      locationSearches.push(search),
    );
    await waitFor(() =>
      expect(screen.getByText("First page intent")).toBeTruthy(),
    );

    fireEvent.click(screen.getByRole("button", { name: /load more/i }));

    await waitFor(() => {
      expect(screen.getByText("First page intent")).toBeTruthy();
      expect(screen.getByText("Second page intent")).toBeTruthy();
    });
    expect(
      capturedParams.some((params) => params.get("cursor") === "page-2"),
    ).toBe(true);
    await waitFor(() => {
      const latest = new URLSearchParams(
        locationSearches[locationSearches.length - 1] ?? "",
      );
      expect(latest.get("cursor")).toBe("page-2");
      expect(latest.get("branch")).toBe("branch-keep");
    });
  });

  it("resyncs filters and cursor when browser navigation changes the URL", async () => {
    const capturedParams: URLSearchParams[] = [];
    server.use(
      http.get("*/api/v1/intent-log", ({ request }) => {
        capturedParams.push(new URL(request.url).searchParams);
        return HttpResponse.json(rowsResponse());
      }),
    );

    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={["/o/w?authorType=human"]}>
          <NavigateProbe to="/o/w?authorType=agent&cursor=page-agent" />
          <IntentLogView
            workspaceId="branch-1"
            repoId="repo-1"
            onNavigate={vi.fn()}
          />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    await waitFor(() =>
      expect(screen.getByText("Please add the intent log")).toBeTruthy(),
    );

    fireEvent.click(screen.getByText("navigate-test"));

    await waitFor(() => {
      expect(
        capturedParams.some(
          (params) =>
            params.get("authorType") === "agent" &&
            params.get("cursor") === "page-agent",
        ),
      ).toBe(true);
    });
  });
});
