import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import React from "react";
import { describe, expect, it } from "vitest";

import type { DocNode } from "../lib/types";
import { server } from "../test/msw/server";
import {
  parseRepositoryDocPath,
  repositoryDocHandle,
  useDocList,
} from "./useDocs";

function makeWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
  };
}

function doc(fields: Partial<DocNode> = {}): DocNode {
  return {
    id: "2ecf80cc-1e56-4a8c-9cdf-23fb5fabdbd0",
    name: "Architecture",
    docType: "design",
    filePath: "docs/architecture.md",
    linkedNodeCount: 0,
    workspaceId: "c958a0a5-2de7-4347-a21d-4cb3cc6b94b1",
    repoId: "82ae45ac-8aca-44e2-9654-34706359c027",
    lastUpliftedAt: null,
    lastDownliftedAt: null,
    contentUpdatedAt: null,
    createdAt: "2026-07-29T00:00:00Z",
    updatedAt: "2026-07-29T00:00:00Z",
    ...fields,
  };
}

describe("current Docs display routing", () => {
  it("reads document summaries from the provenance-bearing list envelope", async () => {
    const listed = doc();
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/docs", () =>
        HttpResponse.json({
          data: {
            docs: [listed],
            source: "flush",
            repoOid: "c".repeat(40),
            snapshotAt: "2026-09-21T00:00:00Z",
          },
        }),
      ),
    );

    const { result } = renderHook(
      () => useDocList("c958a0a5-2de7-4347-a21d-4cb3cc6b94b1"),
      {
        wrapper: makeWrapper(),
      },
    );
    await waitFor(() => expect(result.current.status).toBe("ok"));

    if (result.current.status === "ok") {
      expect(result.current.data).toHaveLength(1);
      expect(result.current.data[0].folder).toBe("docs");
    }
  });

  it("round-trips UTF-8 repository paths in the display handle", () => {
    const handle = repositoryDocHandle(
      doc({ filePath: "docs/设计/architecture.md" }),
    );
    expect(handle).not.toBeNull();
    expect(parseRepositoryDocPath(handle ?? "")).toBe(
      "docs/设计/architecture.md",
    );
  });

  it("rejects incomplete or malformed repository handles", () => {
    expect(
      parseRepositoryDocPath("overlay::ZG9jcy9hcmNoaXRlY3R1cmUubWQ"),
    ).toBeNull();
    expect(
      parseRepositoryDocPath("overlay:not-a-uuid:ZG9jcy9hcmNoaXRlY3R1cmUubWQ"),
    ).toBeNull();
    expect(
      parseRepositoryDocPath("overlay:82ae45ac-8aca-44e2-9654-34706359c027:"),
    ).toBeNull();
  });
});
