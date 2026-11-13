// Production upload modal, file read/Save hooks and Monaco. The browser test
// supplies HTTP storage; git_refs_authoritative tests the real API and live RPC.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { CreateProjectDocModal } from "../../src/components/views/CreateProjectDocModal";
import { FileView } from "../../src/components/files/FileView";
import { ComposerOwner } from "../../src/components/comments/ComposerOwner";
import {
  fetchWorkspaceFile,
  useSaveWorkspaceFile,
  useWorkspaceFileContent,
} from "../../src/hooks/useFiles";
import "../../src/index.css";

const workspaceId = "workspace";
const repoId = "docs";

function UploadedDoc({ path }: { path: string }) {
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
      principal="person:organization"
      lineTarget={null}
      viewerOwnsWorkspace
      ownerVmRunning
      workspaceWritable
      onSave={(content, expectedHash, expectedVmId) =>
        save.mutateAsync({
          workspaceId,
          repoId,
          path,
          content,
          expectedHash,
          expectedVmId,
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

function App() {
  const [path, setPath] = useState(
    new URLSearchParams(location.search).get("path"),
  );
  return (
    <main style={{ height: "100vh" }}>
      <CreateProjectDocModal
        open={!path}
        workspaceId={workspaceId}
        defaultFolderPath={`docs/${workspaceId}`}
        initialMode="upload"
        onClose={() => {}}
        onCreated={(doc) => {
          const path = doc.filePath!;
          history.replaceState(null, "", `?path=${encodeURIComponent(path)}`);
          setPath(path);
        }}
      />
      {path && <UploadedDoc path={path} />}
    </main>
  );
}

createRoot(document.getElementById("root")!).render(
  <QueryClientProvider
    client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
  >
    <MemoryRouter>
      <ComposerOwner scope="workspace">
        <App />
      </ComposerOwner>
    </MemoryRouter>
  </QueryClientProvider>,
);
