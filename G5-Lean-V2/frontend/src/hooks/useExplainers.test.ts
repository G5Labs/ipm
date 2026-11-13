// =============================================================================
// useExplainers.test.ts — Explainer Hook Integration Tests with MSW
// =============================================================================

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { beforeEach, describe, expect, it } from "vitest";
import { server } from "../test/msw/server";
import {
  isExplainerTemplateConflict,
  pushRecentExplainerTemplate,
  readRecentExplainerTemplates,
  recentExplainerTemplatesKey,
  recordRecentExplainerTemplate,
  useExplainerTemplates,
  useSaveExplainerTemplate,
  type ExplainerTemplate,
} from "./useExplainers";

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
      mutations: { retry: false },
    },
  });

  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: queryClient }, children);

  return { wrapper, queryClient };
}

const TEMPLATES: ExplainerTemplate[] = [
  {
    name: "Big picture",
    slug: "big-picture",
    prompt: "Explain the big picture.",
    source: "builtin",
  },
  {
    name: "My saved",
    slug: "my-saved",
    prompt: "Saved prompt.",
    source: "saved",
  },
];

describe("useExplainerTemplates", () => {
  it("lists templates for the branch", async () => {
    server.use(
      http.get(
        "/api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates",
        () => HttpResponse.json({ data: TEMPLATES }),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => useExplainerTemplates("ws-1", "branch-docs"),
      {
        wrapper,
      },
    );

    await waitFor(() => expect(result.current.status).toBe("ok"));
    if (result.current.status === "ok") {
      expect(result.current.data).toEqual(TEMPLATES);
    }
  });
});

describe("useSaveExplainerTemplate", () => {
  it("posts the template and returns headCommit + template", async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates",
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            data: { headCommit: "abc123", template: TEMPLATES[1] },
          });
        },
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => useSaveExplainerTemplate("ws-1", "branch-docs"),
      { wrapper },
    );

    const saved = await result.current.mutateAsync({
      name: "My saved",
      slug: "my-saved",
      prompt: "Saved prompt.",
    });

    expect(saved.headCommit).toBe("abc123");
    expect(saved.template.slug).toBe("my-saved");
    expect(bodies).toEqual([
      { name: "My saved", slug: "my-saved", prompt: "Saved prompt." },
    ]);
  });

  it("surfaces a duplicate-slug 409 distinctly", async () => {
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates",
        () =>
          HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "Template slug already exists",
              },
            },
            { status: 409 },
          ),
      ),
    );

    const { wrapper } = createWrapper();
    const { result } = renderHook(
      () => useSaveExplainerTemplate("ws-1", "branch-docs"),
      { wrapper },
    );

    let caught: unknown = null;
    try {
      await result.current.mutateAsync({
        name: "My saved",
        slug: "my-saved",
        prompt: "Saved prompt.",
      });
    } catch (err: unknown) {
      caught = err;
    }

    expect(caught).not.toBeNull();
    expect(isExplainerTemplateConflict(caught)).toBe(true);
    expect(isExplainerTemplateConflict(new Error("other"))).toBe(false);
  });
});

describe("recent explainer template helpers", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("uses a per-workspace storage key", () => {
    expect(recentExplainerTemplatesKey("ws-9")).toBe(
      "g5:explainerTemplates:recent:ws-9",
    );
  });

  it("prepends, dedupes, and caps at 5 (pure)", () => {
    expect(pushRecentExplainerTemplate([], "a")).toEqual(["a"]);
    expect(pushRecentExplainerTemplate(["a", "b"], "b")).toEqual(["b", "a"]);
    expect(pushRecentExplainerTemplate(["a", "b", "c", "d", "e"], "f")).toEqual(
      ["f", "a", "b", "c", "d"],
    );
  });

  it("round-trips through localStorage most-recent-first", () => {
    expect(readRecentExplainerTemplates("ws-1")).toEqual([]);
    recordRecentExplainerTemplate("ws-1", "big-picture");
    recordRecentExplainerTemplate("ws-1", "balanced");
    recordRecentExplainerTemplate("ws-1", "big-picture");
    expect(readRecentExplainerTemplates("ws-1")).toEqual([
      "big-picture",
      "balanced",
    ]);
    // Other workspaces are unaffected.
    expect(readRecentExplainerTemplates("ws-2")).toEqual([]);
  });

  it("treats malformed stored data as empty", () => {
    localStorage.setItem(recentExplainerTemplatesKey("ws-1"), "not json");
    expect(readRecentExplainerTemplates("ws-1")).toEqual([]);
    localStorage.setItem(
      recentExplainerTemplatesKey("ws-1"),
      JSON.stringify({ nope: true }),
    );
    expect(readRecentExplainerTemplates("ws-1")).toEqual([]);
    localStorage.setItem(
      recentExplainerTemplatesKey("ws-1"),
      JSON.stringify(["ok", 42, "also-ok", null]),
    );
    expect(readRecentExplainerTemplates("ws-1")).toEqual(["ok", "also-ok"]);
  });
});
