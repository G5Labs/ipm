import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import type { DocNode } from "../lib/types";
import { WebSocketManager, type WSEvent } from "../lib/ws";
import { server } from "../test/msw/server";
import { _activeTestClients } from "../test/render";
import { useDocList } from "./useDocs";
import { useWorkspaceFileContent } from "./useFiles";

describe("ordinary agent files on the document surface", () => {
  it.each([
    ["docs-repo", "code-repo", "src/main.rs", false],
    [undefined, "code-repo", "src/main.rs", true],
    [null, "docs-repo", "docs/workspace-1/tour.md", true],
    ["docs-repo", "docs-repo", "docs/workspace-1/tour.md", true],
    ["docs-repo", "docs-repo", "explainers/tour.html", true],
    ["docs-repo", "docs-repo", "designs/tour.md", true],
    ["docs-repo", "docs-repo", "views/tour.html", true],
    ["docs-repo", "docs-repo", "", true],
  ] as const)(
    "refreshes document caches with docs repo %s for %s/%s: %s",
    (docsRepoId, repoId, path, expected) => {
      const workspaceId = "workspace-1";
      const queryClient = new QueryClient();
      _activeTestClients.push(queryClient);
      if (docsRepoId !== undefined) {
        queryClient.setQueryData(["workspace", workspaceId], { docsRepoId });
      }
      const families = ["docs", "views", "view"];
      for (const family of [...families, "workspace-file"]) {
        queryClient.setQueryData([family, workspaceId], []);
      }
      const manager = new WebSocketManager({
        getBaseUrl: () => "ws://test",
        getWsTicket: async () => "ticket",
        queryClient,
        // Each event here stands alone; coalescing is covered by
        // `fileChangeCoalescer.test.ts`.
        fileChangeCoalesceMs: 0,
      });
      (
        manager as unknown as { handleEvent: (event: WSEvent) => void }
      ).handleEvent({
        type: "workspace.file_changed",
        workspaceId,
        repoId,
        path,
        rescanRequired: path === "",
      });
      for (const family of families) {
        expect(
          queryClient.getQueryState([family, workspaceId])?.isInvalidated,
        ).toBe(expected);
      }
      expect(
        queryClient.getQueryState(["workspace-file", workspaceId])
          ?.isInvalidated,
      ).toBe(true);
    },
  );

  it.each([
    ["html", "workspace.file_changed"],
    ["md", "workspace.file_changed"],
    ["html", "doc.content_changed"],
    ["md", "doc.content_changed"],
  ] as const)(
    "refreshes created, edited, and deleted .%s explainers on %s events",
    async (extension, type) => {
      const workspaceId = "11111111-1111-4111-8111-111111111111";
      const repoId = "22222222-2222-4222-8222-222222222222";
      const path = `docs/${workspaceId}/explainers/tour.${extension}`;
      let exists = false;
      let content = "First version";
      const doc: DocNode = {
        workspaceId,
        repoId,
        id: "live-doc",
        name: "Tour",
        docType: "explainer",
        filePath: path,
        draftDirty: true,
        linkedNodeCount: 0,
        lastUpliftedAt: null,
        lastDownliftedAt: null,
        contentUpdatedAt: null,
        createdAt: "2026-09-26T00:00:00Z",
        updatedAt: "2026-09-26T00:00:00Z",
      };
      server.use(
        http.get("/api/v1/workspaces/:id/docs", () =>
          HttpResponse.json({ data: exists ? [doc] : [] }),
        ),
        http.get("/api/v1/workspaces/:id/files/*", ({ request }) => {
          expect(new URL(request.url).searchParams.get("repoId")).toBe(repoId);
          expect(new URL(request.url).pathname).toContain(`/files/${path}`);
          return HttpResponse.json({
            data: {
              path,
              content,
              size: content.length,
              hash: content,
              encoding: "utf-8",
              source: "live",
              vmId: "vm-live",
              repoOid: null,
              snapshotAt: null,
            },
          });
        }),
      );
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false, staleTime: Infinity } },
      });
      _activeTestClients.push(queryClient);
      const manager = new WebSocketManager({
        getBaseUrl: () => "ws://test",
        getWsTicket: async () => "ticket",
        queryClient,
        // Each event here stands alone; coalescing is covered by
        // `fileChangeCoalescer.test.ts`.
        fileChangeCoalesceMs: 0,
      });
      const fileChanged = () => {
        (
          manager as unknown as { handleEvent: (event: WSEvent) => void }
        ).handleEvent({
          type,
          docId: doc.id,
          workspaceId,
          repoId,
          path,
          rescanRequired: false,
        });
      };
      const { result } = renderHook(
        () => {
          const list = useDocList(workspaceId);
          const body = useWorkspaceFileContent(
            workspaceId,
            repoId,
            list.status === "ok" && list.data.length
              ? (list.data[0].filePath ?? "")
              : "",
          );
          return { list, body: body.state };
        },
        {
          wrapper: ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={queryClient}>
              {children}
            </QueryClientProvider>
          ),
        },
      );
      await waitFor(() =>
        expect(result.current.list).toMatchObject({ status: "ok", data: [] }),
      );
      exists = true;
      act(fileChanged);
      await waitFor(() =>
        expect(result.current.list).toMatchObject({
          status: "ok",
          data: [
            expect.objectContaining({ filePath: path, docType: "explainer" }),
          ],
        }),
      );
      await waitFor(() =>
        expect(result.current.body).toMatchObject({
          status: "ok",
          data: { content: "First version" },
        }),
      );
      content = "Edited by the next chat turn";
      act(fileChanged);
      await waitFor(() =>
        expect(result.current.body).toMatchObject({
          status: "ok",
          data: { content },
        }),
      );
      exists = false;
      act(fileChanged);
      await waitFor(() =>
        expect(result.current.list).toMatchObject({ status: "ok", data: [] }),
      );
    },
  );
});
