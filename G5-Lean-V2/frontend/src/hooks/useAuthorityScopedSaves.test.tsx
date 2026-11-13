import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import type { ReactNode } from "react";
import { expect, it } from "vitest";
import { server } from "../test/msw/server";
import { WorkspaceLiveGateProvider } from "../lib/workspaceLiveGate";
import { repositoryDocHandle } from "./useDocs";
import { currentFileTab } from "../lib/currentFileTab";
import { useWorkspaceFileContent, useSaveWorkspaceFile } from "./useFiles";

for (const kind of ["doc", "file"] as const)
  for (const replace of [false, true]) {
    it(`${kind} Save updates its captured reader partition${replace ? " after runtime replacement" : ""}`, async () => {
      const client = new QueryClient({
        defaultOptions: {
          queries: { retry: false, staleTime: Infinity },
          mutations: { retry: false },
        },
      });
      let runtime = "vm-one",
        requested = false,
        releaseWrite!: () => void,
        releaseRead!: () => void;
      const write = new Promise<void>((resolve) => {
        releaseWrite = resolve;
      });
      const read = new Promise<void>((resolve) => {
        releaseRead = resolve;
      });
      const path = "docs/a.md",
        docId = repositoryDocHandle({
          id: "",
          workspaceId: "c958a0a5-2de7-4347-a21d-4cb3cc6b94b1",
          repoId: "82ae45ac-8aca-44e2-9654-34706359c027",
          filePath: path,
        })!;
      const file = currentFileTab(
        kind === "doc"
          ? { type: "doc", docId, name: "Document" }
          : {
              type: "file",
              filePath: path,
              branchId: "82ae45ac-8aca-44e2-9654-34706359c027",
              name: "File",
            },
        [],
        "c958a0a5-2de7-4347-a21d-4cb3cc6b94b1",
      );
      if (file.type !== "file" || !file.branchId)
        throw new Error("The opener did not resolve to a repository file");
      const prefix = [
        "workspace-file",
        "c958a0a5-2de7-4347-a21d-4cb3cc6b94b1",
        file.branchId,
        file.filePath,
      ];
      const key = (runtime: string) => [
        ...prefix,
        JSON.stringify([true, "live", runtime]),
      ];
      const old = {
        id: docId,
        workspaceId: "c958a0a5-2de7-4347-a21d-4cb3cc6b94b1",
        repoId: "82ae45ac-8aca-44e2-9654-34706359c027",
        filePath: path,
        path,
        content: "old",
        contentHash: "old",
        hash: "old",
        source: "live",
        encoding: "utf-8",
        size: 3,
        vmId: "vm-one",
        repoOid: null,
        snapshotAt: null,
      };
      const saved = {
        ...old,
        content: "saved",
        contentHash: "saved",
        hash: "saved",
        size: 5,
      };
      const other = { ...old, content: "other runtime", vmId: "vm-two" };
      client.setQueryData(key("vm-one"), old);
      client.setQueryData(key("vm-two"), other);
      server.use(
        http.put("/api/v1/workspaces/:id/files/*", async () => {
          requested = true;
          await write;
          return HttpResponse.json({ data: saved });
        }),
        http.get("/api/v1/workspaces/:id/files/*", async () => {
          await read;
          return HttpResponse.json({
            data: runtime === "vm-one" ? saved : other,
          });
        }),
      );
      function Wrapper({ children }: { children: ReactNode }) {
        return (
          <QueryClientProvider client={client}>
            <WorkspaceLiveGateProvider
              value={{
                kind: "live",
                live: true,
                owner: true,
                canWrite: true,
                canStart: false,
                reviewCopy: false,
                reviewIdentityKnown: true,
                workspaceNoun: "the workspace",
                runtimeKey: runtime,
              }}
            >
              {children}
            </WorkspaceLiveGateProvider>
          </QueryClientProvider>
        );
      }
      const { result, rerender, unmount } = renderHook(
        () => {
          const content = useWorkspaceFileContent(
            "c958a0a5-2de7-4347-a21d-4cb3cc6b94b1",
            file.branchId!,
            file.filePath,
          );
          return {
            reader: content.state,
            save: useSaveWorkspaceFile(),
          };
        },
        { wrapper: Wrapper },
      );
      try {
        let saving!: Promise<unknown>;
        act(() => {
          saving = result.current.save.mutateAsync({
            workspaceId: "c958a0a5-2de7-4347-a21d-4cb3cc6b94b1",
            repoId: file.branchId!,
            path: file.filePath,
            content: "saved",
            expectedHash: "old",
            expectedVmId: "vm-one",
          });
        });
        await waitFor(() => expect(requested).toBe(true));
        if (replace) {
          runtime = "vm-two";
          rerender();
        }
        await act(async () => {
          releaseWrite();
          await saving;
        });
        expect(
          client.getQueryData<{ content: string }>(key("vm-one"))?.content,
        ).toBe("saved");
        expect(
          client.getQueryData<{ content: string }>(key("vm-two"))?.content,
        ).toBe("other runtime");
        await waitFor(() => {
          expect(result.current.reader.status).toBe("ok");
          if (result.current.reader.status === "ok")
            expect(result.current.reader.data.content).toBe(
              replace ? "other runtime" : "saved",
            );
        });
        expect(client.getQueryData(prefix)).toBeUndefined();
      } finally {
        releaseWrite();
        releaseRead();
        unmount();
        await client.cancelQueries();
        client.clear();
      }
    });
  }
