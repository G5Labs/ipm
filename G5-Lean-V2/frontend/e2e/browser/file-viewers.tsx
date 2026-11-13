// FILES viewers on real decoders: FileView over a generated fixture
// (e2e/fixtures/viewers), opened as the workspace page would open it
// (`g5.documents.viewers`). `?fixture=<file>` reads a fixture;
// `?text=<content>&name=<path>` opens inline text under that name.
import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { FileView } from "../../src/components/files/FileView";
import type { WorkspaceFileContent } from "../../src/hooks/useFiles";
import "../../src/index.css";

const query = new URLSearchParams(location.search);
const fixture = query.get("fixture") ?? "transparent.png";
const name = query.get("name") ?? fixture;
const text = query.get("text");

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

function read(content: string, encoding: "utf-8" | "base64", size: number) {
  return {
    path: name,
    content,
    size,
    encoding,
    hash: `hash-${content.length}`,
    source: "live",
    repoOid: null,
    snapshotAt: null,
    vmId: "vm",
  } satisfies WorkspaceFileContent;
}

function Harness() {
  // Inline text opens at once; a fixture is fetched.
  const [file, setFile] = useState<WorkspaceFileContent | null>(() =>
    text !== null
      ? read(text, "utf-8", new TextEncoder().encode(text).length)
      : null,
  );
  useEffect(() => {
    if (text !== null) return;
    void fetch(`/e2e/fixtures/viewers/${fixture}`)
      .then((response) => response.arrayBuffer())
      .then((buffer) => {
        const bytes = new Uint8Array(buffer);
        if (fixture.endsWith(".svg")) {
          setFile(read(new TextDecoder().decode(bytes), "utf-8", bytes.length));
        } else {
          setFile(read(base64(bytes), "base64", bytes.length));
        }
      });
  }, []);
  if (!file) return <p>Loading fixture…</p>;
  return (
    <FileView
      workspaceId="workspace"
      repoId="repo"
      filePath={name}
      language=""
      file={file}
      principal="person:organization"
      lineTarget={null}
      viewerOwnsWorkspace
      ownerVmRunning
      workspaceWritable
      onSave={async () => ({ hash: "saved" })}
      onReload={async () => file}
      onFetchCurrent={async () => ({ content: file.content, hash: file.hash })}
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
    <main
      data-testid="center-panel"
      style={{ height: "100vh", width: "100vw", display: "flex" }}
    >
      <div style={{ flex: 1, minWidth: 0, minHeight: 0 }}>
        <Harness />
      </div>
    </main>
  </QueryClientProvider>,
);
