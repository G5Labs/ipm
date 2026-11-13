// A findings file in the production file view (g5.documents.findings): the
// owner's Preview with severity, decisions and the filter. The spec serves the
// API (auth, files, the issue tracker) with `page.route`.
import React from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { AuthProvider } from "../../src/hooks/useAuth";
import { FileView } from "../../src/components/files/FileView";
import {
  fetchWorkspaceFile,
  useSaveWorkspaceFile,
  useWorkspaceFileContent,
} from "../../src/hooks/useFiles";
import "../../src/index.css";

const workspaceId = "ws";
const repoId = "repo";
const path = ".g5/TLA_verification/bugs_issues.md";
const seeded: string[] = [];
(window as unknown as { findingsTest: unknown }).findingsTest = { seeded };

function FindingsFile() {
  const read = useWorkspaceFileContent(workspaceId, repoId, path);
  const save = useSaveWorkspaceFile();
  if (read.state.status !== "ok") return <p>{read.state.status}</p>;
  return (
    <FileView
      workspaceId={workspaceId}
      repoId={repoId}
      filePath={path}
      file={read.state.data}
      language="markdown"
      principal="user-1:org-g5labs"
      lineTarget={null}
      viewerOwnsWorkspace
      ownerVmRunning
      workspaceWritable
      previewProps={{ onDiscussFinding: (text) => seeded.push(text) }}
      onSave={(content, expectedHash, expectedVmId, options) =>
        save.mutateAsync({
          workspaceId,
          repoId,
          path,
          content,
          expectedHash,
          expectedVmId,
          commitAndPush: options?.commitAndPush,
        })
      }
      onReload={() => fetchWorkspaceFile(workspaceId, repoId, path)}
      onFetchCurrent={() => fetchWorkspaceFile(workspaceId, repoId, path)}
      onOpenWorkspaceFile={() => {}}
      onStartWorkspace={() => {}}
      editorProps={{ showComments: false, comments: [] }}
    />
  );
}

createRoot(document.getElementById("root")!).render(
  <QueryClientProvider
    client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
  >
    <MemoryRouter initialEntries={[`/g5labs/findings/file/${path}`]}>
      <AuthProvider>
        <main style={{ maxWidth: 1100, margin: "0 auto", height: "100vh" }}>
          <Routes>
            <Route path="/:org/:workspace/file/*" element={<FindingsFile />} />
          </Routes>
        </main>
      </AuthProvider>
    </MemoryRouter>
  </QueryClientProvider>,
);
